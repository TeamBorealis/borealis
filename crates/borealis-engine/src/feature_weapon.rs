//! Weapons a feature gives (`EngineSettings::feature_weapons`, off by default).
//!
//! Some features say in their text that a part of the character "counts as a simple ranged
//! weapon" (a launcher built into a harness, a natural weapon), with its damage and range, but carry
//! no Weapon element: the weapon cannot be wielded, listed in the attacks table or used as the
//! base of a magic weapon. Aurora has no such element either. With the setting on, the universe
//! gets one derived Weapon element per such feature, read from the feature's own text:
//!
//! - `count(s) as a simple|martial melee|ranged weapon` gives its category tag,
//! - the first `deals NdM <type> damage` after it gives its damage (a text without one makes none),
//! - `normal range of N feet and a long range of M feet` gives its range.
//!
//! A text that says "counts as a … weapon" more than once describes several weapons (a choice
//! between models) and makes none. The sheet text is read when the feature has one, else its
//! description. The derived weapon costs and weighs nothing, is held in one hand, and names the
//! proficiency of its category as its `proficiency` (`Simple Melee Weapons` for a simple melee
//! weapon): as the text says it counts as such a weapon, a character proficient with that
//! category is proficient with it, and holding the feature alone does not make them so. Its
//! `feature` setter names the feature, so lists can offer it only to characters that have it.

use borealis_content::{Element, Setter};

/// Prefix of a derived feature weapon id (followed by the feature id without its `ID_`).
pub const ID_PREFIX: &str = "ID_BOREALIS_FEATURE_WEAPON_";

/// Setter naming the feature a derived weapon comes from.
pub const FEATURE_SETTER: &str = "feature";

/// Element types whose text is read: features, traits and feats.
fn is_feature_type(ty: &str) -> bool {
    ty.ends_with("Feature") || ty.contains("Trait") || ty == "Feat"
}

/// Plain lower-case text: tags removed, typographic apostrophes and whitespace normalised.
fn plain(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' => in_tag = false,
            _ if in_tag => {}
            '\u{2019}' | '\u{2018}' => out.push('\''),
            c => out.extend(c.to_lowercase()),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text read for an element: its sheet entries, else its description.
fn feature_text(el: &Element) -> String {
    let sheet: Vec<&str> = el
        .sheet
        .iter()
        .flat_map(|s| s.descriptions.iter())
        .map(|d| d.text.as_str())
        .collect();
    if sheet.is_empty() {
        plain(&el.description)
    } else {
        plain(&sheet.join(" "))
    }
}

/// What a feature's text says about the weapon it counts as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeaponText {
    /// `simple` or `martial`.
    pub class: String,
    /// `melee` or `ranged`.
    pub reach: String,
    /// Damage dice (`1d4`), empty when the text names none.
    pub dice: String,
    /// Damage type (`acid`), empty when the text names none.
    pub damage_type: String,
    /// `N/M` feet, empty when the text names none.
    pub range: String,
}

const COUNTS: [&str; 2] = ["counts as a ", "count as a "];

/// Positions just after each "count(s) as a" whose next words are `<class> <reach> weapon`.
fn weapon_phrases(text: &str) -> Vec<(usize, String, String)> {
    let mut found = Vec::new();
    for pat in COUNTS {
        let mut from = 0;
        while let Some(i) = text[from..].find(pat) {
            let at = from + i + pat.len();
            from = at;
            // "counts as a" is also the tail of "count as a"; keep one of the two
            if pat == COUNTS[1] && text[..at - pat.len()].ends_with('s') {
                continue;
            }
            let mut words = text[at..].split(|c: char| !c.is_alphanumeric());
            let (Some(class), Some(reach), Some(noun)) = (words.next(), words.next(), words.next())
            else {
                continue;
            };
            if matches!(class, "simple" | "martial")
                && matches!(reach, "melee" | "ranged")
                && noun.starts_with("weapon")
            {
                found.push((at, class.to_string(), reach.to_string()));
            }
        }
    }
    found
}

/// `NdM` at the start of `s` (digits, `d`, digits).
fn dice_at(s: &str) -> Option<&str> {
    let n = s.chars().take_while(char::is_ascii_digit).count();
    let rest = s[n..].strip_prefix('d')?;
    let m = rest.chars().take_while(char::is_ascii_digit).count();
    (n > 0 && m > 0).then(|| &s[..n + 1 + m])
}

/// The first `deals NdM <type> damage` in `text`.
fn damage_in(text: &str) -> Option<(String, String)> {
    let mut from = 0;
    while let Some(i) = text[from..].find("deals ") {
        let at = from + i + "deals ".len();
        from = at;
        let Some(dice) = dice_at(&text[at..]) else {
            continue;
        };
        let mut words = text[at + dice.len()..].split_whitespace();
        if let (Some(ty), Some(noun)) = (words.next(), words.next()) {
            if noun.starts_with("damage") && ty.chars().all(char::is_alphabetic) {
                return Some((dice.to_string(), ty.to_string()));
            }
        }
    }
    None
}

/// The number before " feet" at the start of `s`.
fn feet_at(s: &str) -> Option<&str> {
    let n = s.chars().take_while(char::is_ascii_digit).count();
    (n > 0 && s[n..].trim_start().starts_with("feet")).then(|| &s[..n])
}

/// `normal range of N feet and a long range of M feet` in `text`, as `N/M`.
fn range_in(text: &str) -> Option<String> {
    let i = text.find("normal range of ")?;
    let normal = feet_at(&text[i + "normal range of ".len()..])?;
    let j = text[i..].find("long range of ")? + i;
    let long = feet_at(&text[j + "long range of ".len()..])?;
    Some(format!("{normal}/{long}"))
}

/// The weapon a feature's text says it counts as, when it describes exactly one.
#[must_use]
pub fn read(el: &Element) -> Option<WeaponText> {
    if !is_feature_type(&el.ty) {
        return None;
    }
    let text = feature_text(el);
    let phrases = weapon_phrases(&text);
    let [(at, class, reach)] = phrases.as_slice() else {
        return None;
    };
    let after = &text[*at..];
    // no damage in the text: not a weapon description to build one from
    let (dice, damage_type) = damage_in(after)?;
    let range = if reach == "ranged" {
        range_in(after).unwrap_or_default()
    } else {
        String::new()
    };
    Some(WeaponText {
        class: class.clone(),
        reach: reach.clone(),
        dice,
        damage_type,
        range,
    })
}

/// The Proficiency element of the weapon category the text names (`simple` `melee`:
/// Simple Melee Weapons), granted by Simple Weapons and the like.
fn category_proficiency(w: &WeaponText) -> String {
    format!(
        "ID_PROFICIENCY_WEAPON_PROFICIENCY_{}_{}_WEAPONS",
        w.class.to_uppercase(),
        w.reach.to_uppercase()
    )
}

/// The derived Weapon element of feature `el`, when its text describes one weapon.
#[must_use]
pub fn derive(el: &Element) -> Option<Element> {
    let w = read(el)?;
    let tag = format!(
        "ID_INTERNAL_WEAPON_CATEGORY_{}_{}",
        w.class.to_uppercase(),
        w.reach.to_uppercase()
    );
    let mut supports = vec![tag];
    if !w.damage_type.is_empty() {
        supports.push(format!(
            "ID_INTERNAL_DAMAGE_TYPE_{}",
            w.damage_type.to_uppercase()
        ));
    }
    let mut setters = vec![
        Setter::new("category", "Weapons"),
        Setter::new("cost", "0"),
        Setter::new("weight", "0 lb."),
        Setter::new("slot", "onehand"),
        Setter::new("proficiency", category_proficiency(&w)),
        Setter::new(FEATURE_SETTER, el.id.clone()),
    ];
    if !w.dice.is_empty() {
        let mut d = Setter::new("damage", w.dice.clone());
        if !w.damage_type.is_empty() {
            d.attributes.insert("type".into(), w.damage_type.clone());
        }
        setters.push(d);
    }
    if !w.range.is_empty() {
        setters.push(Setter::new("range", w.range.clone()));
    }
    Some(Element {
        id: format!("{ID_PREFIX}{}", el.id.strip_prefix("ID_").unwrap_or(&el.id)),
        name: el.name.clone(),
        ty: "Weapon".into(),
        source: el.source.clone(),
        ruleset: el.ruleset,
        supports,
        description: el.description.clone(),
        setters,
        ..Element::default()
    })
}

/// The feature a derived weapon comes from.
#[must_use]
pub fn source_feature(el: &Element) -> Option<&str> {
    if !el.id.starts_with(ID_PREFIX) {
        return None;
    }
    el.setter_value(FEATURE_SETTER)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feature(text: &str) -> Element {
        Element {
            id: "ID_TEST_FEATURE_TIDE_SLING".into(),
            name: "Tide Sling".into(),
            ty: "Class Feature".into(),
            description: format!("<p>{text}</p>"),
            ..Element::default()
        }
    }

    #[test]
    fn reads_a_ranged_weapon_with_damage_and_range() {
        let f = feature(
            "A pipe on your wrist counts as a simple ranged weapon, with a normal range of \
             30 feet and a long range of 120 feet, and it deals 1d4 acid damage on a hit.",
        );
        let w = read(&f).unwrap();
        assert_eq!((w.class.as_str(), w.reach.as_str()), ("simple", "ranged"));
        assert_eq!((w.dice.as_str(), w.damage_type.as_str()), ("1d4", "acid"));
        assert_eq!(w.range, "30/120");
        let el = derive(&f).unwrap();
        assert_eq!(el.ty, "Weapon");
        assert_eq!(el.id, "ID_BOREALIS_FEATURE_WEAPON_TEST_FEATURE_TIDE_SLING");
        assert!(el
            .supports
            .contains(&"ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_RANGED".to_string()));
        assert_eq!(
            el.setter_value("proficiency"),
            Some("ID_PROFICIENCY_WEAPON_PROFICIENCY_SIMPLE_RANGED_WEAPONS")
        );
        assert_eq!(source_feature(&el), Some(f.id.as_str()));
    }

    #[test]
    fn reads_a_melee_weapon_and_plural_wording() {
        let f = feature(
            "Your hooves each count as a martial melee weapon while empty, and each deals 1d10 \
             radiant damage on a hit.",
        );
        let w = read(&f).unwrap();
        assert_eq!((w.class.as_str(), w.reach.as_str()), ("martial", "melee"));
        assert_eq!(w.dice, "1d10");
        assert!(w.range.is_empty());
    }

    #[test]
    fn two_weapons_in_one_text_make_none() {
        let f = feature(
            "Pick a model. Hooves: they count as a simple melee weapon. Sling: it counts as a \
             simple ranged weapon.",
        );
        assert_eq!(read(&f), None);
    }

    #[test]
    fn a_weapon_without_damage_dice_makes_none() {
        let f = feature("Your claws count as a simple melee weapon. You choose the damage later.");
        assert_eq!(read(&f), None);
    }

    #[test]
    fn items_and_other_wording_are_ignored() {
        let mut item = feature("It counts as a simple melee weapon.");
        item.ty = "Magic Item".into();
        assert_eq!(read(&item), None);
        assert_eq!(read(&feature("It counts as a spellcasting focus.")), None);
    }

    #[test]
    fn sheet_text_wins_over_the_description() {
        let mut f =
            feature("Long story. It counts as a simple ranged weapon and deals 1d4 fire damage.");
        assert!(read(&f).is_some(), "the description alone describes one");
        f.sheet = Some(borealis_content::Sheet {
            descriptions: vec![borealis_content::SheetEntry {
                text: "You design your armor for stealth.".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        assert_eq!(read(&f), None);
    }
}
