//! Deterministic element ids for imported entities.
//!
//! Format: `ID_5ET_<SOURCE>_<KIND>_<NAME...>` (upper snake case, ASCII). Two different
//! entities whose sanitised ids collide get a stable 8 hex digit suffix derived from their
//! raw key, so the result never depends on map iteration order.

use crate::util::{fnv1a, id_fragment};
use std::collections::HashMap;

/// What kind of element an id names. Items of every sort share [`IdKind::Item`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdKind {
    /// Race (2014 style).
    Race,
    /// Subrace.
    Subrace,
    /// Species (2024 style).
    Species,
    /// Racial trait split out of a race or species.
    RacialTrait,
    /// Lineage / legacy / ancestry option of a species.
    SpeciesOption,
    /// Class.
    Class,
    /// Synthetic multiclass id of a class.
    Multiclass,
    /// Subclass.
    Subclass,
    /// Class feature.
    ClassFeature,
    /// Subclass feature.
    SubclassFeature,
    /// Background.
    Background,
    /// Background feature split out of a background.
    BackgroundFeature,
    /// Feat.
    Feat,
    /// Spell.
    Spell,
    /// Any item (weapon, armor, gear, magic item).
    Item,
    /// Optional feature (invocation, metamagic, fighting style, ...).
    OptionalFeature,
    /// Language that has no core id.
    Language,
    /// Source (book) descriptor.
    Source,
    /// Weapon mastery property.
    ItemProperty,
    /// Condition, disease or status.
    Condition,
}

impl IdKind {
    /// The token used inside ids.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Race => "RACE",
            Self::Subrace => "SUBRACE",
            Self::Species => "SPECIES",
            Self::RacialTrait => "RACIALTRAIT",
            Self::SpeciesOption => "SPECIESOPT",
            Self::Class => "CLASS",
            Self::Multiclass => "MULTICLASS",
            Self::Subclass => "SUBCLASS",
            Self::ClassFeature => "CLASSFEATURE",
            Self::SubclassFeature => "SUBCLASSFEATURE",
            Self::Background => "BACKGROUND",
            Self::BackgroundFeature => "BGFEATURE",
            Self::Feat => "FEAT",
            Self::Spell => "SPELL",
            Self::Item => "ITEM",
            Self::OptionalFeature => "OPTFEATURE",
            Self::Language => "LANGUAGE",
            Self::Source => "SOURCE",
            Self::ItemProperty => "ITEMPROP",
            Self::Condition => "CONDITION",
        }
    }
}

/// Allocates collision-free ids. Allocation is idempotent per raw key.
#[derive(Debug, Default)]
pub struct IdAlloc {
    by_key: HashMap<String, String>,
    owner: HashMap<String, String>,
}

impl IdAlloc {
    /// An empty allocator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The id for an entity. `raw_key` identifies the entity (any stable text unique to it);
    /// `parts` are the human readable name parts that appear in the id.
    pub fn id(&mut self, kind: IdKind, source: &str, parts: &[&str], raw_key: &str) -> String {
        let full_key = format!(
            "{}|{}|{}",
            kind.token(),
            source.to_lowercase(),
            raw_key.to_lowercase()
        );
        if let Some(existing) = self.by_key.get(&full_key) {
            return existing.clone();
        }
        let mut id = if source.is_empty() {
            format!("ID_5ET_{}", kind.token())
        } else {
            format!("ID_5ET_{}_{}", id_fragment(source), kind.token())
        };
        for part in parts {
            id.push('_');
            id.push_str(&id_fragment(part));
        }
        if let Some(owner) = self.owner.get(&id) {
            if *owner != full_key {
                id = format!("{id}_{:08X}", fnv1a(&full_key) & 0xFFFF_FFFF);
            }
        }
        self.owner.insert(id.clone(), full_key.clone());
        self.by_key.insert(full_key, id.clone());
        id
    }

    /// Convenience: a single-name entity keyed by `(name, source)`.
    pub fn named(&mut self, kind: IdKind, source: &str, name: &str) -> String {
        self.id(kind, source, &[name], name)
    }

    /// Number of ids handed out.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    /// True if no id was allocated yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_follow_the_documented_format() {
        let mut a = IdAlloc::new();
        assert_eq!(
            a.named(IdKind::Spell, "PHB", "Fireball"),
            "ID_5ET_PHB_SPELL_FIREBALL"
        );
        assert_eq!(
            a.id(
                IdKind::ClassFeature,
                "XPHB",
                &["Fighter", "L1", "Second Wind"],
                "k1"
            ),
            "ID_5ET_XPHB_CLASSFEATURE_FIGHTER_L1_SECOND_WIND"
        );
        assert_eq!(
            a.named(IdKind::Item, "DMG", "+1 Longsword"),
            "ID_5ET_DMG_ITEM_PLUS_1_LONGSWORD"
        );
    }

    #[test]
    fn sourceless_ids_omit_the_source_token() {
        let mut a = IdAlloc::new();
        assert_eq!(
            a.id(IdKind::Language, "", &["Gith"], "gith"),
            "ID_5ET_LANGUAGE_GITH"
        );
    }

    #[test]
    fn allocation_is_idempotent_and_collision_safe() {
        let mut a = IdAlloc::new();
        let first = a.id(IdKind::Spell, "X", &["Foo's Bar"], "Foo's Bar");
        let again = a.id(IdKind::Spell, "X", &["Foo's Bar"], "foo's bar");
        assert_eq!(first, again);
        let other = a.id(IdKind::Spell, "X", &["Foos Bar"], "Foos Bar");
        assert_ne!(first, other);
        assert!(other.starts_with("ID_5ET_X_SPELL_FOOS_BAR_"));
        assert_eq!(other.len(), "ID_5ET_X_SPELL_FOOS_BAR_".len() + 8);
        assert_eq!(a.len(), 2);
        assert!(!a.is_empty());
        // deterministic across runs
        let mut b = IdAlloc::new();
        let _ = b.id(IdKind::Spell, "X", &["Foo's Bar"], "Foo's Bar");
        assert_eq!(b.id(IdKind::Spell, "X", &["Foos Bar"], "Foos Bar"), other);
    }

    #[test]
    fn every_kind_has_a_distinct_token() {
        let kinds = [
            IdKind::Race,
            IdKind::Subrace,
            IdKind::Species,
            IdKind::RacialTrait,
            IdKind::SpeciesOption,
            IdKind::Class,
            IdKind::Multiclass,
            IdKind::Subclass,
            IdKind::ClassFeature,
            IdKind::SubclassFeature,
            IdKind::Background,
            IdKind::BackgroundFeature,
            IdKind::Feat,
            IdKind::Spell,
            IdKind::Item,
            IdKind::OptionalFeature,
            IdKind::Language,
            IdKind::Source,
            IdKind::ItemProperty,
            IdKind::Condition,
        ];
        let tokens: std::collections::BTreeSet<_> = kinds.iter().map(|k| k.token()).collect();
        assert_eq!(tokens.len(), kinds.len());
    }
}
