//! `creator.json` (spec 14.3): the creator's own record inside a homebrew pack folder.
//!
//! `{format "borealis-creator/1", code, createdWith, elements: {<id>: {kind, createdAt, copiedFrom?, nonSrdText}},
//! retiredIds}`. It is plain JSON beside `pack.json`: the content loader ignores it, bundles and device sync carry it
//! (it is part of the pack), and a pack without one (an AI-made pack, a pack edited outside the app) gets one the
//! first time the creator opens it ([`CreatorFile::adopt`]).

use crate::form::kind_for_element;
use borealis_content::ContentFile;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The `format` of a `creator.json`.
pub const CREATOR_FORMAT: &str = "borealis-creator/1";
/// The file name inside the pack folder.
pub const CREATOR_FILE: &str = "creator.json";

/// Where a copied element came from (spec 11, CS1). `srd` is true when the original is bundled SRD content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopiedFrom {
    /// The original element's id.
    pub id: String,
    /// The pack that holds the original.
    pub pack_id: String,
    /// The original is bundled SRD content (CC-BY-4.0).
    pub srd: bool,
}

/// What the creator knows about one element of the pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorElement {
    /// The creator form it opens in (`language`, `other`...).
    pub kind: String,
    /// RFC 3339 time of its first save; empty when the element was made outside the creator.
    #[serde(default)]
    pub created_at: String,
    /// Where it was copied from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copied_from: Option<CopiedFrom>,
    /// Its text was copied from a non-SRD pack (export leaves it out by default, HC8; the pack is not
    /// redistributable).
    #[serde(default)]
    pub non_srd_text: bool,
}

/// `creator.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorFile {
    /// [`CREATOR_FORMAT`].
    pub format: String,
    /// The pack code (`<CODE>` of the ids, the Source abbreviation).
    pub code: String,
    /// The app version that first wrote it.
    #[serde(default)]
    pub created_with: String,
    /// The elements the creator knows, by id.
    #[serde(default)]
    pub elements: BTreeMap<String, CreatorElement>,
    /// Ids of deleted elements: never given out again (spec 5.3), so an old character never silently gets a
    /// different element.
    #[serde(default)]
    pub retired_ids: Vec<String>,
}

/// Why a `creator.json` cannot be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreatorFileError {
    /// Not JSON, or not this shape.
    Unreadable(String),
    /// Another `format`.
    UnknownFormat(String),
}

impl std::fmt::Display for CreatorFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable(e) => write!(f, "creator.json can't be read: {e}"),
            Self::UnknownFormat(v) => write!(f, "creator.json has an unknown format {v:?}"),
        }
    }
}

impl std::error::Error for CreatorFileError {}

impl CreatorFile {
    /// A new record for a pack made in the creator.
    #[must_use]
    pub fn new(code: &str, app_version: &str) -> Self {
        Self {
            format: CREATOR_FORMAT.to_string(),
            code: code.to_string(),
            created_with: app_version.to_string(),
            elements: BTreeMap::new(),
            retired_ids: Vec::new(),
        }
    }

    /// Read a `creator.json`.
    ///
    /// # Errors
    /// [`CreatorFileError`] when the text is not a `borealis-creator/1` record.
    pub fn parse(text: &str) -> Result<Self, CreatorFileError> {
        let file: Self =
            serde_json::from_str(text).map_err(|e| CreatorFileError::Unreadable(e.to_string()))?;
        if file.format != CREATOR_FORMAT {
            return Err(CreatorFileError::UnknownFormat(file.format));
        }
        Ok(file)
    }

    /// The record as stored (pretty JSON, keys in a stable order, a final newline).
    #[must_use]
    pub fn to_json_pretty(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).unwrap_or_default();
        text.push('\n');
        text
    }

    /// A record for a pack the creator opens for the first time (or whose record was lost): every element of
    /// `content` with the form it opens in, no provenance, no retired ids.
    #[must_use]
    pub fn adopt(code: &str, app_version: &str, content: &ContentFile) -> Self {
        Self::new(code, app_version).reconciled(content)
    }

    /// The record matched to the pack's content as it is now (the pack may have been edited outside the app): an
    /// entry for every element (new ones with the form they open in), no entry for an element that is gone. Retired
    /// ids stay retired.
    #[must_use]
    pub fn reconciled(&self, content: &ContentFile) -> Self {
        let mut next = self.clone();
        let present: BTreeSet<&str> = content.elements.iter().map(|e| e.id.as_str()).collect();
        next.elements.retain(|id, _| present.contains(id.as_str()));
        for e in content.elements.iter().filter(|e| e.ty != "Source") {
            next.elements
                .entry(e.id.clone())
                .or_insert_with(|| CreatorElement {
                    kind: kind_for_element(e).id.to_string(),
                    created_at: String::new(),
                    copied_from: None,
                    non_srd_text: false,
                });
        }
        next
    }

    /// The record with `id` retired (deleted elements, spec 5.3).
    #[must_use]
    pub fn with_retired<'a>(&self, ids: impl IntoIterator<Item = &'a str>) -> Self {
        let mut next = self.clone();
        for id in ids {
            next.elements.remove(id);
            if !next.retired_ids.iter().any(|r| r == id) {
                next.retired_ids.push(id.to_string());
            }
        }
        next
    }

    /// The record with an entry for a saved element: kept as it was when it exists, else new.
    #[must_use]
    pub fn with_saved(&self, id: &str, entry: CreatorElement) -> Self {
        let mut next = self.clone();
        let kept = next.elements.get(id).cloned();
        next.elements.insert(
            id.to_string(),
            match kept {
                Some(old) => CreatorElement {
                    kind: entry.kind,
                    copied_from: old.copied_from.or(entry.copied_from),
                    non_srd_text: old.non_srd_text || entry.non_srd_text,
                    created_at: if old.created_at.is_empty() {
                        entry.created_at
                    } else {
                        old.created_at
                    },
                },
                None => entry,
            },
        );
        next
    }

    /// The pack holds text copied from non-SRD packs: it is not redistributable (CS1, HC8).
    #[must_use]
    pub fn has_non_srd_text(&self) -> bool {
        self.elements.values().any(|e| e.non_srd_text)
    }
}
