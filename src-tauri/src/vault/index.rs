//! What the notes know about each other: the links and tags of every note, cached per file and re-read only when its
//! size or modification time changes (the time is a hint; the content is what counts). Backlinks, the tag list and
//! search are answered from it.

use super::paths::Vault;
use super::resolve::FileSet;
use super::syntax;
use super::text;
use super::tree::{EntryKind, VaultEntry};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// Largest note read for the index and search (bigger ones are skipped).
pub const MAX_NOTE_BYTES: u64 = 8 * 1024 * 1024;
/// Most search results.
pub const MAX_SEARCH_RESULTS: usize = 100;
/// Most matching lines shown per search result or backlink.
const MAX_LINES_PER_NOTE: usize = 3;
/// Longest line shown in a result, in characters.
const MAX_LINE_CHARS: usize = 160;

/// One link of a note, for backlinks.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LinkFact {
    target: String,
    line: u32,
    line_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NoteFacts {
    len: u64,
    mtime_ms: u64,
    links: Vec<LinkFact>,
    tags: Vec<String>,
}

/// A line of a note, shown under a backlink or a search result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteLine {
    /// 1-based.
    pub line: u32,
    pub text: String,
}

/// A note that links to the open one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backlink {
    pub path: String,
    /// How many links in it point at the open note.
    pub count: usize,
    /// The lines with those links (the first few).
    pub lines: Vec<NoteLine>,
}

/// A tag and how many notes have it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCount {
    /// As first written (without `#`).
    pub tag: String,
    pub count: usize,
}

/// A search result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub path: String,
    /// The note's name matches.
    pub name_match: bool,
    /// Lines whose text matches (the first few).
    pub lines: Vec<NoteLine>,
}

fn short(line: &str) -> String {
    let t = line.trim();
    if t.chars().count() <= MAX_LINE_CHARS {
        t.to_string()
    } else {
        let mut s: String = t.chars().take(MAX_LINE_CHARS).collect();
        s.push('…');
        s
    }
}

/// Read a note's text for the index (`None` when it is too large or unreadable).
fn read_text(real: &Path) -> Option<String> {
    let meta = std::fs::metadata(real).ok()?;
    if meta.len() > MAX_NOTE_BYTES {
        return None;
    }
    let bytes = std::fs::read(real).ok()?;
    Some(text::decode(&bytes).text)
}

fn facts_of(text: &str, len: u64, mtime_ms: u64) -> NoteFacts {
    let scan = syntax::scan(text);
    NoteFacts {
        len,
        mtime_ms,
        links: scan
            .links
            .iter()
            .map(|l| LinkFact {
                target: l.target.clone(),
                line: text::line_of(text, l.range.start),
                line_text: short(text::line_around(text, l.range.start)),
            })
            .collect(),
        tags: scan.tags,
    }
}

/// The per-vault cache.
#[derive(Debug, Default)]
pub struct Index {
    root: Option<PathBuf>,
    notes: HashMap<String, NoteFacts>,
}

impl Index {
    /// Bring the cache up to date with the tree `entries` of `vault`.
    pub fn refresh(&mut self, vault: &Vault, entries: &[VaultEntry]) {
        if self.root.as_deref() != Some(vault.root()) {
            self.root = Some(vault.root().to_path_buf());
            self.notes.clear();
        }
        let notes: Vec<&VaultEntry> = entries
            .iter()
            .filter(|e| e.kind == EntryKind::Note)
            .collect();
        let live: std::collections::HashSet<&str> = notes.iter().map(|e| e.path.as_str()).collect();
        self.notes.retain(|p, _| live.contains(p.as_str()));
        for e in notes {
            let fresh = self
                .notes
                .get(&e.path)
                .is_some_and(|f| f.len == e.size && f.mtime_ms == e.mtime_ms);
            if fresh {
                continue;
            }
            let facts = vault
                .existing_file(&e.path)
                .ok()
                .and_then(|l| read_text(&l.real))
                .map_or_else(
                    || NoteFacts {
                        len: e.size,
                        mtime_ms: e.mtime_ms,
                        links: Vec::new(),
                        tags: Vec::new(),
                    },
                    |t| facts_of(&t, e.size, e.mtime_ms),
                );
            self.notes.insert(e.path.clone(), facts);
        }
    }

    /// Forget a note so the next refresh reads it again (after Borealis wrote it within the same mtime tick).
    pub fn invalidate(&mut self, path: &str) {
        self.notes.remove(path);
    }

    /// Notes linking to `path`, in path order.
    #[must_use]
    pub fn backlinks(&self, files: &FileSet, path: &str) -> Vec<Backlink> {
        let mut out: Vec<Backlink> = self
            .notes
            .iter()
            .filter(|(from, _)| from.as_str() != path)
            .filter_map(|(from, facts)| {
                let hits: Vec<&LinkFact> = facts
                    .links
                    .iter()
                    .filter(|l| files.resolve(from, &l.target) == Some(path))
                    .collect();
                if hits.is_empty() {
                    return None;
                }
                let mut lines: Vec<NoteLine> = Vec::new();
                for h in &hits {
                    if lines.len() < MAX_LINES_PER_NOTE
                        && lines.last().map(|l| l.line) != Some(h.line)
                    {
                        lines.push(NoteLine {
                            line: h.line,
                            text: h.line_text.clone(),
                        });
                    }
                }
                Some(Backlink {
                    path: from.clone(),
                    count: hits.len(),
                    lines,
                })
            })
            .collect();
        out.sort_by_key(|b| b.path.to_lowercase());
        out
    }

    /// Every tag with its note count (tags differing only in case are one), in name order.
    #[must_use]
    pub fn tags(&self) -> Vec<TagCount> {
        let mut by_key: BTreeMap<String, TagCount> = BTreeMap::new();
        let mut paths: Vec<&String> = self.notes.keys().collect();
        paths.sort();
        for p in paths {
            let mut seen = std::collections::HashSet::new();
            for tag in &self.notes[p].tags {
                let key = tag.to_lowercase();
                if !seen.insert(key.clone()) {
                    continue;
                }
                by_key
                    .entry(key)
                    .or_insert_with(|| TagCount {
                        tag: tag.clone(),
                        count: 0,
                    })
                    .count += 1;
            }
        }
        by_key.into_values().collect()
    }

    /// Notes with `tag` or one of its nested tags (`#place` also finds `#place/town`), in path order.
    #[must_use]
    pub fn notes_with_tag(&self, tag: &str) -> Vec<String> {
        let key = tag.trim_start_matches('#').to_lowercase();
        let nested = format!("{key}/");
        let mut out: Vec<String> = self
            .notes
            .iter()
            .filter(|(_, f)| {
                f.tags.iter().any(|t| {
                    let t = t.to_lowercase();
                    t == key || t.starts_with(&nested)
                })
            })
            .map(|(p, _)| p.clone())
            .collect();
        out.sort_by_key(|p| p.to_lowercase());
        out
    }
}

/// Search note names and contents for `query` (case-insensitive), name matches first.
#[must_use]
pub fn search(vault: &Vault, entries: &[VaultEntry], query: &str) -> Vec<SearchHit> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let mut named = Vec::new();
    let mut by_text = Vec::new();
    for e in entries.iter().filter(|e| e.kind == EntryKind::Note) {
        let name_match = super::paths::stem(&e.name).to_lowercase().contains(&q);
        let text = vault
            .existing_file(&e.path)
            .ok()
            .and_then(|l| read_text(&l.real))
            .unwrap_or_default();
        let lines: Vec<NoteLine> = text
            .lines()
            .enumerate()
            .filter(|(_, l)| l.to_lowercase().contains(&q))
            .take(MAX_LINES_PER_NOTE)
            .map(|(i, l)| NoteLine {
                line: u32::try_from(i + 1).unwrap_or(u32::MAX),
                text: short(l),
            })
            .collect();
        if !name_match && lines.is_empty() {
            continue;
        }
        let hit = SearchHit {
            path: e.path.clone(),
            name_match,
            lines,
        };
        if name_match {
            named.push(hit);
        } else {
            by_text.push(hit);
        }
    }
    named.extend(by_text);
    named.truncate(MAX_SEARCH_RESULTS);
    named
}
