//! 2014 -> 2024 conversion over the bundled core, SRD 5.1 and SRD 5.2.1 packs.

use borealis_character::{
    apply_conversion, convert_to_2024_with, Category, Character, ConversionPreview, ConvertError,
    Outcome,
};
use borealis_content::{ContentFile, ContentStore, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key, Improvement};
use borealis_engine::{CharacterState, Choice, Engine};
use std::path::Path;
use std::sync::OnceLock;

const SYNTHETIC: &str = r#"{"format":"borealis-content/1","ruleset":"dnd2014","elements":[
{"id":"ID_HOMEBREW_ITEM_ODDITY","name":"Oddity","type":"Item","source":"Homebrew"}]}"#;

fn world() -> &'static (ContentStore, Engine) {
    static W: OnceLock<(ContentStore, Engine)> = OnceLock::new();
    W.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let mut b = ContentStoreBuilder::new();
        for p in ["core", "srd-5.1", "srd-5.2.1"] {
            b = b.add_pack_dir(&root.join(p)).expect("pack loads");
        }
        let file = ContentFile::from_json_str(SYNTHETIC).expect("synthetic parses");
        let store = b
            .add_file("homebrew.json", PackOrigin::Custom, file)
            .build();
        let engine = Engine::new(&store);
        (store, engine)
    })
}

fn pick(s: CharacterState, owner: &str, name: &str, id: &str) -> CharacterState {
    let e = &world().1;
    let snap = e.derive(&s);
    let key = slot_key(&snap, owner, name, 1).unwrap_or_else(|| panic!("slot {name}"));
    commands::select(e, &s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("{id}: {err}"))
        .state
}

fn level_to(mut s: CharacterState, level: u32) -> CharacterState {
    while s.level() < level {
        s = commands::level_up_main(&world().1, &s, None).unwrap().state;
    }
    s
}

fn start(race: &str, class: &str, background: &str) -> CharacterState {
    let mut s = commands::new_character(&world().1, Ruleset::Dnd2014);
    s = pick(s, "ID_LEVEL_1", "Race", race);
    s = pick(s, "ID_LEVEL_1", "Class", class);
    pick(s, "ID_LEVEL_1", "Background", background)
}

fn hill_dwarf_life_cleric() -> Character {
    let mut s = level_to(
        start(
            "ID_SRD_RACE_DWARF",
            "ID_WOTC_PHB_CLASS_CLERIC",
            "ID_BACKGROUND_ACOLYTE",
        ),
        5,
    );
    s = pick(
        s,
        "ID_RACIAL_TRAIT_DWARVEN_SUBRACE",
        "Dwarven Subrace",
        "ID_SUB_RACE_HILL_DWARF",
    );
    s = pick(
        s,
        "ID_WOTC_PHB_CLASS_FEATURE_CLERIC_DIVINEDOMAIN",
        "Divine Domain",
        "ID_WOTC_PHB_ARCHETYPE_LIFEDOMAIN",
    );
    s = commands::choose_improvement(&world().1, &s, "Cleric", 4, &Improvement::PlusTwo(4))
        .unwrap()
        .state;
    let mut c = Character::new("Brunhild");
    c.meta.id = "orig-1".into();
    c.build = s;
    c
}

fn convert(c: &Character) -> ConversionPreview {
    let (store, engine) = world();
    convert_to_2024_with(store, engine, c).expect("converts")
}

fn find<'a>(p: &'a ConversionPreview, cat: Category, label: &str) -> &'a Outcome {
    &p.items
        .iter()
        .find(|i| i.category == cat && i.label == label)
        .unwrap_or_else(|| panic!("{cat:?} {label} in {:#?}", p.items))
        .outcome
}

fn has(p: &ConversionPreview, cat: Category, pred: impl Fn(&Outcome) -> bool) -> bool {
    p.items
        .iter()
        .any(|i| i.category == cat && pred(&i.outcome))
}

#[test]
fn hill_dwarf_life_cleric_maps_to_2024() {
    let p = convert(&hill_dwarf_life_cleric());
    assert_eq!(
        find(&p, Category::Species, "Dwarf"),
        &Outcome::Mapped {
            from: "ID_SRD_RACE_DWARF".into(),
            to: "ID_2024_SPECIES_DWARF".into(),
            kind: Some(borealis_content::MapKind::Approximate),
        }
    );
    assert!(matches!(
        find(&p, Category::Species, "Hill Dwarf"),
        Outcome::Mapped { to, .. } if to == "ID_2024_SPECIES_DWARF"
    ));
    assert!(matches!(
        find(&p, Category::Class, "Cleric"),
        Outcome::Mapped { to, .. } if to == "ID_2024_CLASS_CLERIC"
    ));
    assert!(matches!(
        find(&p, Category::Subclass, "Life Domain"),
        Outcome::Mapped { to, .. } if to == "ID_2024_SUBCLASS_CLERIC_LIFE_DOMAIN"
    ));
    assert!(matches!(
        find(&p, Category::Background, "Acolyte"),
        Outcome::Mapped { to, .. } if to == "ID_2024_BACKGROUND_ACOLYTE"
    ));
    // racial increases dropped; the background distribution is pending
    assert!(matches!(
        find(&p, Category::AbilityScore, "Racial ability score increases"),
        Outcome::Dropped { .. }
    ));
    assert!(matches!(
        find(&p, Category::AbilityScore, "Acolyte ability scores"),
        Outcome::NeedsChoice { what } if what.contains("ability score")
    ));
    assert_eq!(p.before.level, 5);
    assert_eq!(p.after.level, 5);
    assert_eq!(p.before.ability_scores[2], 12, "2014 racial Constitution");
    assert_eq!(p.after.ability_scores[2], 10, "no racial increase in 2024");
}

#[test]
fn asi_level_becomes_an_asi_feat_with_the_same_allocation() {
    let p = convert(&hill_dwarf_life_cleric());
    assert!(matches!(
        find(&p, Category::Feat, "Ability Score Improvement (Cleric 4)"),
        Outcome::Mapped { to, .. } if to == "ID_2024_FEAT_ABILITY_SCORE_IMPROVEMENT"
    ));
    assert_eq!(p.after.ability_scores[4], 12, "Wisdom +2 re-applied");
    assert!(!has(&p, Category::AbilityScore, |o| matches!(
        o, Outcome::NeedsChoice { what } if what.contains("Cleric 4")
    )));
}

#[test]
fn unresolved_choices_stay_empty_slots() {
    let p = convert(&hill_dwarf_life_cleric());
    let pending: Vec<_> = p.pending().collect();
    assert!(pending.iter().any(|i| i.category == Category::Language));
    let snap = world().1.derive(&apply_conversion(&p).build);
    assert!(snap
        .slots
        .iter()
        .any(|s| s.chosen.is_none() && s.requires_selection));
}

#[test]
fn fighter_champion_maps() {
    let mut s = level_to(
        start(
            "ID_RACE_HUMAN",
            "ID_WOTC_PHB_CLASS_FIGHTER",
            "ID_BACKGROUND_ACOLYTE",
        ),
        3,
    );
    s = pick(
        s,
        "ID_WOTC_PHB_CLASS_FEATURE_MARTIALARCHETYPE",
        "Martial Archetype",
        "ID_WOTC_PHB_ARCHETYPE_CHAMPION",
    );
    let mut c = Character::new("Rolf");
    c.build = s;
    let p = convert(&c);
    assert!(matches!(
        find(&p, Category::Subclass, "Champion"),
        Outcome::Mapped { to, .. } if to == "ID_2024_SUBCLASS_FIGHTER_CHAMPION"
    ));
    assert!(matches!(
        find(&p, Category::Class, "Fighter"),
        Outcome::Mapped { to, .. } if to == "ID_2024_CLASS_FIGHTER"
    ));
    assert_eq!(p.after.level, 3);
}

#[test]
fn spells_and_items_map_by_id() {
    let mut c = hill_dwarf_life_cleric();
    c.build = pick(
        c.build,
        "ID_WOTC_PHB_CLASS_FEATURE_CLERIC_SPELLCASTING_CLERIC",
        "Cantrip (Cleric)",
        "ID_PHB_SPELL_GUIDANCE",
    );
    c.build = commands::add_item(&world().1, &c.build, "ID_WOTC_PHB_WEAPON_LONGSWORD", 1)
        .unwrap()
        .state;
    let p = convert(&c);
    assert!(matches!(
        find(&p, Category::Spell, "Guidance"),
        Outcome::Mapped { to, .. } if to == "ID_2024_SPELL_GUIDANCE"
    ));
    assert!(matches!(
        find(&p, Category::Item, "Longsword"),
        Outcome::Mapped { to, .. } if to == "ID_2024_WEAPON_LONGSWORD"
    ));
    let items = &apply_conversion(&p).build.inventory.items;
    assert!(items.iter().any(|i| i.item == "ID_2024_WEAPON_LONGSWORD"));
}

fn with_homebrew(strict: bool) -> Character {
    let mut c = hill_dwarf_life_cleric();
    c.build.allow_cross_edition = strict.then_some(false);
    for id in ["ID_HOMEBREW_ITEM_ODDITY", "ID_GONE_FROM_CONTENT"] {
        c.build
            .inventory
            .items
            .push(borealis_engine::state::InventoryItem::new(
                format!("u-{id}"),
                id,
            ));
    }
    c
}

#[test]
fn unmapped_homebrew_is_dropped_with_a_reason_when_cross_edition_is_off() {
    let p = convert(&with_homebrew(true));
    assert!(matches!(
        find(&p, Category::Item, "Oddity"),
        Outcome::Dropped { reason, .. } if reason.contains("cross-edition")
    ));
    assert!(matches!(
        find(&p, Category::Item, "ID_GONE_FROM_CONTENT"),
        Outcome::Dropped { reason, .. } if reason.contains("not defined")
    ));
    let kept = apply_conversion(&p).build.inventory.items;
    assert!(kept
        .iter()
        .all(|i| !i.item.starts_with("ID_HOMEBREW") && i.item != "ID_GONE_FROM_CONTENT"));
}

#[test]
fn unmapped_homebrew_stays_as_cross_edition_pick_when_allowed() {
    let p = convert(&with_homebrew(false));
    assert!(matches!(
        find(&p, Category::Item, "Oddity"),
        Outcome::Kept {
            cross_edition: true,
            ..
        }
    ));
}

#[test]
fn original_is_unchanged_and_the_copy_is_new() {
    let original = hill_dwarf_life_cleric();
    let before = original.clone();
    let p = convert(&original);
    let copy = apply_conversion(&p);
    assert_eq!(original, before);
    assert_ne!(copy.meta.id, original.meta.id);
    assert!(!copy.meta.id.is_empty());
    assert_eq!(copy.meta.converted_from.as_deref(), Some("orig-1"));
    assert_eq!(copy.meta.name, "Brunhild (2024)");
    assert_eq!(copy.ruleset(), Ruleset::Dnd2024);
    assert_eq!(original.ruleset(), Ruleset::Dnd2014);
    let again = apply_conversion(&convert(&original));
    assert_ne!(again.meta.id, copy.meta.id);
}

#[test]
fn only_2014_characters_convert() {
    let (store, engine) = (&world().0, &world().1);
    let c = Character::with_ruleset("New", Ruleset::Dnd2024);
    let err = convert_to_2024_with(store, engine, &c).unwrap_err();
    assert!(matches!(err, ConvertError::NotDnd2014(_)));
}
