//! WB-28 in-app help: the fields a novice can't read from their label alone carry one plain help line (spec G4,
//! 3.2, 6.1), on every kind that shows them. The help is short and names no id, expression or file format.

use borealis_authoring::blocks::catalog;
use borealis_authoring::form::kinds;

/// The parts every form shares (spec 3.2) that need a line; Name and Source read on their own.
const SHARED: &[&str] = &[
    "edition",
    "tags",
    "requirements",
    "prerequisiteText",
    "allowDuplicate",
    "keywords",
    "rules",
    "description",
    "sheet",
    "compendium",
    "mapsTo",
];

/// Kind fields whose label alone leaves a novice guessing (kind id, field key).
const KIND_FIELDS: &[(&str, &str)] = &[
    ("spell", "materialConsumed"),
    ("feat", "casts"),
    ("feat", "casterType"),
    ("feat", "preparation"),
    ("feat", "multiclassSlots"),
    ("feat", "preparedChange"),
    ("class", "primaryAbilities"),
    ("class", "multiclass"),
    ("class", "levels"),
    ("classFeature", "asiLevels"),
    ("classFeature", "epicBoon"),
    ("subclass", "levelGained"),
    ("species", "flySpeed"),
    ("species", "blindsight"),
    ("race", "flySpeed"),
    ("race", "languages"),
    ("race", "maleNames"),
    ("background", "ideals"),
    ("creature", "skills"),
    ("creature", "senses"),
    ("creature", "legendary"),
    ("creature", "lairActions"),
    ("creature", "regionalEffects"),
    ("item", "stackable"),
    ("item", "consumable"),
    ("item", "container"),
    ("weapon", "properties"),
    ("armor", "strength"),
    ("magicItem", "rarity"),
    ("magicItem", "attunement"),
];

/// Core block fields (spec 6.1) whose label alone leaves a novice guessing (block kind, field key).
const BLOCK_FIELDS: &[(&str, &str)] = &[
    ("select", "number"),
    ("select", "supports"),
    ("select", "optional"),
    ("select", "default"),
    ("select", "allow_replace"),
    ("list", "name"),
    ("stat", "equipped"),
    ("stat", "maximum"),
    ("stat", "alt"),
    ("stat", "inline"),
];

/// Longest help line, in characters: a line, not a paragraph.
const MAX_HELP: usize = 170;
/// Words that are file-format or engine talk, not plain words (spec 0.4: no JSON, ids, expressions).
const JARGON: &[&str] = &[
    "json",
    " id ",
    "ids",
    "xml",
    "expression",
    "setter",
    "supports",
    "{{",
];

fn check_line(owner: &str, help: &str) -> Vec<String> {
    let mut bad = Vec::new();
    if help.chars().count() > MAX_HELP {
        bad.push(format!(
            "{owner}: help is {} characters",
            help.chars().count()
        ));
    }
    let padded = format!(" {} ", help.to_lowercase());
    for word in JARGON {
        // the name format help shows the placeholders it asks for
        if *word == "{{" && owner.ends_with(".nameFormat") {
            continue;
        }
        if padded.contains(word) {
            bad.push(format!("{owner}: help says {word:?}"));
        }
    }
    bad
}

#[test]
fn shared_fields_have_help_on_every_kind() {
    let mut missing = Vec::new();
    for k in kinds() {
        for f in k.fields.iter().filter(|f| SHARED.contains(&f.key)) {
            if f.help.is_none() {
                missing.push(format!("{}.{}", k.id, f.key));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "shared fields without help: {missing:?}"
    );
}

#[test]
fn unclear_kind_fields_have_help() {
    let missing: Vec<String> = KIND_FIELDS
        .iter()
        .filter(|(kind, key)| {
            let k = kinds()
                .iter()
                .find(|k| k.id == *kind)
                .unwrap_or_else(|| panic!("no kind {kind}"));
            let f = k
                .fields
                .iter()
                .find(|f| f.key == *key)
                .unwrap_or_else(|| panic!("no field {kind}.{key}"));
            f.help.is_none()
        })
        .map(|(kind, key)| format!("{kind}.{key}"))
        .collect();
    assert!(missing.is_empty(), "fields without help: {missing:?}");
}

#[test]
fn unclear_core_block_fields_have_help() {
    let missing: Vec<String> = BLOCK_FIELDS
        .iter()
        .filter(|(kind, key)| {
            let b = catalog()
                .iter()
                .find(|b| b.kind == *kind)
                .unwrap_or_else(|| panic!("no block {kind}"));
            let f = b
                .fields
                .iter()
                .find(|f| f.key == *key)
                .unwrap_or_else(|| panic!("no field {kind}.{key}"));
            f.help.is_none()
        })
        .map(|(kind, key)| format!("{kind}.{key}"))
        .collect();
    assert!(missing.is_empty(), "block fields without help: {missing:?}");
}

#[test]
fn every_help_line_is_short_and_plain() {
    let mut bad = Vec::new();
    for k in kinds() {
        for f in &k.fields {
            if let Some(h) = f.help {
                bad.extend(check_line(&format!("{}.{}", k.id, f.key), h));
            }
        }
    }
    for b in catalog() {
        for f in b.fields {
            if let Some(h) = f.help {
                bad.extend(check_line(&format!("{}.{}", b.kind, f.key), h));
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}
