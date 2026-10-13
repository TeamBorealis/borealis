//! `additionalSpells`: spells granted or chosen by races, feats, backgrounds, classes.

use super::build::{custom, select, setter_with};
use super::Shared;
use crate::core_ids;
use crate::text::RefKind;
use crate::util::{id_fragment, title_case};
use borealis_content::{GrantRule, Rule};
use serde_json::{json, Value};

/// One named group of additional spells (a lineage, legacy or ancestry option).
#[derive(Debug, Default)]
pub struct SpellOption {
    /// Group name.
    pub name: String,
    /// Rules of the option.
    pub rules: Vec<Rule>,
}

/// Result of converting `additionalSpells`.
#[derive(Debug, Default)]
pub struct SpellRules {
    /// Rules of unnamed groups (apply directly).
    pub direct: Vec<Rule>,
    /// Named groups (the owner offers a choice between them).
    pub options: Vec<SpellOption>,
    /// Losses.
    pub partial: Vec<String>,
    /// Spells that grant `expanded` list entries, as spell ids.
    pub expanded: Vec<String>,
}

/// A parsed spell reference like `fireball|xphb#2`.
#[derive(Debug, PartialEq, Eq)]
pub struct SpellRef {
    /// Spell name (lower case as written).
    pub name: String,
    /// Source abbreviation (defaults to PHB).
    pub source: String,
    /// Cast at this level.
    pub cast_level: Option<u32>,
    /// Flagged `#c`.
    pub cantrip: bool,
}

/// Parse a spell reference string.
#[must_use]
pub fn parse_spell_ref(text: &str) -> SpellRef {
    let (main, suffix) = text.split_once('#').map_or((text, ""), |(m, s)| (m, s));
    let (name, source) = main.split_once('|').map_or((main, "PHB"), |(n, s)| (n, s));
    let source = if source.trim().is_empty() {
        "PHB"
    } else {
        source.trim()
    };
    SpellRef {
        name: name.trim().to_string(),
        source: source.to_uppercase(),
        cast_level: suffix.parse().ok(),
        cantrip: suffix == "c",
    }
}

/// Convert a filter like `level=0|class=Wizard` to a supports expression.
#[must_use]
pub fn filter_to_supports(filter: &str) -> Option<String> {
    let mut parts = Vec::new();
    for pair in filter.split('|') {
        let (k, v) = pair.split_once('=')?;
        let values: Vec<String> = v
            .split(';')
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect();
        if values.is_empty() || values.iter().any(|x| x.starts_with('!')) {
            return None;
        }
        let tags: Vec<String> = match k.trim() {
            "level" => values,
            // class names are matched against spell `supports`, which use the class's own
            // spelling ("Cleric"); filters may be written in lower case
            "class" => values.iter().map(|c| crate::util::title_case(c)).collect(),
            "school" => values.iter().map(|c| school_name(c).to_string()).collect(),
            _ => return None,
        };
        parts.push(if tags.len() == 1 {
            tags[0].clone()
        } else {
            format!("({})", tags.join("||"))
        });
    }
    (!parts.is_empty()).then(|| parts.join("&&"))
}

/// Full school name for a code.
#[must_use]
pub fn school_name(code: &str) -> &'static str {
    match code {
        "A" => "Abjuration",
        "C" => "Conjuration",
        "D" => "Divination",
        "E" => "Enchantment",
        "V" => "Evocation",
        "I" => "Illusion",
        "N" => "Necromancy",
        "T" => "Transmutation",
        "P" => "Psionic",
        _ => "Unknown",
    }
}

fn level_of(key: &str, partial: &mut Vec<String>) -> Option<u32> {
    if key == "_" {
        return Some(1);
    }
    match key.parse::<u32>() {
        Ok(n) => Some(n.max(1)),
        Err(_) => {
            partial.push(format!(
                "additional spells keyed {key:?} were not converted"
            ));
            None
        }
    }
}

struct Ctx<'a> {
    sh: &'a Shared,
    label: String,
    ability: Option<String>,
    partial: Vec<String>,
}

impl Ctx<'_> {
    fn spell_id(&self, r: &SpellRef) -> String {
        self.sh
            .link(RefKind::Spell, &r.name, &r.source)
            .unwrap_or_else(|| {
                format!(
                    "ID_5ET_{}_SPELL_{}",
                    id_fragment(&r.source),
                    id_fragment(&r.name)
                )
            })
    }

    fn grant(
        &self,
        text: &str,
        level: u32,
        prepared: bool,
        mode: Option<(&str, Option<u32>)>,
    ) -> Rule {
        let r = parse_spell_ref(text);
        let mut g = GrantRule {
            id: self.spell_id(&r),
            ty: Some("Spell".into()),
            level: (level > 1).then_some(level),
            spellcasting: Some(self.label.clone()),
            prepared: prepared.then_some(true),
            ..GrantRule::default()
        };
        if let Some(ab) = &self.ability {
            g.setters.push(setter_with("spellcasting ability", ab, &[]));
        }
        if let Some((mode, uses)) = mode {
            let uses_text = uses.map(|u| u.to_string());
            let mut attrs: Vec<(&str, &str)> = Vec::new();
            if let Some(u) = uses_text.as_deref() {
                attrs.push(("uses", u));
            }
            g.setters.push(setter_with("cast", mode, &attrs));
        }
        if let Some(cl) = r.cast_level {
            g.setters
                .push(setter_with("cast level", &cl.to_string(), &[]));
        }
        Rule::Grant(g)
    }

    fn choice(&mut self, choose: &str, count: u32, level: u32, prepared: bool) -> Option<Rule> {
        let Some(supports) = filter_to_supports(choose) else {
            self.partial
                .push(format!("spell choice filter {choose:?} was not converted"));
            return None;
        };
        let name = format!("Spell ({}) {}", self.label, choose.replace('|', ", "));
        let mut s = select("Spell", &name, Some(&supports), count);
        s.level = (level > 1).then_some(level);
        s.spellcasting = Some(self.label.clone());
        s.prepared = prepared.then_some(true);
        Some(Rule::Select(s))
    }

    /// Rules for a list that is an array of strings / choose objects.
    fn spell_list(
        &mut self,
        list: &Value,
        level: u32,
        prepared: bool,
        mode: Option<(&str, Option<u32>)>,
        out: &mut Vec<Rule>,
    ) {
        let Some(items) = list.as_array() else { return };
        for item in items {
            match item {
                Value::String(s) => out.push(self.grant(s, level, prepared, mode)),
                Value::Object(o) => {
                    let count = o
                        .get("count")
                        .and_then(Value::as_u64)
                        .and_then(|c| u32::try_from(c).ok())
                        .unwrap_or(1);
                    if let Some(f) = o.get("choose").and_then(Value::as_str) {
                        if let Some(Rule::Select(new)) = self.choice(f, count, level, prepared) {
                            // identical choices merge: select names must be unique
                            let same = out.iter_mut().find_map(|r| match r {
                                Rule::Select(s) if s.name == new.name && s.level == new.level => {
                                    Some(s)
                                }
                                _ => None,
                            });
                            match same {
                                Some(existing) => {
                                    existing.number = Some(
                                        existing.number.unwrap_or(1) + new.number.unwrap_or(1),
                                    );
                                }
                                None => out.push(Rule::Select(new)),
                            }
                        }
                    } else {
                        self.partial
                            .push("an additional spell entry has an unknown shape".into());
                    }
                }
                _ => {}
            }
        }
    }

    /// `known` / `prepared`: `{ "<level>|_": [..] | { "_": [..] } }`
    fn level_map(&mut self, map: &Value, prepared: bool, out: &mut Vec<Rule>) {
        let Some(map) = map.as_object() else { return };
        for (k, v) in map {
            let Some(level) = level_of(k, &mut self.partial) else {
                continue;
            };
            match v {
                Value::Array(_) => self.spell_list(v, level, prepared, None, out),
                Value::Object(inner) => {
                    for (ik, iv) in inner {
                        if ik == "_" || ik.parse::<u32>().is_ok() {
                            self.spell_list(iv, level, prepared, None, out);
                        } else {
                            self.partial
                                .push(format!("additional spell group {ik:?} was not converted"));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// `innate`: `{ "<level>|_": { daily: { "1": [..] }, will: [..], .. } | [..] }`
    fn innate(&mut self, map: &Value, out: &mut Vec<Rule>) {
        let Some(map) = map.as_object() else { return };
        for (k, v) in map {
            let Some(level) = level_of(k, &mut self.partial) else {
                continue;
            };
            match v {
                Value::Array(_) => self.spell_list(v, level, true, Some(("at will", None)), out),
                Value::Object(modes) => {
                    for (mode, list) in modes {
                        match mode.as_str() {
                            "will" => {
                                self.spell_list(list, level, true, Some(("at will", None)), out)
                            }
                            "ritual" => {
                                self.spell_list(list, level, true, Some(("ritual", None)), out)
                            }
                            "daily" | "rest" | "weekly" | "monthly" | "yearly" => {
                                let Some(by_uses) = list.as_object() else {
                                    continue;
                                };
                                for (uses, spells) in by_uses {
                                    let n = uses.trim_end_matches('e').parse::<u32>().ok();
                                    let label = if uses.ends_with('e') {
                                        format!("{mode} each")
                                    } else {
                                        mode.clone()
                                    };
                                    self.spell_list(spells, level, true, Some((&label, n)), out);
                                }
                            }
                            other => self
                                .partial
                                .push(format!("innate spell mode {other:?} was not converted")),
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// Convert an `additionalSpells` array. `label` names the spellcasting source (the race,
/// feat or class name).
#[must_use]
pub fn convert(sh: &Shared, label: &str, groups: &[Value]) -> SpellRules {
    let mut out = SpellRules::default();
    for group in groups {
        let name = group.get("name").and_then(Value::as_str);
        let group_label = name.map_or_else(|| label.to_string(), |n| format!("{label}: {n}"));
        let (ability, choose_ability) = match group.get("ability") {
            Some(Value::String(a)) => (core_ids::ability_name(a).map(str::to_string), None),
            Some(v) => (
                None,
                v.get("choose").and_then(Value::as_array).map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().and_then(core_ids::ability_stat))
                        .collect::<Vec<_>>()
                }),
            ),
            None => (None, None),
        };
        let mut ctx = Ctx {
            sh,
            label: group_label.clone(),
            ability,
            partial: Vec::new(),
        };
        let mut rules = Vec::new();
        if let Some(options) = choose_ability.filter(|o| !o.is_empty()) {
            rules.push(custom(
                "select.spell_ability",
                json!({"name": format!("Spellcasting Ability ({group_label})"), "options": options, "spellcasting": group_label}),
            ));
        }
        if let Some(v) = group.get("known") {
            ctx.level_map(v, false, &mut rules);
        }
        if let Some(v) = group.get("prepared") {
            ctx.level_map(v, true, &mut rules);
        }
        if let Some(v) = group.get("innate") {
            ctx.innate(v, &mut rules);
        }
        if let Some(Value::Object(exp)) = group.get("expanded") {
            for (_, list) in exp {
                if let Some(items) = list.as_array() {
                    for s in items.iter().filter_map(Value::as_str) {
                        out.expanded.push(ctx.spell_id(&parse_spell_ref(s)));
                    }
                }
            }
        }
        out.partial.extend(ctx.partial);
        match name {
            Some(n) => out.options.push(SpellOption {
                name: title_case(n),
                rules,
            }),
            None => out.direct.extend(rules),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared() -> Shared {
        let mut sh = Shared::default();
        sh.links.insert(
            (RefKind::Spell, "thaumaturgy".into(), "XPHB".into()),
            "ID_5ET_XPHB_SPELL_THAUMATURGY".into(),
        );
        sh
    }

    fn grant_ids(rules: &[Rule]) -> Vec<(String, Option<u32>)> {
        rules
            .iter()
            .filter_map(|r| match r {
                Rule::Grant(g) => Some((g.id.clone(), g.level)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn spell_refs_and_filters() {
        let r = parse_spell_ref("dancing lights|xphb#c");
        assert_eq!(
            r,
            SpellRef {
                name: "dancing lights".into(),
                source: "XPHB".into(),
                cast_level: None,
                cantrip: true
            }
        );
        let r = parse_spell_ref("hellish rebuke#2");
        assert_eq!((r.source.as_str(), r.cast_level), ("PHB", Some(2)));
        assert_eq!(
            filter_to_supports("level=0|class=Wizard").as_deref(),
            Some("0&&Wizard")
        );
        assert_eq!(
            filter_to_supports("level=0|class=cleric;wizard").as_deref(),
            Some("0&&(Cleric||Wizard)")
        );
        assert_eq!(
            filter_to_supports("level=1;2|school=A").as_deref(),
            Some("(1||2)&&Abjuration")
        );
        assert!(filter_to_supports("level=!0").is_none());
        assert!(filter_to_supports("components & miscellaneous=ritual").is_none());
        assert_eq!(school_name("V"), "Evocation");
        assert_eq!(school_name("?"), "Unknown");
    }

    #[test]
    fn tiefling_style_direct_group() {
        let sh = shared();
        let r = convert(
            &sh,
            "Tiefling",
            &[
                json!({"ability": "cha", "known": {"1": ["thaumaturgy#c"]}, "innate": {"3": {"daily": {"1": ["hellish rebuke#2"]}}, "5": {"daily": {"1": ["darkness"]}}}}),
            ],
        );
        assert!(r.options.is_empty());
        let ids = grant_ids(&r.direct);
        assert_eq!(ids[0], ("ID_5ET_PHB_SPELL_THAUMATURGY".to_string(), None));
        assert_eq!(
            ids[1],
            ("ID_5ET_PHB_SPELL_HELLISH_REBUKE".to_string(), Some(3))
        );
        assert_eq!(ids[2], ("ID_5ET_PHB_SPELL_DARKNESS".to_string(), Some(5)));
        let Rule::Grant(g) = &r.direct[1] else {
            panic!()
        };
        assert_eq!(g.spellcasting.as_deref(), Some("Tiefling"));
        assert_eq!(g.prepared, Some(true));
        let cast = g.setters.iter().find(|s| s.name == "cast").unwrap();
        assert_eq!(cast.value, "daily");
        assert_eq!(cast.attribute("uses"), Some("1"));
        assert!(g
            .setters
            .iter()
            .any(|s| s.name == "cast level" && s.value == "2"));
        assert!(g
            .setters
            .iter()
            .any(|s| s.name == "spellcasting ability" && s.value == "Charisma"));
    }

    #[test]
    fn lineage_groups_become_options_with_ability_choice() {
        let sh = shared();
        let r = convert(
            &sh,
            "Elf",
            &[
                json!({"name": "Drow", "ability": {"choose": ["int", "wis", "cha"]},
                       "known": {"1": ["dancing lights|xphb#c"]},
                       "innate": {"3": {"daily": {"1": ["faerie fire|xphb"]}}}}),
                json!({"name": "High Elf", "ability": {"choose": ["int", "wis", "cha"]},
                       "known": {"1": {"_": [{"choose": "level=0|class=Wizard"}]}}}),
            ],
        );
        assert!(r.direct.is_empty());
        assert_eq!(r.options.len(), 2);
        assert_eq!(r.options[0].name, "Drow");
        let Rule::Custom(c) = &r.options[0].rules[0] else {
            panic!()
        };
        assert_eq!(c.kind, "select.spell_ability");
        assert_eq!(
            c.fields["options"],
            json!(["intelligence", "wisdom", "charisma"])
        );
        assert_eq!(grant_ids(&r.options[0].rules).len(), 2);
        let Rule::Select(s) = &r.options[1].rules[1] else {
            panic!()
        };
        assert_eq!(s.supports.as_deref(), Some("0&&Wizard"));
        assert_eq!(s.ty, "Spell");
        assert_eq!(s.spellcasting.as_deref(), Some("Elf: High Elf"));
    }

    #[test]
    fn at_will_ritual_choices_expanded_and_unsupported() {
        let sh = shared();
        let r = convert(
            &sh,
            "Feat",
            &[json!({
                "innate": {"_": {"will": ["light"], "ritual": ["detect magic"], "daily": {"1": [{"choose": "level=1|class=Cleric"}], "2e": ["bless"]}}},
                "known": {"_": [{"choose": "level=0|class=Cleric", "count": 2}, {"choose": "level=!0"}], "s1": ["x"]},
                "prepared": {"3": ["hold person"]},
                "expanded": {"s1": ["burning hands"]},
            })],
        );
        let light = r
            .direct
            .iter()
            .find_map(|x| match x {
                Rule::Grant(g) if g.id.ends_with("LIGHT") => Some(g),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            light
                .setters
                .iter()
                .find(|s| s.name == "cast")
                .unwrap()
                .value,
            "at will"
        );
        assert!(r.direct.iter().any(|x| matches!(x, Rule::Select(s) if s.number == Some(2) && s.supports.as_deref() == Some("0&&Cleric"))));
        assert!(r.direct.iter().any(|x| matches!(x, Rule::Grant(g) if g.id.ends_with("BLESS") && g.setters.iter().any(|s| s.value == "daily each" && s.attribute("uses") == Some("2")))));
        assert!(r.direct.iter().any(|x| matches!(x, Rule::Grant(g) if g.id.ends_with("HOLD_PERSON") && g.level == Some(3) && g.prepared == Some(true))));
        assert!(r.partial.iter().any(|p| p.contains("level=!0")));
        assert!(r.partial.iter().any(|p| p.contains("s1")));
        assert_eq!(r.expanded, vec!["ID_5ET_PHB_SPELL_BURNING_HANDS"]);
    }
}
