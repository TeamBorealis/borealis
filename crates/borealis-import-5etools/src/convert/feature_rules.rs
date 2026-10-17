//! Choices that 5etools keeps in the prose of a class feature, converted the way the bundled SRD
//! classes express them:
//!
//! - skill Expertise from a fixed list (2024 Wizard *Scholar*): a `Class Feature` select over the
//!   core Expertise options of those skills (each requires the skill proficiency; optional, as
//!   a multiclass character may have none of them);
//! - spells chosen from the spellbook at fixed spell levels (Wizard *Spell Mastery*, *Signature
//!   Spells*): always-prepared `Spell` selects of the class's spellcasting;
//! - Weapon Mastery (2024 martial classes): the `weapon_mastery` rule, its count from the class
//!   table's "Weapon Mastery" column ([`MASTERY_KEY`] in `Class.data.progression`) or the number
//!   the text states.

use super::build::{custom, select, stat};
use crate::core_ids;
use borealis_content::Rule;
use regex::Regex;
use serde_json::{json, Value};

/// Progression key (and lower-cased class table column) of the Weapon Mastery count.
pub const MASTERY_KEY: &str = "weapon_mastery";
/// Class table column holding the Weapon Mastery count.
const MASTERY_COLUMN: &str = "weapon mastery";
/// Prefix of the core skill Expertise options (`ID_EXPERTISE_SKILL_ARCANA`).
const EXPERTISE_PREFIX: &str = "ID_EXPERTISE_SKILL_";
const PROFICIENCY_PREFIX: &str = "ID_PROFICIENCY_SKILL_";

/// What a feature's rules need to know about it.
pub struct FeatureText<'a> {
    /// Feature name.
    pub name: &'a str,
    /// Owning class name (`Wizard`).
    pub class: &'a str,
    /// Class level the feature is gained at.
    pub level: u32,
    /// Plain text of the feature entries.
    pub text: &'a str,
}

fn number_word(word: &str) -> Option<u32> {
    match word {
        "a" | "an" | "one" => Some(1),
        "two" => Some(2),
        "three" => Some(3),
        "four" => Some(4),
        other => other.parse().ok(),
    }
}

/// The rules of a class feature stated in its text; empty when it states none of the choices.
#[must_use]
pub fn feature_rules(f: &FeatureText<'_>) -> Vec<Rule> {
    let lower = f.text.to_lowercase();
    let mut rules = Vec::new();
    rules.extend(skill_expertise(f, &lower));
    rules.extend(spellbook_choices(f, &lower));
    rules.extend(weapon_mastery(&lower));
    rules.extend(half_proficiency(&lower));
    rules
}

/// Skills by the ability they use (Constitution has none).
const SKILL_ABILITIES: [(&str, &[&str]); 5] = [
    ("strength", &["athletics"]),
    ("dexterity", &["acrobatics", "sleight of hand", "stealth"]),
    (
        "intelligence",
        &["arcana", "history", "investigation", "nature", "religion"],
    ),
    (
        "wisdom",
        &[
            "animal handling",
            "insight",
            "medicine",
            "perception",
            "survival",
        ],
    ),
    (
        "charisma",
        &["deception", "intimidation", "performance", "persuasion"],
    ),
];

/// "add half your proficiency bonus (round up) to any Strength, Dexterity, or Constitution
/// check you make that doesn't already use your proficiency bonus" (Remarkable Athlete; Jack of
/// All Trades says "any ability check"): half proficiency on each skill of those abilities the
/// character is not proficient in, the shape of the bundled SRD classes. `lower` is the
/// lower-cased plain text.
#[must_use]
pub fn half_proficiency(lower: &str) -> Vec<Rule> {
    let re = Regex::new(
        r"add half your proficiency bonus,? \(?(round(?:ed)? (?:up|down))?\)?,? to any ([a-z, ]+?) checks? you make that doesn['’]t already (?:use|include) your proficiency bonus",
    );
    let Some(caps) = re.ok().and_then(|re| re.captures(lower)) else {
        return Vec::new();
    };
    let value = if caps.get(1).is_some_and(|m| m.as_str().ends_with("up")) {
        "proficiency:half:up"
    } else {
        "proficiency:half"
    };
    let phrase = &caps[2];
    let every = phrase.trim() == "ability";
    SKILL_ABILITIES
        .iter()
        .filter(|(ability, _)| every || phrase.contains(ability))
        .flat_map(|(_, skills)| skills.iter())
        .filter_map(|skill| {
            let id = core_ids::skill_id(skill)?;
            Some(Rule::Stat(borealis_content::StatRule {
                requirements: Some(format!("!{id}")),
                ..stat(&format!("{skill}:misc"), value)
            }))
        })
        .collect()
}

/// "Choose one of the following skills in which you have proficiency: A, B, or C. You have
/// Expertise in the chosen skill."
fn skill_expertise(f: &FeatureText<'_>, lower: &str) -> Option<Rule> {
    if !lower.contains("expertise in the chosen skill") {
        return None;
    }
    let re = Regex::new(
        r"choose (\w+) of the following skills in which you have proficiency: ([^.]+)\.",
    )
    .ok()?;
    let caps = re.captures(lower)?;
    let count = number_word(&caps[1])?;
    let ids: Vec<String> = caps[2]
        .split(',')
        .flat_map(|p| p.split(" or "))
        .flat_map(|p| p.split(" and "))
        .map(|p| {
            p.trim()
                .trim_start_matches("or ")
                .trim_start_matches("and ")
        })
        .filter(|p| !p.is_empty())
        .map(|p| core_ids::skill_id(p).map(|id| id.replace(PROFICIENCY_PREFIX, EXPERTISE_PREFIX)))
        .collect::<Option<_>>()?;
    (ids.len() > 1).then(|| {
        let mut s = select(
            "Class Feature",
            &format!("Expertise ({})", f.name),
            Some(&ids.join("|")),
            count,
        );
        // a character proficient in none of the skills (multiclass) gains nothing
        s.optional = Some(true);
        Rule::Select(s)
    })
}

/// "Choose a 1st-level wizard spell and a 2nd-level wizard spell that are in your spellbook",
/// "Choose two level 3 spells in your spellbook": one always-prepared select per spell level.
fn spellbook_choices(f: &FeatureText<'_>, lower: &str) -> Vec<Rule> {
    let (Ok(sentence), Ok(pick)) = (
        Regex::new(r"choose ([^.]*?) in your spellbook"),
        Regex::new(r"\b(a|an|one|two|three|four) (?:level (\d)|(\d)(?:st|nd|rd|th)-level)\b"),
    ) else {
        return Vec::new();
    };
    let Some(caps) = sentence.captures(lower) else {
        return Vec::new();
    };
    let mut rules = Vec::new();
    for p in pick.captures_iter(&caps[1]) {
        let (Some(count), Some(spell_level)) = (
            number_word(&p[1]),
            p.get(2).or_else(|| p.get(3)).map(|m| m.as_str()),
        ) else {
            continue;
        };
        let mut s = select(
            "Spell",
            &format!("{} (level {spell_level})", f.name),
            Some(&format!("$(spellcasting:list), {spell_level}")),
            count,
        );
        s.level = (f.level > 1).then_some(f.level);
        s.spellcasting = Some(f.class.to_string());
        s.prepared = Some(true);
        rules.push(Rule::Select(s));
    }
    rules
}

/// "use the mastery properties of three kinds of Simple or Martial weapons of your choice".
fn weapon_mastery(lower: &str) -> Option<Rule> {
    let re = Regex::new(r"mastery properties of (\w+) kinds? of ([^.]*?)weapons").ok()?;
    let caps = re.captures(lower)?;
    let kinds = &caps[2];
    let count = if lower.contains(&format!("{MASTERY_COLUMN} column")) {
        json!({"type": "table", "key": MASTERY_KEY})
    } else {
        json!({"type": "fixed", "value": number_word(&caps[1])?})
    };
    let mut filter = json!({
        "weapon_kind": if kinds.contains("melee") { "melee" } else { "any" },
    });
    if lower.contains("weapons with which you have proficiency") {
        filter["proficient"] = Value::Bool(true);
    } else {
        let categories: Vec<&str> = [("simple", "Simple"), ("martial", "Martial")]
            .into_iter()
            .filter(|(k, _)| kinds.contains(k))
            .map(|(_, c)| c)
            .collect();
        if !categories.is_empty() {
            filter["categories"] = json!(categories);
        }
    }
    Some(custom(
        "weapon_mastery",
        json!({"name": "Weapon Mastery", "count": count, "filter": filter, "change_on": "long_rest"}),
    ))
}

/// `Class.data.progression` entries read from the class table columns (`weapon_mastery`).
#[must_use]
pub fn table_progression(groups: &[Value]) -> Option<Value> {
    for g in groups {
        let (Some(labels), Some(rows)) = (
            g.get("colLabels").and_then(Value::as_array),
            g.get("rows").and_then(Value::as_array),
        ) else {
            continue;
        };
        let Some(ci) = labels.iter().position(|l| {
            l.as_str()
                .is_some_and(|s| crate::text::strip_tags(s).eq_ignore_ascii_case(MASTERY_COLUMN))
        }) else {
            continue;
        };
        let values: Option<Vec<i64>> = rows
            .iter()
            .map(|r| {
                let cell = r.get(ci)?;
                cell.as_i64()
                    .or_else(|| cell.as_str().and_then(|s| s.trim().parse().ok()))
            })
            .collect();
        return values.map(|v| json!({ MASTERY_KEY: v }));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(rules: &[Rule]) -> Vec<(String, String, Option<String>)> {
        rules
            .iter()
            .filter_map(|r| match r {
                Rule::Stat(s) => Some((s.name.clone(), s.value.clone(), s.requirements.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn half_proficiency_on_the_skills_of_the_named_abilities() {
        let athlete = half_proficiency(
            "starting at 7th level, you can add half your proficiency bonus (round up) to any strength, dexterity, or constitution check you make that doesn't already use your proficiency bonus.",
        );
        let got = stats(&athlete);
        assert_eq!(got.len(), 4, "{got:?}");
        assert!(got.contains(&(
            "athletics:misc".into(),
            "proficiency:half:up".into(),
            Some("!ID_PROFICIENCY_SKILL_ATHLETICS".into())
        )));
        assert!(got.iter().any(|(n, _, _)| n == "sleight of hand:misc"));
        let jack = half_proficiency(
            "you can add half your proficiency bonus, rounded down, to any ability check you make that doesn't already include your proficiency bonus.",
        );
        let got = stats(&jack);
        assert_eq!(got.len(), 18);
        assert!(got.iter().all(|(_, v, _)| v == "proficiency:half"));
        assert!(half_proficiency("you gain proficiency in athletics.").is_empty());
    }

    fn rules(name: &str, level: u32, text: &str) -> Vec<Rule> {
        feature_rules(&FeatureText {
            name,
            class: "Wizard",
            level,
            text,
        })
    }

    fn selects(rules: &[Rule]) -> Vec<(String, String, u32, Option<u32>)> {
        rules
            .iter()
            .filter_map(|r| match r {
                Rule::Select(s) => Some((
                    s.ty.clone(),
                    s.supports.clone().unwrap_or_default(),
                    s.number.unwrap_or(1),
                    s.level,
                )),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn scholar_expertise_is_limited_to_the_listed_skills() {
        let r = rules(
            "Scholar",
            2,
            "Choose one of the following skills in which you have proficiency: Arcana, History, \
             Investigation, Medicine, Nature, or Religion. You have Expertise in the chosen skill.",
        );
        let s = selects(&r);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].0, "Class Feature");
        assert!(matches!(&r[0], Rule::Select(s) if s.optional == Some(true)));
        assert_eq!(
            s[0].1,
            "ID_EXPERTISE_SKILL_ARCANA|ID_EXPERTISE_SKILL_HISTORY|ID_EXPERTISE_SKILL_INVESTIGATION|\
             ID_EXPERTISE_SKILL_MEDICINE|ID_EXPERTISE_SKILL_NATURE|ID_EXPERTISE_SKILL_RELIGION"
        );
    }

    #[test]
    fn spell_mastery_and_signature_spells_choose_spellbook_spells() {
        let m2014 = rules(
            "Spell Mastery",
            18,
            "At 18th level, you have achieved such mastery over certain spells that you can cast \
             them at will. Choose a 1st-level wizard spell and a 2nd-level wizard spell that are \
             in your spellbook.",
        );
        let m2024 = rules(
            "Spell Mastery",
            18,
            "Choose a level 1 and a level 2 spell in your spellbook that have a casting time of \
             an action.",
        );
        for r in [&m2014, &m2024] {
            assert_eq!(
                selects(r),
                vec![
                    (
                        "Spell".into(),
                        "$(spellcasting:list), 1".into(),
                        1,
                        Some(18)
                    ),
                    (
                        "Spell".into(),
                        "$(spellcasting:list), 2".into(),
                        1,
                        Some(18)
                    ),
                ]
            );
        }
        let sig = rules(
            "Signature Spells",
            20,
            "Choose two level 3 spells in your spellbook as your signature spells.",
        );
        assert_eq!(
            selects(&sig),
            vec![(
                "Spell".into(),
                "$(spellcasting:list), 3".into(),
                2,
                Some(20)
            )]
        );
        assert!(matches!(&sig[0], Rule::Select(s)
            if s.prepared == Some(true) && s.spellcasting.as_deref() == Some("Wizard")));
        let savant = rules(
            "Evocation Savant",
            3,
            "Choose two Wizard spells from the Evocation school, each of which must be no higher \
             than level 2, and add them to your spellbook for free.",
        );
        assert!(savant.is_empty(), "not a choice of spellbook spells");
    }

    #[test]
    fn weapon_mastery_reads_the_table_column_or_the_stated_count() {
        let fighter = weapon_mastery(
            &"Your training with weapons allows you to use the mastery properties of three kinds \
              of Simple or Martial weapons of your choice. ... as shown in the Weapon Mastery \
              column of the Fighter Features table."
                .to_lowercase(),
        );
        let Some(Rule::Custom(c)) = fighter else {
            panic!("weapon_mastery rule");
        };
        assert_eq!(
            c.fields["count"],
            json!({"type": "table", "key": "weapon_mastery"})
        );
        assert_eq!(
            c.fields["filter"],
            json!({"weapon_kind": "any", "categories": ["Simple", "Martial"]})
        );
        let rogue = weapon_mastery(
            "you can use the mastery properties of two kinds of weapons with which you have \
             proficiency, such as daggers and shortbows.",
        );
        let Some(Rule::Custom(c)) = rogue else {
            panic!("weapon_mastery rule");
        };
        assert_eq!(c.fields["count"], json!({"type": "fixed", "value": 2}));
        assert_eq!(
            c.fields["filter"],
            json!({"weapon_kind": "any", "proficient": true})
        );
    }

    #[test]
    fn mastery_column_becomes_a_progression() {
        let groups = [json!({"colLabels": ["Second Wind", "Weapon Mastery"],
            "rows": [["2", "3"], ["2", "4"]]})];
        assert_eq!(
            table_progression(&groups),
            Some(json!({"weapon_mastery": [3, 4]}))
        );
        assert_eq!(
            table_progression(&[json!({"colLabels": ["Rages"], "rows": [[2]]})]),
            None
        );
    }
}
