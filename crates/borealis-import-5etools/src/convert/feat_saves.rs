//! A feat whose ability increase and saving throw proficiency are one choice (Resilient:
//! `ability: [{choose: {from: F}}]` with `savingThrowProficiencies: [{choose: {from: F}}]`).
//! Each ability becomes a `Feat Feature` option (`<Feat> (Strength)`: +1 and the saving throw
//! proficiency), offered by one select, as the feat's other option groups are.

use super::build::{grant, select, stat};
use super::mech::string_list;
use super::{new_element, State};
use crate::core_ids;
use crate::ids::IdKind;
use crate::model::Feat;
use borealis_content::{Element, Rule, Ruleset, Sheet};
use serde_json::Value;

/// The feat being converted.
pub struct FeatRef<'a> {
    /// Element id.
    pub id: &'a str,
    /// Source abbreviation.
    pub source: &'a str,
    /// Source display name.
    pub source_display: &'a str,
    /// Ruleset.
    pub ruleset: Ruleset,
}

/// The select and its options.
pub struct Linked {
    /// `select` of one option.
    pub rule: Rule,
    /// The option elements.
    pub options: Vec<Element>,
}

/// The `from` list of a single `{choose: {from}}` object picking one (`count` 1, `amount` 1).
fn single_choice(list: &[Value]) -> Option<Vec<String>> {
    let [only] = list else { return None };
    let obj = only.as_object()?;
    if obj.len() != 1 {
        return None;
    }
    let choose = obj.get("choose")?;
    let one = |k: &str| choose.get(k).and_then(Value::as_u64).unwrap_or(1) == 1;
    if !one("count") || !one("amount") {
        return None;
    }
    let from = string_list(choose.get("from"));
    (!from.is_empty()).then_some(from)
}

/// The linked choice of `feat`, when it has one.
pub fn linked_choice(st: &mut State, feat: &Feat, f: &FeatRef<'_>) -> Option<Linked> {
    let abilities = single_choice(&feat.ability)?;
    if single_choice(&feat.saving_throw_proficiencies)? != abilities {
        return None;
    }
    let mut options = Vec::new();
    for abbr in &abilities {
        let (Some(name), Some(stat_name), Some(save)) = (
            core_ids::ability_name(abbr),
            core_ids::ability_stat(abbr),
            core_ids::saving_throw_id(abbr),
        ) else {
            return None;
        };
        let oname = format!("{} ({name})", feat.name);
        let oid = st.ids.id(
            IdKind::SpeciesOption,
            f.source,
            &[&feat.name, name],
            &format!("feat|{}|{}|{name}", feat.name, f.source),
        );
        let mut el = new_element(&oid, &oname, "Feat Feature", f.source_display, f.ruleset);
        el.supports = vec![f.id.to_string()];
        el.description = format!(
            "<p>Your {name} score increases by 1, and you gain proficiency in {name} saving throws.</p>"
        );
        el.sheet = Some(Sheet {
            display: Some(false),
            ..Sheet::default()
        });
        el.rules = vec![Rule::Stat(stat(&stat_name, 1)), grant(&save, "Proficiency")];
        options.push(el);
    }
    let rule = Rule::Select(select(
        "Feat Feature",
        &format!("{} Ability", feat.name),
        Some(f.id),
        1,
    ));
    Some(Linked { rule, options })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn feat(raw: Value) -> Feat {
        crate::model::parse(&raw).unwrap()
    }

    const F: FeatRef<'static> = FeatRef {
        id: "ID_5ET_ZZZ_FEAT_STEADY",
        source: "ZZZ",
        source_display: "Test",
        ruleset: Ruleset::Dnd2014,
    };

    #[test]
    fn one_option_per_ability_with_increase_and_save() {
        let mut st = State::default();
        let f = feat(json!({"name": "Steady", "source": "ZZZ",
            "ability": [{"choose": {"from": ["str", "con"], "amount": 1}}],
            "savingThrowProficiencies": [{"choose": {"from": ["str", "con"]}}]}));
        let l = linked_choice(&mut st, &f, &F).expect("linked");
        assert_eq!(l.options.len(), 2);
        let o = &l.options[1];
        assert_eq!(o.name, "Steady (Constitution)");
        assert_eq!(o.ty, "Feat Feature");
        assert_eq!(o.supports, vec!["ID_5ET_ZZZ_FEAT_STEADY".to_string()]);
        assert!(matches!(&o.rules[0], Rule::Stat(s) if s.name == "constitution" && s.value == "1"));
        assert!(
            matches!(&o.rules[1], Rule::Grant(g) if Some(g.id.clone()) == core_ids::saving_throw_id("con"))
        );
        assert!(
            matches!(&l.rule, Rule::Select(s) if s.ty == "Feat Feature" && s.supports.as_deref() == Some("ID_5ET_ZZZ_FEAT_STEADY"))
        );
    }

    #[test]
    fn unlinked_lists_are_left_alone() {
        let mut st = State::default();
        for raw in [
            json!({"name": "A", "source": "ZZZ",
                "ability": [{"choose": {"from": ["str", "con"]}}],
                "savingThrowProficiencies": [{"choose": {"from": ["dex", "con"]}}]}),
            json!({"name": "B", "source": "ZZZ",
                "ability": [{"choose": {"from": ["str", "con"], "amount": 2}}],
                "savingThrowProficiencies": [{"choose": {"from": ["str", "con"]}}]}),
            json!({"name": "C", "source": "ZZZ", "ability": [{"str": 1}]}),
        ] {
            assert!(linked_choice(&mut st, &feat(raw), &F).is_none());
        }
    }
}
