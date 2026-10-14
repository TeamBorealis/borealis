//! Magic items printed with numbered variants ("+1, +2, or +3", "+1/+2/+3") are one element
//! per variant in the bundled 2014 packs ("Shield, +2"); the 2024 packs keep one element. The
//! detection is asked to list variants already; this splits the folded names it still returns.

use super::types::{DetectedItem, Edition, ImportKind, ItemType};

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
}
