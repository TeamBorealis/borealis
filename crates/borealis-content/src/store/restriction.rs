//! Source restriction: the user unchecks sources, their elements stop being offered by
//! `select` rules.

use std::collections::BTreeSet;

/// The set of unchecked source names.
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

    /// Restrict (exclude) the given source names.
    pub fn excluding<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            unchecked: names.into_iter().map(Into::into).collect(),
        }
    }

    /// A copy that also excludes `name`.
    #[must_use]
    pub fn with_excluded(&self, name: impl Into<String>) -> Self {
        let mut next = self.clone();
        next.unchecked.insert(name.into());
        next
    }

    /// True if elements of `source` are filtered out of candidate lists.
    #[must_use]
    pub fn is_restricted(&self, source: &str) -> bool {
        self.unchecked.contains(source)
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
