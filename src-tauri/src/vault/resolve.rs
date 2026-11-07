//! Link resolution the way Obsidian does it. A link names a file by its path from the vault folder, by a path relative
//! to the note (`./`, `../`), or by its shortest unique form: the file name alone, or the end of its path. Matching
//! ignores case. A link without an extension means a note (`.md`). When several files fit, the one in the linking
//! note's own folder wins, then the one closest to the vault folder, then the first by path.

use super::paths;
use std::collections::HashMap;

/// The files of a vault, indexed for lookups.
#[derive(Debug, Clone, Default)]
pub struct FileSet {
    paths: Vec<String>,
    /// Lower-case path -> index.
    by_path: HashMap<String, usize>,
    /// Lower-case file name -> indexes.
    by_name: HashMap<String, Vec<usize>>,
}

impl FileSet {
    /// Index vault-relative file paths (no folders).
    #[must_use]
    pub fn new(files: impl IntoIterator<Item = String>) -> Self {
        let mut set = Self::default();
        for path in files {
            let i = set.paths.len();
            set.by_path.insert(path.to_lowercase(), i);
            set.by_name
                .entry(paths::file_name(&path).to_lowercase())
                .or_default()
                .push(i);
            set.paths.push(path);
        }
        set
    }

    /// Every file, in the order given.
    #[must_use]
    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    /// The file at exactly `path` (any case).
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&str> {
        self.by_path
            .get(&path.to_lowercase())
            .map(|&i| self.paths[i].as_str())
    }

    /// Files whose name is `name` (any case).
    #[must_use]
    pub fn named(&self, name: &str) -> Vec<&str> {
        self.by_name
            .get(&name.to_lowercase())
            .map(|ids| ids.iter().map(|&i| self.paths[i].as_str()).collect())
            .unwrap_or_default()
    }

    /// The file a link `target` in the note `from` points at.
    #[must_use]
    pub fn resolve(&self, from: &str, target: &str) -> Option<&str> {
        let target = target.trim().trim_start_matches('/');
        if target.is_empty() {
            return self.get(from);
        }
        let dir = paths::parent(from);
        if target.starts_with("./") || target.starts_with("../") {
            let joined = normalise(&paths::join(dir, target))?;
            return candidates(&joined).find_map(|c| self.get(&c));
        }
        candidates(target).find_map(|c| {
            if c.contains('/') {
                self.get(&c).or_else(|| self.by_suffix(dir, &c))
            } else {
                self.by_suffix(dir, &c)
            }
        })
    }

    /// The best file whose path is `suffix` or ends with `/suffix`.
    fn by_suffix(&self, dir: &str, suffix: &str) -> Option<&str> {
        let lower = suffix.to_lowercase();
        let tail = format!("/{lower}");
        let dir = dir.to_lowercase();
        self.named(paths::file_name(suffix))
            .into_iter()
            .filter(|p| {
                let p = p.to_lowercase();
                p == lower || p.ends_with(&tail)
            })
            .min_by_key(|p| {
                let same_folder = paths::parent(p).to_lowercase() == dir;
                (!same_folder, p.matches('/').count(), p.to_lowercase())
            })
    }

    /// The link text that names `to` from `from`, shortest form: the name alone when no other file has it, else the
    /// full path. Notes lose their `.md`.
    #[must_use]
    pub fn shortest(&self, to: &str) -> String {
        let name = paths::file_name(to);
        let unique = self.named(name).len() <= 1;
        let path = if unique { name } else { to };
        strip_md(path).to_string()
    }
}

/// `x.md` without the `.md` (other extensions stay).
#[must_use]
pub fn strip_md(path: &str) -> &str {
    match path.len().checked_sub(3) {
        Some(at) if path.is_char_boundary(at) && path[at..].eq_ignore_ascii_case(".md") => {
            &path[..at]
        }
        _ => path,
    }
}

/// The paths a link target may mean: the note (`target.md`) first, then the file as named.
fn candidates(target: &str) -> impl Iterator<Item = String> + '_ {
    let note = (!target.to_lowercase().ends_with(".md")).then(|| format!("{target}.md"));
    note.into_iter().chain(std::iter::once(target.to_string()))
}

/// Resolve `.` and `..` in a vault-relative path; `None` when it climbs out of the vault.
fn normalise(path: &str) -> Option<String> {
    let mut out: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop()?;
            }
            p => out.push(p),
        }
    }
    Some(out.join("/"))
}

/// The relative path from the folder `from_dir` to `to` (`../Places/Town`).
#[must_use]
pub fn relative(from_dir: &str, to: &str) -> String {
    let from: Vec<&str> = from_dir.split('/').filter(|p| !p.is_empty()).collect();
    let to_parts: Vec<&str> = to.split('/').collect();
    let common = from
        .iter()
        .zip(&to_parts)
        .take_while(|(a, b)| a == b)
        .count();
    let ups = from.len() - common;
    let rest = to_parts[common..].join("/");
    if ups == 0 {
        rest
    } else {
        format!("{}{rest}", "../".repeat(ups))
    }
}

#[cfg(test)]
#[path = "resolve_tests.rs"]
mod tests;
