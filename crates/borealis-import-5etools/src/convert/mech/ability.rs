//! Ability score increases (`ability`) of races, backgrounds and feats.
//!
//! Mapping onto the constructs the bundled packs use:
//! - fixed `{"dex": 2}` -> `stat dexterity +2`; with a `max` above 20 (Epic Boons) the
//!   increase goes to `<ability>:boon` with its cap in `<ability>:boon:cap`;
//! - `choose` of one +1 pick -> the classic `Ability Score Improvement` select (Aurora);
//! - `choose` of several picks (+1 to two different abilities, PHB Half-Elf), with an
//!   `amount` above 1 (+2 to one ability), a raised maximum, or the hidden
//!   "+2 to one or +1 to two" pattern -> `ability_increase` (as bundled SRD 5.2.1 feats);
//! - `weighted` (+2/+1 or +1/+1/+1, 2024 backgrounds) -> `ability_distribution`.

use super::{count_of, obj_entries, string_list, AbilitySource, Mech};
use crate::convert::build::{custom, select, stat};
use crate::core_ids as core;
use borealis_content::Rule;
use serde_json::{json, Value};

/// The ordinary ability score maximum.
const DEFAULT_CAP: i64 = 20;

/// One `choose` object, read.
struct Choose {
    /// Lower-case full ability names (`strength`).
    pool: Vec<String>,
    count: u32,
    amount: u32,
}

impl Choose {
    fn read(v: &Value) -> Self {
        let from = string_list(v.get("from"));
        let from: Vec<String> = if from.is_empty() {
            core::ABILITIES
                .iter()
                .map(|(a, _)| (*a).to_string())
                .collect()
        } else {
            from
        };
        Self {
            pool: from.iter().filter_map(|a| core::ability_stat(a)).collect(),
            count: count_of(v.get("count").unwrap_or(&json!(1))),
            amount: count_of(v.get("amount").unwrap_or(&json!(1))),
        }
    }
}

impl Mech<'_> {
    /// `ability` increases.
    pub fn ability(&mut self, list: &[Value], source: AbilitySource) {
        let weighted: Vec<&Value> = list
            .iter()
            .filter(|o| o.get("choose").and_then(|c| c.get("weighted")).is_some())
            .collect();
        if !weighted.is_empty() {
            self.weighted_abilities(&weighted);
            return;
        }
        let visible: Vec<&Value> = list
            .iter()
            .filter(|o| o.get("hidden") != Some(&Value::Bool(true)))
            .collect();
        let first = visible.first().copied().or_else(|| list.first());
        if list.len() > 1 && source != AbilitySource::Feat {
            self.partial
                .push("alternative ability score arrays: only the first was converted".into());
        }
        let Some(first) = first else { return };
        let cap = first
            .get("max")
            .and_then(Value::as_i64)
            .filter(|m| *m > DEFAULT_CAP);
        let hidden = first.get("hidden") == Some(&Value::Bool(true));
        for (k, v) in obj_entries(first) {
            match k.as_str() {
                "choose" => self.ability_choose(v, source, cap, hidden),
                "hidden" | "max" => {}
                abbr => match (core::ability_stat(abbr), v.as_i64()) {
                    (Some(name), Some(n)) => self.fixed_increase(&name, n, cap),
                    _ => self.note("ability", format!("ability entry {abbr:?} not converted")),
                },
            }
        }
    }

    fn fixed_increase(&mut self, ability: &str, amount: i64, cap: Option<i64>) {
        match cap {
            None => self.push(Rule::Stat(stat(ability, amount))),
            Some(c) => {
                self.push(Rule::Stat(stat(&format!("{ability}:boon"), amount)));
                let mut max = stat(&format!("{ability}:boon:cap"), c);
                max.bonus = Some("base".into());
                self.push(Rule::Stat(max));
            }
        }
    }

    fn ability_choose(&mut self, v: &Value, source: AbilitySource, cap: Option<i64>, hidden: bool) {
        let c = Choose::read(v);
        if c.pool.is_empty() {
            return;
        }
        // two or more picks are of different abilities: the engine's increase choice offers
        // distinct combinations, where Ability Score Improvement slots accept one ability twice
        if c.amount > 1 || c.count > 1 || cap.is_some() {
            self.ability_increase(&c, cap.unwrap_or(DEFAULT_CAP), hidden);
            return;
        }
        let owner = if source == AbilitySource::Race {
            "Race"
        } else {
            "Class"
        };
        let names: Vec<String> = c.pool.iter().map(|a| crate::util::title_case(a)).collect();
        let supports = format!("Ability Score Improvement&&{owner}&&({})", names.join("||"));
        let name = self.unique_name("Ability Score Increase");
        let rule = select("Ability Score Improvement", &name, Some(&supports), c.count);
        self.push(Rule::Select(rule));
    }

    /// `+amount` to `count` abilities of the pool (a real +2-to-one choice, not two +1 picks).
    /// The hidden `+2 to one` of a full pool is the 2024 Ability Score Improvement: "+2 to one
    /// ability or +1 to two", exactly as the bundled SRD 5.2.1 feat.
    fn ability_increase(&mut self, c: &Choose, cap: i64, hidden: bool) {
        let all = c.pool.len() == core::ABILITIES.len();
        let options = if hidden && all && c.amount == 2 && c.count == 1 {
            json!([{"abilities": 1, "amount": 2}, {"abilities": 2, "amount": 1}])
        } else {
            json!([{"abilities": c.pool, "pick": c.count, "amount": c.amount}])
        };
        let name = self.unique_name("Ability Score Increase");
        self.push(custom(
            "ability_increase",
            json!({"name": name, "options": options, "cap": cap}),
        ));
    }

    fn weighted_abilities(&mut self, objs: &[&Value]) {
        let mut pool: Vec<String> = Vec::new();
        let mut options: Vec<Vec<i64>> = Vec::new();
        for o in objs {
            let w = &o["choose"]["weighted"];
            for a in string_list(w.get("from")) {
                if let Some(n) = core::ability_stat(&a) {
                    if !pool.contains(&n) {
                        pool.push(n);
                    }
                }
            }
            let weights: Vec<i64> = w
                .get("weights")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default();
            if !weights.is_empty() && !options.contains(&weights) {
                options.push(weights);
            }
        }
        let name = self.unique_name("Ability Score Increase");
        self.push(custom(
            "ability_distribution",
            json!({"name": name, "pool": pool, "options": options, "cap": DEFAULT_CAP}),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_support::{ids_of, shared};
    use super::*;

    fn custom_rule(m: &Mech<'_>, i: usize) -> borealis_content::CustomRule {
        m.rules
            .iter()
            .filter_map(|r| match r {
                Rule::Custom(c) => Some(c.clone()),
                _ => None,
            })
            .nth(i)
            .expect("custom rule")
    }

    #[test]
    fn fixed_and_plus_one_choices_stay_classic() {
        let sh = shared();
        let mut m = Mech::new(&sh, "Elf");
        m.ability(&[json!({"dex": 2, "cha": 1})], AbilitySource::Race);
        m.ability(
            &[json!({"cha": 2, "choose": {"from": ["str", "dex", "con"], "count": 2}})],
            AbilitySource::Race,
        );
        let ids = ids_of(&m);
        assert!(ids.contains(&"stat:dexterity=2".to_string()));
        assert!(ids.contains(&"stat:charisma=1".to_string()));
        assert!(m.partial.is_empty(), "{:?}", m.partial);
        // two picks are two different abilities, not two slots that accept one ability twice
        let two = custom_rule(&m, 0);
        assert_eq!(two.kind, "ability_increase");
        assert_eq!(
            two.fields["options"],
            json!([{"abilities": ["strength", "dexterity", "constitution"], "pick": 2, "amount": 1}])
        );
        let mut one = Mech::new(&sh, "Elf");
        one.ability(
            &[json!({"choose": {"from": ["str", "dex", "con"]}})],
            AbilitySource::Race,
        );
        assert!(ids_of(&one).iter().any(|i| i == "select:Ability Score Improvement:Ability Score Increase (Elf):Ability Score Improvement&&Race&&(Strength||Dexterity||Constitution)"));
        let mut multi = Mech::new(&sh, "M");
        multi.ability(&[json!({"str": 1}), json!({"dex": 1})], AbilitySource::Race);
        assert!(multi.partial.iter().any(|p| p.contains("alternative")));
    }

    #[test]
    fn weighted_becomes_the_engine_distribution_kind() {
        let sh = shared();
        let mut bg = Mech::new(&sh, "Acolyte");
        bg.ability(
            &[
                json!({"choose": {"weighted": {"from": ["int", "wis", "cha"], "weights": [2, 1]}}}),
                json!({"choose": {"weighted": {"from": ["int", "wis", "cha"], "weights": [1, 1, 1]}}}),
            ],
            AbilitySource::Background,
        );
        let c = custom_rule(&bg, 0);
        assert_eq!(c.kind, "ability_distribution");
        assert_eq!(
            c.fields["pool"],
            json!(["intelligence", "wisdom", "charisma"])
        );
        assert_eq!(c.fields["options"], json!([[2, 1], [1, 1, 1]]));
        assert_eq!(c.fields["cap"], 20);
    }

    #[test]
    fn plus_two_to_one_is_a_single_increase_choice() {
        let sh = shared();
        let mut t = Mech::new(&sh, "Hellfire");
        t.ability(
            &[json!({"int": 1, "choose": {"from": ["dex", "cha"], "count": 1, "amount": 2}})],
            AbilitySource::Race,
        );
        assert!(t.partial.is_empty(), "{:?}", t.partial);
        assert!(ids_of(&t).contains(&"stat:intelligence=1".to_string()));
        let c = custom_rule(&t, 0);
        assert_eq!(c.kind, "ability_increase");
        assert_eq!(
            c.fields["options"],
            json!([{"abilities": ["dexterity", "charisma"], "pick": 1, "amount": 2}])
        );
        assert_eq!(c.fields["cap"], 20);
    }

    #[test]
    fn hidden_full_pool_plus_two_is_the_2024_asi() {
        let sh = shared();
        let mut f = Mech::new(&sh, "Feat");
        f.ability(
            &[json!({"choose": {"from": ["str", "dex", "con", "int", "wis", "cha"], "amount": 2}, "hidden": true})],
            AbilitySource::Feat,
        );
        let c = custom_rule(&f, 0);
        assert_eq!(
            c.fields["options"],
            json!([{"abilities": 1, "amount": 2}, {"abilities": 2, "amount": 1}])
        );
    }

    #[test]
    fn raised_maximum_uses_the_boon_cap() {
        let sh = shared();
        let mut b = Mech::new(&sh, "Boon");
        b.ability(
            &[json!({"choose": {"from": ["int", "wis", "cha"]}, "max": 30})],
            AbilitySource::Feat,
        );
        assert!(b.partial.is_empty(), "{:?}", b.partial);
        let c = custom_rule(&b, 0);
        assert_eq!(c.kind, "ability_increase");
        assert_eq!(c.fields["cap"], 30);
        assert_eq!(
            c.fields["options"],
            json!([{"abilities": ["intelligence", "wisdom", "charisma"], "pick": 1, "amount": 1}])
        );
        let mut fixed = Mech::new(&sh, "Fixed");
        fixed.ability(&[json!({"str": 1, "max": 30})], AbilitySource::Feat);
        let ids = ids_of(&fixed);
        assert_eq!(
            ids,
            vec!["stat:strength:boon=1", "stat:strength:boon:cap=30"]
        );
        let Rule::Stat(cap) = &fixed.rules[1] else {
            panic!()
        };
        assert_eq!(cap.bonus.as_deref(), Some("base"));
    }
}
