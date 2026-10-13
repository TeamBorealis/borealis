//! Ruleset tags (`dnd2014` / `dnd2024`) carried by files and elements.

use serde::{Deserialize, Serialize};

/// Which D&D ruleset a file or element targets.
///
/// `borealis-content` only stores the tag; ruleset behaviour lives in `borealis-engine`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ruleset {
    /// SRD 5.1 semantics (legacy 2014 content).
    Dnd2014,
    /// SRD 5.2.1 semantics.
    Dnd2024,
    /// No rules dependency (a language, a generic item).
    Any,
}

impl Ruleset {
    /// The serialized name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dnd2014 => "dnd2014",
            Self::Dnd2024 => "dnd2024",
            Self::Any => "any",
        }
    }

    /// True if content tagged `self` may be used by a character on `profile`.
    #[must_use]
    pub fn compatible_with(self, profile: Ruleset) -> bool {
        self == Self::Any || profile == Self::Any || self == profile
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_names_and_compat() {
        assert_eq!(
            serde_json::to_string(&Ruleset::Dnd2024).unwrap(),
            "\"dnd2024\""
        );
        assert_eq!(
            serde_json::from_str::<Ruleset>("\"any\"").unwrap(),
            Ruleset::Any
        );
        assert!(Ruleset::Any.compatible_with(Ruleset::Dnd2014));
        assert!(!Ruleset::Dnd2014.compatible_with(Ruleset::Dnd2024));
        assert_eq!(Ruleset::Dnd2014.as_str(), "dnd2014");
    }
}
