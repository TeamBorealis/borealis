//! Importing packs: the reference app XML via `borealis-migrate`, JSON content files, and local 5etools data.

use crate::content_dto::{
    ContentPack, FiveToolsImportReport, FiveToolsImportRequest, FiveToolsScan, FiveToolsSource,
    ImportNote, ImportPackRequest, ImportPackResult, MigrationIssue, MigrationReport,
};
use crate::content_packs::{progress, EventSink, PACK_META_FILE};
use crate::error::{ApiError, ApiResult, ErrorCode};
use crate::session::new_id;
use crate::state::{AppState, BUNDLED_PACKS};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_content::{
    ContentFile, PackFile, PackFileKind, PackFormat, PackManifest, PackOrigin, Ruleset, Severity,
};
use borealis_import_5etools::{import_folder_with_progress, ImportOptions, Phase};
use borealis_migrate::{migrate_path, write_output, MigrateOptions};
use std::path::{Path, PathBuf};

/// Largest accepted import file.
pub const MAX_PACK_BYTES: usize = 64 * 1024 * 1024;

/// Prefix given to a manual import whose name collides with a bundled pack id.
const USER_PREFIX: &str = "user-";
/// How many overridden element ids a warning lists.
const OVERRIDE_SAMPLE: usize = 5;

/// True when `id` names a bundled pack (`core` included), which imports must never replace.
#[must_use]
pub fn is_reserved_pack_id(id: &str) -> bool {
    let wanted = slug(id);
    BUNDLED_PACKS.iter().any(|b| slug(b) == wanted)
}

/// The id a manual import is installed under: bundled ids get a `user-` prefix.
#[must_use]
pub fn import_pack_id(file_name: &str) -> String {
    let id = slug(stem(file_name));
    if is_reserved_pack_id(&id) {
        format!("{USER_PREFIX}{id}")
    } else {
        id
    }
}

/// A lower-case, folder-safe id derived from free text.
#[must_use]
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "pack".into()
    } else {
        trimmed.chars().take(64).collect()
    }
}

pub(crate) fn stem(file_name: &str) -> &str {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    base.rsplit_once('.').map_or(base, |(s, _)| s)
}

fn severity(s: Severity) -> &'static str {
    match s {
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Error => "error",
    }
}

/// A scratch folder inside the data directory, deleted on drop.
pub(crate) struct Scratch(pub(crate) PathBuf);

impl Scratch {
    pub(crate) fn new(data: &Path) -> ApiResult<Self> {
        let dir = data.join(format!(".tmp-{}", new_id()));
        std::fs::create_dir_all(&dir).map_err(|e| ApiError::io("prepare an import", e))?;
        Ok(Self(dir))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn write_pack_meta(dir: &Path, kind: &str, extra: &serde_json::Value) {
    let mut meta = serde_json::json!({ "kind": kind });
    if let (Some(m), Some(e)) = (meta.as_object_mut(), extra.as_object()) {
        m.extend(e.clone());
    }
    let text = serde_json::to_string_pretty(&meta).unwrap_or_default();
    let _ = std::fs::write(dir.join(PACK_META_FILE), text);
}

impl AppState {
    /// Move staged pack folders into the library, hot-reload, and make sure each one loaded.
    /// On a load failure the new folders are removed again and the old content is restored.
    ///
    /// # Errors
    /// `content_error` when a pack fails to load; `io_error`.
    pub(crate) fn install_packs(
        &self,
        staged: &[(PathBuf, String)],
        sink: &dyn EventSink,
        reason: &str,
    ) -> ApiResult<()> {
        if let Some((_, id)) = staged.iter().find(|(_, id)| is_reserved_pack_id(id)) {
            return Err(ApiError::invalid(format!(
                "\"{id}\" is a bundled pack id and cannot be replaced."
            )));
        }
        tracing::info!(packs = staged.len(), reason, "installing imported packs");
        let library = self.paths.library();
        let backup = Scratch::new(&self.paths.data)?;
        let mut moved = Vec::new();
        for (dir, id) in staged {
            let target = library.join(slug(id));
            if target.exists() {
                let saved = backup.0.join(slug(id));
                std::fs::rename(&target, &saved).map_err(|e| ApiError::io("replace a pack", e))?;
            }
            std::fs::rename(dir, &target).map_err(|e| ApiError::io("install a pack", e))?;
            moved.push((target, slug(id)));
        }
        let mut prefs = self.content_prefs();
        for (_, id) in staged {
            prefs.disabled_packs.remove(id);
        }
        let loaded = self.reload_content(&prefs, sink, reason).and_then(|()| {
            let hub = self.hub()?;
            let missing = staged
                .iter()
                .find(|(_, id)| !hub.store.packs().iter().any(|p| &p.id == id));
            match missing {
                None => Ok(()),
                Some((_, id)) => Err(ApiError::new(
                    ErrorCode::ContentError,
                    format!("The pack \"{id}\" could not be loaded; it was not installed."),
                )),
            }
        });
        if loaded.is_err() {
            for (target, id) in &moved {
                let _ = std::fs::remove_dir_all(target);
                let saved = backup.0.join(id);
                if saved.exists() {
                    let _ = std::fs::rename(&saved, target);
                }
            }
            let prefs = self.content_prefs();
            self.reload_content(&prefs, sink, "import-rolled-back")?;
        }
        loaded
    }

    /// Convert an uploaded XML, JSON, `.index` or `.zip` content file into a user pack and hot-reload.
    ///
    /// # Errors
    /// `invalid_argument` for undecodable input; `content_error` if the result cannot load.
    pub fn import_pack(
        &self,
        request: &ImportPackRequest,
        sink: &dyn EventSink,
    ) -> ApiResult<ImportPackResult> {
        let _guard = self.content_lock()?;
        let bytes = STANDARD
            .decode(request.content_base64.trim())
            .map_err(|_| ApiError::invalid("The file content is not valid base64."))?;
        if bytes.is_empty() || bytes.len() > MAX_PACK_BYTES {
            return Err(ApiError::invalid(
                "That file is empty or too large to import.",
            ));
        }
        tracing::info!(bytes = bytes.len(), "content file import started");
        sink.content_progress(&progress("converting", 0, 1, &request.file_name));
        self.import_files_now(request, bytes, sink)
    }

    /// A warning when the converted pack reuses element ids that bundled packs define.
    pub(crate) fn bundled_override_warning(&self, out_dir: &Path) -> Option<MigrationIssue> {
        let hub = self.hub().ok()?;
        let manifest =
            PackManifest::from_json_str(&std::fs::read_to_string(out_dir.join("pack.json")).ok()?)
                .ok()?;
        let mut hits: Vec<String> = Vec::new();
        for file in manifest
            .files
            .iter()
            .filter(|f| f.kind == PackFileKind::Content)
        {
            let Ok(text) = std::fs::read_to_string(out_dir.join(&file.path)) else {
                continue;
            };
            let Ok(content) = ContentFile::from_json_str(text.trim_start_matches('\u{feff}'))
            else {
                continue;
            };
            hits.extend(
                content
                    .elements
                    .iter()
                    .filter(|e| {
                        hub.store.get(&e.id).is_some_and(|stored| {
                            // `file` is `<pack id>/<file>`; bundled ids are reserved.
                            stored
                                .file
                                .split('/')
                                .next()
                                .is_some_and(|pack| BUNDLED_PACKS.contains(&pack))
                        })
                    })
                    .map(|e| e.id.clone()),
            );
        }
        if hits.is_empty() {
            return None;
        }
        let sample = hits
            .iter()
            .take(OVERRIDE_SAMPLE)
            .cloned()
            .collect::<Vec<_>>();
        Some(MigrationIssue {
            severity: "warning",
            element: sample.first().cloned(),
            message: format!(
                "This pack overrides {} element(s) from the bundled rules (for example {}).",
                hits.len(),
                sample.join(", ")
            ),
        })
    }

    pub(crate) fn convert_xml(
        scratch: &Path,
        out_dir: &Path,
        request: &ImportPackRequest,
        id: &str,
        bytes: &[u8],
    ) -> ApiResult<MigrationReport> {
        let input = scratch.join("input.xml");
        std::fs::write(&input, bytes).map_err(|e| ApiError::io("read the file", e))?;
        let opts = MigrateOptions {
            pack_id: Some(id.to_string()),
            pack_name: Some(stem(&request.file_name).to_string()),
            origin: PackOrigin::Custom,
            ..MigrateOptions::default()
        };
        let output = migrate_path(&input, &opts)
            .map_err(|e| ApiError::invalid(format!("Cannot convert this file: {e}")))?;
        let report = &output.report;
        let converted = report.totals.elements;
        if converted > 0 {
            write_output(out_dir, &output)
                .map_err(|e| ApiError::invalid(format!("Cannot write the converted pack: {e}")))?;
        }
        Ok(MigrationReport {
            file_name: request.file_name.clone(),
            format: "xml".into(),
            converted,
            skipped: report.totals.dropped_elements + report.summary.skipped_files,
            issues: report
                .issues
                .iter()
                .map(|i| MigrationIssue {
                    severity: severity(i.severity),
                    element: i.element.clone(),
                    message: i.message.clone(),
                })
                .collect(),
        })
    }

    pub(crate) fn convert_json(
        out_dir: &Path,
        request: &ImportPackRequest,
        id: &str,
        bytes: &[u8],
    ) -> ApiResult<MigrationReport> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| ApiError::invalid("The file is not valid UTF-8 text."))?;
        let file = ContentFile::from_json_str(text.trim_start_matches('\u{feff}'))
            .map_err(|e| ApiError::invalid(format!("Not a Borealis content file: {e}")))?;
        let manifest = PackManifest {
            format: PackFormat,
            id: id.to_string(),
            name: stem(&request.file_name).to_string(),
            version: "1.0.0".into(),
            origin: PackOrigin::Custom,
            rulesets: vec![file.ruleset.unwrap_or(Ruleset::Dnd2014)],
            license: None,
            attribution: None,
            description: None,
            files: vec![PackFile {
                path: "content.json".into(),
                kind: PackFileKind::Content,
            }],
        };
        std::fs::create_dir_all(out_dir).map_err(|e| ApiError::io("prepare an import", e))?;
        std::fs::write(out_dir.join("content.json"), text)
            .and_then(|()| {
                std::fs::write(
                    out_dir.join("pack.json"),
                    manifest.to_json_pretty().unwrap_or_default(),
                )
            })
            .map_err(|e| ApiError::io("write the pack", e))?;
        Ok(MigrationReport {
            file_name: request.file_name.clone(),
            format: "json".into(),
            converted: file.elements.len(),
            skipped: 0,
            issues: Vec::new(),
        })
    }

    /// Read-only scan of a local 5etools `data/` folder.
    ///
    /// # Errors
    /// `invalid_argument` for an unknown, expired or wrong-purpose folder token, or if the folder
    /// is missing or is not a 5etools data folder.
    pub fn scan_5etools_folder(&self, folder: &str) -> ApiResult<FiveToolsScan> {
        let dir = self.picked_folder(folder)?;
        let sources = borealis_import_5etools::scan_folder(&dir).map_err(|e| tools_error(&e))?;
        Ok(FiveToolsScan {
            sources: sources
                .into_iter()
                .map(|s| FiveToolsSource {
                    abbr: s.abbr,
                    name: s.name,
                    ruleset: s.ruleset,
                    category: s.category,
                    counts: s.counts,
                })
                .collect(),
        })
    }

    /// Convert the chosen 5etools books into `local-import` packs (never redistributable).
    ///
    /// # Errors
    /// `invalid_argument` for bad input; `content_error` if a converted pack cannot load.
    pub fn import_5etools(
        &self,
        request: &FiveToolsImportRequest,
        sink: &dyn EventSink,
    ) -> ApiResult<FiveToolsImportReport> {
        let _guard = self.content_lock()?;
        let dir = self.picked_folder(&request.folder)?;
        tracing::info!(sources = request.sources.len(), "5etools import started");
        if request.sources.is_empty() {
            return Err(ApiError::invalid("Choose at least one source to import."));
        }
        // The bundled SRD packs let imported entries link to their SRD equivalents (`maps_to`).
        let srd_dirs = SRD_PACKS
            .iter()
            .map(|n| self.paths.content_root.join(n))
            .filter(|d| d.join("pack.json").is_file())
            .collect();
        let options = ImportOptions {
            include_fluff: request.include_fluff,
            srd_dirs,
            ..ImportOptions::default().with_sources(&request.sources)
        };
        let mut emit = |e: &borealis_import_5etools::ProgressEvent| {
            if e.phase != Phase::Done {
                sink.import_progress(&progress("converting", e.current, e.total, &e.message));
            }
        };
        let result =
            import_folder_with_progress(&dir, &options, &mut emit).map_err(|e| tools_error(&e))?;
        if result.packs.is_empty() {
            return Err(ApiError::invalid(
                "None of the chosen sources contain importable data.",
            ));
        }
        let scratch = Scratch::new(&self.paths.data)?;
        let mut staged = Vec::new();
        for pack in &result.packs {
            let path = pack.write_to(&scratch.0).map_err(|e| tools_error(&e))?;
            staged.push((path, pack.manifest.id.clone()));
        }
        let existing: Vec<String> = self.list_packs()?.into_iter().map(|p| p.id).collect();
        let replaced = staged
            .iter()
            .filter(|(_, id)| existing.contains(id))
            .count();
        sink.import_progress(&progress(
            "reloading",
            staged.len(),
            staged.len(),
            "Reloading content",
        ));
        self.install_packs(&staged, sink, "5etools-imported")?;
        sink.import_progress(&progress("done", staged.len(), staged.len(), "Imported"));
        tracing::info!(
            packs = staged.len(),
            converted = result.report.converted,
            replaced,
            partial = result.report.partial.len(),
            skipped = result.report.skipped.len(),
            "5etools import finished"
        );
        let ids: Vec<&String> = staged.iter().map(|(_, id)| id).collect();
        let packs: Vec<ContentPack> = self
            .list_packs()?
            .into_iter()
            .filter(|p| ids.contains(&&p.id))
            .collect();
        let note = |n: &borealis_import_5etools::report::ReportNote| ImportNote {
            element: n.element.clone(),
            kind: n.kind.clone(),
            reason: n.reason.clone(),
        };
        Ok(FiveToolsImportReport {
            converted: result.report.converted,
            replaced,
            partial: result.report.partial.iter().map(note).collect(),
            skipped: result.report.skipped.iter().map(note).collect(),
            packs,
        })
    }
}

impl AppState {
    /// The folder behind a `pick_path` token (purpose `5etools-folder`).
    fn picked_folder(&self, token: &str) -> ApiResult<PathBuf> {
        let dir = self
            .picked
            .resolve(token, crate::picked_paths::PickPurpose::FivetoolsFolder)?;
        if !dir.is_dir() {
            return Err(ApiError::invalid("That folder does not exist."));
        }
        Ok(dir)
    }
}

/// Bundled packs whose elements 5etools imports may map to.
const SRD_PACKS: [&str; 2] = ["srd-5.1", "srd-5.2.1"];

fn tools_error(e: &borealis_import_5etools::error::ImportError) -> ApiError {
    tracing::warn!("5etools import failed: {e}");
    ApiError::invalid("This does not look like a readable 5etools data folder.")
}
