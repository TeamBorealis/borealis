//! The compatibility report: which registered elements came from the other edition and how they
//! were adapted (shown on the character's "Rules" tab).

use crate::snapshot::ElementRef;
use borealis_content::Ruleset;
use serde::{Deserialize, Serialize};

/// One foreign element in use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatEntry {
    /// The element.
    pub element: ElementRef,
    /// Edition it was authored for.
    pub from: Ruleset,
    /// The adapter changed it.
    pub converted: bool,
    /// What was changed (or that it is used as written).
    pub notes: Vec<String>,
}

/// Foreign-edition content registered on a character.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityReport {
    /// Strict mode is in effect (new picks and option lists show only same-edition content).
    pub strict: bool,
    /// Registered foreign elements in tree order, one per element id.
    pub entries: Vec<CompatEntry>,
}

/// How an option relates to the character's edition, for the picker badge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditionBadge {
    /// Edition the option was authored for.
    pub from: Ruleset,
    /// The adapter changed it ("converted" tag).
    pub converted: bool,
    /// Adapter notes for the hover text.
    pub notes: Vec<String>,
}
