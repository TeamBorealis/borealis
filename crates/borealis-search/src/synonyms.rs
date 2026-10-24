//! The versioned synonym / abbreviation table (`data/synonyms.toml`, spec section 5 tweak 6).

use crate::analyzer::plain_words;
use crate::error::{Result, SearchError};
use serde::Deserialize;

const BUNDLED: &str = include_str!("../data/synonyms.toml");

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Group {
    terms: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rename {
    from: String,
    to: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    version: u32,
    #[serde(default)]
    group: Vec<Group>,
    #[serde(default)]
    rename: Vec<Rename>,
}

/// One entry of the table in lookup form: matching `trigger` words in a query adds `expansions`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// Lowercase words that must occur consecutively in the query.
    pub trigger: Vec<String>,
    /// Phrases to search for in addition (plain text; analysed by the caller).
    pub expansions: Vec<String>,
}

/// The parsed table.
#[derive(Debug, Clone)]
pub struct Synonyms {
    version: u32,
    rules: Vec<Rule>,
    groups: Vec<Vec<String>>,
    renames: Vec<(String, Vec<String>)>,
}

impl Synonyms {
    /// The table compiled into the crate.
    ///
    /// # Panics
    /// Never in practice: the bundled table is covered by a unit test.
    #[must_use]
    pub fn bundled() -> Self {
        Self::parse(BUNDLED).expect("bundled synonyms.toml is valid")
    }

    /// Parse a table.
    ///
    /// # Errors
    /// [`SearchError::Synonyms`] for TOML errors, empty terms or groups with fewer than two terms.
    pub fn parse(text: &str) -> Result<Self> {
        let raw: Raw = toml::from_str(text).map_err(|e| SearchError::Synonyms(e.to_string()))?;
        let mut rules = Vec::new();
        for group in &raw.group {
            if group.terms.len() < 2 || group.terms.iter().any(|t| plain_words(t).is_empty()) {
                return Err(SearchError::Synonyms(format!(
                    "group needs two or more non-empty terms: {:?}",
                    group.terms
                )));
            }
            for term in &group.terms {
                rules.push(Rule {
                    trigger: plain_words(term),
                    expansions: group
                        .terms
                        .iter()
                        .filter(|other| *other != term)
                        .cloned()
                        .collect(),
                });
            }
        }
        for rename in &raw.rename {
            if plain_words(&rename.from).is_empty() || rename.to.is_empty() {
                return Err(SearchError::Synonyms(format!(
                    "rename needs from and to: {}",
                    rename.from
                )));
            }
            rules.push(Rule {
                trigger: plain_words(&rename.from),
                expansions: rename.to.clone(),
            });
        }
        Ok(Self {
            version: raw.version,
            rules,
            groups: raw.group.into_iter().map(|g| g.terms).collect(),
            renames: raw.rename.into_iter().map(|r| (r.from, r.to)).collect(),
        })
    }

    /// Table version (goes into the index manifest).
    #[must_use]
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Every rule.
    #[must_use]
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// The equivalence groups as written in the table.
    #[must_use]
    pub fn groups(&self) -> &[Vec<String>] {
        &self.groups
    }

    /// The one-way renames as written in the table: `(from, to...)`.
    #[must_use]
    pub fn renames(&self) -> &[(String, Vec<String>)] {
        &self.renames
    }

    /// Expansion phrases for a query: for each rule whose trigger words occur consecutively in the query, its
    /// expansions, minus phrases the query already contains. Deterministic order, no duplicates.
    #[must_use]
    pub fn expand(&self, query: &str) -> Vec<String> {
        let words = plain_words(query);
        let mut out: Vec<String> = Vec::new();
        for rule in &self.rules {
            if !contains_run(&words, &rule.trigger) {
                continue;
            }
            for expansion in &rule.expansions {
                let exp_words = plain_words(expansion);
                if contains_run(&words, &exp_words) || out.contains(expansion) {
                    continue;
                }
                out.push(expansion.clone());
            }
        }
        out
    }
}

fn contains_run(haystack: &[String], needle: &[String]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_table_parses_and_has_seed_entries() {
        let s = Synonyms::bundled();
        assert!(s.version() >= 1);
        assert_eq!(s.expand("AC"), ["armor class"]);
        assert!(s.expand("OA").contains(&"opportunity attack".to_string()));
        assert_eq!(s.expand("elf race"), ["species"]);
    }

    #[test]
    fn groups_are_bidirectional_renames_are_not() {
        let s = Synonyms::bundled();
        assert_eq!(s.expand("armor class"), ["ac"]);
        assert!(s.expand("species").is_empty());
    }

    #[test]
    fn multi_word_triggers_need_consecutive_words() {
        let s = Synonyms::bundled();
        assert!(s
            .expand("nat 20 damage")
            .contains(&"critical hit".to_string()));
        assert!(s.expand("nat damage 20").is_empty());
    }

    #[test]
    fn malformed_tables_are_rejected() {
        assert!(Synonyms::parse("version = 1\n[[group]]\nterms = [\"x\"]").is_err());
        assert!(Synonyms::parse("version = 1\n[[rename]]\nfrom = \"\"\nto = [\"x\"]").is_err());
        assert!(Synonyms::parse("not toml {").is_err());
    }
}
