//! More item rules: spell attack / save DC and proficiency bonus bonuses, the lifted Dex cap of
//! medium armor with `dexterityMax: null`, and the languages and proficiencies that items
//! flagged `grantsLanguage` / `grantsProficiency` state in their text (the flags carry no
//! list, so the printed sentence is read: "You can speak, read, and write Abyssal and
//! Infernal.", "you have proficiency with {@item smith's tools|PHB}").

use super::super::build::{stat, stat_bonus};
use super::super::mech::Mech;
use super::super::Shared;
use crate::core_ids;
use crate::model::Item;
use crate::text::strip_tags;
use borealis_content::Rule;
use regex::Regex;
use serde_json::{json, Value};
use std::sync::OnceLock;

use super::signed_value;

/// A Dex cap no modifier reaches: medium armor without a cap.
const NO_DEX_CAP: i64 = 10;

/// `bonusSpellAttack` / `bonusSpellSaveDc` / `bonusProficiencyBonus` as the statistics core
/// adds to every spellcasting block (`spellcasting:attack` / `spellcasting:dc`) and to the
/// proficiency bonus.
pub fn bonus_stats(it: &Item, mech: &mut Mech<'_>) {
    for (key, name) in [
        ("bonusSpellAttack", "spellcasting:attack"),
        ("bonusSpellSaveDc", "spellcasting:dc"),
        ("bonusProficiencyBonus", "proficiency"),
        // built-in groups: every ability check (skills, passives, initiative) and the
        // Constitution save to keep concentration
        ("bonusAbilityCheck", "ability checks"),
        ("bonusSavingThrowConcentration", "concentration:save"),
    ] {
        let n = match it.extra.get(key) {
            Some(Value::String(s)) => signed_value(s),
            Some(Value::Number(n)) => n.as_i64(),
            _ => None,
        };
        if let Some(n) = n {
            mech.push(Rule::Stat(stat(name, n)));
        }
    }
}

/// Medium armor with `dexterityMax: null` adds the whole Dex modifier (`raw` is the entry
/// before parsing, which drops `null` fields).
pub fn dex_cap(raw: &Value, code: &str, mech: &mut Mech<'_>) {
    if code == "MA" && raw.get("dexterityMax") == Some(&Value::Null) {
        mech.push(stat_bonus("ac:armored:dexterity:cap", NO_DEX_CAP, "base"));
    }
}

/// Every string of an entries tree, tags kept.
fn strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
        Value::Object(o) => {
            for k in ["entries", "items", "entry"] {
                if let Some(x) = o.get(k) {
                    strings(x, out);
                }
            }
        }
        _ => {}
    }
}

fn language_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"\b[Yy]ou (?:can |also )?(?:speak|know)(?:, read, and (?:write|understand)| and understand|, understand, and read)? ([A-Z][A-Za-z'-]*(?: [A-Z][A-Za-z'-]*)*(?:(?:,? and |, )[A-Z][A-Za-z'-]*(?: [A-Z][A-Za-z'-]*)*)*)",
        )
        .expect("language pattern")
    })
}

/// Languages a text says the wearer knows, by lower-case name (only names that are languages
/// of core or of the import).
fn languages_in(sh: &Shared, text: &str) -> Vec<String> {
    let plain = strip_tags(text);
    let mut out = Vec::new();
    for cap in language_re().captures_iter(&plain) {
        for name in cap[1].split([',']).flat_map(|p| p.split(" and ")) {
            let name = name.trim().trim_start_matches("and ").trim().to_lowercase();
            let known =
                core_ids::core_language_id(&name).is_some() || sh.languages.contains_key(&name);
            if known && !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

fn proficiency_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"\b(?:[Yy]ou (?:have|gain) proficiency (?:with|in)|considered proficient in) ([^.;]*)",
        )
        .expect("proficiency pattern")
    })
}

fn tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{@(item|skill) ([^|}]+)[^}]*\}").expect("tag pattern"))
}

/// Proficiencies a text grants: 5etools `skillProficiencies` / `toolProficiencies` /
/// `weaponProficiencies` shaped lists, for [`Mech`].
#[derive(Debug, Default, PartialEq)]
struct Granted {
    skills: Vec<Value>,
    tools: Vec<Value>,
    weapons: Vec<Value>,
}

fn proficiencies_in(text: &str) -> Granted {
    let mut g = Granted::default();
    for cap in proficiency_re().captures_iter(text) {
        let rest = &cap[1];
        let choice = rest.contains("of your choice") || rest.contains("one of");
        let mut skills: Vec<String> = Vec::new();
        let mut named = false;
        for t in tag_re().captures_iter(rest) {
            named = true;
            let name = t[2].trim().to_lowercase();
            if &t[1] == "skill" {
                skills.push(name);
            } else if core_ids::tool_prof_id(&name).is_some() {
                g.tools.push(json!({ name: true }));
            } else if core_ids::weapon_prof_id(&name).is_some() {
                g.weapons.push(json!({ name: true }));
            }
        }
        if choice && !skills.is_empty() {
            g.skills
                .push(json!({"choose": {"from": skills, "count": 1}}));
        } else {
            g.skills
                .extend(skills.into_iter().map(|s| json!({ s: true })));
        }
        if !named {
            // "proficiency with greatswords, and ...": one plain weapon name
            let plain = strip_tags(rest).to_lowercase();
            let word = plain
                .split([',', '('])
                .next()
                .unwrap_or("")
                .split(" and ")
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            let single = word.strip_suffix('s').unwrap_or(&word);
            if core_ids::weapon_prof_id(single).is_some() {
                g.weapons.push(json!({ single: true }));
            }
        }
    }
    g
}

/// "You are considered proficient with this armor", "you have proficiency with it": the item
/// makes its wearer or wielder proficient with itself (setter `proficient`).
#[must_use]
pub fn proficient_with_itself(it: &Item) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:considered (?:proficient|trained) with|you have proficiency with|you are proficient with) (?:this (?:armor|weapon|item|tool)|it|the sword)\b",
        )
        .expect("self proficiency pattern")
    });
    let mut texts = Vec::new();
    strings(&Value::Array(it.entries.clone()), &mut texts);
    texts.iter().any(|t| re.is_match(&strip_tags(t)))
}

/// Partial note of a listed proficiency choice whose text doubles the bonus (not modelled, as
/// in Aurora's own element of the same item).
const DOUBLED_NOT_APPLIED: &str =
    "the doubled proficiency bonus of the chosen proficiency is not applied";

fn listed_choice_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\byou gain (?:one|1) of the following proficiencies\b")
            .expect("listed proficiency choice pattern")
    })
}

/// One option of a listed proficiency choice: a skill or a tool (its id) or one kind of tool
/// "of your choice" (its tag). `None` for anything else.
fn listed_option(v: &Value) -> Option<String> {
    let text = v.as_str()?;
    let tags: Vec<_> = tag_re().captures_iter(text).collect();
    let [tag] = tags.as_slice() else {
        return None;
    };
    let name = tag[2].trim().to_lowercase();
    let kind = strip_tags(text).to_lowercase().contains("of your choice");
    match (&tag[1], kind) {
        ("skill", false) => core_ids::skill_id(&name),
        ("item", true) => Mech::tool_kind_tag(&name).map(str::to_string),
        ("item", false) => core_ids::tool_prof_id(&name),
        _ => None,
    }
}

/// "You gain one of the following proficiencies" followed by a list (ERLW Living Gloves): the
/// listed options, as Aurora's own element of that item offers them in one Proficiency select.
/// `None` when there is no such list or one of its options is not a skill, a tool or a kind of
/// tool (nothing is guessed).
fn listed_choice(entries: &[Value]) -> Option<Vec<String>> {
    for pair in entries.windows(2) {
        if let [Value::String(lead), list] = pair {
            if listed_choice_re().is_match(&strip_tags(lead))
                && list.get("type").and_then(Value::as_str) == Some("list")
            {
                let items = list.get("items")?.as_array()?;
                return items.iter().map(listed_option).collect();
            }
        }
    }
    entries
        .iter()
        .filter_map(|e| e.get("entries").and_then(Value::as_array))
        .find_map(|inner| listed_choice(inner))
}

/// Languages and proficiencies of items flagged `grantsLanguage` / `grantsProficiency`.
pub fn granted_rules(sh: &Shared, it: &Item, mech: &mut Mech<'_>, partial: &mut Vec<String>) {
    let lang = it.extra.get("grantsLanguage") == Some(&Value::Bool(true));
    let prof = it.extra.get("grantsProficiency") == Some(&Value::Bool(true));
    if !lang && !prof {
        return;
    }
    let mut texts = Vec::new();
    strings(&Value::Array(it.entries.clone()), &mut texts);
    if lang {
        let mut names: Vec<String> = Vec::new();
        for t in &texts {
            for n in languages_in(sh, t) {
                if !names.contains(&n) {
                    names.push(n);
                }
            }
        }
        if names.is_empty() {
            partial.push("the languages the item grants were not found in its text".into());
        } else {
            let obj: serde_json::Map<String, Value> =
                names.into_iter().map(|n| (n, json!(true))).collect();
            mech.languages(&[Value::Object(obj)]);
        }
    }
    if prof {
        let mut all = Granted::default();
        for t in &texts {
            let g = proficiencies_in(t);
            all.skills.extend(g.skills);
            all.tools.extend(g.tools);
            all.weapons.extend(g.weapons);
        }
        let listed = listed_choice(&it.entries);
        if let Some(options) = &listed {
            mech.proficiency_choice(options);
            if texts
                .iter()
                .any(|t| t.contains("double your proficiency bonus"))
            {
                partial.push(DOUBLED_NOT_APPLIED.into());
            }
        }
        if all == Granted::default() && listed.is_none() && !proficient_with_itself(it) {
            partial.push("the proficiencies the item grants were not found in its text".into());
        }
        mech.skills(&all.skills);
        mech.tools(&all.tools);
        mech.weapons(&all.weapons);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(raw: Value) -> Item {
        crate::model::parse(&raw).unwrap()
    }

    #[test]
    fn items_that_make_you_proficient_with_themselves() {
        for text in [
            "You are considered proficient with this armor even if you lack proficiency with medium armor.",
            "You are considered trained with this armor even if you lack training with Medium armor.",
            "You have proficiency with it, and it deals 1d8 slashing damage on a hit.",
            "You are considered proficient with the sword, and you have a +1 bonus.",
        ] {
            let it = item(json!({"name": "T", "source": "ZZZ", "entries": [text]}));
            assert!(proficient_with_itself(&it), "{text}");
        }
        let deck = item(json!({"name": "D", "source": "ZZZ",
            "entries": ["A magic weapon with which you are proficient appears."]}));
        assert!(!proficient_with_itself(&deck));
        // the wielder's own proficiencies, named in the text
        let (rules, partial) = rules_of(json!({"name": "Blade", "source": "ZZZ",
            "grantsProficiency": true, "entries": [
            "While attuned to it, you have proficiency with greatswords, and you can use Charisma."]}));
        assert!(partial.is_empty(), "{partial:?}");
        assert!(
            rules.iter().any(|r| matches!(r, Rule::Grant(g)
            if g.id == "ID_PROFICIENCY_WEAPON_PROFICIENCY_GREATSWORD")),
            "{rules:?}"
        );
        let (rules, partial) = rules_of(json!({"name": "Key", "source": "ZZZ",
            "grantsProficiency": true, "entries": [
            "When using the key, you are considered proficient in {@item thieves' tools|PHB}."]}));
        assert!(partial.is_empty(), "{partial:?}");
        assert!(
            rules.iter().any(|r| matches!(r, Rule::Grant(g)
            if g.id == "ID_PROFICIENCY_TOOL_PROFICIENCY_THIEVES_TOOLS")),
            "{rules:?}"
        );
    }

    fn rules_of(raw: Value) -> (Vec<Rule>, Vec<String>) {
        let sh = Shared::default();
        let mut mech = Mech::new(&sh, "x");
        let mut partial = Vec::new();
        let it = item(raw.clone());
        bonus_stats(&it, &mut mech);
        dex_cap(&raw, "MA", &mut mech);
        granted_rules(&sh, &it, &mut mech, &mut partial);
        (mech.rules, partial)
    }

    fn grants(rules: &[Rule], ty: &str) -> Vec<String> {
        rules
            .iter()
            .filter_map(|r| match r {
                Rule::Grant(g) if g.ty.as_deref() == Some(ty) => Some(g.id.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn spell_and_proficiency_bonuses_become_stats() {
        let (rules, _) = rules_of(json!({"name": "W", "source": "ZZZ",
            "bonusSpellAttack": "+2", "bonusSpellSaveDc": "+1", "bonusProficiencyBonus": "+1"}));
        let got: Vec<(String, String)> = rules
            .iter()
            .filter_map(|r| match r {
                Rule::Stat(s) => Some((s.name.clone(), s.value.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            got,
            [
                ("spellcasting:attack".to_string(), "2".to_string()),
                ("spellcasting:dc".into(), "1".into()),
                ("proficiency".into(), "1".into())
            ]
        );
    }

    #[test]
    fn medium_armor_without_a_cap() {
        let (rules, _) = rules_of(json!({"name": "S", "source": "ZZZ", "dexterityMax": null}));
        assert!(matches!(&rules[0], Rule::Stat(s) if s.name == "ac:armored:dexterity:cap"));
        let (rules, _) = rules_of(json!({"name": "S", "source": "ZZZ"}));
        assert!(rules.is_empty());
    }

    #[test]
    fn languages_from_the_text() {
        let (rules, partial) = rules_of(
            json!({"name": "B", "source": "ZZZ", "grantsLanguage": true,
            "entries": [
                "The weapon can speak, read, and understand Abyssal, Common, and Elvish.",
                {"type": "list", "items": ["You can speak, read, and write Dwarvish and Infernal."]},
                "You know Auran, you have {@variantrule Resistance|XPHB} to Lightning damage.",
                "You can speak Aquan fluently."
            ]}),
        );
        assert!(partial.is_empty(), "{partial:?}");
        let mut got = grants(&rules, "Language");
        got.sort();
        let mut want: Vec<String> = ["dwarvish", "infernal", "auran", "aquan"]
            .iter()
            .map(|n| core_ids::core_language_id(n).unwrap())
            .collect();
        want.sort();
        assert_eq!(got, want);
        let (_, partial) = rules_of(json!({"name": "B", "source": "ZZZ", "grantsLanguage": true,
            "entries": ["It hums."]}));
        assert_eq!(partial.len(), 1);
    }

    #[test]
    fn proficiencies_from_the_text() {
        let (rules, partial) = rules_of(
            json!({"name": "A", "source": "ZZZ", "grantsProficiency": true,
            "entries": [
                "You have proficiency with {@item Brewer's Supplies|XPHB}, {@item Mason's Tools|XPHB}, and {@item Smith's Tools|XPHB}.",
                "While wearing these bracers, you have proficiency with the {@item longbow|PHB} and {@item shortbow|PHB}.",
                "You gain proficiency in the {@skill History} skill.",
                "While attuned, you have proficiency with greatswords."
            ]}),
        );
        assert!(partial.is_empty(), "{partial:?}");
        let got = grants(&rules, "Proficiency");
        assert_eq!(got.len(), 7, "{got:?}");
        assert!(got.contains(&core_ids::skill_id("history").unwrap()));
        assert!(got.contains(&core_ids::weapon_prof_id("greatsword").unwrap()));
        let (rules, _) = rules_of(
            json!({"name": "A", "source": "ZZZ", "grantsProficiency": true,
            "entries": ["When you attune, you gain proficiency in one of the following skills of your choice: {@skill Arcana|XPHB}, {@skill History|XPHB}, or {@skill Nature|XPHB}."]}),
        );
        assert!(rules
            .iter()
            .any(|r| matches!(r, Rule::Select(s) if s.ty == "Proficiency")));
    }

    fn proficiency_selects(rules: &[Rule]) -> Vec<String> {
        rules
            .iter()
            .filter_map(|r| match r {
                Rule::Select(s) if s.ty == "Proficiency" => s.supports.clone(),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_listed_choice_of_proficiencies_is_one_select() {
        let (rules, partial) = rules_of(json!({"name": "G", "source": "ZZZ",
        "grantsProficiency": true, "entries": [
            "While you wear them, you gain one of the following proficiencies (chosen once):",
            {"type": "list", "items": [
                "{@skill Stealth}",
                "{@item Thieves' tools|PHB}",
                "One kind of {@item artisan's tools|PHB} of your choice",
                "One kind of {@item musical instrument|PHB} of your choice"
            ]},
            "A check with the chosen proficiency adds double your proficiency bonus."
        ]}));
        let want = [
            core_ids::skill_id("stealth").unwrap(),
            core_ids::tool_prof_id("thieves' tools").unwrap(),
            "Artisan tools".to_string(),
            "Musical Instrument".to_string(),
        ]
        .join("||");
        assert_eq!(proficiency_selects(&rules), [want]);
        assert!(grants(&rules, "Proficiency").is_empty(), "{rules:?}");
        assert_eq!(partial, [DOUBLED_NOT_APPLIED]);
        // an option that is not a skill, a tool or a kind of tool: nothing is guessed
        let (rules, partial) = rules_of(json!({"name": "G", "source": "ZZZ",
        "grantsProficiency": true, "entries": [
            "You gain one of the following proficiencies:",
            {"type": "list", "items": ["{@skill Stealth}", "One simple weapon of your choice"]}
        ]}));
        assert!(proficiency_selects(&rules).is_empty(), "{rules:?}");
        assert_eq!(partial.len(), 1, "{partial:?}");
    }
}
