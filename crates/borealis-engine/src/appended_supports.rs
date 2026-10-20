//! IM11: supports tags that content appends onto a 2014 element the 2024 universe supersedes
//! (`maps_to` kind `equivalent`) also apply to its native 2024 equivalent.
//!
//! A homebrew class spell list is often built from appends onto 2014 PHB spell ids. In a 2024
//! universe those spells are superseded (never offered for new picks), so without this the
//! list's select would offer nothing. Only the tags the appends added count (the store records
//! them per element); the 2014 element's own supports are already modelled natively by its 2024
//! version. The superseded element keeps its tags, so a stored pick of it still replays. Nothing
//! changes in a 2014 universe, and stored character data is untouched (R4).

use crate::ids;
use crate::universe::UElem;
use borealis_content::{ContentStore, MapKind, Ruleset};
use std::collections::HashMap;

/// Add to each native 2024 element the supports that appends put on the superseded elements it
/// is `equivalent` to (same type), plus the names of the `ID_` support tags among them.
pub(crate) fn share(
    elems: &mut [UElem],
    store: &ContentStore,
    ruleset: Ruleset,
    support_names: &HashMap<&str, &str>,
) {
    if ruleset != Ruleset::Dnd2024 {
        return;
    }
    let pos: HashMap<&str, usize> = elems
        .iter()
        .enumerate()
        .map(|(i, u)| (u.el.id.as_str(), i))
        .collect();
    let additions: Vec<(usize, Vec<String>)> = elems
        .iter()
        .enumerate()
        .filter(|(_, u)| u.el.ruleset == Some(ruleset))
        .filter_map(|(i, u)| {
            let tags = appended_onto_superseded(u, elems, &pos, store, support_names);
            (!tags.is_empty()).then_some((i, tags))
        })
        .collect();
    for (i, tags) in additions {
        elems[i].supports.extend(tags);
    }
}

/// The appended tags of the superseded counterparts of native `u` that `u` lacks.
fn appended_onto_superseded(
    u: &UElem,
    elems: &[UElem],
    pos: &HashMap<&str, usize>,
    store: &ContentStore,
    support_names: &HashMap<&str, &str>,
) -> Vec<String> {
    let Some(m) =
        u.el.maps_to
            .as_ref()
            .filter(|m| m.kind == Some(MapKind::Equivalent))
    else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    let counterparts = [&m.dnd2014, &m.dnd2024]
        .into_iter()
        .flatten()
        .filter(|t| **t != u.el.id)
        .filter(|t| {
            pos.get(t.as_str())
                .is_some_and(|&j| elems[j].superseded && elems[j].el.ty == u.el.ty)
        });
    for target in counterparts {
        let Some(stored) = store.get(target) else {
            continue;
        };
        for tag in &stored.appended_supports {
            let name = tag
                .starts_with(ids::SUPPORT_PREFIX)
                .then(|| support_names.get(tag.as_str()))
                .flatten()
                .map(|n| (*n).to_string());
            for t in std::iter::once(tag.clone()).chain(name) {
                if !u.supports.contains(&t) && !out.contains(&t) {
                    out.push(t);
                }
            }
        }
    }
    out
}
