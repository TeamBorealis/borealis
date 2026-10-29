//! Source restriction: the user unchecks sources, their elements stop being offered by
//! `select` rules.

use std::collections::BTreeSet;

/// Sources a restriction never excludes. Aurora's source list is built from `Source` elements and
/// skips `internal` and `core` (`SourcesManager.GetUndefinedSourceNames`), so their elements (levels,
/// the ability score options, proficiencies, sizes) stay offered whatever the user unchecks.
pub const UNRESTRICTABLE_SOURCES: [&str; 2] = ["Internal", "Core"];

/// Is `source` one of [`UNRESTRICTABLE_SOURCES`] (ignoring case, as Aurora compares them)?
#[must_use]
pub fn is_unrestrictable_source(source: &str) -> bool {
    UNRESTRICTABLE_SOURCES
        .iter()
        .any(|s| s.eq_ignore_ascii_case(source))
}

/// Element types a restriction never excludes, whatever their source: `SourcesManager`'s element
/// query (`GetUndefinedSourceNames`) leaves them out of every source item, so Aurora offers a
/// PHB ability score option to a feat of another book while the PHB is unchecked (review M1).
pub const UNRESTRICTABLE_TYPES: [&str; 8] = [
    "Source",
    "Internal",
    "Core",
    "Ability Score Improvement",
    "Level",
    "Multiclass",
    "Skill",
    "Support",
];

/// The set of unchecked source names (lower-cased: Aurora matches element sources to its source
/// items ignoring case, `StringComparison.OrdinalIgnoreCase`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceRestriction {
    unchecked: BTreeSet<String>,
}

impl SourceRestriction {
    /// No restriction.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Restrict (exclude) the given source names; [`UNRESTRICTABLE_SOURCES`] are ignored (a
    /// character saved by 1.1.x can list `Internal`).
    pub fn excluding<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            unchecked: names
                .into_iter()
                .map(Into::into)
                .filter(|n: &String| !is_unrestrictable_source(n))
                .map(|n| n.to_lowercase())
                .collect(),
        }
    }

    /// A copy that also excludes `name` (unless it is one of [`UNRESTRICTABLE_SOURCES`]).
    #[must_use]
    pub fn with_excluded(&self, name: impl Into<String>) -> Self {
        let name = name.into();
        let mut next = self.clone();
        if !is_unrestrictable_source(&name) {
            next.unchecked.insert(name.to_lowercase());
        }
        next
    }

    /// True if elements of `source` are filtered out of candidate lists (ignoring case).
    #[must_use]
    pub fn is_restricted(&self, source: &str) -> bool {
        !self.unchecked.is_empty() && self.unchecked.contains(&source.to_lowercase())
    }

    /// True if an element of `source` and type `ty` is filtered out: never for the
    /// [`UNRESTRICTABLE_TYPES`].
    #[must_use]
    pub fn is_restricted_element(&self, source: &str, ty: &str) -> bool {
        !UNRESTRICTABLE_TYPES.contains(&ty) && self.is_restricted(source)
    }

    /// True when nothing is excluded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.unchecked.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auroras_never_restricted_types_and_case_insensitive_sources() {
        // review M1/L5: SourcesManager.GetUndefinedSourceNames leaves these types out of every
        // source item, and matches element sources to source items ignoring case
        let r = SourceRestriction::excluding(["Player’s Handbook"]);
        for ty in [
            "Ability Score Improvement",
            "Level",
            "Multiclass",
            "Skill",
            "Support",
            "Source",
        ] {
            assert!(!r.is_restricted_element("Player’s Handbook", ty), "{ty}");
        }
        assert!(r.is_restricted_element("Player’s Handbook", "Feat"));
        assert!(r.is_restricted("player’s handbook") && r.is_restricted("PLAYER’S HANDBOOK"));
    }

    #[test]
    fn internal_and_core_are_never_restricted() {
        let r = SourceRestriction::excluding(["Internal", "core", "Player’s Handbook"]);
        assert!(!r.is_restricted("Internal") && !r.is_restricted("Core"));
        assert!(r.is_restricted("Player’s Handbook"));
        let r = SourceRestriction::none().with_excluded("Internal");
        assert!(r.is_empty());
    }

    #[test]
    fn restriction_basics() {
        let none = SourceRestriction::none();
        assert!(none.is_empty() && !none.is_restricted("PHB"));
        let r = SourceRestriction::excluding(["Player’s Handbook"]);
        assert!(r.is_restricted("Player’s Handbook"));
        assert!(!r.is_restricted("Player's Handbook"));
        let r2 = r.with_excluded("XGE");
        assert!(r2.is_restricted("XGE") && !r.is_restricted("XGE"));
    }
}
