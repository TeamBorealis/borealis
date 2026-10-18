//! Rules-based content import with a preview: convert first, show what would be installed, install on
//! confirmation. Inputs are single or several XML/JSON files, `.index` manifests (their listed files that
//! are present are converted with them), a `.zip`, or a whole folder (converted as one set so references
//! between files resolve). No AI is involved. `import_pack` uses the same staging, so installing a preview
//! produces exactly what `import_pack` would have.

use crate::content_dto::{ImportPackRequest, ImportPackResult, MigrationIssue, MigrationReport};
use crate::content_import::{
    import_pack_id, is_reserved_pack_id, slug, stem, write_pack_meta, Scratch, MAX_PACK_BYTES,
};
use crate::content_packs::{progress, EventSink};
use crate::content_report::{name_scratch_file, MAX_DIAGNOSTICS};
use crate::error::{ApiError, ApiResult};
use crate::picked_paths::PickPurpose;
use crate::session::new_id;
use crate::state::AppState;
use crate::zip_read::read_zip;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_content::PackOrigin;
use borealis_migrate::{convert_xml, migrate_path, write_output, Document, MigrateOptions};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Most previews kept while the user decides (the oldest is discarded).
const MAX_STAGED: usize = 3;
/// Scratch copy of a single uploaded XML file.
pub(crate) const SCRATCH_XML: &str = "input.xml";
/// Name used when several loose files have no common name.
const SET_SUFFIX: &str = "more";

/// One uploaded file.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFileInput {
    pub file_name: String,
    pub content_base64: String,
}

/// `preview_import_pack` argument: uploaded files and/or a folder picked with purpose `content-folder`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewImportRequest {
    #[serde(default)]
    pub files: Vec<ImportFileInput>,
    /// `pickPath({ purpose: "content-folder" })` token (desktop).
    #[serde(default)]
    pub folder: Option<String>,
}

/// Converted element count for one element type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TypeCount {
    #[serde(rename = "type")]
    pub ty: String,
    pub count: usize,
}

/// One finding of a preview: the same precise shape as an install report issue (`file` is the
/// path inside the imported set).
pub type PreviewDiagnostic = MigrationIssue;

/// A referenced id that is neither in the imported set nor in the installed content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedRef {
    pub target: String,
    pub count: usize,
    pub referenced_by: Vec<String>,
}

/// Severity totals.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct SeverityCounts {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
}

/// `preview_import_pack` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub preview_id: String,
    pub pack_name: String,
    pub pack_id: String,
    pub file_count: usize,
    pub converted: usize,
    pub skipped: usize,
    pub counts: Vec<TypeCount>,
    pub summary: SeverityCounts,
    pub diagnostics: Vec<PreviewDiagnostic>,
    pub diagnostics_truncated: bool,
    pub unresolved: Vec<UnresolvedRef>,
    pub unresolved_total: usize,
    /// An installed pack with the same id will be replaced.
    pub replaces_existing: bool,
}

/// A file handed to the app by the OS ("open with" / share), waiting for the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedFile {
    pub file_name: String,
    pub content_base64: String,
}

pub(crate) struct Staged {
    /// Keeps the staged folder alive until the preview is installed or dropped.
    _scratch: Scratch,
    out_dir: PathBuf,
    id: String,
    file_name: String,
    report: MigrationReport,
    preview: ImportPreview,
}

static STAGED: Mutex<Vec<(String, Staged)>> = Mutex::new(Vec::new());
static OPENED: Mutex<Vec<OpenedFile>> = Mutex::new(Vec::new());

/// What the importer reads from an archive or folder.
fn is_content_name(lower: &str) -> bool {
    lower.ends_with(".xml") || lower.ends_with(".index")
}

/// The pack id for a set of files: its name as a slug, never a bundled id.
fn set_id(name: &str) -> String {
    let id = slug(name);
    if is_reserved_pack_id(&id) {
        format!("user-{id}")
    } else {
        id
    }
}

fn base_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

enum Inputs {
    Files(Vec<(String, Vec<u8>)>),
    Folder(PathBuf),
}

impl AppState {
    /// Convert without installing; the result stays staged until [`Self::install_import_preview`].
    ///
    /// # Errors
    /// `invalid_argument` for undecodable, empty or oversized input or a bad folder token.
    pub fn preview_import_pack(&self, request: &PreviewImportRequest) -> ApiResult<ImportPreview> {
        let _guard = self.content_lock()?;
        let inputs = match (&request.folder, request.files.is_empty()) {
            (Some(token), true) => {
                let dir = self.picked.resolve(token, PickPurpose::ContentFolder)?;
                if !dir.is_dir() {
                    return Err(ApiError::invalid("That folder does not exist."));
                }
                Inputs::Folder(dir)
            }
            (None, false) => Inputs::Files(decode_files(&request.files)?),
            _ => return Err(ApiError::invalid("Choose files or a folder to import.")),
        };
        let staged = self.stage_import(inputs, None)?;
        let preview = staged.preview.clone();
        let mut all = STAGED.lock()?;
        while all.len() >= MAX_STAGED {
            all.remove(0);
        }
        all.push((preview.preview_id.clone(), staged));
        Ok(preview)
    }

    /// Install a previewed set.
    ///
    /// # Errors
    /// `invalid_argument` when the preview expired or nothing converted; `content_error` if it cannot load.
    pub fn install_import_preview(
        &self,
        preview_id: &str,
        sink: &dyn EventSink,
    ) -> ApiResult<ImportPackResult> {
        let _guard = self.content_lock()?;
        let staged = {
            let mut all = STAGED.lock()?;
            let at = all.iter().position(|(id, _)| id == preview_id);
            at.map(|i| all.remove(i).1)
        }
        .ok_or_else(|| ApiError::invalid("This preview has expired. Choose the files again."))?;
        self.install_staged(staged, sink)
    }

    /// Discard a preview.
    ///
    /// # Errors
    /// `internal` if the lock is poisoned.
    pub fn cancel_import_preview(&self, preview_id: &str) -> ApiResult<()> {
        STAGED.lock()?.retain(|(id, _)| id != preview_id);
        Ok(())
    }

    /// `import_pack`: stage, then install at once.
    pub(crate) fn import_files_now(
        &self,
        request: &ImportPackRequest,
        bytes: Vec<u8>,
        sink: &dyn EventSink,
    ) -> ApiResult<ImportPackResult> {
        let staged = self.stage_import(
            Inputs::Files(vec![(request.file_name.clone(), bytes)]),
            request.format.as_deref(),
        )?;
        if staged.preview.converted == 0 {
            sink.content_progress(&progress("done", 1, 1, "Nothing to import"));
            return Ok(ImportPackResult {
                pack: None,
                report: staged.report,
            });
        }
        self.install_staged(staged, sink)
    }

    fn install_staged(&self, staged: Staged, sink: &dyn EventSink) -> ApiResult<ImportPackResult> {
        if staged.preview.converted == 0 {
            return Err(ApiError::invalid("Nothing could be imported."));
        }
        write_pack_meta(
            &staged.out_dir,
            "imported",
            &serde_json::json!({ "fileName": staged.file_name }),
        );
        sink.content_progress(&progress("reloading", 1, 1, "Reloading content"));
        self.install_packs(
            &[(staged.out_dir.clone(), staged.id.clone())],
            sink,
            "pack-imported",
        )?;
        sink.content_progress(&progress("done", 1, 1, "Imported"));
        let pack = self.list_packs()?.into_iter().find(|p| p.id == staged.id);
        Ok(ImportPackResult {
            pack,
            report: staged.report,
        })
    }

    fn stage_import(&self, inputs: Inputs, format: Option<&str>) -> ApiResult<Staged> {
        let scratch = Scratch::new(&self.paths.data)?;
        let out_dir = scratch.0.join("out");
        let (display, id, file_name, file_count, conv) = match inputs {
            Inputs::Folder(dir) => {
                let name = dir.file_name().map_or_else(
                    || "content".to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
                let id = set_id(&name);
                let conv = convert_set(&dir, &out_dir, &id, &name)?;
                let count = count_files(&dir);
                (name.clone(), id, name, count, conv)
            }
            Inputs::Files(files) => stage_files(&scratch, &out_dir, files, format)?,
        };
        let found = self.findings(&conv, &out_dir)?;
        let report = found.report(&file_name, &conv);
        let truncated = found.diagnostics.len() > MAX_DIAGNOSTICS;
        let mut diagnostics = found.diagnostics;
        diagnostics.truncate(MAX_DIAGNOSTICS);
        let preview = ImportPreview {
            preview_id: format!("prev_{}", new_id()),
            pack_name: display,
            pack_id: id.clone(),
            file_count,
            converted: conv.converted,
            skipped: conv.skipped,
            counts: conv
                .counts
                .clone()
                .into_iter()
                .map(|(ty, count)| TypeCount { ty, count })
                .collect(),
            summary: found.summary,
            diagnostics,
            diagnostics_truncated: truncated,
            unresolved: found.unresolved,
            unresolved_total: found.unresolved_total,
            replaces_existing: self.paths.library().join(&id).exists(),
        };
        Ok(Staged {
            _scratch: scratch,
            out_dir,
            id,
            file_name,
            report,
            preview,
        })
    }
}

/// The outcome of converting one input set.
pub(crate) struct Converted {
    pub(crate) converted: usize,
    pub(crate) skipped: usize,
    pub(crate) format: String,
    counts: BTreeMap<String, usize>,
    pub(crate) raw: Vec<borealis_content::Issue>,
    /// Source element type by id (converted, dropped and renamed elements).
    pub(crate) types: BTreeMap<String, String>,
    /// Source file of each written content file (output path -> input path, both relative).
    pub(crate) sources: BTreeMap<String, String>,
}

fn decode_files(files: &[ImportFileInput]) -> ApiResult<Vec<(String, Vec<u8>)>> {
    let mut total = 0usize;
    files
        .iter()
        .map(|f| {
            let bytes = STANDARD
                .decode(f.content_base64.trim())
                .map_err(|_| ApiError::invalid("The file content is not valid base64."))?;
            total = total.saturating_add(bytes.len());
            if bytes.is_empty() || bytes.len() > MAX_PACK_BYTES || total > MAX_PACK_BYTES * 2 {
                return Err(ApiError::invalid(
                    "That file is empty or too large to import.",
                ));
            }
            Ok((relative_name(&f.file_name), bytes))
        })
        .collect()
}

/// A dropped file's path inside the dropped folder, without empty, `.`, `..` or drive segments
/// (a folder drop keeps its layout so nested indexes and same-named files stay apart).
fn relative_name(name: &str) -> String {
    let parts: Vec<&str> = name
        .split(['/', '\\'])
        .filter(|p| !p.is_empty() && *p != "." && *p != ".." && !p.contains(':'))
        .collect();
    if parts.is_empty() {
        base_name(name).to_string()
    } else {
        parts.join("/")
    }
}

/// The folder every entry sits in, when they were all dropped from one folder.
fn common_folder(entries: &[(String, Vec<u8>)]) -> Option<&str> {
    let first = entries.first()?.0.split_once('/')?.0;
    entries
        .iter()
        .all(|(n, _)| n.split_once('/').is_some_and(|(top, _)| top == first))
        .then_some(first)
}

fn count_files(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter_map(|e| Some((e.path(), e.file_type().ok()?)))
        .filter(|(_, t)| !t.is_symlink())
        .map(|(p, t)| {
            if t.is_dir() {
                count_files(&p)
            } else {
                usize::from(is_content_name(&p.to_string_lossy().to_lowercase()))
            }
        })
        .sum()
}

fn stage_files(
    scratch: &Scratch,
    out_dir: &Path,
    files: Vec<(String, Vec<u8>)>,
    format: Option<&str>,
) -> ApiResult<(String, String, String, usize, Converted)> {
    let first = files
        .first()
        .map(|f| base_name(&f.0).to_string())
        .unwrap_or_default();
    let lower_first = first.to_lowercase();
    if files.len() == 1 && !lower_first.ends_with(".zip") && !lower_first.ends_with(".index") {
        let bytes = &files[0].1;
        let xml = match format {
            Some("xml") => true,
            Some("json") => false,
            _ => {
                lower_first.ends_with(".xml")
                    || bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'<')
            }
        };
        let id = import_pack_id(&first);
        let name = stem(&first).to_string();
        let conv = if xml {
            let input = scratch.0.join(SCRATCH_XML);
            std::fs::write(&input, bytes).map_err(|e| ApiError::io("read the file", e))?;
            let mut conv = convert_set(&input, out_dir, &id, &name)?;
            name_scratch_file(&mut conv.raw, SCRATCH_XML, &first);
            conv
        } else {
            convert_json_single(out_dir, &first, &id, bytes)?
        };
        return Ok((name, id, first, 1, conv));
    }
    let mut entries = Vec::new();
    let mut zip_names = Vec::new();
    for (name, bytes) in files {
        if name.to_lowercase().ends_with(".zip") {
            zip_names.push(name);
            entries.extend(
                read_zip(&bytes, is_content_name)?
                    .into_iter()
                    .filter(|(n, _)| !is_archive_junk(n)),
            );
        } else if is_content_name(&name.to_lowercase()) {
            entries.push((name, bytes));
        }
    }
    if entries.is_empty() {
        return Err(ApiError::invalid(
            "No .xml or .index files were found to import.",
        ));
    }
    let display = set_name(&entries, &zip_names);
    // A zip or dropped folder that keeps its top folder converts exactly like that folder picked.
    let (entries, as_folder) = match common_folder(&entries).map(str::to_string) {
        Some(top) => (strip_folder(entries, &top), true),
        None => (entries, !zip_names.is_empty()),
    };
    let in_dir = scratch.0.join("in");
    lay_out(&in_dir, &entries, !as_folder)?;
    let id = set_id(&display);
    let conv = convert_set(&in_dir, out_dir, &id, &display)?;
    let source = zip_names
        .first()
        .cloned()
        .unwrap_or_else(|| display.clone());
    Ok((display, id, source, entries.len(), conv))
}

/// A display name for a set: the archive, else the index, else the first file and a count.
fn set_name(entries: &[(String, Vec<u8>)], zips: &[String]) -> String {
    if let [zip] = zips {
        return stem(zip).to_string();
    }
    if let Some(folder) = common_folder(entries) {
        return folder.to_string();
    }
    let lower = |n: &str| n.to_lowercase();
    if let Some((n, _)) = entries.iter().find(|(n, _)| lower(n).ends_with(".index")) {
        return stem(base_name(n)).to_string();
    }
    let first = stem(base_name(&entries[0].0)).to_string();
    match entries.len() {
        1 => first,
        n => format!("{first} and {} {SET_SUFFIX}", n - 1),
    }
}

/// macOS archive metadata (`__MACOSX/`, `._name` resource forks), never content.
fn is_archive_junk(name: &str) -> bool {
    name.split('/')
        .any(|part| part == "__MACOSX" || part.starts_with("._"))
}

/// The entries with the leading `top/` removed.
fn strip_folder(entries: Vec<(String, Vec<u8>)>, top: &str) -> Vec<(String, Vec<u8>)> {
    let prefix = format!("{top}/");
    entries
        .into_iter()
        .map(|(name, bytes)| {
            let rel = name
                .strip_prefix(&prefix)
                .map_or(name.clone(), str::to_string);
            (rel, bytes)
        })
        .collect()
}

/// Write the entries under `dir`. With `place_loose`, a loose XML file named by a loose `.index`
/// goes in that index's content folder, so the index finds it exactly as it would on disk (a
/// folder or archive keeps its own layout, as a picked folder does).
fn lay_out(dir: &Path, entries: &[(String, Vec<u8>)], place_loose: bool) -> ApiResult<()> {
    let mut place: BTreeMap<String, String> = BTreeMap::new();
    for (name, bytes) in entries.iter().filter(|_| place_loose) {
        if name.contains('/') || !name.to_lowercase().ends_with(".index") {
            continue;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        if let Some(Document::Index(index)) = convert_xml(text, name).document {
            let folder = borealis_content::IndexFile::content_dir_name(name).to_string();
            for e in &index.files {
                place
                    .entry(e.name.clone())
                    .or_insert_with(|| folder.clone());
            }
        }
    }
    for (name, bytes) in entries {
        let rel = match place.get(name).filter(|_| !name.contains('/')) {
            Some(folder) => format!("{folder}/{name}"),
            None => name.clone(),
        };
        let path = dir.join(&rel);
        if !path.starts_with(dir) || rel.split('/').any(|p| p == "..") {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ApiError::io("prepare an import", e))?;
        }
        std::fs::write(&path, bytes).map_err(|e| ApiError::io("read the files", e))?;
    }
    Ok(())
}

/// Convert a file or a folder as one set (the folder-drop path; Aurora indexes reuse it).
pub(crate) fn convert_set(
    input: &Path,
    out_dir: &Path,
    id: &str,
    name: &str,
) -> ApiResult<Converted> {
    let opts = MigrateOptions {
        pack_id: Some(id.to_string()),
        pack_name: Some(name.to_string()),
        origin: PackOrigin::Custom,
        ..MigrateOptions::default()
    };
    let output = migrate_path(input, &opts)
        .map_err(|e| ApiError::invalid(format!("Cannot convert this content: {e}")))?;
    let totals = &output.report.totals;
    let converted = totals.elements;
    if converted > 0 {
        write_output(out_dir, &output)
            .map_err(|e| ApiError::invalid(format!("Cannot write the converted pack: {e}")))?;
    }
    Ok(Converted {
        converted,
        skipped: totals.dropped_elements + output.report.summary.skipped_files,
        format: "xml".into(),
        counts: totals.elements_by_type.clone(),
        raw: output.report.issues.clone(),
        types: output.report.element_types.clone(),
        sources: output
            .report
            .files
            .iter()
            .filter_map(|f| Some((f.output.clone()?, f.input.clone())))
            .collect(),
    })
}

fn convert_json_single(
    out_dir: &Path,
    file_name: &str,
    id: &str,
    bytes: &[u8],
) -> ApiResult<Converted> {
    let request = ImportPackRequest {
        file_name: file_name.to_string(),
        content_base64: String::new(),
        format: Some("json".into()),
    };
    let report = AppState::convert_json(out_dir, &request, id, bytes)?;
    let mut counts = BTreeMap::new();
    if let Ok(file) = borealis_content::ContentFile::from_json_str(
        std::str::from_utf8(bytes)
            .unwrap_or_default()
            .trim_start_matches('\u{feff}'),
    ) {
        for e in &file.elements {
            *counts.entry(e.ty.clone()).or_default() += 1;
        }
    }
    Ok(Converted {
        converted: report.converted,
        skipped: report.skipped,
        format: report.format,
        counts,
        raw: Vec::new(),
        types: BTreeMap::new(),
        sources: BTreeMap::new(),
    })
}

/// Park a file the OS opened with Borealis until the UI asks for it.
pub fn push_opened_file(path: &Path) {
    let lower = path.to_string_lossy().to_lowercase();
    let known = [".xml", ".index", ".zip", ".dnd5e", ".json"];
    if !known.iter().any(|e| lower.ends_with(e)) {
        return;
    }
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.len() == 0 || meta.len() > MAX_PACK_BYTES as u64 {
        return;
    }
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    let file_name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    if let Ok(mut q) = OPENED.lock() {
        q.push(OpenedFile {
            file_name,
            content_base64: STANDARD.encode(bytes),
        });
    }
}

/// Take the files the OS handed to the app since the last call.
#[must_use]
pub fn take_opened_files() -> Vec<OpenedFile> {
    OPENED
        .lock()
        .map(|mut q| std::mem::take(&mut *q))
        .unwrap_or_default()
}
