//! Proficiency choices a class or subclass feature states only in its prose, read into the
//! structured fields 5etools uses for the same choice elsewhere (`skillProficiencies`,
//! `toolProficiencies`), which [`Mech::proficiency_fields`] then converts like any other entry.
//!
//! The reading is narrow and deterministic (never AI, IM1). Only the feature's own top-level
//! sentences are read (a nested option such as one totem animal is never read as the whole
//! feature's rule), and only a sentence that is not conditional ("If you already have ...",
//! "Alternatively, ...", "... instead ..."). The shapes read:
//!
//! - "you (also) gain proficiency in / become proficient in (your choice of) N of the
//!   following skills (of your choice): A, B, or C" (PHB Acolyte of Nature): a skill choice
//!   over exactly the listed skills; every entry must be a skill, or nothing is read;
//! - "you gain proficiency with one type of artisan's tools / gaming set / musical instrument
//!   of your choice" (PHB Student of War): a tool choice of that kind.
//!
//! Used only for a feature that does not take the bundled SRD feature's rules.

use super::mech::Mech;
use crate::core_ids;
use regex::Regex;
use serde_json::{json, Value};

/// A skill choice the feature states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillChoice {
    /// How many skills are chosen.
    pub count: u32,
    /// Lower-case skill names, in the text's order.
    pub skills: Vec<String>,
}

/// The choices one feature states.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Stated {
    /// Skill choices, in the text's order.
    pub skills: Vec<SkillChoice>,
    /// `toolProficiencies` entries.
    pub tools: Vec<Value>,
}

impl Stated {
    /// The 5etools fields stating the same choices.
    #[must_use]
    pub fn fields(&self) -> Value {
        let skills: Vec<Value> = self
            .skills
            .iter()
            .map(|c| json!({"choose": {"from": c.skills, "count": c.count}}))
            .collect();
        json!({"skillProficiencies": skills, "toolProficiencies": self.tools})
    }
}

pub(crate) fn number_word(word: &str) -> Option<u32> {
    match word {
        "a" | "an" | "one" => Some(1),
        "two" => Some(2),
        "three" => Some(3),
        "four" => Some(4),
        _ => None,
    }
}

/// The feature's top-level sentences, plain lower-case text without the closing period.
#[must_use]
pub fn sentences(entries: &[Value]) -> Vec<String> {
    entries
        .iter()
        .filter_map(Value::as_str)
        .flat_map(|s| {
            crate::text::strip_tags(s)
                .to_lowercase()
                .replace('’', "'")
                .split('.')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
        })
        .collect()
}

fn conditional(sentence: &str) -> bool {
    sentence.starts_with("if ")
        || sentence.starts_with("alternatively")
        || sentence.contains("instead")
}

/// The items of "A, B, or C" / "A and B".
fn list_items(list: &str) -> Vec<String> {
    list.split(',')
        .flat_map(|p| p.split(" or "))
        .flat_map(|p| p.split(" and "))
        .map(|p| {
            let p = p.trim();
            p.strip_prefix("or ")
                .or_else(|| p.strip_prefix("and "))
                .unwrap_or(p)
                .trim()
                .to_string()
        })
        .filter(|p| !p.is_empty())
        .collect()
}

fn skill_choice(sentence: &str) -> Option<SkillChoice> {
    let re = Regex::new(
        r"\byou (?:also )?(?:gain proficiency (?:in|with)|become proficient in)\b[^:]*?\b(?:your choice of )?(a|one|two|three|four) (?:of the following skills|of these skills|skills from the following list)(?: of your choice)?: (.+)$",
    )
    .ok()?;
    let caps = re.captures(sentence)?;
    let count = number_word(&caps[1])?;
    let skills = list_items(&caps[2]);
    let known = skills.iter().all(|s| core_ids::is_skill(s));
    (known && skills.len() > 1 && (count as usize) < skills.len())
        .then_some(SkillChoice { count, skills })
}

fn tool_choice(sentence: &str) -> Option<Value> {
    let re = Regex::new(
        r"\byou (?:also )?gain proficiency with (a|one|two|three) types? of (artisan's tools|gaming sets?|musical instruments?) of your choice",
    )
    .ok()?;
    let caps = re.captures(sentence)?;
    let count = number_word(&caps[1])?;
    let kind = match &caps[2] {
        "artisan's tools" => "artisan's tools",
        k if k.starts_with("gaming") => "gaming set",
        _ => "musical instrument",
    };
    Some(json!({"choose": {"from": [kind], "count": count}}))
}

/// The proficiency choices the feature's own sentences state.
#[must_use]
pub fn read(entries: &[Value]) -> Stated {
    let mut out = Stated::default();
    for s in sentences(entries).iter().filter(|s| !conditional(s)) {
        out.skills.extend(skill_choice(s));
        out.tools.extend(tool_choice(s));
    }
    out
}

/// Push the rules of the stated choices.
pub fn apply(mech: &mut Mech<'_>, stated: &Stated) {
    if stated.skills.is_empty() && stated.tools.is_empty() {
        return;
    }
    mech.proficiency_fields(&stated.fields());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_one(text: &str) -> Stated {
        read(&[json!(text)])
    }

    #[test]
    fn a_skill_choice_from_a_listed_set() {
        let s = read_one(
            "You learn a cantrip. You also gain proficiency in one of the following skills of \
             your choice: {@skill Animal Handling}, {@skill Medicine}, or {@skill Survival}.",
        );
        assert_eq!(
            s.skills,
            vec![SkillChoice {
                count: 1,
                skills: vec![
                    "animal handling".into(),
                    "medicine".into(),
                    "survival".into()
                ],
            }]
        );
        assert!(s.tools.is_empty());
        let s = read_one(
            "You also become proficient in your choice of two of the following skills: \
             {@skill Arcana}, {@skill Insight}, {@skill Medicine}, or {@skill Religion}.",
        );
        assert_eq!(s.skills[0].count, 2);
        assert_eq!(s.skills[0].skills.len(), 4);
        let s = read_one(
            "You also gain proficiency in one of these skills of your choice: \
             {@skill Stealth|XPHB}, {@skill Insight|XPHB}, or {@skill Persuasion|XPHB}.",
        );
        assert_eq!(s.skills[0].skills[2], "persuasion");
    }

    #[test]
    fn conditional_nested_and_unknown_lists_are_not_read() {
        let conditional = read_one(
            "If you already have it, you gain proficiency in one of the following skills of your \
             choice: {@skill Insight}, {@skill Stealth}, or {@skill Medicine}.",
        );
        assert_eq!(conditional, Stated::default());
        let alternative = read_one(
            "If you already have it, you gain proficiency with one other type of \
             {@item artisan's tools|PHB} of your choice.",
        );
        assert_eq!(alternative, Stated::default());
        // a totem option nested in the feature is not the feature's own rule
        let nested = read(&[json!({"type": "entries", "name": "Option A", "entries": [
            "You gain proficiency in two skills from the following list: {@skill Athletics}, \
             {@skill Insight}, {@skill Stealth}, and {@skill Medicine}."]})]);
        assert_eq!(nested, Stated::default());
        let not_skills = read_one(
            "You gain proficiency in one of the following skills of your choice: Arcana, \
             Cooking, or History.",
        );
        assert_eq!(not_skills, Stated::default());
        let the_helper = read_one(
            "The helper gains proficiency in two of the following skills: {@skill Arcana}, \
             {@skill History}, and {@skill Insight}.",
        );
        assert_eq!(the_helper, Stated::default());
    }

    #[test]
    fn a_tool_choice_of_a_kind() {
        let s = read_one(
            "You gain proficiency with one type of {@filter artisan's tools|items|\
             type=artisan's tools} of your choice.",
        );
        assert_eq!(
            s.tools,
            vec![json!({"choose": {"from": ["artisan's tools"], "count": 1}})]
        );
        let both = read_one(
            "You gain proficiency with one type of {@item Artisan's Tools|XPHB} of your choice \
             and in two of the following skills of your choice: {@skill Arcana|XPHB}, \
             {@skill Insight|XPHB}, {@skill Stealth|XPHB}, or {@skill Medicine|XPHB}.",
        );
        assert_eq!(both.tools.len(), 1);
        assert_eq!(both.skills[0].count, 2);
    }

    #[test]
    fn stated_choices_become_the_core_selects() {
        let sh = super::super::mech::tests_support::shared();
        let mut m = Mech::new(&sh, "Feature A");
        apply(
            &mut m,
            &read_one(
                "You also gain proficiency in one of the following skills of your choice: \
                 {@skill Animal Handling}, {@skill Medicine}, or {@skill Survival}.",
            ),
        );
        let mut tool = Mech::new(&sh, "Feature B");
        apply(
            &mut tool,
            &read_one(
                "You gain proficiency with one type of {@filter artisan's tools|items} of your choice.",
            ),
        );
        let ids = super::super::mech::tests_support::ids_of(&m);
        assert_eq!(
            ids,
            vec!["select:Proficiency:Skill Proficiency (Feature A):ID_PROFICIENCY_SKILL_ANIMALHANDLING|ID_PROFICIENCY_SKILL_MEDICINE|ID_PROFICIENCY_SKILL_SURVIVAL"]
        );
        assert_eq!(
            super::super::mech::tests_support::ids_of(&tool),
            vec!["select:Proficiency:Tool Proficiency (Feature B):Artisan tools"]
        );
        assert!(m.partial.is_empty() && tool.partial.is_empty());
    }
}
