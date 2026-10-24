//! Server configuration and read-only pack loading.
//!
//! The only filesystem access of the whole crate happens here: reading `pack.json` and the files
//! it lists below the configured content directories, and opening the app's search index
//! **read-only** ([`open_search`]). Nothing here ever creates, writes or deletes a file.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use borealis_content::file::PackManifest;
use borealis_content::{ContentStore, ContentStoreBuilder};
use borealis_search::{SearchEngine, SearchError};

/// Id of the always-loaded base pack.
pub const CORE_PACK: &str = "core";

/// Which campaign's notes the model may read in this run (spec 11, "Session consent per use").
///
/// Fixed by the host in the per-invocation MCP launch arguments; no tool argument can change it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SessionScope {
    /// No session text is reachable: `search_sessions` is not listed and is rejected if called.
    #[default]
    None,
    /// Only this campaign's notes.
    Campaign(String),
}

impl SessionScope {
    /// The granted campaign id, if any.
    #[must_use]
    pub fn campaign(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::Campaign(id) => Some(id),
        }
    }
}

/// What the server may read.
#[derive(Debug, Clone, Default)]
pub struct McpConfig {
    /// Content directories: the bundled pack root and the user library. Each is either a pack
    /// folder itself (has `pack.json`) or a folder of pack folders.
    pub content_dirs: Vec<PathBuf>,
    /// Pack ids to load; `None` loads every pack found. `core` is always loaded.
    pub enabled_packs: Option<BTreeSet<String>>,
    /// The app's `<app-data>/search/v1` folder. Opened read-only and lazily; when it is absent or
    /// unusable the search tools answer from the in-memory content index.
    pub search_dir: Option<PathBuf>,
    /// Session notes granted to this run. Default: none.
    pub session_scope: SessionScope,
}

/// Open the app's search index for reading. `ReloadPolicy::Manual`, no writer is ever created.
///
/// # Errors
/// [`SearchError::NotBuilt`] before the app's first index build, [`SearchError::Stale`] when the
/// app's index was built by other versions, or an I/O / index error.
pub fn open_search(dir: &Path) -> Result<SearchEngine, SearchError> {
    SearchEngine::open_read_only(dir)
}

/// Packs found on disk: `(pack id, directory)`.
fn discover(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut dirs = Vec::new();
    if dir.join("pack.json").is_file() {
        dirs.push(dir.to_path_buf());
    } else if let Ok(read) = std::fs::read_dir(dir) {
        let mut children: Vec<PathBuf> = read
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.join("pack.json").is_file())
            .collect();
        children.sort();
        dirs.extend(children);
    }
    dirs.into_iter()
        .filter_map(|d| {
            let text = std::fs::read_to_string(d.join("pack.json")).ok()?;
            let manifest = PackManifest::from_json_str(&text).ok()?;
            Some((manifest.id, d))
        })
        .collect()
}

/// Load the configured packs. Returns the builder (kept for overlays) and the built store.
///
/// # Errors
/// A message when the core pack is missing or invalid. Other packs that fail to load are skipped
/// and reported on stderr.
pub fn load_base(config: &McpConfig) -> Result<(ContentStoreBuilder, ContentStore), String> {
    let mut found: Vec<(String, PathBuf)> = config
        .content_dirs
        .iter()
        .flat_map(|d| discover(d))
        .filter(|(id, _)| {
            id == CORE_PACK
                || config
                    .enabled_packs
                    .as_ref()
                    .is_none_or(|set| set.contains(id))
        })
        .collect();
    // A later pack must never shadow an earlier one with the same id (bundled dirs come first),
    // so a library pack claiming `core` or an SRD id is ignored.
    let mut seen = std::collections::HashSet::new();
    found.retain(|(id, dir)| {
        let first = seen.insert(id.clone());
        if !first {
            eprintln!(
                "borealis-mcp: ignoring duplicate pack id {id} at {}",
                dir.display()
            );
        }
        first
    });
    // Core first; the rest keep configured order (bundled before user library).
    found.sort_by_key(|(id, _)| id != CORE_PACK);
    if !found.iter().any(|(id, _)| id == CORE_PACK) {
        return Err("the core content pack was not found in the configured content dirs".into());
    }
    let mut builder = ContentStoreBuilder::new();
    for (id, dir) in found {
        match builder.clone().add_pack_dir(&dir) {
            Ok(next) => builder = next,
            Err(e) if id == CORE_PACK => return Err(format!("core pack failed to load: {e}")),
            Err(e) => eprintln!("borealis-mcp: skipping pack {id}: {e}"),
        }
    }
    let store = builder.clone().build();
    Ok((builder, store))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_library_pack_claiming_a_bundled_id_is_ignored() {
        let content = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let base = McpConfig {
            content_dirs: vec![content.clone()],
            enabled_packs: None,
            ..McpConfig::default()
        };
        let (_, baseline) = load_base(&base).expect("baseline");
        let lib = std::env::temp_dir().join(format!("borealis-mcp-shadow-{}", std::process::id()));
        let fake = lib.join("fake");
        std::fs::create_dir_all(&fake).unwrap();
        // The real core manifest points at files this folder lacks: loading it would fail.
        std::fs::copy(content.join("core/pack.json"), fake.join("pack.json")).unwrap();
        let shadowed = McpConfig {
            content_dirs: vec![content, lib.clone()],
            enabled_packs: None,
            ..McpConfig::default()
        };
        let loaded = load_base(&shadowed);
        let _ = std::fs::remove_dir_all(&lib);
        let (_, store) = loaded.expect("the shadow pack must be ignored");
        assert_eq!(store.len(), baseline.len());
    }
}
