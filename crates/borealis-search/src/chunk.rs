//! The unit of indexing: a [`Chunk`], grouped per pack as a [`PackInput`].

use borealis_content::Ruleset;
use sha2::{Digest, Sha256};

/// Edition tag stored in the `ruleset` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EditionTag {
    /// 2014 rules (SRD 5.1).
    Y2014,
    /// 2024 rules (SRD 5.2.1).
    Y2024,
    /// No rules dependency or untagged.
    None,
}

impl EditionTag {
    /// The value indexed in the `ruleset` field.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Y2014 => "2014",
            Self::Y2024 => "2024",
            Self::None => "none",
        }
    }

    /// Parse the stored value; unknown values are `None`.
    #[must_use]
    pub fn from_stored(s: &str) -> Self {
        match s {
            "2014" => Self::Y2014,
            "2024" => Self::Y2024,
            _ => Self::None,
        }
    }
}

impl From<Option<Ruleset>> for EditionTag {
    fn from(r: Option<Ruleset>) -> Self {
        match r {
            Some(Ruleset::Dnd2014) => Self::Y2014,
            Some(Ruleset::Dnd2024) => Self::Y2024,
            Some(Ruleset::Any) | None => Self::None,
        }
    }
}

/// One indexed document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// Stable citation id (`rules:<pack>/<section>[#n]`, `<element id>[#n]`, `session:<entry>#n`).
    pub doc_key: String,
    /// Title or element name.
    pub name: String,
    /// Heading path (`Combat > Actions > Dash`) or element type; analysed and shown.
    pub headings: String,
    /// Plain text.
    pub body: String,
    /// Element type (`Spell`, `Condition`, `Rules`, ...).
    pub ty: String,
    /// Edition tag.
    pub edition: EditionTag,
    /// Pack the chunk belongs to (the unit of incremental updates).
    pub pack: String,
    /// Source name (`SRD 5.2.1`), used for source restrictions.
    pub source: String,
    /// Campaign id (sessions only).
    pub campaign: Option<String>,
    /// True for the first chunk of a split section: only it gets the exact-name and prefix fields, so a name query
    /// does not match every part of a long description with the same boost.
    pub is_primary: bool,
}

impl Chunk {
    /// SHA-256 (hex) over the indexed content; stable across runs.
    #[must_use]
    pub fn content_hash(&self) -> String {
        let mut h = Sha256::new();
        for part in [&self.name, &self.headings, &self.body, &self.ty] {
            h.update(part.as_bytes());
            h.update([0u8]);
        }
        h.update(self.edition.as_str().as_bytes());
        hex(&h.finalize())
    }
}

/// All chunks of one pack plus the fingerprint used to detect changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackInput {
    /// Pack id.
    pub pack: String,
    /// Changes whenever any chunk key or content changes.
    pub fingerprint: String,
    /// The chunks.
    pub chunks: Vec<Chunk>,
}

impl PackInput {
    /// Build a pack input, computing the content fingerprint from the chunks.
    #[must_use]
    pub fn new(pack: impl Into<String>, chunks: Vec<Chunk>) -> Self {
        let pack = pack.into();
        let mut h = Sha256::new();
        h.update(pack.as_bytes());
        for c in &chunks {
            h.update(c.doc_key.as_bytes());
            h.update(c.content_hash().as_bytes());
        }
        Self {
            fingerprint: hex(&h.finalize()),
            pack,
            chunks,
        }
    }
}

/// Lowercase hex of a digest.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn sample(key: &str, body: &str) -> Chunk {
        Chunk {
            doc_key: key.into(),
            name: "N".into(),
            headings: "H".into(),
            body: body.into(),
            ty: "T".into(),
            edition: EditionTag::None,
            pack: "p".into(),
            source: "S".into(),
            campaign: None,
            is_primary: true,
        }
    }

    #[test]
    fn fingerprint_tracks_content() {
        let a = PackInput::new("p", vec![sample("k", "one")]);
        let same = PackInput::new("p", vec![sample("k", "one")]);
        let changed = PackInput::new("p", vec![sample("k", "two")]);
        assert_eq!(a.fingerprint, same.fingerprint);
        assert_ne!(a.fingerprint, changed.fingerprint);
    }

    #[test]
    fn edition_tag_round_trips() {
        for t in [EditionTag::Y2014, EditionTag::Y2024, EditionTag::None] {
            assert_eq!(EditionTag::from_stored(t.as_str()), t);
        }
        assert_eq!(EditionTag::from(Some(Ruleset::Any)), EditionTag::None);
        assert_eq!(EditionTag::from(Some(Ruleset::Dnd2024)), EditionTag::Y2024);
    }
}
