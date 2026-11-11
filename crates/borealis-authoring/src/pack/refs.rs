//! References between elements (spec 10.3 delete scan, 12.3 where used): one scanner for both.
//!
//! An id counts where it stands as a whole token (a run of `A-Z a-z 0-9 _`) in any string or object key, so it is
//! found inside expressions too (`ID_A,ID_B`, `!ID_A`, `ID_A|ID_B`) and never inside a longer id (`ID_AB`). The delete
//! scan asks [`mentions`] of a character file; where-used asks [`use_of`] of every loaded element, which says how an
//! element uses another: it grants it, a choice of it offers it, its prerequisite names it, or it lists it (a spell
//! list, a package, a data reference). Deterministic, no AI (G7).

use borealis_content::expr::{BoolExpr, SupportsExpr, SupportsLeaf};
use borealis_content::{Element, Rule};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;

/// How one element uses another (`HomebrewUseRelation` on the wire).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UseRelation {
    /// A rule gives it (a grant, or a custom rule that names it).
    Grants,
    /// A choice offers it: by its id, or by a tag it carries.
    Selects,
    /// A prerequisite names it.
    Requires,
    /// A spell list holds it, or the element names it anywhere else (a package's contents, `data.owner`).
    Lists,
}

impl UseRelation {
    /// The wire word (`grants`, `selects`, `requires`, `lists`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Grants => "grants",
            Self::Selects => "selects",
            Self::Requires => "requires",
            Self::Lists => "lists",
        }
    }
}

const fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The whole tokens of `s` (runs of `A-Z a-z 0-9 _`).
pub fn id_tokens(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| !is_word(c)).filter(|t| !t.is_empty())
}

/// True when `s` holds one of `ids` as a whole token.
#[must_use]
pub fn names_any(s: &str, ids: &HashSet<&str>) -> bool {
    id_tokens(s).any(|t| ids.contains(t))
}

/// True when any string or object key anywhere in `v` holds one of `ids` as a whole token (the delete scan of a
/// character file, spec 10.3).
#[must_use]
pub fn mentions(v: &Value, ids: &HashSet<&str>) -> bool {
    match v {
        Value::String(s) => names_any(s, ids),
        Value::Array(a) => a.iter().any(|x| mentions(x, ids)),
        Value::Object(o) => o.iter().any(|(k, x)| names_any(k, ids) || mentions(x, ids)),
        _ => false,
    }
}

/// Two support tags are the same, a typographic apostrophe counting as a straight one (the engine's rule, IM15).
pub(crate) fn same_tag(a: &str, b: &str) -> bool {
    let fold = |c: char| if c == '\u{2019}' { '\'' } else { c };
    a == b || a.chars().map(fold).eq(b.chars().map(fold))
}

fn eval(expr: &BoolExpr<SupportsLeaf>, leaf: &dyn Fn(&SupportsLeaf) -> bool) -> bool {
    match expr {
        BoolExpr::Leaf(l) => leaf(l),
        BoolExpr::Not(e) => !eval(e, leaf),
        BoolExpr::And(items) => items.iter().all(|e| eval(e, leaf)),
        BoolExpr::Or(items) => items.iter().any(|e| eval(e, leaf)),
    }
}

/// True when the supports expression `text` offers `target` by its id or by its own tags (dynamic `$(...)` terms
/// match nothing: they depend on a character).
#[must_use]
pub fn offers(text: &str, target: &Element) -> bool {
    let Ok(Some(expr)) = SupportsExpr::parse(text) else {
        return false;
    };
    let range = expr.is_id_range_mode();
    eval(expr.root(), &|leaf| match leaf {
        SupportsLeaf::Tag(t) if range && t.starts_with("ID_") => target.id == *t,
        SupportsLeaf::Tag(t) => target.id == *t || target.supports.iter().any(|s| same_tag(s, t)),
        SupportsLeaf::Dynamic(_) => false,
    })
}

/// How `user` uses `target`, the strongest way first (grants, then selects, requires, lists); `None` when it does
/// not. An element never uses itself.
#[must_use]
pub fn use_of(user: &Element, target: &Element) -> Option<UseRelation> {
    if user.id == target.id {
        return None;
    }
    let ids: HashSet<&str> = std::iter::once(target.id.as_str()).collect();
    let mut uses: Vec<UseRelation> = Vec::new();
    let mut found = |r: UseRelation| uses.push(r);
    for rule in user.all_rules() {
        match rule {
            Rule::Grant(g) if names_any(&g.id, &ids) => found(UseRelation::Grants),
            Rule::Select(s) => {
                let text = s.supports.as_deref().unwrap_or_default();
                let by_id = names_any(text, &ids)
                    || s.default_id.as_deref().is_some_and(|d| names_any(d, &ids));
                let by_tag = !text.trim().is_empty()
                    && s.ty.eq_ignore_ascii_case(&target.ty)
                    && offers(text, target);
                if by_id || by_tag {
                    found(UseRelation::Selects);
                }
            }
            Rule::Custom(_) | Rule::Unknown(_) if json_mentions(rule, &ids) => {
                found(UseRelation::Grants);
            }
            _ => {}
        }
        if rule.requirements().is_some_and(|t| names_any(t, &ids)) {
            found(UseRelation::Requires);
        }
    }
    if names_any(&user.requirements, &ids) {
        found(UseRelation::Requires);
    }
    if let Some(sc) = &user.spellcasting {
        let listed = target.ty == "Spell"
            && sc
                .list
                .as_ref()
                .is_some_and(|l| offers(&l.supports, target));
        if listed || sc.extends.iter().any(|l| names_any(&l.supports, &ids)) {
            found(UseRelation::Lists);
        }
    }
    match uses.into_iter().min() {
        Some(r) => Some(r),
        None => elsewhere(user, &ids).then_some(UseRelation::Lists),
    }
}

/// The id anywhere else in the element (its tags, setters, data, package contents, multiclass section, per-edition
/// overrides and unknown parts), outside its own id, name, source, description and rules. Typed parts are read as
/// they are; only the rare nested parts are turned into JSON.
fn elsewhere(user: &Element, ids: &HashSet<&str>) -> bool {
    user.supports.iter().any(|t| names_any(t, ids))
        || user.setters.iter().any(|s| names_any(&s.value, ids))
        || user.data.values().any(|v| mentions(v, ids))
        || user
            .profile_overrides
            .values()
            .flat_map(|m| m.values())
            .any(|v| mentions(v, ids))
        || user.extract.as_ref().is_some_and(|x| json_mentions(x, ids))
        || user
            .equipment
            .as_ref()
            .is_some_and(|x| json_mentions(x, ids))
        || user
            .multiclass
            .as_ref()
            .is_some_and(|x| json_mentions(x, ids))
        || (!user.extra.is_empty() && json_mentions(&user.extra, ids))
}

fn json_mentions<T: Serialize>(part: &T, ids: &HashSet<&str>) -> bool {
    serde_json::to_value(part).is_ok_and(|v| mentions(&v, ids))
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::rules::{GrantRule, SelectRule};
    use borealis_content::spellcasting::{Spellcasting, SpellcastingList};
    use serde_json::json;

    fn el(id: &str, ty: &str) -> Element {
        Element::new(id, id, ty, "Test")
    }

    #[test]
    fn mentions_are_whole_tokens_at_any_depth_and_in_keys() {
        let ids: HashSet<&str> = ["ID_X"].into();
        assert!(mentions(&json!({"a": [{"b": "ID_X"}]}), &ids));
        assert!(mentions(&json!({"choices": {"ID_X": 1}}), &ids));
        assert!(mentions(&json!({"req": "!ID_X,ID_Y"}), &ids));
        assert!(!mentions(&json!({"a": "ID_XY"}), &ids));
        assert!(!mentions(&json!({"a": "MY_ID_X"}), &ids));
    }

    #[test]
    fn grants_selects_requires_and_lists_are_told_apart() {
        let target = Element {
            supports: vec!["Fighting Style".into(), "Wizard".into()],
            ..el("ID_T", "Option")
        };
        let grant = Element {
            rules: vec![Rule::Grant(GrantRule {
                id: "ID_T".into(),
                ..GrantRule::default()
            })],
            ..el("ID_G", "Class")
        };
        assert_eq!(use_of(&grant, &target), Some(UseRelation::Grants));
        let by_tag = Element {
            rules: vec![Rule::Select(SelectRule {
                ty: "Option".into(),
                name: "Style".into(),
                supports: Some("Fighting Style".into()),
                ..SelectRule::default()
            })],
            ..el("ID_S", "Class")
        };
        assert_eq!(use_of(&by_tag, &target), Some(UseRelation::Selects));
        let other_type = Element {
            rules: vec![Rule::Select(SelectRule {
                ty: "Feat".into(),
                name: "Style".into(),
                supports: Some("Fighting Style".into()),
                ..SelectRule::default()
            })],
            ..el("ID_O", "Class")
        };
        assert_eq!(use_of(&other_type, &target), None, "a choice of feats");
        let by_id = Element {
            rules: vec![Rule::Select(SelectRule {
                ty: "Feat".into(),
                name: "Pick".into(),
                supports: Some("ID_A|ID_T".into()),
                ..SelectRule::default()
            })],
            ..el("ID_I", "Feat")
        };
        assert_eq!(use_of(&by_id, &target), Some(UseRelation::Selects));
        let requires = Element {
            requirements: "!ID_T".into(),
            ..el("ID_R", "Feat")
        };
        assert_eq!(use_of(&requires, &target), Some(UseRelation::Requires));
        let data = Element {
            data: [("owner".to_string(), json!("ID_T"))].into(),
            ..el("ID_D", "Class Feature")
        };
        assert_eq!(use_of(&data, &target), Some(UseRelation::Lists));
        assert_eq!(use_of(&target, &target), None, "never itself");
        assert_eq!(use_of(&el("ID_N", "Feat"), &target), None);
    }

    #[test]
    fn a_spell_list_lists_the_spells_it_offers() {
        let spell = Element {
            supports: vec!["Wizard".into()],
            ..el("ID_FIRE", "Spell")
        };
        let class = Element {
            spellcasting: Some(Spellcasting {
                list: Some(SpellcastingList::new("Wizard")),
                ..Spellcasting::default()
            }),
            ..el("ID_WIZ", "Class")
        };
        assert_eq!(use_of(&class, &spell), Some(UseRelation::Lists));
        let cleric = Element {
            spellcasting: Some(Spellcasting {
                list: Some(SpellcastingList::new("Cleric")),
                ..Spellcasting::default()
            }),
            ..el("ID_CLR", "Class")
        };
        assert_eq!(use_of(&cleric, &spell), None);
        let patron = Element {
            spellcasting: Some(Spellcasting {
                extends: vec![SpellcastingList::new("ID_FIRE")],
                ..Spellcasting::default()
            }),
            ..el("ID_PATRON", "Archetype")
        };
        assert_eq!(use_of(&patron, &spell), Some(UseRelation::Lists));
    }

    #[test]
    fn grants_win_over_weaker_uses() {
        let target = el("ID_T", "Feat");
        let both = Element {
            requirements: "ID_T".into(),
            rules: vec![Rule::Grant(GrantRule {
                id: "ID_T".into(),
                ..GrantRule::default()
            })],
            ..el("ID_B", "Feat")
        };
        assert_eq!(use_of(&both, &target), Some(UseRelation::Grants));
    }
}
