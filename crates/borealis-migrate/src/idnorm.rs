//! Deterministic normalisation of element ids that Borealis' id rules reject.
//!
//! Legacy homebrew files contain ids such as `ID_X_(Y)` or `ID_O'BRIEN`, but characters that are
//! operators in the expression languages are forbidden in Borealis ids. Instead of dropping
//! such content, every offending id of one conversion set is mapped to a safe id, the
//! original is kept as the element's `renamed_from` (the store resolves it as an alias, so
//! characters and other content keep working) and every reference in the set is rewritten.

use borealis_content::idnorm::{hashed_safe_id, is_unsafe_id, safe_id};
use borealis_content::{ContentFile, Issue, IssueCode};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Keys whose text is prose and never an id reference.
const PROSE_KEYS: &[&str] = &[
    "name",
    "description",
    "text",
    "prerequisite",
    "renamed_from",
];

/// Map every invalid id in `files` to a safe one. Order independent.
fn build_map(files: &[(String, ContentFile)]) -> BTreeMap<String, String> {
    let mut existing = BTreeSet::new();
    let mut bad = BTreeSet::new();
    for (_, f) in files {
        for e in &f.elements {
            if is_unsafe_id(&e.id) {
                bad.insert(e.id.clone());
            } else {
                existing.insert(e.id.clone());
            }
        }
    }
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for id in bad {
        groups.entry(safe_id(&id)).or_default().push(id);
    }
    let mut map = BTreeMap::new();
    for (cand, ids) in groups {
        let clash = ids.len() > 1 || existing.contains(&cand);
        for id in ids {
            let new = if clash {
                hashed_safe_id(&id)
            } else {
                cand.clone()
            };
            map.insert(id, new);
        }
    }
    map
}

fn is_id_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Replace `old` by `new` in `text` where it is not glued to more id characters.
fn replace_bounded(text: &str, old: &str, new: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find(old) {
        let before = rest[..pos].chars().next_back();
        let after = rest[pos + old.len()..].chars().next();
        let glued_l = before.is_some_and(is_id_char) && old.chars().next().is_some_and(is_id_char);
        let glued_r =
            after.is_some_and(is_id_char) && old.chars().next_back().is_some_and(is_id_char);
        out.push_str(&rest[..pos]);
        out.push_str(if glued_l || glued_r { old } else { new });
        rest = &rest[pos + old.len()..];
    }
    out.push_str(rest);
    out
}

fn rewrite(value: &mut Value, key: Option<&str>, order: &[(&String, &String)]) {
    match value {
        Value::String(s) => {
            if key.is_some_and(|k| PROSE_KEYS.contains(&k)) {
                return;
            }
            if let Some((_, new)) = order.iter().find(|(old, _)| old.as_str() == s.as_str()) {
                *s = (*new).clone();
                return;
            }
            let mut cur = s.clone();
            for (old, new) in order {
                if cur.contains(old.as_str()) {
                    cur = replace_bounded(&cur, old, new);
                }
            }
            *s = cur;
        }
        Value::Array(items) => items.iter_mut().for_each(|v| rewrite(v, key, order)),
        Value::Object(map) => map
            .iter_mut()
            .for_each(|(k, v)| rewrite(v, Some(k.as_str()), order)),
        _ => {}
    }
}

/// Normalise ids across a conversion set. Returns the files that changed (index into
/// `files`) plus one Info issue per renamed element. No-op when every id is valid.
pub(crate) fn normalise(
    files: &mut [(String, ContentFile)],
) -> Result<(Vec<usize>, Vec<Issue>), serde_json::Error> {
    let map = build_map(files);
    if map.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    // Longest originals first so a shorter id never clobbers part of a longer one.
    let mut order: Vec<(&String, &String)> = map.iter().collect();
    order.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(b.0)));
    let mut changed = Vec::new();
    let mut issues = Vec::new();
    for (i, (rel, file)) in files.iter_mut().enumerate() {
        let mut value = serde_json::to_value(&*file)?;
        rewrite(&mut value, None, &order);
        let mut new_file: ContentFile = serde_json::from_value(value)?;
        for (el, old) in new_file.elements.iter_mut().zip(file.elements.iter()) {
            if el.id == old.id {
                continue;
            }
            if el.renamed_from.is_none() {
                el.renamed_from = Some(old.id.clone());
            } else {
                el.extra
                    .attributes
                    .insert("legacy-id".into(), old.id.clone());
            }
            issues.push(
                Issue::info(
                    IssueCode::Normalized,
                    format!("id normalised: {:?} -> {:?}", old.id, el.id),
                )
                .in_file(rel.clone())
                .at_element(el.id.clone()),
            );
        }
        if new_file != *file {
            *file = new_file;
            changed.push(i);
        }
    }
    Ok((changed, issues))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_replace_respects_id_edges() {
        assert_eq!(replace_bounded("[ID_A] ID_AB", "ID_A", "X"), "[X] ID_AB");
        assert_eq!(replace_bounded("ID_A(1)|ID_A(1)", "ID_A(1)", "Y"), "Y|Y");
    }
}
