//! The rules edition of authored or imported content.

use borealis_content::Ruleset;
use serde::{Deserialize, Serialize};

/// Rules edition of the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Edition {
    /// 2014 rules.
    #[serde(rename = "2014")]
    Y2014,
    /// 2024 rules.
    #[serde(rename = "2024")]
    Y2024,
}

impl Edition {
    /// The engine ruleset.
    pub fn ruleset(self) -> Ruleset {
        match self {
            Self::Y2014 => Ruleset::Dnd2014,
            Self::Y2024 => Ruleset::Dnd2024,
        }
    }

    /// From an engine ruleset (`Any` counts as 2014).
    pub fn from_ruleset(r: Ruleset) -> Self {
        match r {
            Ruleset::Dnd2024 => Self::Y2024,
            _ => Self::Y2014,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edition_maps_to_and_from_rulesets() {
        assert_eq!(Edition::Y2014.ruleset(), Ruleset::Dnd2014);
        assert_eq!(Edition::Y2024.ruleset(), Ruleset::Dnd2024);
        assert_eq!(Edition::from_ruleset(Ruleset::Any), Edition::Y2014);
        assert_eq!(Edition::from_ruleset(Ruleset::Dnd2024), Edition::Y2024);
        assert_eq!(serde_json::to_value(Edition::Y2024).unwrap(), "2024");
    }
}
