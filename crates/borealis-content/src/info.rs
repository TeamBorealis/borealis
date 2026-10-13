//! The `<info>` block of element files and indexes.

use crate::extra::Extra;
use serde::{Deserialize, Serialize};

/// `<author url="..">name</author>`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Author {
    /// Author name.
    pub name: String,
    /// Author URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Unknown attributes.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

/// `<update><file name url/></update>`: where the authoritative copy lives. Stored only;
/// this crate never touches the network.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateFile {
    /// Local file name for the saved copy.
    pub name: String,
    /// Authoritative remote copy.
    pub url: String,
    /// Unknown attributes.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

/// `<update version=".." revised="..">`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Update {
    /// .NET version string, 2 to 4 numeric components.
    pub version: String,
    /// Free text, informational only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revised: Option<String>,
    /// The remote file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<UpdateFile>,
    /// Unknown attributes and children.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

/// The `<info>` block.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Info {
    /// Display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Plain text description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Author.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<Author>,
    /// Update metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update: Option<Update>,
    /// Unknown children.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

impl Info {
    /// The remote URL of the authoritative copy, if declared.
    #[must_use]
    pub fn remote_url(&self) -> Option<&str> {
        self.update
            .as_ref()
            .and_then(|u| u.file.as_ref())
            .map(|f| f.url.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_url() {
        let info = Info {
            update: Some(Update {
                version: "1.0.0".into(),
                file: Some(UpdateFile {
                    name: "a.xml".into(),
                    url: "https://example.invalid/a.xml".into(),
                    extra: Extra::default(),
                }),
                ..Update::default()
            }),
            ..Info::default()
        };
        assert_eq!(info.remote_url(), Some("https://example.invalid/a.xml"));
        assert_eq!(Info::default().remote_url(), None);
    }
}
