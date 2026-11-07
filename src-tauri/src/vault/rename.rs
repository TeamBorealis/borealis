//! Renaming a file and the links to it. The links that point at the old name are found in every note (outside code)
//! and only their file part is rewritten, keeping each link's own form: a wikilink stays a wikilink, a Markdown link
//! stays a Markdown link, a bare name stays a bare name, a path keeps its folders, `#heading`, `|alias` and an `.md`
//! the user wrote stay as they are. When the new bare name would point at another file, the link gets the full path.
//! Nothing else in a note changes.

use super::paths;
use super::resolve::{self, FileSet};
use super::syntax;
use super::text;
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use serde::Serialize;
use std::ops::Range;

/// What a Markdown link destination encodes when the old one was encoded (spaces and the characters that end it).
const DEST_ENCODE: &AsciiSet = &CONTROLS.add(b' ').add(b'(').add(b')').add(b'<').add(b'>');

/// One text replacement in a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub replacement: String,
}

/// One changed line, for the confirmation dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineChange {
    /// 1-based.
    pub line: u32,
    pub before: String,
    pub after: String,
}

/// The link updates of one note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteEdits {
    /// The note's path before the rename.
    pub path: String,
    pub edits: Vec<Edit>,
    pub changes: Vec<LineChange>,
}

/// The new file part for a link that wrote `old_target` for `from` (old file) and must now name `to`.
fn new_target(after: &FileSet, note: &str, old_target: &str, to: &str) -> String {
    let new_name = paths::file_name(to);
    let wrote_ext = paths::extension(paths::file_name(old_target))
        .is_some_and(|e| paths::extension(new_name).as_deref() == Some(e.as_str()));
    let name = if wrote_ext {
        new_name
    } else {
        resolve::strip_md(new_name)
    };
    let candidate = match old_target.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{name}"),
        None => name.to_string(),
    };
    if after.resolve(note, &candidate) == Some(to) {
        return candidate;
    }
    // The short form would now mean another file: use the full path.
    if wrote_ext {
        to.to_string()
    } else {
        resolve::strip_md(to).to_string()
    }
}

/// The link edits renaming `from` to `to` needs in the note `note` (path before the rename) with text `text`.
/// `before` and `after` are the vault's files before and after the rename.
#[must_use]
pub fn edits_for(
    before: &FileSet,
    after: &FileSet,
    note: &str,
    note_after: &str,
    text: &str,
    from: &str,
    to: &str,
) -> Vec<Edit> {
    syntax::scan(text)
        .links
        .into_iter()
        .filter(|l| !l.target.is_empty() && before.resolve(note, &l.target) == Some(from))
        .map(|l| {
            let target = new_target(after, note_after, &l.target, to);
            let written = &text[l.target_range.clone()];
            let angled = text[..l.target_range.start].ends_with('<');
            let must_encode = written != l.target || (!angled && target.contains(' '));
            let replacement = if l.markdown && must_encode {
                utf8_percent_encode(&target, DEST_ENCODE).to_string()
            } else {
                target
            };
            Edit {
                range: l.target_range,
                replacement,
            }
        })
        .filter(|e| text[e.range.clone()] != e.replacement)
        .collect()
}

/// Apply non-overlapping `edits` to `text`.
#[must_use]
pub fn apply(text: &str, edits: &[Edit]) -> String {
    let mut sorted: Vec<&Edit> = edits.iter().collect();
    sorted.sort_by_key(|e| e.range.start);
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for e in sorted {
        if e.range.start < at {
            continue;
        }
        out.push_str(&text[at..e.range.start]);
        out.push_str(&e.replacement);
        at = e.range.end;
    }
    out.push_str(&text[at..]);
    out
}

/// The changed lines, before and after, for the dialog.
#[must_use]
pub fn line_changes(text: &str, edits: &[Edit]) -> Vec<LineChange> {
    let after = apply(text, edits);
    let mut lines: Vec<u32> = edits
        .iter()
        .map(|e| text::line_of(text, e.range.start))
        .collect();
    lines.dedup();
    let before_lines: Vec<&str> = text.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    lines
        .into_iter()
        .filter_map(|n| {
            let i = usize::try_from(n).ok()?.checked_sub(1)?;
            Some(LineChange {
                line: n,
                before: before_lines.get(i)?.trim_end_matches('\r').to_string(),
                after: after_lines.get(i)?.trim_end_matches('\r').to_string(),
            })
        })
        .collect()
}

/// The vault's files after renaming `from` to `to`.
#[must_use]
pub fn renamed(files: &FileSet, from: &str, to: &str) -> FileSet {
    FileSet::new(
        files
            .paths()
            .iter()
            .map(|p| if p == from { to.to_string() } else { p.clone() }),
    )
}

#[cfg(test)]
#[path = "rename_tests.rs"]
mod tests;
