//! Text normalisation for the typed sheet model: formula evaluation, feature de-duplication and
//! proficiency lists without repeats or internal group labels.

use crate::model_types::{ArmorTraining, FeatureData, FeatureGroup, ProficiencyData};
use borealis_engine::snapshot::{ClassSnapshot, FeatureDetail};
use borealis_engine::CharacterSnapshot;

/// Features that are bookkeeping, not sheet content.
const HIDDEN_FEATURES: &[&str] = &["ability score improvement"];
/// Longest short summary, in characters.
const SUMMARY_MAX: usize = 180;
/// Weapon names by category (SRD weapon table; facts used to collapse category-plus-member lists).
const SIMPLE_MELEE: &[&str] = &[
    "club",
    "dagger",
    "greatclub",
    "handaxe",
    "javelin",
    "light hammer",
    "mace",
    "quarterstaff",
    "sickle",
    "spear",
    "unarmed strike",
];
const SIMPLE_RANGED: &[&str] = &["light crossbow", "dart", "shortbow", "sling"];
const MARTIAL_MELEE: &[&str] = &[
    "battleaxe",
    "flail",
    "glaive",
    "greataxe",
    "greatsword",
    "halberd",
    "lance",
    "longsword",
    "maul",
    "morningstar",
    "pike",
    "rapier",
    "scimitar",
    "shortsword",
    "trident",
    "war pick",
    "warhammer",
    "whip",
];
const MARTIAL_RANGED: &[&str] = &[
    "blowgun",
    "hand crossbow",
    "heavy crossbow",
    "longbow",
    "net",
];
const LIGHT_ARMOR: &[&str] = &["padded", "leather", "studded leather"];
const MEDIUM_ARMOR: &[&str] = &[
    "hide",
    "chain shirt",
    "scale mail",
    "breastplate",
    "half plate",
];
const HEAVY_ARMOR: &[&str] = &["ring mail", "chain mail", "splint", "plate"];

/// Evaluate `NxM` products left in sheet text ("5x5" becomes "25") and replace unresolved
/// `{{placeholders}}`. `2d6x2`, hex literals and words are left alone.
pub(crate) fn evaluate_formulas(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '{' && chars.get(i + 1) == Some(&'{') {
            if let Some(end) = (i + 2..chars.len().saturating_sub(1))
                .find(|&j| chars[j] == '}' && chars[j + 1] == '}')
            {
                out.push('\u{2026}');
                i = end + 2;
                continue;
            }
        }
        let boundary = i == 0 || !chars[i - 1].is_alphanumeric();
        if c.is_ascii_digit() && boundary {
            if let Some((product, next)) = product_at(&chars, i) {
                out.push_str(&product.to_string());
                i = next;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn digits(chars: &[char], from: usize) -> (u64, usize) {
    let mut n = 0u64;
    let mut j = from;
    while let Some(d) = chars.get(j).and_then(|c| c.to_digit(10)) {
        n = n.saturating_mul(10).saturating_add(u64::from(d));
        j += 1;
    }
    (n, j)
}

/// `N x M` starting at `at`, when it is a standalone product.
fn product_at(chars: &[char], at: usize) -> Option<(u64, usize)> {
    let (a, mut j) = digits(chars, at);
    while chars.get(j) == Some(&' ') {
        j += 1;
    }
    if !matches!(chars.get(j), Some('x' | '\u{d7}')) {
        return None;
    }
    j += 1;
    while chars.get(j) == Some(&' ') {
        j += 1;
    }
    if !chars.get(j).is_some_and(char::is_ascii_digit) {
        return None;
    }
    let (b, end) = digits(chars, j);
    if chars.get(end).is_some_and(|c| c.is_alphanumeric()) {
        return None;
    }
    Some((a.saturating_mul(b), end))
}

fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}

/// Drop leading repeats of the feature's own name ("Stonecunning. Stonecunning. Whenever...").
pub(crate) fn strip_own_name(name: &str, text: &str) -> String {
    let mut rest = text.trim();
    while let Some(after) = strip_prefix_ci(rest, name) {
        let sep = after.trim_start();
        if !(sep.is_empty() || sep.starts_with(['.', ':', '\u{2014}'])) {
            break;
        }
        rest = sep.trim_start_matches(['.', ':', '\u{2014}', ' ']);
    }
    rest.to_string()
}

/// Byte length of the first sentence of `text` (all of it without a sentence break).
fn first_sentence_len(text: &str) -> usize {
    text.char_indices()
        .find(|&(i, c)| c == '.' && text[i + 1..].starts_with(' '))
        .map_or(text.len(), |(i, _)| i + 1)
}

/// Whether a sentence only announces what follows ("You gain the following benefits.").
fn is_lead_in(sentence: &str) -> bool {
    let s = sentence.to_lowercase();
    s.contains("the following benefit") || s.trim_end().ends_with(':')
}

/// Words that make a run-in heading ("Initiative Proficiency.") rather than a sentence.
const RUN_IN_MAX_WORDS: usize = 4;

/// A short Title Case fragment ("Initiative Proficiency.").
fn is_run_in(sentence: &str) -> bool {
    let words: Vec<&str> = sentence.split_whitespace().collect();
    words.len() <= RUN_IN_MAX_WORDS
        && words
            .iter()
            .all(|w| w.chars().next().is_some_and(char::is_uppercase))
}

/// First sentence, or a word-boundary cut at [`SUMMARY_MAX`] characters. A lead-in sentence
/// that says nothing ("You gain the following benefits.") is skipped, and a run-in heading
/// after it keeps its sentence ("Initiative Proficiency. When you roll ...").
pub(crate) fn short_summary(text: &str) -> String {
    let mut text = text.trim();
    let mut skipped = false;
    let lead = first_sentence_len(text);
    if lead < text.len() && is_lead_in(&text[..lead]) {
        text = text[lead..].trim_start();
        skipped = true;
    }
    let mut end = first_sentence_len(text);
    if skipped && is_run_in(&text[..end]) && end < text.len() {
        end += first_sentence_len(&text[end..]);
    }
    let first = &text[..end];
    if first.chars().count() <= SUMMARY_MAX {
        return first.trim().to_string();
    }
    let cut: String = first.chars().take(SUMMARY_MAX).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(h, _)| h);
    format!("{}\u{2026}", cut.trim_end_matches([',', ';', ' ']))
}

fn display_name(name: &str) -> String {
    let n = name.trim();
    strip_prefix_ci(n, "Feature: ")
        .unwrap_or(n)
        .trim()
        .to_string()
}

fn group_of(ty: &str) -> FeatureGroup {
    match ty {
        "Class Feature" => FeatureGroup::Class,
        "Archetype Feature" => FeatureGroup::Subclass,
        "Racial Trait" => FeatureGroup::Species,
        "Feat" | "Feat Feature" => FeatureGroup::Feat,
        "Background Feature" => FeatureGroup::Background,
        _ => FeatureGroup::Other,
    }
}

fn section_rank(g: FeatureGroup) -> u8 {
    match g {
        FeatureGroup::Class | FeatureGroup::Subclass => 0,
        FeatureGroup::Species => 1,
        FeatureGroup::Feat => 2,
        FeatureGroup::Background => 3,
        FeatureGroup::Other => 4,
    }
}

/// Index of the class a feature belongs to, by its granting element name.
fn class_index(classes: &[ClassSnapshot], granted_by: Option<&str>) -> usize {
    let Some(g) = granted_by else { return 0 };
    classes
        .iter()
        .position(|c| {
            c.class.as_ref().is_some_and(|e| e.name == g)
                || c.archetypes.iter().any(|a| a.name == g)
        })
        .unwrap_or(0)
}

fn feature_text(f: &FeatureDetail, name: &str) -> String {
    let raw = f
        .sheet_summary
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map_or_else(
            || {
                crate::card_model::plain_paragraphs(&crate::table_text::tables_as_rows(
                    &f.description,
                ))
            },
            str::to_string,
        );
    evaluate_formulas(&strip_own_name(name, &raw))
}

/// Tables of a feature's description found in its sheet text (none for a sheet summary).
fn feature_tables(f: &FeatureDetail, text: &str) -> Vec<crate::model_types::TextTable> {
    let summarised = f
        .sheet_summary
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());
    if summarised {
        Vec::new()
    } else {
        crate::table_text::locate_tables(&f.description, text)
    }
}

/// Displayed features, de-duplicated by name (sources merged), formulas evaluated and ordered:
/// per class base before subclass at equal level, then species, feats, background.
pub(crate) fn features(snap: &CharacterSnapshot) -> Vec<FeatureData> {
    struct Keyed {
        key: (u8, usize, u32, u8, usize),
        data: FeatureData,
    }
    let mut out: Vec<Keyed> = Vec::new();
    for (idx, f) in snap.features.iter().enumerate() {
        let name = display_name(&f.name);
        let lower = name.to_lowercase();
        if !f.displayed || HIDDEN_FEATURES.contains(&lower.as_str()) || lower.ends_with(" subclass")
        {
            continue;
        }
        let group = group_of(&f.ty);
        let text = feature_text(f, &name);
        let source = f.granted_by.clone();
        if let Some(prev) = out
            .iter_mut()
            .find(|k| k.data.name.eq_ignore_ascii_case(&name))
        {
            if prev.data.text.is_empty() && !text.is_empty() {
                prev.data.summary = short_summary(&text);
                prev.data.tables = feature_tables(f, &text);
                prev.data.text = text;
            }
            if let Some(s) = source.filter(|s| !prev.data.sources.contains(s)) {
                prev.data.sources.push(s);
            }
            continue;
        }
        let ty_rank = u8::from(group == FeatureGroup::Subclass);
        out.push(Keyed {
            key: (
                section_rank(group),
                class_index(&snap.classes, f.granted_by.as_deref()),
                f.level,
                ty_rank,
                idx,
            ),
            data: FeatureData {
                id: f.id.clone(),
                name,
                group,
                class: matches!(group, FeatureGroup::Class | FeatureGroup::Subclass)
                    .then(|| f.granted_by.clone())
                    .flatten(),
                level: f.level,
                sources: source.into_iter().collect(),
                summary: short_summary(&text),
                tables: feature_tables(f, &text),
                text,
            },
        });
    }
    out.sort_by_key(|k| k.key);
    out.into_iter().map(|k| k.data).collect()
}

/// Registered element names of the given types, unique, in registration order.
pub(crate) fn element_names(snap: &CharacterSnapshot, tys: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for e in snap
        .elements
        .iter()
        .filter(|e| tys.contains(&e.element.ty.as_str()))
    {
        if !out.contains(&e.element.name) {
            out.push(e.element.name.clone());
        }
    }
    out
}

fn singular(s: &str) -> String {
    let l = s.trim().to_lowercase();
    l.strip_suffix('s').map_or(l.clone(), str::to_string)
}

fn push_unique(list: &mut Vec<String>, item: &str) {
    if !list.iter().any(|x| singular(x) == singular(item)) {
        list.push(item.trim().to_string());
    }
}

/// Category rows keep their place; members of a held category are dropped.
fn collapse(
    items: &[String],
    categories: &[(&str, &[&str])],
    out: &mut Vec<String>,
) -> Vec<String> {
    let lower: Vec<String> = items.iter().map(|s| s.to_lowercase()).collect();
    let has = |name: &str| lower.iter().any(|l| l == name);
    let mut covered: Vec<&[&str]> = Vec::new();
    for (cat, members) in categories {
        if has(cat) {
            covered.push(members);
        }
    }
    let mut extras = Vec::new();
    for item in items {
        let l = item.to_lowercase();
        let is_category = categories.iter().any(|(c, _)| *c == l);
        if is_category {
            push_unique(out, item);
        } else if !covered.iter().any(|m| m.contains(&l.as_str())) {
            extras.push(item.clone());
        }
    }
    extras
}

fn weapon_members(cat: &str) -> Vec<&'static str> {
    let parts: &[&[&str]] = match cat {
        "simple weapons" => &[SIMPLE_MELEE, SIMPLE_RANGED],
        "martial weapons" => &[MARTIAL_MELEE, MARTIAL_RANGED],
        "simple melee weapons" => &[SIMPLE_MELEE],
        "simple ranged weapons" => &[SIMPLE_RANGED],
        "martial melee weapons" => &[MARTIAL_MELEE],
        "martial ranged weapons" => &[MARTIAL_RANGED],
        _ => &[],
    };
    parts.iter().flat_map(|p| p.iter().copied()).collect()
}

/// "Crossbow, Light" -> "Light Crossbow": an inverted "Base, Qualifier" name is one item.
pub(crate) fn uninvert(name: &str) -> String {
    match name.split_once(',') {
        Some((base, qualifier)) if !qualifier.contains(',') && !base.trim().is_empty() => {
            format!("{} {}", qualifier.trim(), base.trim())
        }
        _ => name.trim().to_string(),
    }
}

fn normalise_weapons(items: &[String]) -> Vec<String> {
    let items: Vec<String> = items.iter().map(|i| uninvert(i)).collect();
    let items = items.as_slice();
    let cats = [
        "simple weapons",
        "martial weapons",
        "simple melee weapons",
        "simple ranged weapons",
        "martial melee weapons",
        "martial ranged weapons",
    ];
    let lower: Vec<String> = items.iter().map(|s| s.to_lowercase()).collect();
    // "Simple Weapons" subsumes its melee/ranged halves as well as its members.
    let subsumes = |cat: &str, other: &str| {
        let (a, b) = (weapon_members(cat), weapon_members(other));
        cat != other && !b.is_empty() && b.iter().all(|m| a.contains(m))
    };
    let held: Vec<&str> = cats
        .iter()
        .copied()
        .filter(|c| lower.iter().any(|l| l == c))
        .collect();
    let mut out = Vec::new();
    for (item, l) in items.iter().zip(&lower) {
        if cats.contains(&l.as_str()) {
            if !held.iter().any(|h| subsumes(h, l)) {
                push_unique(&mut out, item);
            }
        } else if !held.iter().any(|h| weapon_members(h).contains(&l.as_str())) {
            push_unique(&mut out, item);
        }
    }
    // Categories first, individually granted weapons after.
    out.sort_by_key(|w| !cats.contains(&w.to_lowercase().as_str()));
    out
}

fn normalise_armor(items: &[String]) -> Vec<String> {
    let items: Vec<String> = items.iter().map(|i| uninvert(i)).collect();
    let items = items.as_slice();
    let cats: [(&str, &[&str]); 3] = [
        ("light armor", LIGHT_ARMOR),
        ("medium armor", MEDIUM_ARMOR),
        ("heavy armor", HEAVY_ARMOR),
    ];
    let mut out = Vec::new();
    let extras = collapse(items, &cats, &mut out);
    for e in extras {
        push_unique(&mut out, &e);
    }
    out
}

/// Armour training ticks from the normalised armour list.
pub(crate) fn armor_training(armor: &[String]) -> ArmorTraining {
    let has = |n: &str| armor.iter().any(|a| a.eq_ignore_ascii_case(n));
    ArmorTraining {
        light: has("light armor"),
        medium: has("medium armor"),
        heavy: has("heavy armor"),
        shields: armor.iter().any(|a| singular(a) == "shield"),
    }
}

fn is_internal_label(prefix: &str, rest: &str) -> bool {
    prefix.to_lowercase().contains("group") || rest.to_lowercase().contains("group")
}

/// The text inside the outer parentheses of a proficiency name: only the closing parenthesis of
/// the name itself goes, so "Vehicles (water)" keeps its own.
fn proficiency_item(inside: &str) -> String {
    inside
        .strip_suffix(')')
        .unwrap_or(inside)
        .trim()
        .to_string()
}

#[cfg(test)]
mod proficiency_item_tests {
    use super::proficiency_item;

    #[test]
    fn keeps_nested_parentheses() {
        assert_eq!(proficiency_item("Vehicles (water))"), "Vehicles (water)");
        assert_eq!(proficiency_item("Club)"), "Club");
    }
}

/// Proficiencies without skill/save rows, internal group labels, duplicates or members of a
/// category the character holds as a whole.
pub(crate) fn proficiencies(snap: &CharacterSnapshot) -> ProficiencyData {
    let (mut armor, mut weapons, mut tools) = (Vec::new(), Vec::new(), Vec::new());
    for name in element_names(snap, &["Proficiency"]) {
        let Some((prefix, rest)) = name.split_once(" (") else {
            continue;
        };
        let item = proficiency_item(rest);
        if prefix.starts_with("Saving Throw")
            || prefix.starts_with("Skill")
            || is_internal_label(prefix, &item)
        {
            continue;
        }
        if prefix.starts_with("Armor") {
            armor.push(item);
        } else if prefix.starts_with("Weapon") {
            weapons.push(item);
        } else {
            tools.push(item);
        }
    }
    let mut unique_tools = Vec::new();
    for t in &tools {
        push_unique(&mut unique_tools, &uninvert(t));
    }
    let mut languages = Vec::new();
    for l in element_names(snap, &["Language"]) {
        push_unique(&mut languages, &l);
    }
    ProficiencyData {
        armor: normalise_armor(&armor),
        weapons: normalise_weapons(&weapons),
        tools: unique_tools,
        languages,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn products_are_evaluated_but_dice_and_words_are_not() {
        assert_eq!(
            evaluate_formulas("Restore up to 5x5 hit points"),
            "Restore up to 25 hit points"
        );
        assert_eq!(evaluate_formulas("10 x 3 feet"), "30 feet");
        assert_eq!(evaluate_formulas("deal 2d6x2 damage"), "deal 2d6x2 damage");
        assert_eq!(evaluate_formulas("0x1F and box5x"), "0x1F and box5x");
        assert_eq!(
            evaluate_formulas("DC {{missing:thing}} here"),
            "DC \u{2026} here"
        );
    }

    #[test]
    fn own_name_is_stripped_once_or_repeatedly() {
        assert_eq!(
            strip_own_name("Stonecunning", "Stonecunning. Whenever you"),
            "Whenever you"
        );
        assert_eq!(
            strip_own_name("Stonecunning", "Stonecunning. Stonecunning: Whenever"),
            "Whenever"
        );
        assert_eq!(strip_own_name("Rage", "Ragefully loud"), "Ragefully loud");
    }

    #[test]
    fn summary_skips_an_empty_lead_in_and_keeps_a_run_in_heading_with_its_sentence() {
        assert_eq!(
            short_summary(
                "You gain the following benefits. Initiative Proficiency. When you roll \
                 Initiative, you can add your Proficiency Bonus to the roll. Swap. More."
            ),
            "Initiative Proficiency. When you roll Initiative, you can add your Proficiency \
             Bonus to the roll."
        );
        assert_eq!(
            short_summary("You gain the following benefits. You learn two cantrips. More."),
            "You learn two cantrips."
        );
        assert_eq!(
            short_summary("You gain the following benefits."),
            "You gain the following benefits."
        );
    }

    #[test]
    fn summary_cuts_at_the_first_sentence_or_word_boundary() {
        assert_eq!(short_summary("One. Two."), "One.");
        let long = "word ".repeat(80);
        let s = short_summary(long.trim());
        assert!(s.chars().count() <= SUMMARY_MAX + 1 && s.ends_with('\u{2026}'));
    }

    #[test]
    fn weapons_collapse_into_held_categories() {
        let items: Vec<String> = [
            "Simple Weapons",
            "Simple Melee Weapons",
            "Club",
            "Dagger",
            "Simple Ranged Weapons",
            "Dart",
            "Martial Weapons",
            "Battleaxe",
            "Crossbow, Hand",
            "Warhammer",
            "Boomerang",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            normalise_weapons(&items),
            ["Simple Weapons", "Martial Weapons", "Boomerang"]
        );
        let lone: Vec<String> = ["Dagger", "Dart", "Dagger"].map(String::from).to_vec();
        assert_eq!(normalise_weapons(&lone), ["Dagger", "Dart"]);
    }

    #[test]
    fn armor_drops_members_and_the_shield_duplicate() {
        let items: Vec<String> = [
            "Light Armor",
            "Padded",
            "Leather",
            "Medium Armor",
            "Hide",
            "Shields",
            "Shield",
            "Plate",
        ]
        .map(String::from)
        .to_vec();
        let out = normalise_armor(&items);
        assert_eq!(out, ["Light Armor", "Medium Armor", "Shields", "Plate"]);
        let t = armor_training(&out);
        assert!(t.light && t.medium && t.shields && !t.heavy);
    }
}
