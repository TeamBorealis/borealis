//! Canonical JSON content model for Borealis.
//!
//! * serde types for content files, elements, rules, setters, sheets and spellcasting
//!   ([`ContentFile`], [`Element`], [`Rule`]) with lossless `extra` maps
//! * parsers (not evaluators) for the expression sub-languages ([`expr`])
//! * a [`ContentStore`](store::ContentStore) that loads many packs with the load order,
//!   override and append semantics of the content format, and a validator
//! * the embedded JSON schema ([`schema`])
//!
//! Bundled game content is SRD 5.1 / 5.2 only (CC-BY-4.0).

pub mod element;
pub mod error;
pub mod expr;
pub mod extra;
pub mod file;
pub mod idnorm;
pub mod info;
pub mod issue;
pub mod legacy;
pub mod monster;
pub mod monster_types;
mod monster_validate;
pub mod rule_kinds;
pub mod rules;
pub mod ruleset;
pub mod schema;
pub mod setter;
pub mod sheet;
pub mod spellcasting;
pub mod store;
pub mod types;
pub mod validate;
pub mod version;

use serde::{Deserialize, Serialize};

pub use element::{
    Append, Element, Equipment, Extract, ExtractItem, MapKind, MapsTo, Multiclass, SourceRef,
};
pub use error::ContentError;
pub use extra::{Extra, UnknownNode};
pub use file::{
    ContentFile, ContentFormat, IndexEntry, IndexFile, IndexFormat, PackFile, PackFileKind,
    PackFormat, PackManifest, PackOrigin,
};
pub use info::{Author, Info, Update, UpdateFile};
pub use issue::{count_severity, Issue, IssueCode, Severity};
pub use monster::{MonsterData, MonsterError, MonsterIssue};
pub use monster_types::Cr;
pub use rules::{
    CustomRule, GrantRule, ListItem, Rule, RuleKind, SelectRule, StatRule, UnknownRule,
};
pub use ruleset::Ruleset;
pub use setter::Setter;
pub use sheet::{Compendium, Sheet, SheetEntry};
pub use spellcasting::{Spellcasting, SpellcastingList};
pub use store::{ContentStore, ContentStoreBuilder, SourceRestriction, StoredElement};
pub use types::{KnownType, TypeCategory};
pub use version::Version;

/// Reference to a content element by its stable id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ElementId(pub String);

impl ElementId {
    /// Create an id from any string-like value.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_id_roundtrips_through_json() {
        let id = ElementId::new("ID_SRD_CLASS_FIGHTER");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(serde_json::from_str::<ElementId>(&json).unwrap(), id);
    }
}
