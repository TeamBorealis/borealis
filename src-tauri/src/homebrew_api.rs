//! `ai_validate_homebrew` and `ai_add_homebrew_to_pack`: check a homebrew element (or content
//! document) without a model, with the homebrew balance flags as warnings, and file it in a user pack.

use crate::content_dto::{ContentPack, PackKind};
use crate::content_import::{slug, write_pack_meta, Scratch};
use crate::content_packs::EventSink;
use crate::error::{ApiError, ApiResult};
use crate::import_check::{check_content, content_doc};
use crate::state::AppState;
use borealis_content::{
    ContentFile, PackFile, PackFileKind, PackFormat, PackManifest, PackOrigin, Ruleset,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

/// Pack that receives homebrew when the caller names none.
pub const DEFAULT_HOMEBREW_PACK: &str = "homebrew";
/// Display name of a pack created here.
const DEFAULT_HOMEBREW_NAME: &str = "My Homebrew";
/// Most elements accepted in one call.
const MAX_ELEMENTS: usize = 200;
const CONTENT_FILE: &str = "content.json";

/// `ValidationReport` of the UI contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationReport {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// An element object or a `{elements: [...]}` document, as a content document.
fn as_doc(json: &Value) -> ApiResult<Value> {
    let Some(obj) = json.as_object() else {
        return Err(ApiError::invalid("Homebrew must be a JSON object."));
    };
    if obj.get("elements").is_some_and(Value::is_array) {
        let mut doc = json.clone();
        if let Some(o) = doc.as_object_mut() {
            o.entry("format")
                .or_insert_with(|| Value::String(borealis_content::file::ContentFormat::ID.into()));
        }
        return Ok(doc);
    }
    Ok(content_doc(std::slice::from_ref(json)))
}

fn ruleset_of(doc: &Value) -> Ruleset {
    let tag = |v: &Value| serde_json::from_value::<Ruleset>(v["ruleset"].clone()).ok();
    tag(doc)
        .or_else(|| {
            doc["elements"]
                .as_array()
                .and_then(|a| a.iter().find_map(tag))
        })
        .filter(|r| *r != Ruleset::Any)
        .unwrap_or(Ruleset::Dnd2024)
}

fn ids_of(doc: &Value) -> Vec<String> {
    doc["elements"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| e["id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

impl AppState {
    /// Validate homebrew against the content schema and the loaded packs, with balance flags as
    /// warnings.
    ///
    /// # Errors
    /// `invalid_argument` when `json` is not an object.
    pub fn ai_validate_homebrew(&self, json: &Value) -> ApiResult<ValidationReport> {
        let doc = as_doc(json)?;
        if ids_of(&doc).len() > MAX_ELEMENTS {
            return Err(ApiError::invalid("Too many elements in one go."));
        }
        let hub = self.hub()?;
        let check = check_content(&hub.builder, ruleset_of(&doc), &doc);
        let mut warnings = check.warnings;
        for id in ids_of(&doc).iter().filter(|id| hub.store.contains_id(id)) {
            warnings.push(format!("{id} already exists in the loaded content."));
        }
        Ok(ValidationReport {
            valid: check.errors.is_empty(),
            errors: check.errors,
            warnings,
        })
    }

    /// File validated homebrew in a user pack (created on first use) and hot-reload.
    ///
    /// # Errors
    /// `invalid_argument` when it does not validate or the target is not a homebrew pack,
    /// `content_error` when the pack fails to load.
    pub fn ai_add_homebrew_to_pack(
        &self,
        json: &Value,
        pack_id: Option<&str>,
        sink: &dyn EventSink,
    ) -> ApiResult<ContentPack> {
        let report = self.ai_validate_homebrew(json)?;
        if !report.valid {
            return Err(ApiError::invalid(format!(
                "This homebrew is not valid: {}",
                report
                    .errors
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }
        let doc = as_doc(json)?;
        let new_file = ContentFile::from_json_str(&doc.to_string())?;
        let ruleset = ruleset_of(&doc);
        let target = slug(pack_id.map_or(DEFAULT_HOMEBREW_PACK, str::trim));
        let _guard = self.content_lock()?;
        let existing = self
            .installed_packs()
            .into_iter()
            .find(|p| !p.bundled && p.manifest.id == target);
        if let Some(p) = &existing {
            let kind = self
                .list_packs()?
                .into_iter()
                .find(|c| c.id == target)
                .map(|c| c.kind);
            if kind != Some(PackKind::Homebrew) {
                return Err(ApiError::invalid(format!(
                    "\"{}\" is not a homebrew pack; pick another pack.",
                    p.manifest.name
                )));
            }
        }
        let (mut manifest, mut content) = match &existing {
            Some(p) => {
                let text = std::fs::read_to_string(p.dir.join(CONTENT_FILE))
                    .map_err(|e| ApiError::io("read the homebrew pack", e))?;
                (p.manifest.clone(), ContentFile::from_json_str(&text)?)
            }
            None => (new_manifest(&target, ruleset), ContentFile::new(Vec::new())),
        };
        let own: HashSet<String> = content.elements.iter().map(|e| e.id.clone()).collect();
        let hub = self.hub()?;
        if let Some(clash) = new_file
            .elements
            .iter()
            .find(|e| hub.store.contains_id(&e.id) && !own.contains(&e.id))
        {
            return Err(ApiError::invalid(format!(
                "{} already exists in other content. Give it a different id.",
                clash.id
            )));
        }
        let replaced: HashSet<&str> = new_file.elements.iter().map(|e| e.id.as_str()).collect();
        content
            .elements
            .retain(|e| !replaced.contains(e.id.as_str()));
        content.elements.extend(new_file.elements.iter().cloned());
        if !manifest.rulesets.contains(&ruleset) {
            manifest.rulesets.push(ruleset);
        }
        let scratch = Scratch::new(&self.paths.data)?;
        let out = scratch.0.join("out");
        stage_pack(
            existing.as_ref().map(|p| p.dir.as_path()),
            &out,
            &manifest,
            &content,
        )?;
        self.install_packs(&[(out, target.clone())], sink, "homebrew-added")?;
        self.list_packs()?
            .into_iter()
            .find(|p| p.id == target)
            .ok_or_else(|| ApiError::internal("the homebrew pack is missing after install"))
    }
}

fn new_manifest(id: &str, ruleset: Ruleset) -> PackManifest {
    let name = if id == DEFAULT_HOMEBREW_PACK {
        DEFAULT_HOMEBREW_NAME
    } else {
        id
    };
    PackManifest {
        format: PackFormat,
        id: id.to_string(),
        name: name.to_string(),
        version: "1.0.0".into(),
        origin: PackOrigin::Custom,
        rulesets: vec![ruleset],
        license: None,
        attribution: None,
        description: Some("Homebrew created in Borealis.".into()),
        files: vec![PackFile {
            path: CONTENT_FILE.into(),
            kind: PackFileKind::Content,
        }],
    }
}

fn stage_pack(
    from: Option<&Path>,
    out: &Path,
    manifest: &PackManifest,
    content: &ContentFile,
) -> ApiResult<()> {
    std::fs::create_dir_all(out).map_err(|e| ApiError::io("prepare the pack", e))?;
    if let Some(dir) = from {
        for entry in std::fs::read_dir(dir)
            .map_err(|e| ApiError::io("read the pack", e))?
            .flatten()
        {
            if entry.path().is_file() {
                std::fs::copy(entry.path(), out.join(entry.file_name()))
                    .map_err(|e| ApiError::io("copy the pack", e))?;
            }
        }
    }
    std::fs::write(out.join("pack.json"), manifest.to_json_pretty()?)
        .map_err(|e| ApiError::io("write the pack", e))?;
    std::fs::write(out.join(CONTENT_FILE), content.to_json_pretty()?)
        .map_err(|e| ApiError::io("write the pack", e))?;
    write_pack_meta(out, "homebrew", &Value::Null);
    Ok(())
}
