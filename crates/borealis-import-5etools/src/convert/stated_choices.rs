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
//!   of your choice" (PHB Student of War): a tool choice of that kind;
//! - "you (also) learn N languages of your choice": `languageProficiencies` `anyStandard: N`
//!   (the field 5etools gives the same words on a background);
//! - "your proficiency bonus is doubled for any ability check you make that uses either of
//!   those skills" after the feature's one skill choice (PHB Blessings of Knowledge): the
//!   skills are offered as option elements of the feature, each granting the skill
//!   proficiency and doubling its proficiency bonus (Aurora's own model of that feature), so
//!   the expertise follows exactly the chosen skills. No 5etools field states that pairing.
//!
//! Used only for a feature that does not take the bundled SRD feature's rules.

use super::build::{grant, select, stat_bonus};
use super::mech::Mech;
use super::new_element;
use crate::core_ids;
use crate::util::title_case;
use borealis_content::{Element, Rule, Ruleset};
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
    /// Standard languages learned.
    pub languages: Option<u32>,
    /// The proficiency bonus is doubled for the chosen skills.
    pub doubled: bool,
}

impl Stated {
    /// The 5etools fields stating the same choices.
    #[must_use]
    pub fn fields(&self) -> Value {
        let skills: Vec<Value> = self
            .skills
            .iter()
            .filter(|c| Some(*c) != self.expertise_choice())
            .map(|c| json!({"choose": {"from": c.skills, "count": c.count}}))
            .collect();
        let languages: Vec<Value> = self
            .languages
            .map(|n| json!({ "anyStandard": n }))
            .into_iter()
            .collect();
        json!({
            "skillProficiencies": skills,
            "toolProficiencies": self.tools,
            "languageProficiencies": languages,
        })
    }

    /// The feature's one skill choice when its chosen skills also get expertise.
    #[must_use]
    pub fn expertise_choice(&self) -> Option<&SkillChoice> {
        match self.skills.as_slice() {
            [only] if self.doubled => Some(only),
            _ => None,
        }
    }

    fn is_empty(&self) -> bool {
        self.skills.is_empty() && self.tools.is_empty() && self.languages.is_none()
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

fn languages(sentence: &str) -> Option<u32> {
    let re = Regex::new(
        r"^(?:at \w+ level, )?you (?:also )?learn (a|one|two|three|four) languages? of your choice$",
    )
    .ok()?;
    number_word(&re.captures(sentence)?[1])
}

fn doubled(sentence: &str) -> bool {
    Regex::new(
        r"\byour proficiency bonus is doubled for any ability check you make that uses (?:either|any|each|both) of (?:those|these|the chosen) skills$",
    )
    .is_ok_and(|re| re.is_match(sentence))
}

/// "you gain proficiency in one skill of your choice from the (list of) skills available to
/// <class>s at level 1 / 1st level" (XPHB Student of War): a choice over the class's own
/// level 1 skill list, when the sentence names the feature's own class. A sentence that also
/// grants it again later ("and again at 10th level") is not read.
fn class_skill_choice(sentence: &str, class: &str, class_skills: &[String]) -> Option<SkillChoice> {
    let re = Regex::new(
        r"\byou (?:also )?gain proficiency in (a|one|two) skills? of your choice from (?:the list of )?(?:the )?skills available to ([a-z]+?)s? at (?:level 1|1st level)$",
    )
    .ok()?;
    let caps = re.captures(sentence)?;
    let count = number_word(&caps[1])?;
    let same_class = caps[2].eq_ignore_ascii_case(class.trim_end_matches('s'));
    (same_class && !sentence.contains("again at") && class_skills.len() > count as usize).then(
        || SkillChoice {
            count,
            skills: class_skills.to_vec(),
        },
    )
}

/// The proficiency choices the feature's own sentences state.
#[must_use]
pub fn read(entries: &[Value]) -> Stated {
    read_for_class(entries, "", &[])
}

/// [`read`], also reading a choice from `class`'s level 1 skill list `class_skills`.
#[must_use]
pub fn read_for_class(entries: &[Value], class: &str, class_skills: &[String]) -> Stated {
    let mut out = Stated::default();
    for s in sentences(entries).iter().filter(|s| !conditional(s)) {
        out.skills.extend(skill_choice(s));
        if !class_skills.is_empty() {
            out.skills
                .extend(class_skill_choice(s, &class.to_lowercase(), class_skills));
        }
        out.tools.extend(tool_choice(s));
        out.languages = out.languages.or_else(|| languages(s));
        out.doubled |= doubled(s);
    }
    out
}

/// Push the rules of the stated choices.
pub fn apply(mech: &mut Mech<'_>, stated: &Stated) {
    if stated.is_empty() {
        return;
    }
    mech.proficiency_fields(&stated.fields());
}

/// The feature that owns option elements.
pub struct OptionOwner<'a> {
    /// Feature name.
    pub feature: &'a str,
    /// Element type of the feature and its options (`Archetype Feature`).
    pub ty: &'a str,
    /// Source display name.
    pub source_display: &'a str,
    /// Ruleset.
    pub ruleset: Ruleset,
}

/// The select and option elements of a skill choice whose skills also get expertise: one
/// option per skill (`<feature> (Arcana)`), granting the proficiency and doubling its bonus.
/// `id_of` gives the option element id for an option name.
pub fn expertise_options(
    owner: &OptionOwner<'_>,
    choice: &SkillChoice,
    id_of: &mut dyn FnMut(&str) -> String,
) -> (Rule, Vec<Element>) {
    let mut elements = Vec::new();
    for skill in &choice.skills {
        let Some(proficiency) = core_ids::skill_id(skill) else {
            continue;
        };
        let name = format!("{} ({})", owner.feature, title_case(skill));
        let mut el = new_element(
            &id_of(&name),
            &name,
            owner.ty,
            owner.source_display,
            owner.ruleset,
        );
        el.rules = vec![
            grant(&proficiency, "Proficiency"),
            stat_bonus(&format!("{skill}:proficiency"), "proficiency", "double"),
        ];
        el.description = format!(
            "<p>{}: proficiency, with the proficiency bonus doubled; one of the skill choices of \
             {}.</p>",
            title_case(skill),
            owner.feature
        );
        elements.push(el);
    }
    let ids: Vec<&str> = elements.iter().map(|e| e.id.as_str()).collect();
    let rule = Rule::Select(select(
        owner.ty,
        owner.feature,
        Some(&ids.join("|")),
        choice.count,
    ));
    (rule, elements)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_one(text: &str) -> Stated {
        read(&[json!(text)])
    }

    #[test]
    fn a_skill_choice_from_the_class_level_1_list() {
        let list: Vec<String> = ["acrobatics", "athletics", "history"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let text = "You gain proficiency with one type of {@item Artisan's Tools|XPHB} of your \
                    choice, and you gain proficiency in one skill of your choice from the skills \
                    available to Fighters at level 1.";
        let st = read_for_class(&[json!(text)], "Fighter", &list);
        assert_eq!(
            st.skills,
            vec![SkillChoice {
                count: 1,
                skills: list.clone()
            }]
        );
        assert_eq!(st.tools.len(), 1);
        // another class's list, a repeated grant, or no list: nothing is read
        assert!(read_for_class(&[json!(text)], "Cleric", &list)
            .skills
            .is_empty());
        let again = "When you reach 3rd level and again at 10th level, you gain proficiency in \
                     one skill of your choice from the list of skills available to fighters at \
                     1st level.";
        assert!(read_for_class(&[json!(again)], "Fighter", &list)
            .skills
            .is_empty());
        assert!(read_one(text).skills.is_empty());
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
    fn languages_and_doubled_skills() {
        let s = read(&[
            json!(
                "At 1st level, you learn two languages of your choice. You also become \
                   proficient in your choice of two of the following skills: {@skill Arcana}, \
                   {@skill Insight}, {@skill Medicine}, or {@skill Religion}."
            ),
            json!(
                "Your proficiency bonus is doubled for any ability check you make that uses \
                   either of those skills."
            ),
        ]);
        assert_eq!(s.languages, Some(2));
        assert!(s.doubled);
        assert_eq!(s.expertise_choice().map(|c| c.count), Some(2));
        // the skills go to the option elements, not to a plain proficiency select
        assert_eq!(s.fields()["skillProficiencies"], json!([]));
        assert_eq!(
            s.fields()["languageProficiencies"],
            json!([{"anyStandard": 2}])
        );
        let alternative = read_one("Alternatively, you learn one language of your choice.");
        assert_eq!(alternative.languages, None);
        let named = read_one("You learn to speak Draconic or one other language of your choice.");
        assert_eq!(named.languages, None);
        // doubled without a skill choice (fixed proficiencies) is no expertise choice
        let fixed = read_one(
            "Your proficiency bonus is doubled for any ability check you make that uses either \
             of those skills.",
        );
        assert!(fixed.expertise_choice().is_none());
    }

    #[test]
    fn expertise_options_grant_proficiency_and_double_it() {
        let choice = SkillChoice {
            count: 2,
            skills: vec!["arcana".into(), "animal handling".into()],
        };
        let owner = OptionOwner {
            feature: "Feature C",
            ty: "Archetype Feature",
            source_display: "Book",
            ruleset: Ruleset::Dnd2014,
        };
        let (rule, options) = expertise_options(&owner, &choice, &mut |n| {
            format!("ID_X_{}", crate::util::id_fragment(n))
        });
        let Rule::Select(s) = rule else {
            panic!("select");
        };
        assert_eq!((s.ty.as_str(), s.number), ("Archetype Feature", Some(2)));
        assert_eq!(
            s.supports.as_deref(),
            Some("ID_X_FEATURE_C_ARCANA|ID_X_FEATURE_C_ANIMAL_HANDLING")
        );
        assert_eq!(options[1].name, "Feature C (Animal Handling)");
        assert_eq!(options[1].ty, "Archetype Feature");
        let Rule::Grant(g) = &options[1].rules[0] else {
            panic!("grant");
        };
        assert_eq!(g.id, "ID_PROFICIENCY_SKILL_ANIMALHANDLING");
        let Rule::Stat(st) = &options[1].rules[1] else {
            panic!("stat");
        };
        assert_eq!(
            (st.name.as_str(), st.value.as_str(), st.bonus.as_deref()),
            ("animal handling:proficiency", "proficiency", Some("double"))
        );
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
