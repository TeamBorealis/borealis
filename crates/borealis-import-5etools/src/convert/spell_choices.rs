//! Spell choice selects of a caster: cantrips known, spells known, spellbook pages and spells of
//! a fixed level (Mystic Arcanum). These are the constructs the bundled SRD classes use
//! (`Cantrip (Wizard 1)`, `Spell (Sorcerer)`, `Spellbook (Wizard 2)`), so an imported caster
//! learns and prepares spells exactly like a bundled one.

use super::build::{custom, select};
use borealis_content::Rule;
use serde_json::{json, Value};

/// Spells of 1st level and higher in the caster's list that a slot can cast.
const LIST_AND_SLOTS: &str = "$(spellcasting:list), $(spellcasting:slots)";
/// Cantrips of the caster's list.
const LIST_CANTRIPS: &str = "$(spellcasting:list), 0";
/// Gold and hours to copy one spell level into a spellbook (the standard spellbook rule; 5etools
/// keeps it in prose only).
const BOOK_COPY_GP_PER_LEVEL: i64 = 50;
const BOOK_COPY_HOURS_PER_LEVEL: i64 = 2;

/// A school restriction on learned spells (2014 Eldritch Knight, Arcane Trickster): most picks
/// must come from `schools`; the picks gained at `any_levels` (and `first_any` of the first
/// level's picks) may come from any school.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchoolRule {
    /// Full school names (`Abjuration`).
    pub schools: Vec<String>,
    /// Restricted picks among the first level's picks.
    pub first_restricted: i64,
    /// Class levels whose new pick may come from any school.
    pub any_levels: Vec<u32>,
}

/// Inputs of [`spell_choices`].
#[derive(Debug, Default)]
pub struct ChoiceInput<'a> {
    /// Spellcasting name (class or subclass).
    pub owner: &'a str,
    /// Cantrips known per class level (cumulative).
    pub cantrips: &'a [i64],
    /// Spells known per class level (cumulative).
    pub known: &'a [i64],
    /// Spellbook spells gained per class level.
    pub book: &'a [i64],
    /// Spellbook spells may be of any level with slots.
    pub book_allow_lower: bool,
    /// `{"<class level>": {"<spell level>": count}}`.
    pub by_level: Option<&'a Value>,
    /// Highest spell level with a slot at any class level.
    pub max_slot_level: u32,
    /// The caster prepares spells.
    pub prepares: bool,
    /// `full`, `1/2`, `1/3`, `pact`, `artificer`.
    pub progression: &'a str,
    /// `level` or `restLong`.
    pub prepared_change: Option<&'a str>,
    /// School restriction of learned spells.
    pub schools: Option<&'a SchoolRule>,
    /// 2024 rules.
    pub is_2024: bool,
}

/// Result of [`spell_choices`].
#[derive(Debug, Default)]
pub struct ChoiceOut {
    /// Select and custom rules.
    pub rules: Vec<Rule>,
    /// Losses.
    pub partial: Vec<String>,
}

/// Positive per-level increments of a cumulative series as `(class level, increment)`.
fn increments(
    series: &[i64],
    what: &str,
    owner: &str,
    partial: &mut Vec<String>,
) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let mut prev = 0i64;
    for (i, v) in series.iter().enumerate() {
        let delta = v - prev;
        prev = *v;
        let Ok(level) = u32::try_from(i + 1) else {
            continue;
        };
        match u32::try_from(delta) {
            Ok(0) => {}
            Ok(n) => out.push((level, n)),
            Err(_) => partial.push(format!(
                "{what} of {owner} shrink at level {level}; nothing is removed"
            )),
        }
    }
    out
}

fn spell_select(name: &str, supports: &str, number: u32, level: u32, owner: &str) -> Rule {
    let mut s = select("Spell", name, Some(supports), number);
    s.level = (level > 1).then_some(level);
    s.spellcasting = Some(owner.to_string());
    Rule::Select(s)
}

fn with(rule: Rule, edit: impl FnOnce(&mut borealis_content::SelectRule)) -> Rule {
    let Rule::Select(mut s) = rule else {
        return rule;
    };
    edit(&mut s);
    Rule::Select(s)
}

fn cantrip_selects(input: &ChoiceInput<'_>, out: &mut ChoiceOut) {
    let owner = input.owner;
    for (level, n) in increments(input.cantrips, "cantrips known", owner, &mut out.partial) {
        let rule = spell_select(
            &format!("Cantrip ({owner} {level})"),
            LIST_CANTRIPS,
            n,
            level,
            owner,
        );
        // 2024: a cantrip may be swapped on level up (bundled 2024 classes do the same)
        out.rules.push(with(rule, |s| {
            s.allow_replace = input.is_2024.then_some(true);
        }));
    }
}

fn known_selects(input: &ChoiceInput<'_>, out: &mut ChoiceOut) {
    let owner = input.owner;
    let steps = increments(input.known, "spells known", owner, &mut out.partial);
    let first = steps.first().map(|s| s.0);
    for (level, n) in steps {
        let Some(rule) = input.schools else {
            out.rules.push(spell_select(
                &format!("Spell ({owner} {level})"),
                LIST_AND_SLOTS,
                n,
                level,
                owner,
            ));
            continue;
        };
        let any = if Some(level) == first {
            (i64::from(n) - rule.first_restricted).clamp(0, i64::from(n))
        } else if rule.any_levels.contains(&level) {
            i64::from(n)
        } else {
            0
        };
        let any = u32::try_from(any).unwrap_or(0);
        let restricted = n - any;
        if restricted > 0 {
            let schools = rule.schools.join("||");
            out.rules.push(spell_select(
                &format!("Spell ({owner} {level})"),
                &format!("{LIST_AND_SLOTS}, ({schools})"),
                restricted,
                level,
                owner,
            ));
        }
        if any > 0 {
            out.rules.push(spell_select(
                &format!("Spell of Any School ({owner} {level})"),
                LIST_AND_SLOTS,
                any,
                level,
                owner,
            ));
        }
    }
}

fn book_selects(input: &ChoiceInput<'_>, out: &mut ChoiceOut) {
    let owner = input.owner;
    if input.book.is_empty() {
        return;
    }
    if !input.book_allow_lower {
        out.partial.push(format!(
            "{owner}: spellbook spells limited to the highest slot level are offered at any slot level"
        ));
    }
    for (i, n) in input.book.iter().enumerate() {
        let (Ok(level), Ok(n)) = (u32::try_from(i + 1), u32::try_from(*n)) else {
            continue;
        };
        if n > 0 {
            out.rules.push(spell_select(
                &format!("Spellbook ({owner} {level})"),
                LIST_AND_SLOTS,
                n,
                level,
                owner,
            ));
        }
    }
    if input.is_2024 {
        // 2024: picks fill the spellbook, which is the preparation source (bundled 2024 Wizard)
        let initial = input.book.first().copied().unwrap_or(0);
        let later: Vec<i64> = input.book.iter().skip(1).copied().collect();
        let per_level = later.first().copied().unwrap_or(0);
        if later.iter().any(|n| *n != per_level) {
            out.partial.push(format!(
                "{owner}: uneven spellbook growth; the book capacity uses {per_level} per level"
            ));
        }
        out.rules.push(custom(
            "spellbook",
            json!({"initial_spells": initial, "per_level": per_level,
                "copy_cost_gp_per_level": BOOK_COPY_GP_PER_LEVEL,
                "copy_hours_per_level": BOOK_COPY_HOURS_PER_LEVEL, "prepare_from": "spellbook"}),
        ));
    }
}

fn fixed_level_selects(input: &ChoiceInput<'_>, out: &mut ChoiceOut) {
    let owner = input.owner;
    let Some(map) = input.by_level.and_then(Value::as_object) else {
        return;
    };
    let mut picks: Vec<(u32, u32, u32)> = Vec::new();
    for (class_level, spells) in map {
        let Ok(class_level) = class_level.parse::<u32>() else {
            out.partial
                .push(format!("{owner}: fixed spell level key {class_level:?}"));
            continue;
        };
        for (spell_level, count) in spells.as_object().into_iter().flatten() {
            let (Ok(spell_level), Some(count)) = (
                spell_level.parse::<u32>(),
                count.as_u64().and_then(|c| u32::try_from(c).ok()),
            ) else {
                out.partial
                    .push(format!("{owner}: fixed spell entry {spell_level:?}"));
                continue;
            };
            picks.push((class_level, spell_level, count));
        }
    }
    picks.sort_unstable();
    let mut arcanum = Vec::new();
    for (class_level, spell_level, count) in picks {
        let rule = spell_select(
            &format!("Level {spell_level} Spell ({owner} {class_level})"),
            &format!("$(spellcasting:list), {spell_level}"),
            count,
            class_level,
            owner,
        );
        // known for good and not counted against spells known / prepared
        out.rules.push(with(rule, |s| s.prepared = Some(true)));
        if spell_level > input.max_slot_level {
            arcanum.push(json!({"spell_level": spell_level, "level": class_level}));
        }
    }
    if !arcanum.is_empty() {
        // above every slot the caster has: cast once per long rest (Mystic Arcanum)
        out.rules.push(custom(
            "mystic_arcanum",
            json!({"slots": arcanum, "free_cast": {"uses": 1, "recharge": "long_rest"}}),
        ));
    }
}

fn prepared_change(input: &ChoiceInput<'_>, out: &mut ChoiceOut) {
    if !input.is_2024 || !input.prepares {
        return;
    }
    let (when, count) = match input.prepared_change {
        Some("level") => ("gain_level", "one"),
        Some("restLong") if input.progression == "full" => ("long_rest", "any"),
        Some("restLong") => ("long_rest", "one"),
        Some(other) => {
            out.partial
                .push(format!("{}: prepared spell change {other:?}", input.owner));
            return;
        }
        None => return,
    };
    out.rules.push(custom(
        "prepared_spell_change",
        json!({"when": when, "count": count}),
    ));
}

/// Spells known per level less the free spell choices that class-level `additionalSpells`
/// make at the same levels (2014 Bard's Magical Secrets): the class table's "Spells Known"
/// already counts those, and they get selects of their own. Alternatives (2+ groups) are not
/// counted.
#[must_use]
pub fn known_without_extra_choices(known: &[i64], additional: &[Value]) -> Vec<i64> {
    let [group] = additional else {
        return known.to_vec();
    };
    let mut extra = vec![0i64; known.len()];
    for (key, list) in group
        .get("known")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let Some(at) = key
            .parse::<usize>()
            .ok()
            .filter(|l| (1..=known.len()).contains(l))
        else {
            continue;
        };
        let chosen: i64 = list
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| e.get("choose").is_some())
            .map(|e| e.get("count").and_then(Value::as_i64).unwrap_or(1))
            .sum();
        for slot in &mut extra[at - 1..] {
            *slot += chosen;
        }
    }
    known
        .iter()
        .zip(extra)
        .map(|(k, e)| (k - e).max(0))
        .collect()
}

/// Spell choice rules of a caster.
#[must_use]
pub fn spell_choices(input: &ChoiceInput<'_>) -> ChoiceOut {
    let mut out = ChoiceOut::default();
    cantrip_selects(input, &mut out);
    known_selects(input, &mut out);
    book_selects(input, &mut out);
    fixed_level_selects(input, &mut out);
    prepared_change(input, &mut out);
    out
}

fn school_of(word: &str) -> Option<&'static str> {
    const SCHOOLS: [&str; 8] = [
        "Abjuration",
        "Conjuration",
        "Divination",
        "Enchantment",
        "Evocation",
        "Illusion",
        "Necromancy",
        "Transmutation",
    ];
    SCHOOLS
        .iter()
        .find(|s| s.eq_ignore_ascii_case(word))
        .copied()
}

fn number_word(word: &str) -> Option<i64> {
    let n = match word {
        "one" => 1,
        "two" => 2,
        "three" => 3,
        "four" => 4,
        other => return other.parse().ok(),
    };
    Some(n)
}

/// The school restriction stated in a subclass's spellcasting text ("two of which you must
/// choose from the abjuration and evocation spells", "the spells you learn at 8th, 14th, and
/// 20th level can come from any school of magic"). `None` when the text states none.
#[must_use]
pub fn school_rule(text: &str) -> Option<SchoolRule> {
    let lower = text.to_lowercase();
    let pick = regex::Regex::new(
        r"(\w+) of which you must choose from the (\w+)(?:, (\w+))?,? (?:and|or) (\w+) spells",
    )
    .ok()?;
    let caps = pick.captures(&lower)?;
    let first_restricted = number_word(&caps[1])?;
    let schools: Option<Vec<String>> = [caps.get(2), caps.get(3), caps.get(4)]
        .into_iter()
        .flatten()
        .map(|m| school_of(m.as_str()).map(str::to_string))
        .collect();
    let schools = schools?;
    let any = regex::Regex::new(r"learn at ([^.]*?) levels? can come from any school").ok()?;
    let ordinal = regex::Regex::new(r"(\d+)(?:st|nd|rd|th)").ok()?;
    let any_levels = any
        .captures(&lower)
        .map(|c| {
            ordinal
                .captures_iter(&c[1])
                .filter_map(|o| o[1].parse().ok())
                .collect()
        })
        .unwrap_or_default();
    Some(SchoolRule {
        schools,
        first_restricted,
        any_levels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selects(rules: &[Rule]) -> Vec<(String, String, u32, u32)> {
        rules
            .iter()
            .filter_map(|r| match r {
                Rule::Select(s) => Some((
                    s.name.clone(),
                    s.supports.clone().unwrap_or_default(),
                    s.number.unwrap_or(1),
                    s.level.unwrap_or(1),
                )),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn known_caster_gets_cantrip_and_spell_selects_per_increment() {
        let out = spell_choices(&ChoiceInput {
            owner: "Sorcerer",
            cantrips: &[4, 4, 4, 5],
            known: &[2, 3, 4, 5],
            progression: "full",
            ..ChoiceInput::default()
        });
        let s = selects(&out.rules);
        assert_eq!(s.len(), 6, "{s:?}");
        assert_eq!(
            s[0],
            ("Cantrip (Sorcerer 1)".into(), LIST_CANTRIPS.into(), 4, 1)
        );
        assert_eq!(
            s[1],
            ("Cantrip (Sorcerer 4)".into(), LIST_CANTRIPS.into(), 1, 4)
        );
        assert_eq!(
            s[2],
            ("Spell (Sorcerer 1)".into(), LIST_AND_SLOTS.into(), 2, 1)
        );
        assert_eq!(
            s[4],
            ("Spell (Sorcerer 3)".into(), LIST_AND_SLOTS.into(), 1, 3)
        );
        let Rule::Select(c) = &out.rules[0] else {
            panic!()
        };
        assert_eq!(c.spellcasting.as_deref(), Some("Sorcerer"));
        assert_eq!(c.allow_replace, None, "2014 cantrips are not swapped");
        assert!(out.partial.is_empty());
    }

    #[test]
    fn spellbook_2024_adds_the_book_rule_and_change_rule() {
        let out = spell_choices(&ChoiceInput {
            owner: "Wizard",
            cantrips: &[3, 3],
            book: &[6, 2, 2],
            book_allow_lower: true,
            prepares: true,
            progression: "full",
            prepared_change: Some("restLong"),
            is_2024: true,
            ..ChoiceInput::default()
        });
        let s = selects(&out.rules);
        assert_eq!(
            s[1],
            ("Spellbook (Wizard 1)".into(), LIST_AND_SLOTS.into(), 6, 1)
        );
        assert_eq!(
            s[3],
            ("Spellbook (Wizard 3)".into(), LIST_AND_SLOTS.into(), 2, 3)
        );
        let Rule::Select(c) = &out.rules[0] else {
            panic!()
        };
        assert_eq!(c.allow_replace, Some(true));
        let customs: Vec<(&str, &serde_json::Map<String, Value>)> = out
            .rules
            .iter()
            .filter_map(|r| match r {
                Rule::Custom(c) => Some((c.kind.as_str(), &c.fields)),
                _ => None,
            })
            .collect();
        assert_eq!(customs[0].0, "spellbook");
        assert_eq!(customs[0].1["initial_spells"], 6);
        assert_eq!(customs[0].1["per_level"], 2);
        assert_eq!(customs[1].0, "prepared_spell_change");
        assert_eq!(customs[1].1["count"], "any");
        let ranger = spell_choices(&ChoiceInput {
            owner: "Ranger",
            prepares: true,
            progression: "artificer",
            prepared_change: Some("restLong"),
            is_2024: true,
            ..ChoiceInput::default()
        });
        assert!(matches!(&ranger.rules[0], Rule::Custom(c) if c.fields["count"] == "one"));
        let odd = spell_choices(&ChoiceInput {
            owner: "X",
            book: &[4, 1, 3],
            prepares: true,
            prepared_change: Some("whenever"),
            is_2024: true,
            ..ChoiceInput::default()
        });
        assert_eq!(odd.partial.len(), 3, "{:?}", odd.partial);
    }

    #[test]
    fn fixed_level_spells_above_the_slots_are_an_arcanum() {
        let by_level = json!({"13": {"7": 1}, "11": {"6": 1}, "x": {}, "15": {"y": 1}});
        let out = spell_choices(&ChoiceInput {
            owner: "Warlock",
            by_level: Some(&by_level),
            max_slot_level: 5,
            ..ChoiceInput::default()
        });
        let s = selects(&out.rules);
        assert_eq!(
            s,
            vec![
                (
                    "Level 6 Spell (Warlock 11)".into(),
                    "$(spellcasting:list), 6".into(),
                    1,
                    11
                ),
                (
                    "Level 7 Spell (Warlock 13)".into(),
                    "$(spellcasting:list), 7".into(),
                    1,
                    13
                ),
            ]
        );
        assert!(matches!(&out.rules[0], Rule::Select(s) if s.prepared == Some(true)));
        let Rule::Custom(a) = out.rules.last().unwrap() else {
            panic!()
        };
        assert_eq!(a.kind, "mystic_arcanum");
        assert_eq!(a.fields["slots"][1], json!({"spell_level": 7, "level": 13}));
        assert_eq!(out.partial.len(), 2);
    }

    #[test]
    fn school_restricted_known_spells_split_by_level() {
        let text = "You know three 1st-level wizard spells of your choice, two of which you must \
            choose from the abjuration and evocation spells on the wizard spell list. The spells \
            you learn at 8th, 14th, and 20th level can come from any school of magic.";
        let rule = school_rule(text).unwrap();
        assert_eq!(rule.schools, vec!["Abjuration", "Evocation"]);
        assert_eq!(rule.first_restricted, 2);
        assert_eq!(rule.any_levels, vec![8, 14, 20]);
        let mut known = vec![0, 0, 3, 4, 4, 4, 5, 6];
        known.resize(20, 6);
        let out = spell_choices(&ChoiceInput {
            owner: "Eldritch Knight",
            known: &known,
            schools: Some(&rule),
            ..ChoiceInput::default()
        });
        let s = selects(&out.rules);
        let restricted = format!("{LIST_AND_SLOTS}, (Abjuration||Evocation)");
        assert_eq!(
            s,
            vec![
                ("Spell (Eldritch Knight 3)".into(), restricted.clone(), 2, 3),
                (
                    "Spell of Any School (Eldritch Knight 3)".into(),
                    LIST_AND_SLOTS.into(),
                    1,
                    3
                ),
                ("Spell (Eldritch Knight 4)".into(), restricted.clone(), 1, 4),
                ("Spell (Eldritch Knight 7)".into(), restricted, 1, 7),
                (
                    "Spell of Any School (Eldritch Knight 8)".into(),
                    LIST_AND_SLOTS.into(),
                    1,
                    8
                ),
            ]
        );
        assert!(school_rule("You know three spells of any school.").is_none());
        assert!(school_rule("two of which you must choose from the fire and ice spells").is_none());
    }

    #[test]
    fn class_level_known_choices_are_part_of_spells_known() {
        let known = [4, 5, 6, 7];
        let extra = [
            json!({"name": "Hidden Lore", "known": {"3": [{"choose": "level=1"}, {"choose": "level=2", "count": 2}], "x": [], "9": [{"choose": ""}], "2": ["fixed spell"]}}),
        ];
        assert_eq!(
            known_without_extra_choices(&known, &extra),
            vec![4, 5, 3, 4]
        );
        let alternatives = [extra[0].clone(), extra[0].clone()];
        assert_eq!(
            known_without_extra_choices(&known, &alternatives),
            known.to_vec()
        );
    }

    #[test]
    fn shrinking_series_are_reported() {
        let out = spell_choices(&ChoiceInput {
            owner: "X",
            cantrips: &[2, 1],
            ..ChoiceInput::default()
        });
        assert_eq!(selects(&out.rules).len(), 1);
        assert_eq!(out.partial.len(), 1);
    }
}
