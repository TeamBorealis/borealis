//! Overrides in a homebrew pack (HC20, spec 22): elements of other packs (bundled SRD, imported books) changed in place.
//!
//! An override lives in the pack's overrides file (`borealis_content::OVERRIDES_FILE`, [`overrides_file`]; in memory the
//! content's `overrides` list, never written into `content.json`, so Borealis 1.5.0 still loads the pack and shows the
//! originals), keeps its original's id, type and source, and
//! replaces the original wherever it loads (the engine applies overrides after all other content, spec 22.3).
//! `creator.json` records each one ([`CreatorOverride`]): the original's pack, the original's digest when the override
//! was last saved (change detection, spec 22.6), and for a child override the main element of its edit.
//!
//! Saving an override draft ([`split_override`]): the main element is always an override; a child (a feature edited
//! inside its class) is one only while it differs from its original; a child no other pack has is a new element of the
//! pack. Reverting ([`revert_override`]) removes the override and its parts; nothing is retired. Deterministic, no AI
//! (G7).

use super::creator::{CreatorFile, CreatorOverride};
use borealis_content::{ContentFile, Element, OverridesFile, OverridesFormat};
use std::collections::{BTreeMap, HashSet};

/// What saving an override draft writes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OverrideSplit {
    /// Overrides to write (the main element first).
    pub overrides: Vec<Element>,
    /// New elements of the pack (children the draft added), with the pack's book as their source.
    pub own: Vec<Element>,
    /// Ids of the draft's earlier child overrides that are back to equal their original (to drop).
    pub reverted: Vec<String>,
}

/// Split the elements an override draft writes (main element first, then its children, as the form writes them).
/// `original` gives the original of an id as loaded (the element an override of it replaces), `None` for an id no other
/// pack has. `previous_parts` are the ids the pack overrode with this edit before.
#[must_use]
pub fn split_override<'a>(
    written: Vec<Element>,
    original: impl Fn(&str) -> Option<&'a Element>,
    previous_parts: &HashSet<&str>,
    own_source: &str,
) -> OverrideSplit {
    let mut out = OverrideSplit::default();
    for (i, e) in written.into_iter().enumerate() {
        match original(&e.id) {
            Some(orig) if i > 0 && orig.equals_with_ruleset(&e, orig.ruleset) => {
                if previous_parts.contains(e.id.as_str()) {
                    out.reverted.push(e.id);
                }
            }
            Some(_) => out.overrides.push(e),
            // the main element stays an override while its original is not loaded (its book is off)
            None if i == 0 => out.overrides.push(e),
            None => out.own.push(Element {
                source: own_source.to_string(),
                ..e
            }),
        }
    }
    out
}

/// The content with `overrides` saved: each replaces the override with its id in place, new ones go last.
#[must_use]
pub fn with_overrides(content: &ContentFile, overrides: &[Element]) -> ContentFile {
    let mut next = content.clone();
    for e in overrides {
        match next.overrides.iter_mut().find(|x| x.id == e.id) {
            Some(slot) => *slot = e.clone(),
            None => next.overrides.push(e.clone()),
        }
    }
    next
}

/// The content without the overrides `ids`, and the removed overrides (in file order).
#[must_use]
pub fn without_overrides(
    content: &ContentFile,
    ids: &HashSet<&str>,
) -> (ContentFile, Vec<Element>) {
    let mut next = content.clone();
    let (removed, kept): (Vec<Element>, Vec<Element>) = next
        .overrides
        .into_iter()
        .partition(|e| ids.contains(e.id.as_str()));
    next.overrides = kept;
    (next, removed)
}

/// The override `main` and the child overrides saved with it (`partOf`), the main id first; only ids the content
/// overrides.
#[must_use]
pub fn override_parts(content: &ContentFile, creator: &CreatorFile, main: &str) -> Vec<String> {
    let overridden: HashSet<&str> = content.overrides.iter().map(|e| e.id.as_str()).collect();
    let mut out: Vec<String> = Vec::new();
    if overridden.contains(main) {
        out.push(main.to_string());
    }
    out.extend(
        creator
            .overrides
            .iter()
            .filter(|(id, o)| {
                o.part_of.as_deref() == Some(main) && overridden.contains(id.as_str())
            })
            .map(|(id, _)| id.clone()),
    );
    out
}

/// Revert (spec 22.6): the content and record without the override `main` and its parts, and the removed overrides.
#[must_use]
pub fn revert_override(
    content: &ContentFile,
    creator: &CreatorFile,
    main: &str,
) -> (ContentFile, CreatorFile, Vec<Element>) {
    let parts = override_parts(content, creator, main);
    let ids: HashSet<&str> = parts.iter().map(String::as_str).collect();
    let (next, removed) = without_overrides(content, &ids);
    (
        next,
        creator.without_overrides(ids.iter().copied()),
        removed,
    )
}

/// The original changed since the override was last saved: its digest now differs from the recorded one. An override
/// made outside the creator (no recorded digest) never counts as changed.
#[must_use]
pub fn original_changed(entry: Option<&CreatorOverride>, digest_now: &str) -> bool {
    entry.is_some_and(|o| !o.original_digest.is_empty() && o.original_digest != digest_now)
}

/// The pack's overrides file: the content's overrides and their records; `None` when the pack has no override (the
/// file is then removed).
#[must_use]
pub fn overrides_file(content: &ContentFile, creator: &CreatorFile) -> Option<OverridesFile> {
    if content.overrides.is_empty() {
        return None;
    }
    let records = content
        .overrides
        .iter()
        .filter_map(|e| {
            let record = creator.overrides.get(&e.id)?;
            Some((e.id.clone(), serde_json::to_value(record).ok()?))
        })
        .collect();
    Some(OverridesFile {
        format: OverridesFormat,
        ruleset: content.ruleset,
        overrides: content.overrides.clone(),
        records,
    })
}

/// The override records of an overrides file (a record that can't be read is left out; the creator makes a plain one).
#[must_use]
pub fn override_records(file: &OverridesFile) -> BTreeMap<String, CreatorOverride> {
    file.records
        .iter()
        .filter_map(|(id, v)| Some((id.clone(), serde_json::from_value(v.clone()).ok()?)))
        .collect()
}

/// The content as its content file is written: everything but the overrides, which go to the overrides file.
#[must_use]
pub fn content_on_disk(content: &ContentFile) -> ContentFile {
    ContentFile {
        overrides: Vec::new(),
        ..content.clone()
    }
}

#[cfg(test)]
#[path = "overrides_tests.rs"]
mod tests;
