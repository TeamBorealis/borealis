//! Settings > DATA FILES "SECONDARY CONTENT DIRECTORY": content files found in a user-linked
//! folder are converted and installed as user packs (`sec-<name>`) through the same pipeline as a
//! manual import: size caps, conversion, reserved bundled ids (`install_packs`), atomic install.
//! The folder is re-read when it is linked and at startup; unlinking removes those packs.

use crate::content_dto::ImportPackRequest;
use crate::content_import::{slug, write_pack_meta, Scratch, MAX_PACK_BYTES};
use crate::content_index::sha256_hex;
use crate::content_packs::{EventSink, PACK_META_FILE};
use crate::data_dirs::validate_user_dir;
use crate::error::{ApiError, ApiResult};
use crate::picked_paths::PickPurpose;
use crate::settings_dto::{SecondaryContentReport, SecondaryContentResult, SkippedContentFile};
use crate::state::{user_pack_dirs, AppState};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Pack id prefix for secondary-directory packs (never a bundled id).
pub const SECONDARY_PREFIX: &str = "sec-";
/// `pack-meta.json` kind for secondary-directory packs.
const SECONDARY_KIND: &str = "secondary";
/// Most content files read from the folder.
pub const MAX_SECONDARY_FILES: usize = 200;
/// Deepest sub-folder level scanned (0 = the folder itself).
const MAX_SECONDARY_DEPTH: usize = 3;
/// Most bytes read from the folder in one pass.
const MAX_SECONDARY_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
const EXTENSIONS: [&str; 2] = ["xml", "json"];

struct Found {
    rel: String,
    path: PathBuf,
}

fn skip(file: &str, reason: &str) -> SkippedContentFile {
    SkippedContentFile {
        file: file.to_string(),
        reason: reason.to_string(),
    }
}

fn rel_name(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Content files under `root` (no symlinks, no hidden entries, never inside `excluded`, e.g.
/// the app's own data folder when the user links one of its parents), within the caps.
fn scan(root: &Path, excluded: &[PathBuf], skipped: &mut Vec<SkippedContentFile>) -> Vec<Found> {
    let mut found = Vec::new();
    let mut total: u64 = 0;
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            let Some(name) = name.filter(|n| !n.starts_with('.')) else {
                continue;
            };
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.is_dir() {
                let blocked = std::fs::canonicalize(&path)
                    .map_or(true, |c| excluded.iter().any(|x| c.starts_with(x)));
                if depth < MAX_SECONDARY_DEPTH && !blocked {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            let ext = name.rsplit_once('.').map(|(_, e)| e.to_lowercase());
            if !meta.is_file() || !ext.is_some_and(|e| EXTENSIONS.contains(&e.as_str())) {
                continue;
            }
            let rel = rel_name(root, &path);
            if found.len() >= MAX_SECONDARY_FILES {
                skipped.push(skip(&rel, "Too many content files in the folder."));
            } else if meta.len() == 0 || meta.len() > MAX_PACK_BYTES as u64 {
                skipped.push(skip(&rel, "The file is empty or too large to import."));
            } else if total + meta.len() > MAX_SECONDARY_TOTAL_BYTES {
                skipped.push(skip(&rel, "The folder holds too much content to load."));
            } else {
                total += meta.len();
                found.push(Found { rel, path });
            }
        }
    }
    found.sort_by(|a, b| a.rel.cmp(&b.rel));
    found
}

fn pack_meta(dir: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(dir.join(PACK_META_FILE)).ok()?).ok()
}

/// Installed secondary packs: `{id -> sha256}`.
fn installed_secondary(library: &Path) -> BTreeMap<String, String> {
    user_pack_dirs(library)
        .into_iter()
        .filter_map(|dir| {
            let meta = pack_meta(&dir)?;
            (meta["kind"] == SECONDARY_KIND).then(|| {
                let id = dir.file_name()?.to_string_lossy().into_owned();
                Some((id, meta["sha256"].as_str().unwrap_or_default().to_string()))
            })?
        })
        .collect()
}

impl AppState {
    /// Link the folder behind a `secondary-content-dir` pick and load its content files.
    ///
    /// # Errors
    /// `invalid_argument` for a bad token or an unusable folder; `io_error` / `content_error`.
    pub fn set_secondary_content_dir(
        &self,
        token: &str,
        sink: &dyn EventSink,
    ) -> ApiResult<SecondaryContentResult> {
        let picked = self.picked.take(token, PickPurpose::SecondaryContentDir)?;
        let dir = validate_user_dir(&picked, &self.secondary_forbidden(), false)?;
        self.store_secondary_dir(Some(dir.to_string_lossy().into_owned()))?;
        let report = self.sync_secondary_content(sink)?;
        Ok(SecondaryContentResult {
            dirs: self.data_dirs()?,
            report,
        })
    }

    /// Reference app "UNLINK SECONDARY DIRECTORY": forget the folder and remove the packs it provided.
    ///
    /// # Errors
    /// `io_error` / `content_error`.
    pub fn unlink_secondary_content_dir(
        &self,
        sink: &dyn EventSink,
    ) -> ApiResult<SecondaryContentResult> {
        self.store_secondary_dir(None)?;
        let report = self.sync_secondary_content(sink)?;
        Ok(SecondaryContentResult {
            dirs: self.data_dirs()?,
            report,
        })
    }

    fn store_secondary_dir(&self, dir: Option<String>) -> ApiResult<()> {
        let mut next = self.settings()?;
        next.secondary_content_dir = dir;
        next.save(&self.paths.settings_file())?;
        self.set_settings(next)
    }

    /// Bring the `sec-*` packs in line with the linked folder: convert new or changed files,
    /// remove packs whose file is gone (all of them when unlinked), then hot-reload once.
    /// A folder that no longer validates is treated as unlinked for this pass.
    ///
    /// # Errors
    /// `io_error` / `content_error` from installing or reloading.
    pub fn sync_secondary_content(
        &self,
        sink: &dyn EventSink,
    ) -> ApiResult<SecondaryContentReport> {
        let _guard = self.content_lock()?;
        let mut report = SecondaryContentReport::default();
        let root = self
            .settings()?
            .secondary_content_dir
            .map(PathBuf::from)
            .and_then(|d| validate_user_dir(&d, &self.secondary_forbidden(), false).ok());
        let library = self.paths.library();
        let installed = installed_secondary(&library);
        let found = root
            .as_deref()
            .map(|r| {
                let excluded: Vec<PathBuf> = self
                    .secondary_forbidden()
                    .iter()
                    .filter_map(|f| std::fs::canonicalize(f).ok())
                    .collect();
                scan(r, &excluded, &mut report.skipped)
            })
            .unwrap_or_default();
        let mut seen = BTreeSet::new();
        let mut staged = Vec::new();
        let mut scratches = Vec::new();
        for file in &found {
            let stem = file
                .rel
                .rsplit_once('.')
                .map_or(file.rel.as_str(), |(s, _)| s);
            let id = format!("{SECONDARY_PREFIX}{}", slug(stem));
            if !seen.insert(id.clone()) {
                report
                    .skipped
                    .push(skip(&file.rel, "Another file already uses this name."));
                continue;
            }
            if !installed.contains_key(&id) && library.join(&id).exists() {
                report.skipped.push(skip(
                    &file.rel,
                    "A pack with this name is already installed.",
                ));
                continue;
            }
            match self.stage_secondary(file, &id, installed.get(&id).map(String::as_str)) {
                Ok(Some(scratch)) => {
                    staged.push((scratch.0.join("out"), id.clone()));
                    scratches.push(scratch);
                }
                Ok(None) => {}
                // A previously installed copy stays until the file converts again.
                Err(reason) => report.skipped.push(skip(&file.rel, &reason)),
            }
        }
        let mut prefs = self.content_prefs();
        for id in installed.keys().filter(|id| !seen.contains(*id)) {
            std::fs::remove_dir_all(library.join(id))
                .map_err(|e| ApiError::io("remove secondary content", e))?;
            prefs.disabled_packs.remove(id);
            report.removed.push(id.clone());
        }
        if !staged.is_empty() {
            if !report.removed.is_empty() {
                prefs.save(&self.paths.prefs_file())?;
            }
            self.install_packs(&staged, sink, "secondary-content")?;
            report.installed = staged.into_iter().map(|(_, id)| id).collect();
        } else if !report.removed.is_empty() {
            self.reload_content(&prefs, sink, "secondary-content")?;
        }
        Ok(report)
    }

    /// Convert one file into a scratch pack; `Ok(None)` when the installed copy is current.
    fn stage_secondary(
        &self,
        file: &Found,
        id: &str,
        installed_sha: Option<&str>,
    ) -> Result<Option<Scratch>, String> {
        let bytes =
            std::fs::read(&file.path).map_err(|_| "The file cannot be read.".to_string())?;
        if bytes.is_empty() || bytes.len() > MAX_PACK_BYTES {
            return Err("The file is empty or too large to import.".into());
        }
        let sha = sha256_hex(&bytes);
        if installed_sha == Some(sha.as_str()) {
            return Ok(None);
        }
        let scratch = Scratch::new(&self.paths.data).map_err(|e| e.message)?;
        let out_dir = scratch.0.join("out");
        let request = ImportPackRequest {
            file_name: file.rel.rsplit('/').next().unwrap_or(&file.rel).to_string(),
            content_base64: String::new(),
            format: None,
        };
        let xml = file.rel.to_lowercase().ends_with(".xml");
        let report = if xml {
            Self::convert_xml(&scratch.0, &out_dir, &request, id, &bytes)
        } else {
            Self::convert_json(&out_dir, &request, id, &bytes)
        }
        .map_err(|e| e.message)?;
        if report.converted == 0 {
            return Err("The file holds no content Borealis can load.".into());
        }
        write_pack_meta(
            &out_dir,
            SECONDARY_KIND,
            &serde_json::json!({ "sha256": sha, "fileName": file.rel }),
        );
        Ok(Some(scratch))
    }
}
