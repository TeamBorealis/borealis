//! `attachedSpells` of an item: the spells it lets its wielder cast, as `grant_spell` rules in
//! the bundled representation. Spells paid with charges spend the item's charge `resource`
//! (`resource_cost`, id = the item id), per-day / per-rest / limited uses are `free_cast`, at-will
//! spells `at_will_no_slot`, ritual spells `ritual_only`; spells whose casting only the item text
//! describes (`other`, a plain list) are plain Spell grants.

use super::super::build::custom;
use super::super::spells_extra::{parse_spell_ref, spell_ref_id, uses_value};
use super::super::Shared;
use crate::core_ids;
use borealis_content::{GrantRule, Rule};
use serde_json::{json, Map, Value};

/// How one group of attached spells is cast.
enum Cast {
    /// Spends this many charges (0: castable while the item has charges, nothing spent).
    Charges(u32),
    /// Free uses (`uses` key as written) with a recharge.
    Uses(String, &'static str, bool),
    AtWill,
    Ritual,
    /// Described by the item text only.
    Plain,
}

struct Out<'a> {
    sh: &'a Shared,
    item_id: &'a str,
    has_charges: bool,
    ability: Option<&'static str>,
    rules: Vec<Rule>,
    seen: Vec<String>,
    partial: Vec<String>,
}

impl Out<'_> {
    fn spell(&mut self, text: &str, cast: &Cast) {
        let r = parse_spell_ref(text);
        let id = spell_ref_id(self.sh, &r);
        if self.seen.contains(&id) {
            return;
        }
        self.seen.push(id.clone());
        let mut f = Map::new();
        f.insert("spell".into(), json!(id));
        let mode = match cast {
            Cast::Plain => {
                self.rules.push(Rule::Grant(GrantRule {
                    id,
                    ty: Some("Spell".into()),
                    ..GrantRule::default()
                }));
                return;
            }
            Cast::AtWill | Cast::Charges(0) => "at_will_no_slot",
            Cast::Ritual => "ritual_only",
            Cast::Charges(_) | Cast::Uses(..) => "known",
        };
        f.insert("mode".into(), json!(mode));
        match cast {
            Cast::Charges(n) if *n > 0 => {
                if !self.has_charges {
                    self.partial.push(format!(
                        "{text} costs {n} charges but the item has no charge count"
                    ));
                }
                f.insert(
                    "resource_cost".into(),
                    json!({"resource": self.item_id, "cost": n}),
                );
            }
            Cast::Uses(uses, recharge, shared) => {
                let Some(n) = uses_value(uses) else {
                    self.partial
                        .push(format!("{text}: uses {uses:?} were not converted"));
                    return;
                };
                let mut fc = json!({"uses": n, "recharge": recharge});
                if *shared {
                    fc["shared"] = Value::Bool(true);
                }
                f.insert("free_cast".into(), fc);
            }
            _ => {}
        }
        if let Some(cl) = r.cast_level {
            f.insert("cast_level".into(), json!(cl));
        }
        if let Some(ab) = self.ability {
            f.insert("ability".into(), json!(ab));
        }
        self.rules.push(custom("grant_spell", Value::Object(f)));
    }

    fn list(&mut self, v: &Value, cast: &Cast) {
        for text in v.as_array().into_iter().flatten().filter_map(Value::as_str) {
            self.spell(text, cast);
        }
    }

    /// `{ "<uses>": [spells] }` groups (`charges`, `daily`, `rest`, `limited`).
    fn by_uses(&mut self, v: &Value, cast: impl Fn(&str, bool) -> Option<Cast>) {
        for (key, spells) in v.as_object().into_iter().flatten() {
            let shared = !key.ends_with('e') && spells.as_array().is_some_and(|a| a.len() > 1);
            match cast(key, shared) {
                Some(c) => self.list(spells, &c),
                None => self
                    .partial
                    .push(format!("attached spells keyed {key:?} were not converted")),
            }
        }
    }
}

/// Rules for an item's `attachedSpells`; `has_charges` says whether the item got a charge
/// `resource` (id = `item_id`).
pub fn attached_spell_rules(
    sh: &Shared,
    item_id: &str,
    has_charges: bool,
    attached: &Value,
    partial: &mut Vec<String>,
) -> Vec<Rule> {
    let mut out = Out {
        sh,
        item_id,
        has_charges,
        ability: attached
            .get("ability")
            .and_then(Value::as_str)
            .and_then(core_ids::ability_name),
        rules: Vec::new(),
        seen: Vec::new(),
        partial: Vec::new(),
    };
    match attached {
        Value::Array(_) => out.list(attached, &Cast::Plain),
        Value::Object(groups) => {
            for (key, v) in groups {
                let recharge = match key.as_str() {
                    "daily" => Some("long_rest"),
                    "rest" => Some("short_rest"),
                    "limited" => Some("none"),
                    _ => None,
                };
                match (key.as_str(), recharge) {
                    ("ability", _) => {}
                    ("charges", _) => out.by_uses(v, |k, _| k.parse().ok().map(Cast::Charges)),
                    (_, Some(rc)) => {
                        out.by_uses(v, |k, shared| Some(Cast::Uses(k.to_string(), rc, shared)))
                    }
                    ("will", _) => out.list(v, &Cast::AtWill),
                    ("ritual", _) => out.list(v, &Cast::Ritual),
                    ("other", _) => out.list(v, &Cast::Plain),
                    (other, _) => out
                        .partial
                        .push(format!("attached spells {other:?} were not converted")),
                }
            }
        }
        _ => {}
    }
    partial.extend(out.partial);
    out.rules
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(attached: Value, has_charges: bool) -> (Vec<Value>, Vec<String>) {
        let sh = Shared::default();
        let mut partial = Vec::new();
        let r = attached_spell_rules(&sh, "ID_ITEM_X", has_charges, &attached, &mut partial);
        let v = r.iter().map(|r| serde_json::to_value(r).unwrap()).collect();
        (v, partial)
    }

    fn by<'a>(rules: &'a [Value], suffix: &str) -> &'a Value {
        rules
            .iter()
            .find(|r| {
                r.get("spell")
                    .or_else(|| r.get("id"))
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.ends_with(suffix))
            })
            .unwrap_or_else(|| panic!("{suffix} in {rules:?}"))
    }

    #[test]
    fn charge_spells_spend_the_item_charges() {
        let (r, partial) = rules(
            json!({"charges": {"1": ["glow orb"], "3": ["frost lance#4"], "0": ["spark"]},
                   "ability": "int"}),
            true,
        );
        assert!(partial.is_empty(), "{partial:?}");
        let orb = by(&r, "GLOW_ORB");
        assert_eq!(orb["kind"], "grant_spell");
        assert_eq!(orb["mode"], "known");
        assert_eq!(
            orb["resource_cost"],
            json!({"resource": "ID_ITEM_X", "cost": 1})
        );
        assert_eq!(orb["ability"], "Intelligence");
        assert_eq!(by(&r, "FROST_LANCE")["cast_level"], 4);
        let spark = by(&r, "SPARK");
        assert_eq!(spark["mode"], "at_will_no_slot");
        assert!(spark.get("resource_cost").is_none());
    }

    #[test]
    fn daily_rest_limited_will_ritual_and_other_spells() {
        let (r, partial) = rules(
            json!({"daily": {"1e": ["mist step", "veil"]}, "rest": {"2": ["calm", "hush"]},
                   "limited": {"3": ["far call"]}, "will": ["light touch"], "ritual": ["omen"],
                   "other": ["wish well"]}),
            false,
        );
        assert!(partial.is_empty(), "{partial:?}");
        assert_eq!(
            by(&r, "MIST_STEP")["free_cast"],
            json!({"uses": 1, "recharge": "long_rest"})
        );
        assert_eq!(
            by(&r, "CALM")["free_cast"],
            json!({"uses": 2, "recharge": "short_rest", "shared": true})
        );
        assert_eq!(by(&r, "FAR_CALL")["free_cast"]["recharge"], "none");
        assert_eq!(by(&r, "LIGHT_TOUCH")["mode"], "at_will_no_slot");
        assert_eq!(by(&r, "OMEN")["mode"], "ritual_only");
        let plain = by(&r, "WISH_WELL");
        assert_eq!(
            (plain["kind"].as_str(), plain["type"].as_str()),
            (Some("grant"), Some("Spell"))
        );
        for g in r.iter().filter(|g| g["kind"] == "grant_spell") {
            let Ok(Rule::Custom(c)) = serde_json::from_value::<Rule>(g.clone()) else {
                panic!("custom rule {g}");
            };
            assert!(
                borealis_content::rule_kinds::check_custom(&c).is_empty(),
                "{g}"
            );
            let spec = borealis_content::rule_kinds::spec("grant_spell").unwrap();
            assert!(spec.shape_problems(&c.fields).is_empty(), "{g}");
        }
    }

    #[test]
    fn a_plain_list_grants_and_a_missing_charge_count_is_noted() {
        let (r, _) = rules(json!(["glow orb"]), false);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0]["kind"], "grant");
        let (_, partial) = rules(json!({"charges": {"2": ["glow orb"]}}), false);
        assert_eq!(partial.len(), 1, "{partial:?}");
    }
}
