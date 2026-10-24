//! Import options and progress events.

use borealis_content::Ruleset;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Options of [`import_folder`](crate::import_folder).
#[derive(Debug, Clone)]
pub struct ImportOptions {
    /// Only import entries of these source abbreviations (case-insensitive). `None` imports
    /// every source found.
    pub sources: Option<BTreeSet<String>>,
    /// Append fluff (lore) text from the `fluff-*.json` files to the descriptions.
    pub include_fluff: bool,
    /// Only import entries of this ruleset.
    pub ruleset: Option<Ruleset>,
    /// Only import entries flagged as part of the SRD (`srd` / `srd52`). Useful to preview
    /// what could be redistributed; ordinary imports leave this off.
    pub only_srd: bool,
    /// Directories of Borealis SRD packs (`pack.json` trees). When given, imported entities
    /// whose name matches an SRD element get a `maps_to` link to it.
    pub srd_dirs: Vec<PathBuf>,
    /// Version string written to every pack manifest.
    pub pack_version: String,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            sources: None,
            include_fluff: false,
            ruleset: None,
            only_srd: false,
            srd_dirs: Vec::new(),
            pack_version: "1.0.0".to_string(),
        }
    }
}

impl ImportOptions {
    /// Restrict to the given source abbreviations.
    #[must_use]
    pub fn with_sources<I, S>(mut self, sources: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.sources = Some(
            sources
                .into_iter()
                .map(|s| s.as_ref().trim().to_uppercase())
                .collect(),
        );
        self
    }

    /// True if entries of `source` pass the allowlist.
    #[must_use]
    pub fn allows_source(&self, source: &str) -> bool {
        self.sources
            .as_ref()
            .is_none_or(|set| set.contains(&source.to_uppercase()))
    }
}

/// Phase of an import run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Looking at the folder.
    Scanning,
    /// Reading data files.
    Reading,
    /// Resolving `_copy` / `_mod` / `_versions`.
    Resolving,
    /// Converting entities.
    Converting,
    /// Writing packs to disk.
    Writing,
    /// Finished.
    Done,
}

/// A progress notification for the UI.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProgressEvent {
    /// Current phase.
    pub phase: Phase,
    /// Work items done in this phase.
    pub current: usize,
    /// Work items in this phase (0 if unknown).
    pub total: usize,
    /// Short human readable label (a file name, a kind of entity, ...).
    pub message: String,
}

/// Progress callback type.
pub type ProgressFn<'a> = dyn FnMut(&ProgressEvent) + 'a;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_allowlist_is_case_insensitive() {
        let all = ImportOptions::default();
        assert!(all.allows_source("PHB"));
        let some = ImportOptions::default().with_sources(["phb", " xge "]);
        assert!(some.allows_source("PHB"));
        assert!(some.allows_source("xge"));
        assert!(!some.allows_source("TCE"));
        assert_eq!(some.pack_version, "1.0.0");
    }
}
