//! Watching the vault for changes other apps make (Obsidian, a sync service, a file manager), with the notify crate
//! (FSEvents, inotify, ReadDirectoryChangesW). Events are gathered for a short pause and reported once, as
//! vault-relative paths; hidden paths are dropped, except `.obsidian/app.json`, which is reported as a settings change.
//! Borealis's own temporary files are hidden, so a save reports only the note itself.

use super::config::APP_JSON;
use super::paths::{self, Vault};
use notify::{RecursiveMode, Watcher as _};
use serde::Serialize;
use std::collections::BTreeSet;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

/// How long events are gathered before they are reported.
pub const DEBOUNCE: Duration = Duration::from_millis(300);
/// More changed paths than this are reported as "everything changed".
pub const MAX_PATHS: usize = 200;

/// A batch of changes (`vault:changed` in the UI contract).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultChange {
    /// Changed, created or removed paths, vault-relative, in order.
    pub paths: Vec<String>,
    /// Too many changes to list: re-read everything.
    pub all: bool,
    /// `.obsidian/app.json` changed.
    pub config: bool,
}

/// A running watcher; dropping it stops watching.
pub struct VaultWatcher {
    _inner: notify::RecommendedWatcher,
}

impl std::fmt::Debug for VaultWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VaultWatcher")
    }
}

/// Sort raw event paths into a [`VaultChange`].
fn classify(vault: &Vault, raw: &BTreeSet<std::path::PathBuf>) -> VaultChange {
    let mut out = VaultChange::default();
    for path in raw {
        // A removed file cannot be canonicalised; its folder still can.
        let rel = vault.relative(path).or_else(|| {
            let dir = std::fs::canonicalize(path.parent()?).ok()?;
            let name = path.file_name()?.to_str()?;
            if dir == vault.root() {
                Some(name.to_string())
            } else {
                Some(paths::join(&vault.relative(&dir)?, name))
            }
        });
        let Some(rel) = rel else { continue };
        if rel == APP_JSON {
            out.config = true;
        } else if !rel.split('/').any(paths::is_hidden_name) {
            out.paths.push(rel);
        }
    }
    out.paths.dedup();
    if out.paths.len() > MAX_PATHS {
        out.paths.clear();
        out.all = true;
    }
    out
}

/// Start watching `vault`; `on_change` runs on a background thread for each batch with something to report.
///
/// # Errors
/// When the OS refuses to watch the folder.
pub fn start(
    vault: Vault,
    on_change: impl Fn(VaultChange) + Send + 'static,
) -> notify::Result<VaultWatcher> {
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut inner = notify::recommended_watcher(tx)?;
    inner.watch(vault.root(), RecursiveMode::Recursive)?;
    std::thread::Builder::new()
        .name("vault-watch".into())
        .spawn(move || {
            // Ends when the watcher is dropped (its sender goes away).
            while let Ok(first) = rx.recv() {
                let mut raw = BTreeSet::new();
                let mut add = |ev: notify::Result<notify::Event>| match ev {
                    Ok(ev) => raw.extend(ev.paths),
                    Err(e) => tracing::warn!("vault watch: {e}"),
                };
                add(first);
                loop {
                    match rx.recv_timeout(DEBOUNCE) {
                        Ok(ev) => add(ev),
                        Err(RecvTimeoutError::Timeout) => break,
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
                let change = classify(&vault, &raw);
                if change.all || change.config || !change.paths.is_empty() {
                    on_change(change);
                }
            }
        })
        .map_err(|e| notify::Error::generic(&e.to_string()))?;
    Ok(VaultWatcher { _inner: inner })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::Receiver;

    /// Wait for a batch that mentions `path` (the OS may report earlier, unrelated batches first).
    fn wait_for(rx: &Receiver<VaultChange>, path: &str) -> Option<VaultChange> {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match rx.recv_timeout(left) {
                Ok(c) if c.all || c.paths.iter().any(|p| p == path) => return Some(c),
                Ok(_) => {}
                Err(_) => return None,
            }
        }
        None
    }

    #[test]
    fn changes_made_by_another_app_are_reported_as_vault_paths() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Places")).unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let (tx, rx) = mpsc::channel();
        let watcher = start(vault.clone(), move |c| {
            let _ = tx.send(c);
        })
        .unwrap();
        // FSEvents may need a moment before it reports.
        std::thread::sleep(Duration::from_millis(500));
        std::fs::write(vault.root().join("Places/Harbor.md"), "# Harbor").unwrap();
        let change = wait_for(&rx, "Places/Harbor.md").expect("a change for the new note");
        assert!(!change.config);
        std::fs::remove_file(vault.root().join("Places/Harbor.md")).unwrap();
        assert!(
            wait_for(&rx, "Places/Harbor.md").is_some(),
            "the removal is reported"
        );
        drop(watcher);
    }

    #[test]
    fn hidden_paths_are_dropped_and_the_obsidian_settings_are_flagged() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".obsidian")).unwrap();
        std::fs::write(dir.path().join("Note.md"), "x").unwrap();
        std::fs::write(dir.path().join(".obsidian/app.json"), "{}").unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let root = vault.root().to_path_buf();
        let raw: BTreeSet<_> = [
            root.join("Note.md"),
            root.join(".obsidian/app.json"),
            root.join(".obsidian/workspace.json"),
            root.join(".borealis-x.tmp"),
            root.join("Gone.md"),
        ]
        .into_iter()
        .collect();
        let change = classify(&vault, &raw);
        assert_eq!(change.paths, ["Gone.md", "Note.md"]);
        assert!(change.config);
        assert!(!change.all);
    }

    #[test]
    fn a_flood_of_changes_is_reported_as_everything() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let raw: BTreeSet<_> = (0..=MAX_PATHS)
            .map(|i| vault.root().join(format!("n{i}.md")))
            .collect();
        let change = classify(&vault, &raw);
        assert!(change.all && change.paths.is_empty());
    }
}
