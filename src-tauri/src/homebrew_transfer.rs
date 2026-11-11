//! A homebrew pack as a file (spec 14.2, HC7, HC8; WB-24): **Export pack** writes a `.borealis-pack.zip` holding the
//! pack folder (`pack.json`, its content file, `creator.json`: no drafts, no `.trash`, no `pack-meta.json`), with the
//! text copied from non-SRD packs left out when asked; **Import** reads one back through the import preview
//! (`preview_import_pack` / `install_import_preview`): `creator.json` marks it a creator pack, so it installs as
//! `kind: homebrew` (editable) with its provenance. A pack id that is already installed is either replaced (the old
//! folder is copied to the app's trash first) or imported as a copy (a new code, id and book name; every id of the pack
//! given again, references following). Deterministic, no AI.

use crate::content_dto::{ImportPackResult, PackKind};
use crate::content_import::{is_reserved_pack_id, slug, Scratch};
use crate::content_packs::{progress, EventSink};
use crate::content_preview::{Converted, ImportPreview, Staged, TypeCount};
use crate::content_report::MAX_DIAGNOSTICS;
use crate::error::{ApiError, ApiResult};
use crate::homebrew_files::{read_pack, write_new_pack, PackWrite};
use crate::session::new_id;
use crate::state::AppState;
use crate::zip_read::{entry_names, read_zip, PACK_LIMITS};
use borealis_authoring::ids::{is_pack_code, IdGenerator};
use borealis_authoring::pack::{
    copy_code, copy_name, pack_copy, pack_id_for, without_copied_text, CreatorFile, CREATOR_FILE,
    PACK_FILE_SUFFIX, SOURCE_TYPE,
};
use borealis_content::validate::validate_elements;
use borealis_content::{ContentFile, PackFileKind, PackManifest, ParsedPack};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};

const MANIFEST_FILE: &str = "pack.json";
/// The `format` of a creator pack file in an import report.
pub const PACK_FILE_FORMAT: &str = "borealis-pack";
/// The `content-reloaded` reason of a pack file import.
pub const IMPORTED: &str = "homebrew-pack-imported";

/// An exported pack file.
#[derive(Debug, Clone)]
pub struct PackExportFile {
    /// `<Pack name>.borealis-pack.zip`.
    pub file_name: String,
    pub bytes: Vec<u8>,
    /// Elements whose copied text was left out.
    pub left_out: usize,
}

/// What an import preview of a creator pack file adds (`ImportPreview.homebrew`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewImportInfo {
    /// The pack's code.
    pub code: String,
    /// Another pack or book already uses the code (only Import as a copy is offered then, unless the pack itself is
    /// installed: Replace).
    pub code_taken: bool,
    /// The code and name Import as a copy gives.
    pub copy_code: String,
    pub copy_name: String,
}

/// A creator pack read from a pack file.
#[derive(Debug, Clone)]
pub(crate) struct CreatorImport {
    manifest: PackManifest,
    content_path: String,
    content: ContentFile,
    creator: CreatorFile,
}

fn damaged(what: &str) -> ApiError {
    ApiError::invalid(format!(
        "This pack file is damaged: its {what} can't be read. Nothing was imported."
    ))
}

fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// macOS archive metadata (`__MACOSX/`, `._name`), never part of the pack.
fn is_junk(name: &str) -> bool {
    name.split('/')
        .any(|part| part == "__MACOSX" || part.starts_with("._"))
}

/// A plain file name (no folder part).
fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != ".."
}

/// The creator pack in `files` when they are one `.zip` holding a `creator.json` beside a `pack.json` (the creator
/// marker); `None` for anything else (the XML import reads it).
///
/// # Errors
/// `invalid_argument` for a damaged archive or a marked pack whose files can't be read.
pub(crate) fn creator_pack_in(files: &[(String, Vec<u8>)]) -> ApiResult<Option<CreatorImport>> {
    let [(name, bytes)] = files else {
        return Ok(None);
    };
    if !name.to_lowercase().ends_with(".zip") {
        return Ok(None);
    }
    let names = entry_names(bytes, &PACK_LIMITS)?;
    let Some(marker) = names
        .iter()
        .filter(|n| !is_junk(n))
        .find(|n| base_name(n) == CREATOR_FILE)
    else {
        return Ok(None);
    };
    let folder = marker[..marker.len() - CREATOR_FILE.len()].to_string();
    let entries: BTreeMap<String, Vec<u8>> = read_zip(bytes, |_| true)?
        .into_iter()
        .filter(|(n, _)| !is_junk(n))
        .filter_map(|(n, b)| Some((n.strip_prefix(&folder)?.to_string(), b)))
        .collect();
    let text = |file: &str| -> ApiResult<String> {
        let bytes = entries.get(file).ok_or_else(|| damaged(file))?;
        String::from_utf8(bytes.clone())
            .map(|t| t.trim_start_matches('\u{feff}').to_string())
            .map_err(|_| damaged(file))
    };
    let manifest =
        PackManifest::from_json_str(&text(MANIFEST_FILE)?).map_err(|_| damaged(MANIFEST_FILE))?;
    let creator = CreatorFile::parse(&text(CREATOR_FILE)?).map_err(|_| damaged(CREATOR_FILE))?;
    let mut contents = manifest
        .files
        .iter()
        .filter(|f| f.kind == PackFileKind::Content);
    let (Some(entry), None) = (contents.next(), contents.next()) else {
        return Err(damaged("content file"));
    };
    let content_path = entry.path.clone();
    if !is_plain_file_name(&content_path) {
        return Err(damaged("content file"));
    }
    let content =
        ContentFile::from_json_str(&text(&content_path)?).map_err(|_| damaged(&content_path))?;
    if slug(&manifest.id) != manifest.id || is_reserved_pack_id(&manifest.id) {
        return Err(ApiError::invalid(
            "This pack file names a pack Borealis can't install. Nothing was imported.",
        ));
    }
    if !is_pack_code(&creator.code) {
        return Err(damaged(CREATOR_FILE));
    }
    Ok(Some(CreatorImport {
        manifest,
        content_path,
        content,
        creator,
    }))
}

/// A zip of `files` (`(name, text)`) inside the folder `folder`.
fn pack_zip(folder: &str, files: &[(String, String)]) -> ApiResult<Vec<u8>> {
    use zip::write::SimpleFileOptions;
    let io = |e: std::io::Error| ApiError::io("write the pack file", e);
    let zipped =
        |e: zip::result::ZipError| ApiError::io("write the pack file", std::io::Error::other(e));
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, text) in files {
        zip.start_file(format!("{folder}/{name}"), options)
            .map_err(zipped)?;
        zip.write_all(text.as_bytes()).map_err(io)?;
    }
    Ok(zip.finish().map_err(zipped)?.into_inner())
}

/// Copy a folder (files and folders, never links) to `to`.
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

impl AppState {
    /// The pack file of a homebrew pack (spec 14.2). With `leave_out_copied_text`, the elements holding text copied
    /// from non-SRD packs export without it (HC8). Drafts and `.trash` never go in (HC7).
    ///
    /// # Errors
    /// `not_found` for an unknown pack, `requirement_failed` for a pack that is not a homebrew pack, `content_error`
    /// when its files can't be read.
    pub fn homebrew_export_file(
        &self,
        pack_id: &str,
        leave_out_copied_text: bool,
    ) -> ApiResult<PackExportFile> {
        let is_homebrew = self
            .list_packs()?
            .iter()
            .any(|p| p.id == pack_id && p.kind == PackKind::Homebrew);
        if !is_homebrew {
            return Err(ApiError::requirement(
                "Only your homebrew packs can be exported as a pack file.",
            ));
        }
        let pack = self.hb_pack(pack_id)?;
        let (content, left_out) = if leave_out_copied_text {
            let (content, changed) = without_copied_text(&pack.disk.content, &pack.creator);
            (content, changed.len())
        } else {
            (pack.disk.content.clone(), 0)
        };
        let files = vec![
            (
                MANIFEST_FILE.to_string(),
                pack.disk.manifest.to_json_pretty()?,
            ),
            (pack.disk.content_path.clone(), content.to_json_pretty()?),
            (CREATOR_FILE.to_string(), pack.creator.to_json_pretty()),
        ];
        let stem = crate::ids::file_stem(&pack.disk.manifest.name);
        Ok(PackExportFile {
            file_name: format!("{stem}{PACK_FILE_SUFFIX}"),
            bytes: pack_zip(pack.id(), &files)?,
            left_out,
        })
    }

    /// A free code for an imported copy of a pack with the code `code`.
    fn hb_free_copy_code(&self, code: &str) -> ApiResult<String> {
        let hub = self.hub()?;
        let mut taken = self.hb_taken_codes(&hub, None);
        taken.insert(code.to_string());
        let library = self.paths.library();
        loop {
            let next = copy_code(code, &taken);
            if !taken.contains(&next) && !library.join(pack_id_for(&next)).exists() {
                return Ok(next);
            }
            if taken.contains(&next) {
                return Err(ApiError::invalid(
                    "Every code for a copy of this pack is taken. Nothing was imported.",
                ));
            }
            taken.insert(next);
        }
    }

    /// Stage a creator pack read from a pack file for the import preview: its files in `out_dir` (with `pack-meta.json`
    /// `kind: homebrew`), checked against the loaded content.
    pub(crate) fn stage_creator_pack(
        &self,
        scratch: Scratch,
        out_dir: PathBuf,
        file_name: String,
        pack: CreatorImport,
    ) -> ApiResult<Staged> {
        write_new_pack(
            &out_dir,
            &PackWrite {
                manifest: pack.manifest.clone(),
                content: pack.content.clone(),
                creator: pack.creator.clone(),
                trashed: Vec::new(),
            },
            &pack.content_path,
        )?;
        let id = pack.manifest.id.clone();
        let hub = self.hub()?;
        let parsed = ParsedPack::from_parts(
            pack.manifest.clone(),
            vec![(pack.content_path.clone(), pack.content.clone())],
        );
        let builder = hub.builder.as_ref().clone();
        let builder = if builder.has_pack(&id) {
            builder
                .replace_pack(parsed)
                .ok_or_else(|| ApiError::internal("a loaded pack could not be replaced"))?
        } else {
            builder.add_parsed_pack(parsed)
        };
        let store = builder.build();
        let ids: HashSet<&str> = pack
            .content
            .elements
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        let elements: Vec<_> = pack
            .content
            .elements
            .iter()
            .filter(|e| e.ty != SOURCE_TYPE)
            .collect();
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for e in &elements {
            *counts.entry(e.ty.clone()).or_default() += 1;
        }
        let conv = Converted {
            converted: elements.len(),
            skipped: 0,
            format: PACK_FILE_FORMAT.to_string(),
            counts: counts.clone(),
            raw: validate_elements(&store, &ids),
            types: pack
                .content
                .elements
                .iter()
                .map(|e| (e.id.clone(), e.ty.clone()))
                .collect(),
            sources: BTreeMap::new(),
        };
        let code_taken = self
            .hb_taken_codes(&hub, Some(&id))
            .contains(&pack.creator.code);
        drop(hub);
        let found = self.findings(&conv, &out_dir)?;
        let report = found.report(&file_name, &conv);
        let truncated = found.diagnostics.len() > MAX_DIAGNOSTICS;
        let mut diagnostics = found.diagnostics;
        diagnostics.truncate(MAX_DIAGNOSTICS);
        let installed = self
            .installed_packs()
            .iter()
            .any(|p| !p.bundled && p.manifest.id == id)
            || self.paths.library().join(slug(&id)).exists();
        let preview = ImportPreview {
            preview_id: format!("prev_{}", new_id()),
            pack_name: pack.manifest.name.clone(),
            pack_id: id.clone(),
            file_count: 1,
            converted: conv.converted,
            skipped: 0,
            counts: counts
                .into_iter()
                .map(|(ty, count)| TypeCount { ty, count })
                .collect(),
            summary: found.summary,
            diagnostics,
            diagnostics_truncated: truncated,
            unresolved: found.unresolved,
            unresolved_total: found.unresolved_total,
            conditions: found.conditions,
            conditions_total: found.conditions_total,
            replaces_existing: installed,
            index_url: None,
            books: Vec::new(),
            homebrew: Some(HomebrewImportInfo {
                code: pack.creator.code.clone(),
                code_taken,
                copy_code: self.hb_free_copy_code(&pack.creator.code)?,
                copy_name: copy_name(&pack.manifest.name),
            }),
        };
        Ok(Staged {
            scratch,
            out_dir,
            id,
            file_name,
            report,
            preview,
        })
    }

    /// Install a staged creator pack: as it is (an installed pack of the same id is copied to the app's trash, then
    /// replaced), or `as_copy` (a new code, id and name; every id given again).
    ///
    /// # Errors
    /// `invalid_argument` when its code is taken and it is not imported as a copy; `content_error` when it can't
    /// load (nothing is installed then); `io_error`.
    pub(crate) fn install_creator_staged(
        &self,
        staged: Staged,
        as_copy: bool,
        sink: &dyn EventSink,
    ) -> ApiResult<ImportPackResult> {
        let info = staged
            .preview
            .homebrew
            .clone()
            .ok_or_else(|| ApiError::internal("not a creator pack"))?;
        if !as_copy && info.code_taken && !staged.preview.replaces_existing {
            return Err(ApiError::invalid(format!(
                "Another pack already uses the code {}. Import it as a copy instead.",
                info.code
            )));
        }
        let (dir, id) = if as_copy {
            let disk = read_pack(&staged.out_dir)?;
            let creator = disk
                .creator
                .clone()
                .ok_or_else(|| ApiError::internal("the staged pack lost its creator.json"))?;
            let code = self.hb_free_copy_code(&creator.code)?;
            let hub = self.hub()?;
            let loaded = hub.store.iter().map(|s| s.element.id.as_str());
            let mut ids = IdGenerator::new(&code, loaded, std::iter::empty::<&str>())
                .map_err(|e| ApiError::invalid(e.to_string()))?;
            let copy = pack_copy(
                &disk.manifest,
                &disk.content,
                &creator,
                &copy_name(&disk.manifest.name),
                &mut ids,
            )
            .map_err(|e| ApiError::internal(format!("copy the pack: {e}")))?;
            drop(hub);
            let dir = staged.scratch.0.join("copy");
            write_new_pack(
                &dir,
                &PackWrite {
                    manifest: copy.manifest.clone(),
                    content: copy.content,
                    creator: copy.creator,
                    trashed: Vec::new(),
                },
                &disk.content_path,
            )?;
            (dir, copy.manifest.id)
        } else {
            let target = self.paths.library().join(slug(&staged.id));
            if target.exists() {
                let trash = borealis_bundle::trash_dir(&self.paths.data).join(format!(
                    "{}-{}",
                    now_ms(),
                    slug(&staged.id)
                ));
                copy_dir(&target, &trash)
                    .map_err(|e| ApiError::io("keep a copy of the replaced pack", e))?;
            }
            (staged.out_dir.clone(), staged.id.clone())
        };
        sink.content_progress(&progress("reloading", 1, 1, "Reloading content"));
        self.install_packs(&[(dir, id.clone())], sink, IMPORTED)?;
        sink.content_progress(&progress("done", 1, 1, "Imported"));
        let pack = self.list_packs()?.into_iter().find(|p| p.id == id);
        Ok(ImportPackResult {
            pack,
            report: staged.report,
        })
    }
}
