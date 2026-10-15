//! Conditions an item's own rules put on the item itself ("while you wear it", "while attuned").
//!
//! An item's rules only take part in the graph while it is equipped (and attuned, when it
//! requires attunement and is not a weapon or armor), so such a condition is either redundant
//! or, written loosely (`"equipped": "true"`, `"requirements": "attuned"`, the item's own name),
//! a condition the evaluators read as false: the rule would never apply. Before evaluation an
//! item's self-references are rewritten: an `equipped` condition becomes the exact test of the
//! item itself (`[item:<id>]` / `[attuned:<id>]`), a `requirements` self-reference is dropped
//! (the requirement language has no equipment terms; registration already implies it).

use borealis_content::{Element, Rule};
use serde_json::Value;

/// Element types an inventory line can hold.
pub const ITEM_TYPES: [&str; 4] = ["Magic Item", "Item", "Weapon", "Armor"];

/// Keys and bare words that mean "this item is equipped / worn / held".
const EQUIPPED_WORDS: [&str; 10] = [
    "true", "yes", "1", "equipped", "equip", "worn", "wearing", "wielded", "held", "item",
];
/// Keys and bare words that mean "this item is attuned".
const ATTUNED_WORDS: [&str; 3] = ["attuned", "attunement", "attune"];

/// What a condition says about the item that carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelfRef {
    Equipped,
    Attuned,
}

/// The item's id and name.
#[derive(Clone, Copy)]
struct Me<'a> {
    id: &'a str,
    name: &'a str,
}

/// Does `value` name the item itself (empty, "true", its id or its name)?
fn names_self(value: &str, el: Me<'_>) -> bool {
    let v = value.trim();
    v.is_empty()
        || matches!(v, "true" | "yes" | "self" | "this")
        || v.eq_ignore_ascii_case(el.id)
        || v.eq_ignore_ascii_case(el.name)
}

/// The self-reference `text` makes, if it is one (case and brackets ignored).
fn self_ref(text: &str, el: Me<'_>) -> Option<SelfRef> {
    let t = text.trim().trim_matches(['[', ']']).trim().to_lowercase();
    let t = t.strip_prefix("while ").unwrap_or(&t).trim();
    if t.eq_ignore_ascii_case(el.id) || t.eq_ignore_ascii_case(el.name) {
        return Some(SelfRef::Equipped);
    }
    let (key, value) = t.split_once(':').unwrap_or((t, ""));
    let key = key.trim();
    if !names_self(value, el) {
        return None;
    }
    if ATTUNED_WORDS.contains(&key) {
        Some(SelfRef::Attuned)
    } else if EQUIPPED_WORDS.contains(&key) {
        Some(SelfRef::Equipped)
    } else {
        None
    }
}

/// Does `text` (an `equipped` or `requirements` condition of a rule of the item `id` / `name`)
/// only say that this item is equipped or attuned?
#[must_use]
pub fn is_self_reference(text: &str, id: &str, name: &str) -> bool {
    self_ref(text, Me { id, name }).is_some()
}

/// The exact equipped test of the item itself.
fn exact(r: SelfRef, el: Me<'_>) -> String {
    let id = el.id.to_lowercase();
    match r {
        SelfRef::Equipped => format!("[item:{id}]"),
        SelfRef::Attuned => format!("[attuned:{id}]"),
    }
}

/// `None` when `req` is a self-reference of `el`, else `req` unchanged.
fn without_self(req: Option<&String>, el: Me<'_>) -> Option<String> {
    req.filter(|t| self_ref(t, el).is_none()).cloned()
}

/// Rewrite the self-referencing conditions of an item's rules (see the module docs).
pub fn normalize(el: &mut Element) {
    if !ITEM_TYPES.contains(&el.ty.as_str()) {
        return;
    }
    let me = Me {
        id: &el.id,
        name: &el.name,
    };
    for rule in &mut el.rules {
        match rule {
            Rule::Stat(s) => {
                if let Some(r) = s.equipped.as_deref().and_then(|t| self_ref(t, me)) {
                    s.equipped = Some(exact(r, me));
                }
                s.requirements = without_self(s.requirements.as_ref(), me);
            }
            Rule::Grant(g) => g.requirements = without_self(g.requirements.as_ref(), me),
            Rule::Select(s) => s.requirements = without_self(s.requirements.as_ref(), me),
            Rule::Custom(c) => {
                let own = c
                    .fields
                    .get("requirements")
                    .and_then(Value::as_str)
                    .is_some_and(|t| self_ref(t, me).is_some());
                if own {
                    c.fields.remove("requirements");
                }
            }
            Rule::Unknown(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{self_ref, Me, SelfRef};

    fn item() -> Me<'static> {
        Me {
            id: "ID_T_BAND",
            name: "Test Band",
        }
    }

    #[test]
    fn loose_self_references_are_recognised() {
        let el = item();
        for t in [
            "true",
            "[equipped]",
            "equipped:true",
            "Test Band",
            "ID_T_BAND",
            "[item:ID_T_BAND]",
            "while worn",
        ] {
            assert_eq!(self_ref(t, el), Some(SelfRef::Equipped), "{t}");
        }
        for t in ["attuned", "[attuned]", "[attunement:Test Band]"] {
            assert_eq!(self_ref(t, el), Some(SelfRef::Attuned), "{t}");
        }
    }

    #[test]
    fn real_conditions_are_left_alone() {
        let el = item();
        for t in [
            "[armor:none]",
            "[shield:any]",
            "[item:ID_OTHER]",
            "[attuned:ID_OTHER]",
            "ID_WOTC_CLASS_X",
        ] {
            assert_eq!(self_ref(t, el), None, "{t}");
        }
    }
}
