//! Magic items printed with numbered variants ("+1, +2, or +3", "+1/+2/+3") are one element
//! per variant in the bundled 2014 packs ("Shield, +2"); the 2024 packs keep one element. The
//! detection is asked to list variants already; this splits the folded names it still returns.

use super::types::{DetectedItem, Edition, ImportKind, ItemType, PlannedElement, Role};

/// The `+N` bonuses of a folded variant name, with the name before them; `None` unless at
/// least two bonuses are named.
fn folded(name: &str) -> Option<(String, Vec<String>)> {
    let first = name.find('+')?;
    let (base, tail) = name.split_at(first);
    let bonuses: Vec<String> = tail
        .split(|c: char| !(c == '+' || c.is_ascii_digit()))
        .filter(|t| t.len() > 1 && t.starts_with('+') && t[1..].chars().all(|c| c.is_ascii_digit()))
        .map(str::to_string)
        .collect();
    let rest_is_list = tail
        .split(|c: char| c == '+' || c.is_ascii_digit())
        .all(|w| {
            matches!(
                w.trim().trim_matches(['/', ',', '(', ')']).trim(),
                "" | "or" | "and"
            )
        });
    let base = base
        .trim()
        .trim_end_matches([',', '(', ' '])
        .trim()
        .to_string();
    (bonuses.len() >= 2 && rest_is_list && !base.is_empty()).then_some((base, bonuses))
}

/// One detected item per numbered variant of a folded 2014 magic item.
pub(crate) fn split(items: Vec<DetectedItem>, edition: Edition) -> Vec<DetectedItem> {
    if edition != Edition::Y2014 {
        return items;
    }
    items
        .into_iter()
        .flat_map(|item| {
            let magic = item.kind == ImportKind::Item
                && matches!(item.item_type, None | Some(ItemType::MagicItem));
            match folded(&item.name).filter(|_| magic) {
                Some((base, bonuses)) => bonuses
                    .into_iter()
                    .map(|b| DetectedItem {
                        name: format!("{base}, {b}"),
                        ..item.clone()
                    })
                    .collect(),
                None => vec![item],
            }
        })
        .collect()
}

/// The printed name of a variant named "<Item>, <variant>" (used to find its passage).
pub(crate) fn base_name(name: &str) -> Option<&str> {
    name.rsplit_once(", ")
        .map(|(b, _)| b.trim())
        .filter(|b| !b.is_empty())
}

fn is_magic(item: &DetectedItem) -> bool {
    item.kind == ImportKind::Item && matches!(item.item_type, None | Some(ItemType::MagicItem))
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// "+1" / "+2": a numbered variant (the bundled packs keep no base item for those).
fn is_numbered(version: &str) -> bool {
    version
        .trim()
        .strip_prefix('+')
        .is_some_and(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()))
}

/// The base name of a family of two or more per-type magic item variants ("<Item>, <type>",
/// not numbered) that has no item of its own, with the index of its first variant.
fn missing_bases(items: &[DetectedItem]) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    for (i, item) in items.iter().enumerate().filter(|(_, i)| is_magic(i)) {
        let Some((base, version)) = item.name.rsplit_once(", ") else {
            continue;
        };
        if base.trim().is_empty() || is_numbered(version) || out.iter().any(|(_, b)| same(b, base))
        {
            continue;
        }
        let versions = items
            .iter()
            .filter(|o| is_magic(o))
            .filter(|o| o.name.rsplit_once(", ").is_some_and(|(b, _)| same(b, base)))
            .count();
        let has_base = items.iter().any(|o| same(&o.name, base));
        if versions > 1 && !has_base {
            out.push((i, base.trim().to_string()));
        }
    }
    out
}

/// Per-type variants keep the item they are versions of: a base item (the name a character
/// finds and equips, with every version in its text) is added before them. Numbered "+N"
/// variants stay base-less like the bundled 2014 packs.
pub(crate) fn with_bases(items: Vec<DetectedItem>) -> Vec<DetectedItem> {
    let bases = missing_bases(&items);
    let mut out = Vec::with_capacity(items.len() + bases.len());
    for (i, item) in items.into_iter().enumerate() {
        if let Some((_, base)) = bases.iter().find(|(at, _)| *at == i) {
            out.push(DetectedItem {
                name: base.clone(),
                ..item.clone()
            });
        }
        out.push(item);
    }
    out
}

/// Briefs that tie a base item and its versions together (by name).
pub(crate) fn link_briefs(elements: &mut [PlannedElement]) {
    let items: Vec<(String, String)> = elements
        .iter()
        .filter(|e| e.role == Role::Standalone && e.ty == "Magic Item")
        .map(|e| (e.id.clone(), e.name.clone()))
        .collect();
    for (id, name) in &items {
        let versions: Vec<&str> = items
            .iter()
            .filter(|(_, n)| base_name(n).is_some_and(|b| same(b, name)))
            .map(|(_, n)| n.as_str())
            .collect();
        if versions.is_empty() {
            continue;
        }
        let Some(e) = elements.iter_mut().find(|e| &e.id == id) else {
            continue;
        };
        e.brief = format!(
            "{} The item as printed, with EVERY version in its description; its versions ({}) are separate items, so its rules are the ones all versions share.",
            e.brief,
            versions.join(", ")
        )
        .trim()
        .to_string();
        for v in elements
            .iter_mut()
            .filter(|v| v.role == Role::Standalone && versions.contains(&v.name.as_str()))
        {
            v.brief = format!("{} One version of {name}: its own rules.", v.brief)
                .trim()
                .to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str) -> DetectedItem {
        DetectedItem {
            kind: ImportKind::Item,
            name: name.into(),
            parent_hint: None,
            item_type: Some(ItemType::MagicItem),
        }
    }

    #[test]
    fn numbered_variants_become_one_item_each_in_2014_only() {
        let names = |v: Vec<DetectedItem>| v.into_iter().map(|i| i.name).collect::<Vec<_>>();
        let got = split(
            vec![
                item("Lantern of Dusk +1/+2/+3"),
                item("Buckler of Thorns, +1, +2, or +3"),
                item("Cloak of Many Pockets"),
                item("Wand +1 of Sparks"),
            ],
            Edition::Y2014,
        );
        assert_eq!(
            names(got),
            [
                "Lantern of Dusk, +1",
                "Lantern of Dusk, +2",
                "Lantern of Dusk, +3",
                "Buckler of Thorns, +1",
                "Buckler of Thorns, +2",
                "Buckler of Thorns, +3",
                "Cloak of Many Pockets",
                "Wand +1 of Sparks",
            ]
        );
        let kept = split(vec![item("Lantern of Dusk +1/+2/+3")], Edition::Y2024);
        assert_eq!(names(kept), ["Lantern of Dusk +1/+2/+3"]);
        assert_eq!(base_name("Lantern of Dusk, +2"), Some("Lantern of Dusk"));
    }

    #[test]
    fn per_type_variants_keep_a_base_item_and_numbered_ones_do_not() {
        let names = |v: Vec<DetectedItem>| v.into_iter().map(|i| i.name).collect::<Vec<_>>();
        let got = with_bases(vec![
            item("Cloak of Many Pockets"),
            item("Gloves of the Fen, Heron"),
            item("Gloves of the Fen, Newt"),
            item("Lantern of Dusk, +1"),
            item("Lantern of Dusk, +2"),
            item("Charm of Tides"),
            item("Charm of Tides, Pearl"),
            item("Charm of Tides, Coral"),
            item("Rope, Silk"),
        ]);
        assert_eq!(
            names(got),
            [
                "Cloak of Many Pockets",
                "Gloves of the Fen",
                "Gloves of the Fen, Heron",
                "Gloves of the Fen, Newt",
                "Lantern of Dusk, +1",
                "Lantern of Dusk, +2",
                "Charm of Tides",
                "Charm of Tides, Pearl",
                "Charm of Tides, Coral",
                "Rope, Silk",
            ]
        );
    }
}
