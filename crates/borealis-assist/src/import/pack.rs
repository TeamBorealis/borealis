//! Assemble the imported elements into a pack that is clearly marked as a user import.

use std::path::Path;

use borealis_content::file::{
    ContentFile, PackFile, PackFileKind, PackFormat, PackManifest, PackOrigin,
};
use borealis_content::Ruleset;
use serde::{Deserialize, Serialize};

use super::report::{ImportReport, REPORT_FILE};
use crate::error::AssistError;

/// Provenance written next to the manifest (the manifest format itself has no such field).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Always `"user-import"`.
    pub origin: String,
    /// Always false: imported text may be someone else's copyrighted material.
    pub redistributable: bool,
    /// `"text"` or `"pdf"`.
    pub source_kind: String,
    /// Reminder shown to the user.
    pub notice: String,
}

/// A pack ready to be written to disk and loaded as a custom pack.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedPack {
    /// Loadable `pack.json`.
    pub manifest: PackManifest,
    /// Import provenance (`import.json`).
    pub provenance: Provenance,
    /// The converted elements (`content.json`).
    pub content: ContentFile,
    /// The import report (`report.json`), when the pipeline produced one.
    pub report: Option<ImportReport>,
}

/// File name of the content file inside the pack.
pub const CONTENT_FILE: &str = "content.json";

/// Build the pack for `content`.
pub fn assemble(
    pack_id: &str,
    pack_name: &str,
    ruleset: Ruleset,
    source_kind: &str,
    content: ContentFile,
) -> ImportedPack {
    let notice = "Imported from user-supplied text for personal use. Not redistributable; do not publish this pack.";
    ImportedPack {
        manifest: PackManifest {
            format: PackFormat,
            id: pack_id.to_string(),
            name: pack_name.to_string(),
            version: "1.0.0".to_string(),
            origin: PackOrigin::Custom,
            rulesets: vec![ruleset],
            license: None,
            attribution: Some(notice.to_string()),
            description: Some("Converted from imported text; review before use.".to_string()),
            files: vec![PackFile {
                path: CONTENT_FILE.to_string(),
                kind: PackFileKind::Content,
            }],
        },
        provenance: Provenance {
            origin: "user-import".to_string(),
            redistributable: false,
            source_kind: source_kind.to_string(),
            notice: notice.to_string(),
        },
        content,
        report: None,
    }
}

impl ImportedPack {
    /// Write `pack.json`, `content.json`, `import.json` and, when present, `report.json` into
    /// `dir` (created if missing).
    ///
    /// # Errors
    /// [`AssistError::Invalid`] on I/O or serialization failures.
    pub fn write_to_dir(&self, dir: &Path) -> Result<(), AssistError> {
        let io = |e: std::io::Error| AssistError::Invalid(format!("cannot write pack: {e}"));
        let json = |e: borealis_content::ContentError| AssistError::Invalid(e.to_string());
        std::fs::create_dir_all(dir).map_err(io)?;
        std::fs::write(
            dir.join("pack.json"),
            self.manifest.to_json_pretty().map_err(json)?,
        )
        .map_err(io)?;
        std::fs::write(
            dir.join(CONTENT_FILE),
            self.content.to_json_pretty().map_err(json)?,
        )
        .map_err(io)?;
        let prov = serde_json::to_string_pretty(&self.provenance)
            .map_err(|e| AssistError::Invalid(e.to_string()))?;
        std::fs::write(dir.join("import.json"), prov).map_err(io)?;
        if let Some(report) = &self.report {
            let text = serde_json::to_string_pretty(report)
                .map_err(|e| AssistError::Invalid(e.to_string()))?;
            std::fs::write(dir.join(REPORT_FILE), text).map_err(io)?;
        }
        Ok(())
    }
}
