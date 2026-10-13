//! `<append>` merge semantics.

use crate::element::{Append, Element};
use crate::setter;

/// Apply `append` to `target`, returning the merged element and whether anything changed.
///
/// * `supports`: union (new tags go to the end)
/// * setters: added only if the target has none of that name *before* this append
/// * rules: appended after the target's rules
/// * `spellcasting`: adopted only if the target has none
/// * everything else in the append is ignored
#[must_use]
pub fn apply_append(target: &Element, append: &Append) -> (Element, bool) {
    let mut merged = target.clone();
    let mut changed = false;

    for tag in &append.supports {
        if !merged.supports.iter().any(|t| t == tag) {
            merged.supports.push(tag.clone());
            changed = true;
        }
    }

    for new in &append.setters {
        if setter::find(&target.setters, &new.name).is_none() {
            merged.setters.push(new.clone());
            changed = true;
        }
    }

    if !append.rules.is_empty() {
        merged.rules.extend(append.rules.iter().cloned());
        changed = true;
    }

    if merged.spellcasting.is_none() {
        if let Some(sc) = &append.spellcasting {
            merged.spellcasting = Some(sc.clone());
            changed = true;
        }
    }

    (merged, changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{GrantRule, Rule};
    use crate::setter::Setter;
    use crate::spellcasting::Spellcasting;

    fn target() -> Element {
        let mut e = Element::new("ID_T", "T", "Spell", "Core");
        e.supports = vec!["Wizard".into()];
        e.setters = vec![Setter::new("Level", "1")];
        e
    }

    #[test]
    fn union_supports_and_conditional_setters() {
        let append = Append {
            id: "ID_T".into(),
            supports: vec!["Wizard".into(), "Sorcerer".into()],
            setters: vec![
                Setter::new("level", "9"),
                Setter::new("keywords", "a"),
                Setter::new("keywords", "b"),
            ],
            ..Append::default()
        };
        let (merged, changed) = apply_append(&target(), &append);
        assert!(changed);
        assert_eq!(merged.supports, ["Wizard", "Sorcerer"]);
        assert_eq!(merged.setter_value("level"), Some("1"));
        assert_eq!(
            merged
                .setters
                .iter()
                .filter(|s| s.name == "keywords")
                .count(),
            2
        );
    }

    #[test]
    fn rules_append_and_spellcasting_adopted_once() {
        let sc = Spellcasting {
            name: Some("Warlock".into()),
            ..Spellcasting::default()
        };
        let append = Append {
            id: "ID_T".into(),
            rules: vec![Rule::Grant(GrantRule {
                id: "ID_X".into(),
                ..GrantRule::default()
            })],
            spellcasting: Some(sc.clone()),
            ..Append::default()
        };
        let (merged, changed) = apply_append(&target(), &append);
        assert!(changed && merged.rules.len() == 1 && merged.spellcasting == Some(sc));
        let other = Append {
            id: "ID_T".into(),
            spellcasting: Some(Spellcasting {
                name: Some("Other".into()),
                ..Spellcasting::default()
            }),
            ..Append::default()
        };
        let (again, changed) = apply_append(&merged, &other);
        assert!(!changed);
        assert_eq!(again.spellcasting.unwrap().name.as_deref(), Some("Warlock"));
    }

    #[test]
    fn noop_append_reports_unchanged() {
        let (merged, changed) = apply_append(
            &target(),
            &Append {
                id: "ID_T".into(),
                ..Append::default()
            },
        );
        assert!(!changed);
        assert_eq!(merged, target());
    }
}
