//! Item shapes the engine reads beyond plain statistics (`specs/handoffs/det-engine-1.md`,
//! "Representation"): the ammunition kind of ammunition and of the weapons that fire it, a
//! weapon's own critical range, and an attunement restriction as a requirements expression
//! built from `reqAttuneTags`.

use super::super::build::custom;
use super::{flag, strip_source};
use crate::core_ids;
use crate::model::Item;
use crate::util::title_case;
use borealis_content::Rule;
use serde_json::{json, Value};

/// Ammunition kind by 5etools flag.
const KIND_FLAGS: [(&str, &str); 6] = [
    ("arrow", "arrow"),
    ("bolt", "bolt"),
    ("bulletSling", "sling bullet"),
    ("needleBlowgun", "needle"),
    ("bulletFirearm", "bullet"),
    ("cellEnergy", "energy cell"),
];

/// Ammunition kind by a word of a name, in this order ("sling bullet" before "bullet").
const KIND_WORDS: [(&str, &str); 6] = [
    ("bolt", "bolt"),
    ("arrow", "arrow"),
    ("needle", "needle"),
    ("sling", "sling bullet"),
    ("bullet", "bullet"),
    ("energy cell", "energy cell"),
];

/// Kind of ammunition that fits every weapon with the Ammunition property.
const ANY_KIND: &str = "any";

/// Lowest and highest critical threshold a d20 can have.
const CRIT_RANGE: std::ops::RangeInclusive<i64> = 2..=20;

/// The nine alignments of the core pack, by (law axis, moral axis) code.
const LAW_AXIS: [(&str, &str); 3] = [("L", "LAWFUL"), ("NX", "NEUTRAL"), ("C", "CHAOTIC")];
const MORAL_AXIS: [(&str, &str); 3] = [("G", "GOOD"), ("NY", "NEUTRAL"), ("E", "EVIL")];
/// Neutral on whichever axis no other code names.
const NEUTRAL: &str = "N";
/// Any alignment.
const ANY_ALIGNMENT: &str = "A";

fn kind_in_name(name: &str) -> Option<&'static str> {
    let n = name.to_lowercase();
    KIND_WORDS
        .iter()
        .find(|(word, _)| n.contains(word))
        .map(|(_, kind)| *kind)
}

/// The `ammunition` setter of ammunition (type `A`/`AF`, or flagged `ammo`): the kind of its
/// 5etools flag, else the kind its name names, else `any`. A bundle such as `+1 Arrows (20)`
/// is filed under Equipment Packs and is only seen as ammunition through this setter.
#[must_use]
pub fn ammo_item_kind(it: &Item, code: &str) -> Option<&'static str> {
    if let Some((_, kind)) = KIND_FLAGS.iter().find(|(f, _)| flag(it, f)) {
        return Some(kind);
    }
    (matches!(code, "A" | "AF") || flag(it, "ammo"))
        .then(|| kind_in_name(&it.name).unwrap_or(ANY_KIND))
}

/// The kind a weapon fires, from its `ammoType` (`arrow|phb`, `renaissance bullet`).
#[must_use]
pub fn weapon_ammo_kind(it: &Item) -> Option<&'static str> {
    let ammo = it.extra.get("ammoType")?.as_str()?;
    kind_in_name(strip_source(ammo))
}

/// `critThreshold`: on a weapon a critical range of that weapon only; on anything else
/// (Butcher's Bib) a critical range of every weapon attack.
#[must_use]
pub fn crit_rule(it: &Item, is_weapon: bool) -> Option<Rule> {
    let min = match it.extra.get("critThreshold")? {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
    .filter(|n| CRIT_RANGE.contains(n))?;
    Some(if is_weapon {
        custom("critical_range", json!({"min": min, "item": "self"}))
    } else {
        custom(
            "critical_range",
            json!({"min": min, "applies_to": ["weapon"]}),
        )
    })
}

/// The attunement restriction of `reqAttuneTags` as a requirements expression: an OR over the
/// tag objects, an AND within one. `None` when a tag names something the engine cannot test
/// (a background, a creature type, a size, psionics, a skill or language proficiency): the
/// printed text then stays a note, as it is without tags.
#[must_use]
pub fn attune_requirements(it: &Item) -> Option<String> {
    let tags = it.extra.get("reqAttuneTags")?.as_array()?;
    let branches = tags
        .iter()
        .map(tag_requirement)
        .collect::<Option<Vec<_>>>()?;
    // a branch without any restriction (alignment "any") lets everyone attune
    if branches.is_empty() || branches.iter().any(Option::is_none) {
        return None;
    }
    let branches: Vec<String> = branches.into_iter().flatten().collect();
    Some(match branches.as_slice() {
        [one] => one.clone(),
        many => many
            .iter()
            .map(|b| format!("({b})"))
            .collect::<Vec<_>>()
            .join("||"),
    })
}

/// One tag object: `Some(None)` = no restriction, `None` = not testable.
fn tag_requirement(tag: &Value) -> Option<Option<String>> {
    let mut terms = Vec::new();
    let mut alignment = None;
    for (key, value) in tag.as_object()? {
        match key.as_str() {
            "class" => terms.push(format!(
                "[{}:1]",
                strip_source(value.as_str()?).trim().to_lowercase()
            )),
            "spellcasting" if value.as_bool() == Some(true) => {
                terms.push("[feature:spellcasting]".to_string());
            }
            // a race; a sub-race ("dwarf (mark of warding)") is printed by a name the
            // element does not carry word for word: text only
            "race" => {
                let race = strip_source(value.as_str()?).trim();
                if race.contains('(') {
                    return None;
                }
                terms.push(format!("[feature:{}]", title_case(race)));
            }
            "alignment" => alignment = alignment_requirement(value.as_array()?)?,
            other => {
                let ability = core_ids::ability_stat(other)?;
                terms.push(format!("[{ability}:{}]", value.as_i64()?));
            }
        }
    }
    if let Some(a) = alignment {
        terms.push(if terms.is_empty() {
            a
        } else {
            format!("({a})")
        });
    }
    Some((!terms.is_empty()).then(|| terms.join("&&")))
}

/// The alignments 5etools codes allow, as an `||` of core alignment ids that also holds while
/// no alignment is chosen (the engine's reading of "of good alignment"). `Some(None)` = any.
fn alignment_requirement(codes: &[Value]) -> Option<Option<String>> {
    let codes: Vec<&str> = codes.iter().filter_map(Value::as_str).collect();
    let known = |c: &str| {
        [NEUTRAL, ANY_ALIGNMENT].contains(&c)
            || LAW_AXIS.iter().chain(&MORAL_AXIS).any(|(k, _)| *k == c)
    };
    if codes.is_empty() || !codes.iter().all(|c| known(c)) {
        return None;
    }
    if codes.contains(&ANY_ALIGNMENT) {
        return Some(None);
    }
    let axis = |table: &[(&'static str, &'static str); 3], neutral: &str| {
        let named: Vec<&'static str> = table
            .iter()
            .filter(|(k, _)| codes.contains(k))
            .map(|(_, w)| *w)
            .collect();
        match (named.is_empty(), codes.contains(&NEUTRAL)) {
            (true, true) => vec![table
                .iter()
                .find(|(k, _)| *k == neutral)
                .map_or("", |t| t.1)],
            (true, false) => table.iter().map(|(_, w)| *w).collect(),
            (false, _) => named,
        }
    };
    let (law, moral) = (axis(&LAW_AXIS, "NX"), axis(&MORAL_AXIS, "NY"));
    let id = |l: &str, m: &str| match (l, m) {
        ("NEUTRAL", "NEUTRAL") => "ID_ALIGNMENT_NEUTRAL".to_string(),
        (l, m) => format!("ID_ALIGNMENT_{l}_{m}"),
    };
    let all: Vec<String> = LAW_AXIS
        .iter()
        .flat_map(|(_, l)| MORAL_AXIS.iter().map(move |(_, m)| id(l, m)))
        .collect();
    let allowed: Vec<String> = law
        .iter()
        .flat_map(|l| moral.iter().map(move |m| id(l, m)))
        .collect();
    if allowed.len() == all.len() {
        return Some(None);
    }
    Some(Some(format!(
        "{}||!({})",
        allowed.join("||"),
        all.join("||")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::expr::RequirementsExpr;

    fn item(raw: Value) -> Item {
        crate::model::parse(&raw).unwrap()
    }

    fn req(tags: Value) -> Option<String> {
        attune_requirements(&item(
            json!({"name": "T", "source": "ZZZ", "reqAttuneTags": tags}),
        ))
    }

    #[test]
    fn ammunition_kinds_come_from_flags_names_and_ammo_types() {
        let bundle = item(
            json!({"name": "+1 Arrows (20)", "source": "ZZZ", "type": "A",
            "arrow": true, "packContents": [{"item": "arrow|phb", "quantity": 20}]}),
        );
        assert_eq!(ammo_item_kind(&bundle, "A"), Some("arrow"));
        let sling = item(json!({"name": "Sling Bullets of X", "source": "ZZZ", "type": "A"}));
        assert_eq!(ammo_item_kind(&sling, "A"), Some("sling bullet"));
        let modern = item(
            json!({"name": "+2 Modern Bullet", "source": "ZZZ", "type": "AF|DMG",
            "bulletFirearm": true}),
        );
        assert_eq!(ammo_item_kind(&modern, "AF"), Some("bullet"));
        let generic = item(json!({"name": "+1 Ammunition", "source": "ZZZ", "ammo": true}));
        assert_eq!(ammo_item_kind(&generic, "GV"), Some("any"));
        let wand = item(json!({"name": "Wand of Arrows", "source": "ZZZ", "type": "WD"}));
        assert_eq!(ammo_item_kind(&wand, "WD"), None);
        for (ammo, kind) in [
            ("arrow|phb", Some("arrow")),
            ("crossbow bolt|phb", Some("bolt")),
            ("renaissance bullet", Some("bullet")),
            ("firearm bullet|xphb", Some("bullet")),
            ("blowgun needle|phb", Some("needle")),
            ("sling bullet|xphb", Some("sling bullet")),
            ("energy cell|xdmg", Some("energy cell")),
            ("pebble", None),
        ] {
            let bow = item(json!({"name": "Launcher", "source": "ZZZ", "ammoType": ammo}));
            assert_eq!(weapon_ammo_kind(&bow), kind, "{ammo}");
        }
    }

    #[test]
    fn crit_threshold_is_the_weapon_s_own_range() {
        let kas = item(json!({"name": "Blade", "source": "ZZZ", "critThreshold": 19}));
        let Some(Rule::Custom(c)) = crit_rule(&kas, true) else {
            panic!("no rule")
        };
        assert_eq!(c.kind, "critical_range");
        assert_eq!(c.fields["min"], 19);
        assert_eq!(c.fields["item"], "self");
        let Some(Rule::Custom(c)) = crit_rule(&kas, false) else {
            panic!("no rule")
        };
        assert_eq!(c.fields["applies_to"], json!(["weapon"]));
        assert!(!c.fields.contains_key("item"));
        let odd = item(json!({"name": "Odd", "source": "ZZZ", "critThreshold": 30}));
        assert!(crit_rule(&odd, true).is_none());
    }

    #[test]
    fn attunement_tags_become_requirements() {
        assert_eq!(
            req(json!([{"class": "cleric"}, {"class": "artificer|tce"}])).as_deref(),
            Some("([cleric:1])||([artificer:1])")
        );
        assert_eq!(
            req(json!([{"spellcasting": true}])).as_deref(),
            Some("[feature:spellcasting]")
        );
        assert_eq!(
            req(json!([{"race": "warforged|erlw"}])).as_deref(),
            Some("[feature:Warforged]")
        );
        assert_eq!(req(json!([{"race": "dwarf (mark of warding)|erlw"}])), None);
        assert_eq!(
            req(json!([{"int": 3}])).as_deref(),
            Some("[intelligence:3]")
        );
        let moon = req(json!([{"race": "elf", "alignment": ["N", "G"]},
                              {"race": "half-elf", "alignment": ["N", "G"]}]))
        .unwrap();
        assert!(
            moon.starts_with(
                "([feature:Elf]&&(ID_ALIGNMENT_NEUTRAL_GOOD||!(ID_ALIGNMENT_LAWFUL_GOOD||"
            ),
            "{moon}"
        );
        assert!(moon.contains(")||([feature:Half-Elf]&&(ID_ALIGNMENT_NEUTRAL_GOOD||!("));
        // every requirement is valid requirement grammar
        assert!(RequirementsExpr::parse(&moon).is_ok(), "{moon}");
        // a background, a creature type or psionics is not testable: text only
        assert_eq!(req(json!([{"background": "x|ggr"}])), None);
        assert_eq!(
            req(json!([{"class": "bard"}, {"creatureType": "humanoid"}])),
            None
        );
        assert_eq!(req(json!([{"psionics": true}])), None);
        assert_eq!(req(json!([])), None);
    }

    #[test]
    fn alignment_codes_name_the_allowed_alignments() {
        let ids = |codes: Value| {
            let r = req(json!([{ "alignment": codes }]))?;
            let (allowed, _) = r.split_once("||!(").unwrap();
            Some(allowed.split("||").map(str::to_string).collect::<Vec<_>>())
        };
        assert_eq!(
            ids(json!(["G"])).unwrap(),
            [
                "ID_ALIGNMENT_LAWFUL_GOOD",
                "ID_ALIGNMENT_NEUTRAL_GOOD",
                "ID_ALIGNMENT_CHAOTIC_GOOD"
            ]
        );
        assert_eq!(ids(json!(["N"])).unwrap(), ["ID_ALIGNMENT_NEUTRAL"]);
        assert_eq!(
            ids(json!(["C", "N"])).unwrap(),
            ["ID_ALIGNMENT_CHAOTIC_NEUTRAL"]
        );
        assert_eq!(
            ids(json!(["L", "E"])).unwrap(),
            ["ID_ALIGNMENT_LAWFUL_EVIL"]
        );
        // non-lawful: six alignments, none lawful
        let non_lawful = ids(json!(["NX", "C", "G", "NY", "E"])).unwrap();
        assert_eq!(non_lawful.len(), 6);
        assert!(non_lawful.iter().all(|a| !a.contains("LAWFUL")));
        // any alignment, or every alignment listed: no restriction
        assert_eq!(req(json!([{ "alignment": ["A"] }])), None);
        assert_eq!(
            req(json!([{ "alignment": ["L", "NX", "C", "G", "NY", "E"] }])),
            None
        );
        assert_eq!(req(json!([{ "alignment": ["Q"] }])), None);
    }
}
