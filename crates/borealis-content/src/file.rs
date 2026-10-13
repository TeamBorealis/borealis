//! Whole-file documents: content files, index files and pack manifests.

use crate::element::{Append, Element};
use crate::error::ContentError;
use crate::extra::Extra;
use crate::info::Info;
use crate::ruleset::Ruleset;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

macro_rules! format_tag {
    ($(#[$m:meta])* $name:ident, $text:literal) => {
        format_tag!($(#[$m])* $name, $text, []);
    };
    ($(#[$m:meta])* $name:ident, $text:literal, [$($legacy:literal),*]) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
        pub struct $name;

        impl $name {
            /// The literal format identifier.
            pub const ID: &'static str = $text;
            /// Older identifiers still accepted on read (never written).
            pub const LEGACY_IDS: &'static [&'static str] = &[$($legacy),*];

            /// Whether `text` is the current identifier or an accepted legacy one.
            pub fn accepts(text: &str) -> bool {
                text == Self::ID || Self::LEGACY_IDS.contains(&text)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(Self::ID)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = String::deserialize(d)?;
                if Self::accepts(&text) {
                    Ok(Self)
                } else {
                    Err(serde::de::Error::custom(format!(
                        "unsupported format {text:?}, expected {:?}", Self::ID
                    )))
                }
            }
        }
    };
}

format_tag!(
    /// The `"borealis-content/1"` marker of a [`ContentFile`]; the pre-rename
    /// `"aurora-content/1"` is still accepted on read.
    ContentFormat,
    "borealis-content/1",
    ["aurora-content/1"]
);
format_tag!(
    /// The `"borealis-index/1"` marker of an [`IndexFile`]; the pre-rename
    /// `"aurora-index/1"` is still accepted on read.
    IndexFormat,
    "borealis-index/1",
    ["aurora-index/1"]
);
format_tag!(
    /// The `"borealis-pack/1"` marker of a [`PackManifest`].
    PackFormat,
    "borealis-pack/1"
);

/// Render a document as pretty JSON with a trailing newline (stable, diff friendly).
fn to_pretty<T: Serialize>(value: &T) -> Result<String, ContentError> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    Ok(text)
}

/// One JSON content file (the JSON form of an Aurora `elements` XML file).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentFile {
    /// Schema identifier.
    pub format: ContentFormat,
    /// Default ruleset for the elements of this file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ruleset: Option<Ruleset>,
    /// File information and update metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info: Option<Info>,
    /// XML root attribute `app`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_app_version: Option<String>,
    /// XML root attribute `ignore`: skip the whole file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore: Option<bool>,
    /// Elements in file order.
    pub elements: Vec<Element>,
    /// Extension records applied after all files are loaded.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub appends: Vec<Append>,
    /// Unknown root attributes and children.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

impl Default for ContentFile {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl ContentFile {
    /// A file holding just these elements.
    #[must_use]
    pub fn new(elements: Vec<Element>) -> Self {
        Self {
            format: ContentFormat,
            ruleset: None,
            info: None,
            min_app_version: None,
            ignore: None,
            elements,
            appends: Vec::new(),
            extra: Extra::default(),
        }
    }

    /// Parse a JSON document.
    ///
    /// # Errors
    /// [`ContentError::Json`] for malformed or non-conforming JSON.
    pub fn from_json_str(text: &str) -> Result<Self, ContentError> {
        Ok(serde_json::from_str(text)?)
    }

    /// Serialize as pretty JSON with a trailing newline.
    ///
    /// # Errors
    /// [`ContentError::Json`] if serialization fails.
    pub fn to_json_pretty(&self) -> Result<String, ContentError> {
        to_pretty(self)
    }

    /// True when the loader must skip this file (`ignore: true`).
    #[must_use]
    pub fn is_ignored(&self) -> bool {
        self.ignore.unwrap_or(false)
    }
}

/// One entry of an index.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexEntry {
    /// File name under the index content directory; a name ending in `index` is a nested index.
    pub name: String,
    /// Remote location. Stored only.
    pub url: String,
    /// XML `<obsolete>` instead of `<file>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obsolete: Option<bool>,
    /// Unknown attributes.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

impl IndexEntry {
    /// True for a nested index (name ends with the text `index`, case-sensitive).
    #[must_use]
    pub fn is_nested_index(&self) -> bool {
        self.name.ends_with("index")
    }

    /// True for a retired file.
    #[must_use]
    pub fn is_obsolete(&self) -> bool {
        self.obsolete.unwrap_or(false)
    }
}

/// The JSON form of an Aurora `.index` manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexFile {
    /// Schema identifier.
    pub format: IndexFormat,
    /// XML root attribute `app`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_app_version: Option<String>,
    /// Name, author and update URL of the index itself.
    pub info: Info,
    /// Listed files.
    pub files: Vec<IndexEntry>,
    /// Unknown root attributes and children.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

impl IndexFile {
    /// Parse a JSON document.
    ///
    /// # Errors
    /// [`ContentError::Json`] for malformed or non-conforming JSON.
    pub fn from_json_str(text: &str) -> Result<Self, ContentError> {
        Ok(serde_json::from_str(text)?)
    }

    /// Serialize as pretty JSON with a trailing newline.
    ///
    /// # Errors
    /// [`ContentError::Json`] if serialization fails.
    pub fn to_json_pretty(&self) -> Result<String, ContentError> {
        to_pretty(self)
    }

    /// Local content directory name: the index file name minus its extension.
    #[must_use]
    pub fn content_dir_name(index_file_name: &str) -> &str {
        index_file_name
            .rsplit_once('.')
            .map_or(index_file_name, |(stem, _)| stem)
    }
}

/// Whether a pack's files replace earlier definitions (custom) or yield to them (core).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackOrigin {
    /// Embedded/system content: a duplicate id keeps the first definition.
    Core,
    /// User content: a duplicate id replaces the earlier definition.
    #[default]
    Custom,
}

/// What a pack manifest file entry contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackFileKind {
    /// A [`ContentFile`].
    Content,
    /// An [`IndexFile`] (metadata only).
    Index,
}

/// One file listed by a manifest, relative to the manifest directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackFile {
    /// Relative path using `/` separators.
    pub path: String,
    /// What the file contains.
    pub kind: PackFileKind,
}

/// `pack.json`: lists a pack's files in load order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackManifest {
    /// Schema identifier.
    pub format: PackFormat,
    /// Stable pack id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Pack version.
    pub version: String,
    /// Core or custom load semantics.
    #[serde(default)]
    pub origin: PackOrigin,
    /// Rulesets the pack targets.
    pub rulesets: Vec<Ruleset>,
    /// License identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Attribution text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    /// Free text description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Files in load order.
    pub files: Vec<PackFile>,
}

impl PackManifest {
    /// Parse a JSON document.
    ///
    /// # Errors
    /// [`ContentError::Json`] for malformed or non-conforming JSON.
    pub fn from_json_str(text: &str) -> Result<Self, ContentError> {
        Ok(serde_json::from_str(text)?)
    }

    /// Serialize as pretty JSON with a trailing newline.
    ///
    /// # Errors
    /// [`ContentError::Json`] if serialization fails.
    pub fn to_json_pretty(&self) -> Result<String, ContentError> {
        to_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_file_round_trip_and_format_check() {
        let file = ContentFile::new(vec![Element::new("ID_A", "A", "Spell", "Core")]);
        let text = file.to_json_pretty().unwrap();
        assert!(text.starts_with("{\n  \"format\": \"borealis-content/1\""));
        assert!(text.ends_with("}\n"));
        assert_eq!(ContentFile::from_json_str(&text).unwrap(), file);
        let bad = text.replace("borealis-content/1", "borealis-content/9");
        assert!(ContentFile::from_json_str(&bad).is_err());
        assert!(!file.is_ignored());
    }

    #[test]
    fn legacy_content_format_id_is_accepted_and_rewritten_as_current() {
        let legacy = r#"{"format":"aurora-content/1","elements":[]}"#;
        let file = ContentFile::from_json_str(legacy).expect("legacy id loads");
        let written = file.to_json_pretty().unwrap();
        assert!(written.contains("\"borealis-content/1\""));
        assert!(!written.contains("aurora-content"));
        assert!(ContentFormat::accepts("aurora-content/1"));
        assert!(!ContentFormat::accepts("aurora-content/2"));
    }

    #[test]
    fn index_format_writes_new_id_and_reads_legacy() {
        let legacy = r#"{"format":"aurora-index/1","info":{"name":"x"},"files":[]}"#;
        let idx: IndexFile = serde_json::from_str(legacy).unwrap();
        let written = serde_json::to_string(&idx).unwrap();
        assert!(written.contains("\"borealis-index/1\""));
        assert!(!written.contains("aurora-index"));
        assert!(IndexFormat::accepts("aurora-index/1"));
        assert!(!IndexFormat::accepts("aurora-index/2"));
    }

    #[test]
    fn index_helpers() {
        assert!(IndexEntry {
            name: "sub.index".into(),
            ..IndexEntry::default()
        }
        .is_nested_index());
        assert!(!IndexEntry {
            name: "reindex.xml".into(),
            ..IndexEntry::default()
        }
        .is_nested_index());
        assert_eq!(IndexFile::content_dir_name("core.index"), "core");
        assert_eq!(IndexFile::content_dir_name("noext"), "noext");
        let entry = IndexEntry {
            obsolete: Some(true),
            ..IndexEntry::default()
        };
        assert!(entry.is_obsolete());
    }

    #[test]
    fn manifest_round_trip() {
        let manifest = PackManifest {
            format: PackFormat,
            id: "srd".into(),
            name: "SRD".into(),
            version: "1.0.0".into(),
            origin: PackOrigin::Core,
            rulesets: vec![Ruleset::Dnd2014],
            license: None,
            attribution: None,
            description: None,
            files: vec![PackFile {
                path: "a.json".into(),
                kind: PackFileKind::Content,
            }],
        };
        let text = manifest.to_json_pretty().unwrap();
        assert_eq!(PackManifest::from_json_str(&text).unwrap(), manifest);
        let index = IndexFile {
            format: IndexFormat,
            min_app_version: None,
            info: Info::default(),
            files: vec![],
            extra: Extra::default(),
        };
        assert_eq!(
            IndexFile::from_json_str(&index.to_json_pretty().unwrap()).unwrap(),
            index
        );
    }
}
