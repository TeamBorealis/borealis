//! Content tools: installed packs and the deterministic imports (content index URL, 5etools
//! folder or zip, content file). No AI is involved in any of them (IM1).

use super::{parse, Drive, DriveError, DriveResult};
use crate::content_dto::{ContentPack, FiveToolsImportRequest, ImportPackRequest};
use crate::content_index::HttpsFetcher;
use crate::content_packs::NullSink;
use crate::content_preview::ImportFileInput;
use crate::picked_paths::PickPurpose;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;

/// Issues listed in an import result (the rest are counted).
const MAX_LISTED_ISSUES: usize = 20;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UrlArgs {
    pub url: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathArgs {
    pub path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FiveToolsArgs {
    pub path: String,
    pub sources: Option<Vec<String>>,
    #[serde(default)]
    pub include_fluff: bool,
}

/// Route a content tool; `None` when the name is not one of them.
pub fn dispatch(d: &Drive, name: &str, args: &Value) -> Option<DriveResult<Value>> {
    Some(match name {
        "list_packs" => list_packs(d),
        "import_index" => parse(args).and_then(|a| import_index(d, &a)),
        "scan_5etools" => parse(args).and_then(|a: PathArgs| scan(d, &a.path)),
        "import_5etools" => parse(args).and_then(|a| import_5etools(d, &a)),
        "import_content_file" => parse(args).and_then(|a: PathArgs| import_file(d, &a.path)),
        _ => return None,
    })
}

fn pack(p: &ContentPack) -> Value {
    json!({
        "id": p.id, "name": p.name, "version": p.version, "kind": p.kind,
        "elements": p.element_count, "enabled": p.enabled,
    })
}

pub fn list_packs(d: &Drive) -> DriveResult<Value> {
    let packs: Vec<Value> = d.app().list_packs()?.iter().map(pack).collect();
    Ok(json!({"count": packs.len(), "packs": packs}))
}

pub fn import_index(d: &Drive, a: &UrlArgs) -> DriveResult<Value> {
    d.app().add_index(&a.url)?;
    let r = d.app().update_indexes(&HttpsFetcher, &NullSink)?;
    Ok(json!({
        "updated": r.updated, "failed": r.failed, "reloaded": r.reloaded,
        "changed": r.changed, "newHosts": r.new_hosts,
        "converted": r.reports.iter().map(|x| x.converted).sum::<usize>(),
        "skipped": r.reports.iter().map(|x| x.skipped).sum::<usize>(),
    }))
}

fn read_scoped(d: &Drive, raw: &str) -> DriveResult<(std::path::PathBuf, Vec<u8>)> {
    let path = d.scoped_path(raw)?;
    if !path.is_file() {
        return Err(DriveError::new("not_found", format!("no such file: {raw}")));
    }
    let bytes = std::fs::read(&path)
        .map_err(|e| DriveError::new("io_error", format!("cannot read {raw}: {e}")))?;
    Ok((path, bytes))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A `5etools-folder` token for a data folder, or for a `.zip` unpacked by the app.
fn five_tools_token(d: &Drive, raw: &str) -> DriveResult<String> {
    let path = d.scoped_path(raw)?;
    let is_zip = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    if is_zip {
        let (path, bytes) = read_scoped(d, raw)?;
        let input = ImportFileInput {
            file_name: file_name(&path),
            content_base64: STANDARD.encode(bytes),
        };
        return Ok(d.app().stage_5etools_zip(&input)?.token);
    }
    if !path.is_dir() {
        return Err(DriveError::new(
            "not_found",
            format!("no such folder or .zip: {raw}"),
        ));
    }
    Ok(d.app()
        .picked
        .grant(path, PickPurpose::FivetoolsFolder)?
        .token)
}

pub fn scan(d: &Drive, raw: &str) -> DriveResult<Value> {
    let token = five_tools_token(d, raw)?;
    let found = d.app().scan_5etools_folder(&token)?;
    Ok(json!({"sources": serde_json::to_value(&found.sources).unwrap_or(Value::Null)}))
}

pub fn import_5etools(d: &Drive, a: &FiveToolsArgs) -> DriveResult<Value> {
    let token = five_tools_token(d, &a.path)?;
    let sources = match &a.sources {
        Some(list) => list.clone(),
        None => d
            .app()
            .scan_5etools_folder(&token)?
            .sources
            .into_iter()
            .map(|s| s.abbr)
            .collect(),
    };
    let request = FiveToolsImportRequest {
        folder: token,
        sources,
        include_fluff: a.include_fluff,
    };
    let r = d.app().import_5etools(&request, &NullSink)?;
    Ok(json!({
        "converted": r.converted, "replaced": r.replaced,
        "partial": r.partial.len(), "skipped": r.skipped.len(),
        "packs": r.packs.iter().map(pack).collect::<Vec<_>>(),
    }))
}

pub fn import_file(d: &Drive, raw: &str) -> DriveResult<Value> {
    let (path, bytes) = read_scoped(d, raw)?;
    let request = ImportPackRequest {
        file_name: file_name(&path),
        content_base64: STANDARD.encode(bytes),
        format: None,
    };
    let r = d.app().import_pack(&request, &NullSink)?;
    let issues: Vec<Value> = r
        .report
        .issues
        .iter()
        .take(MAX_LISTED_ISSUES)
        .map(|i| serde_json::to_value(i).unwrap_or(Value::Null))
        .collect();
    Ok(json!({
        "pack": r.pack.as_ref().map(pack),
        "format": r.report.format,
        "converted": r.report.converted,
        "skipped": r.report.skipped,
        "issueCount": r.report.issues.len(),
        "issues": issues,
    }))
}
