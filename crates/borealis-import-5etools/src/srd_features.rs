//! Mechanics of the bundled SRD class and subclass features, reused for the same features of a
//! 5etools class.
//!
//! 5etools states many class features in prose only (Unarmored Defense, Fast Movement,
//! Expertise, Diamond Soul, Divine Order...). The bundled SRD packs model those exact features
//! with rules. A 5etools feature that is the SRD feature (same ruleset, class, subclass and
//! feature name, printed in its class's own book) takes the SRD feature's rules, provided every
//! one of them is a mechanic the importer can carry over unchanged ([`portable`]): ability
//! scores and their caps, AC formulas, hit points, speeds, saving throw and skill modifiers,
//! initiative, proficiency and language grants, and proficiency / language / option /
//! expertise choices. Deterministic, never AI (IM1); the SRD element id is recorded on the
//! imported feature (`data.srd_feature`).

use borealis_content::{ContentFile, Element, Rule, Ruleset};
use serde_json::Value;
use std::collections::BTreeMap;

/// Ability scores (lower case).
const ABILITIES: [&str; 6] = [
    "strength",
    "dexterity",
    "constitution",
    "intelligence",
    "wisdom",
    "charisma",
];

/// Skill names (lower case), for `<skill>:misc`.
const SKILLS: [&str; 18] = [
    "acrobatics",
    "animal handling",
    "arcana",
    "athletics",
    "deception",
    "history",
    "insight",
    "intimidation",
    "investigation",
    "medicine",
    "nature",
    "perception",
    "performance",
    "persuasion",
    "religion",
    "sleight of hand",
    "stealth",
    "survival",
];

/// Grant types carried over.
const GRANT_TYPES: [&str; 3] = ["Proficiency", "Language", "Condition"];
/// Select types carried over (their options are core or SRD elements).
const SELECT_TYPES: [&str; 4] = ["Proficiency", "Language", "Option", "Condition"];
/// Supports of the core skill Expertise options.
const EXPERTISE_SUPPORT: &str = "ID_INTERNAL_SUPPORT_EXPERTISE";
/// Custom rule kinds carried over (engine kinds the bundled SRD 5.2.1 classes use).
const CUSTOM_KINDS: [&str; 4] = [
    "expertise",
    "skill_bonus_unproficient",
    "aura",
    "select_param",
];

/// One SRD feature: its id, the class level it is gained at and its rules.
#[derive(Debug, Clone, PartialEq)]
pub struct SrdFeature {
    /// Element id in the SRD pack.
    pub id: String,
    /// Class level the SRD class grants it at.
    pub level: u32,
    /// The feature's rules.
    pub rules: Vec<Rule>,
}

impl SrdFeature {
    /// The rules that belong to a 5etools feature gained at `level` when the class prints the
    /// same-named feature at `levels` (5etools repeats "Expertise" at levels 1 and 6 where the
    /// SRD has one feature with two level-gated selects): rules gained from `level` up to the
    /// next printing; the first printing also takes the rules gained before it.
    #[must_use]
    pub fn rules_for(&self, level: u32, levels: &[u32]) -> Vec<Rule> {
        let first = levels.iter().min().map_or(true, |m| level <= *m);
        let next = levels.iter().filter(|l| **l > level).min().copied();
        self.rules
            .iter()
            .filter(|r| {
                let at = rule_level(r).unwrap_or(self.level);
                (first || at >= level) && next.map_or(true, |n| at < n)
            })
            .cloned()
            .collect()
    }
}

/// The level a rule is gated at, if any.
fn rule_level(rule: &Rule) -> Option<u32> {
    match rule {
        Rule::Stat(s) => s.level,
        Rule::Grant(g) => g.level,
        Rule::Select(s) => s.level,
        Rule::Custom(c) => c
            .fields
            .get("level")
            .and_then(Value::as_u64)
            .and_then(|l| u32::try_from(l).ok()),
        Rule::Unknown(_) => None,
    }
}

/// True for a stat the importer may carry over: a character statistic (ability score or cap,
/// AC formula part, hit points, a speed, a save or skill modifier, initiative).
fn statistic(name: &str) -> bool {
    let n = name.to_lowercase();
    let speed = |s: &str| s == "speed" || s.starts_with("speed:");
    n == "ac"
        || n.starts_with("ac:")
        || n == "hp"
        || speed(&n)
        || n.strip_prefix("innate ").is_some_and(speed)
        || n == "initiative"
        || n.starts_with("initiative:")
        || ABILITIES
            .iter()
            .any(|a| n == *a || n == format!("{a}:max") || n == format!("{a}:save:misc"))
        || SKILLS.iter().any(|s| n == format!("{s}:misc"))
}

/// True when every rule of an SRD feature is portable: statistics, helper stats that feed
/// them (or are namespaced by the feature name), proficiency / language / condition grants,
/// proficiency / language / option / condition / expertise selects and the listed engine
/// kinds. One rule outside that set keeps the whole feature out (no partial copies).
#[must_use]
pub fn portable(feature_name: &str, rules: &[Rule]) -> bool {
    let own = format!("{}:", feature_name.to_lowercase());
    let fed: Vec<String> = rules
        .iter()
        .filter_map(|r| match r {
            Rule::Stat(s) if statistic(&s.name) => Some(s.value.to_lowercase()),
            _ => None,
        })
        .collect();
    !rules.is_empty()
        && rules.iter().all(|r| match r {
            Rule::Stat(s) => {
                let n = s.name.to_lowercase();
                !n.contains(":spellcasting")
                    && (statistic(&n) || n.starts_with(&own) || fed.contains(&n))
            }
            Rule::Grant(g) => g.ty.as_deref().is_some_and(|t| GRANT_TYPES.contains(&t)),
            Rule::Select(s) => {
                SELECT_TYPES.contains(&s.ty.as_str())
                    || (s.ty == "Class Feature" && s.supports.as_deref() == Some(EXPERTISE_SUPPORT))
            }
            Rule::Custom(c) => CUSTOM_KINDS.contains(&c.kind.as_str()),
            Rule::Unknown(_) => false,
        })
}

fn normalize(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// `(ruleset, class, subclass, feature)`, normalised; the subclass is empty for class features.
type Key = (Ruleset, String, String, String);

/// SRD class and subclass features by identity. A key printed twice is ambiguous and left out.
#[derive(Debug, Default, Clone)]
pub struct SrdFeatures {
    map: BTreeMap<Key, Option<SrdFeature>>,
    /// Statistic stats of the SRD feats, by `(ruleset, normalised name)`.
    feats: BTreeMap<(Ruleset, String), Vec<Rule>>,
}

impl SrdFeatures {
    /// The SRD class feature `name` of `class`.
    #[must_use]
    pub fn class_feature(&self, ruleset: Ruleset, class: &str, name: &str) -> Option<&SrdFeature> {
        self.map
            .get(&(ruleset, normalize(class), String::new(), normalize(name)))?
            .as_ref()
    }

    /// The SRD subclass feature `name` of the SRD subclass of `class` whose name contains the
    /// 5etools subclass short name (`Draconic` in `Draconic Bloodline`); none when several do.
    #[must_use]
    pub fn subclass_feature(
        &self,
        ruleset: Ruleset,
        class: &str,
        short_name: &str,
        name: &str,
    ) -> Option<&SrdFeature> {
        let (class, short, name) = (normalize(class), normalize(short_name), normalize(name));
        if short.is_empty() {
            return None;
        }
        let mut hits = self.map.iter().filter(|((rs, c, sub, n), _)| {
            *rs == ruleset && *c == class && !sub.is_empty() && sub.contains(&short) && *n == name
        });
        let (_, only) = hits.next()?;
        if hits.next().is_some() {
            return None;
        }
        only.as_ref()
    }

    /// The statistic stats of the SRD feat `name` (XPHB Alert's initiative bonus): the part of
    /// a feat 5etools states in prose only; its other fields convert from the 5etools entry.
    #[must_use]
    pub fn feat_statistics(&self, ruleset: Ruleset, name: &str) -> &[Rule] {
        self.feats
            .get(&(ruleset, normalize(name)))
            .map_or(&[], Vec::as_slice)
    }

    /// Number of indexed features.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// True if nothing is indexed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    fn insert(&mut self, key: Key, feature: SrdFeature) {
        self.map
            .entry(key)
            .and_modify(|e| *e = None)
            .or_insert(Some(feature));
    }

    /// Index the class and subclass features of one SRD content file. The class comes from the
    /// feature's `data.class` or the file's Class element; the subclass from `data.subclass` or
    /// the Archetype element that grants it; the level from `data.level` or the granting rule.
    pub fn index_file(&mut self, file: &ContentFile, pack_ruleset: Option<Ruleset>) {
        let class = file.elements.iter().find(|e| e.ty == "Class");
        let mut granted: BTreeMap<&str, (Option<&str>, u32)> = BTreeMap::new();
        for owner in file
            .elements
            .iter()
            .filter(|e| e.ty == "Class" || e.ty == "Archetype")
        {
            let sub = (owner.ty == "Archetype").then_some(owner.name.as_str());
            for r in &owner.rules {
                if let Rule::Grant(g) = r {
                    granted
                        .entry(g.id.as_str())
                        .or_insert((sub, g.level.unwrap_or(1)));
                }
            }
        }
        for e in file.elements.iter().filter(|e| e.ty == "Feat") {
            let Some(ruleset) = e.ruleset.or(file.ruleset).or(pack_ruleset) else {
                continue;
            };
            let stats: Vec<Rule> = e
                .rules
                .iter()
                .filter(|r| matches!(r, Rule::Stat(s) if statistic(&s.name)))
                .cloned()
                .collect();
            if !stats.is_empty() {
                self.feats.insert((ruleset, normalize(&e.name)), stats);
            }
        }
        for e in &file.elements {
            let is_sub = match e.ty.as_str() {
                "Class Feature" => false,
                "Archetype Feature" => true,
                _ => continue,
            };
            let Some(ruleset) = e.ruleset.or(file.ruleset).or(pack_ruleset) else {
                continue;
            };
            let data = |k: &str| e.data.get(k).and_then(Value::as_str);
            let Some(class_name) = data("class").or(class.map(|c| c.name.as_str())) else {
                continue;
            };
            let grant = granted.get(e.id.as_str());
            let sub = if is_sub {
                match data("subclass").or(grant.and_then(|g| g.0)) {
                    Some(s) => normalize(s),
                    None => continue,
                }
            } else {
                String::new()
            };
            let level = data_level(e).or(grant.map(|g| g.1)).unwrap_or(1);
            self.insert(
                (ruleset, normalize(class_name), sub, normalize(&e.name)),
                SrdFeature {
                    id: e.id.clone(),
                    level,
                    rules: e.rules.clone(),
                },
            );
        }
    }
}

fn data_level(e: &Element) -> Option<u32> {
    e.data
        .get("level")
        .and_then(Value::as_u64)
        .and_then(|l| u32::try_from(l).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(json: &str) -> Vec<Rule> {
        serde_json::from_str(json).expect("rules")
    }

    fn file(json: &str) -> ContentFile {
        ContentFile::from_json_str(json).expect("content file")
    }

    #[test]
    fn statistics_choices_and_engine_kinds_are_portable() {
        assert!(portable(
            "Unarmored Defense",
            &rules(
                r#"[{"kind":"stat","name":"ac:ud:base","value":"10"},
                {"kind":"stat","name":"ac:calculation","value":"ac:ud","bonus":"calculation"}]"#
            )
        ));
        assert!(portable(
            "Aura of Protection",
            &rules(
                r#"[{"kind":"stat","name":"aura of protection:range","value":"10"},
                {"kind":"stat","name":"aura of protection:save bonus","value":"charisma:modifier"},
                {"kind":"stat","name":"wisdom:save:misc","value":"aura of protection:save bonus"}]"#
            )
        ));
        assert!(portable(
            "Unarmored Movement",
            &rules(
                r#"[{"kind":"stat","name":"monk:unarmored movement","value":"10","level":2},
                {"kind":"stat","name":"innate speed:misc","value":"monk:unarmored movement"}]"#
            )
        ));
        assert!(portable(
            "Divine Order",
            &rules(
                r#"[{"kind":"select","type":"Option","name":"Divine Order","supports":"Divine Order"}]"#
            )
        ));
        assert!(portable(
            "Expertise",
            &rules(
                r#"[{"kind":"select","type":"Class Feature","name":"E","number":2,"supports":"ID_INTERNAL_SUPPORT_EXPERTISE"},
                {"kind":"expertise","count":2,"level":6}]"#
            )
        ));
        assert!(portable(
            "Diamond Soul",
            &rules(
                r#"[{"kind":"grant","id":"ID_PROFICIENCY_SAVINGTHROW_WISDOM","type":"Proficiency"}]"#
            )
        ));
    }

    #[test]
    fn anything_else_keeps_the_whole_feature_out() {
        // spell slots are converted from the class's own fields
        assert!(!portable(
            "Spellcasting",
            &rules(
                r#"[{"kind":"stat","name":"paladin:spellcasting:slots:1","value":"2","level":2}]"#
            )
        ));
        // a resource counter next to a statistic
        assert!(!portable(
            "Rage",
            &rules(
                r#"[{"kind":"stat","name":"barbarian rage:count","value":"2"},
                {"kind":"stat","name":"strength","value":"1"}]"#
            )
        ));
        assert!(!portable(
            "Ability Score Improvement",
            &rules(
                r#"[{"kind":"select","type":"Class Feature","name":"I","supports":"Improvement Option"}]"#
            )
        ));
        assert!(!portable(
            "Circle Spells",
            &rules(
                r#"[{"kind":"select_param","name":"Land","options":["A"]},{"kind":"grant_spell","spell":"X"}]"#
            )
        ));
        assert!(!portable("Nothing", &[]));
    }

    #[test]
    fn repeated_printings_split_the_rules_by_level() {
        let f = SrdFeature {
            id: "X".into(),
            level: 1,
            rules: rules(
                r#"[{"kind":"select","type":"Class Feature","name":"E1","level":1,"supports":"ID_INTERNAL_SUPPORT_EXPERTISE"},
                    {"kind":"select","type":"Class Feature","name":"E6","level":6,"supports":"ID_INTERNAL_SUPPORT_EXPERTISE"},
                    {"kind":"stat","name":"strength","value":"1"}]"#,
            ),
        };
        let names = |rs: Vec<Rule>| -> Vec<String> {
            rs.iter()
                .map(|r| match r {
                    Rule::Select(s) => s.name.clone(),
                    Rule::Stat(s) => s.name.clone(),
                    _ => String::new(),
                })
                .collect()
        };
        assert_eq!(names(f.rules_for(1, &[1, 6])), ["E1", "strength"]);
        assert_eq!(names(f.rules_for(6, &[1, 6])), ["E6"]);
        assert_eq!(names(f.rules_for(1, &[1])).len(), 3);
    }

    #[test]
    fn features_index_by_class_subclass_and_level() {
        let mut idx = SrdFeatures::default();
        idx.index_file(
            &file(
                r#"{"format":"borealis-content/1","ruleset":"dnd2014","elements":[
                {"id":"C","name":"Barbarian","type":"Class","source":"S","rules":[
                    {"kind":"grant","id":"F","type":"Class Feature","level":5}]},
                {"id":"A","name":"Path of the Berserker","type":"Archetype","source":"S","rules":[
                    {"kind":"grant","id":"G","type":"Archetype Feature","level":3}]},
                {"id":"F","name":"Fast Movement","type":"Class Feature","source":"S","rules":[]},
                {"id":"G","name":"Frenzy","type":"Archetype Feature","source":"S","rules":[]},
                {"id":"H","name":"Orphan","type":"Archetype Feature","source":"S","rules":[]}]}"#,
            ),
            None,
        );
        let fm = idx
            .class_feature(Ruleset::Dnd2014, "barbarian", "Fast Movement")
            .expect("class feature");
        assert_eq!((fm.id.as_str(), fm.level), ("F", 5));
        assert!(idx
            .class_feature(Ruleset::Dnd2024, "Barbarian", "Fast Movement")
            .is_none());
        let fr = idx
            .subclass_feature(Ruleset::Dnd2014, "Barbarian", "Berserker", "Frenzy")
            .expect("subclass feature");
        assert_eq!((fr.id.as_str(), fr.level), ("G", 3));
        assert!(idx
            .subclass_feature(Ruleset::Dnd2014, "Barbarian", "Zealot", "Frenzy")
            .is_none());
        assert_eq!(
            idx.len(),
            2,
            "a subclass feature no subclass grants is left out"
        );
        // a second printing of the same key makes it ambiguous
        idx.index_file(
            &file(
                r#"{"format":"borealis-content/1","ruleset":"dnd2014","elements":[
                {"id":"F2","name":"Fast Movement","type":"Class Feature","source":"S","data":{"class":"Barbarian"}}]}"#,
            ),
            None,
        );
        assert!(idx
            .class_feature(Ruleset::Dnd2014, "Barbarian", "Fast Movement")
            .is_none());
    }

    #[test]
    fn bundled_srd_packs_index_the_class_features() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let idx = crate::srd::SrdIndex::load(&[root.join("srd-5.1"), root.join("srd-5.2.1")]);
        let f = idx.features();
        let ud = f
            .class_feature(Ruleset::Dnd2014, "Barbarian", "Unarmored Defense")
            .expect("SRD 5.1 Unarmored Defense");
        assert!(portable("Unarmored Defense", &ud.rules));
        let dr = f
            .subclass_feature(
                Ruleset::Dnd2024,
                "Sorcerer",
                "Draconic",
                "Draconic Resilience",
            )
            .expect("SRD 5.2.1 Draconic Resilience");
        assert!(portable("Draconic Resilience", &dr.rules));
        assert_eq!(dr.level, 3);
        let alert = f.feat_statistics(Ruleset::Dnd2024, "Alert");
        assert!(
            matches!(alert, [Rule::Stat(s)] if s.name == "initiative" && s.value == "proficiency"),
            "{alert:?}"
        );
        assert!(f.feat_statistics(Ruleset::Dnd2014, "Alert").is_empty());
        let sc = f
            .class_feature(Ruleset::Dnd2014, "Paladin", "Spellcasting")
            .expect("SRD 5.1 Paladin Spellcasting");
        assert!(!portable("Spellcasting", &sc.rules));
    }
}
