//! st3-golden findings fixed in the engine (det-engine-5), on the bundled SRD packs: F5 species
//! and feat casting blocks, F4 Breath Weapon rows, F9 proficiency filters, F10 Pact of the
//! Blade, F11/F13 feature levels, F14 table free casts, F16 replaced known spells.

use borealis_content::{ContentStoreBuilder, Ruleset};
use borealis_engine::commands;
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::{CharacterState, EquipSlot};
use borealis_engine::{Choice, Engine};
use std::path::Path;

fn engine(pack: &str) -> Option<Engine> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let dir = root.join(pack);
    if !dir.join("pack.json").is_file() {
        eprintln!("SKIPPED: content/{pack} not present");
        return None;
    }
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&dir))
        .expect("packs load")
        .build();
    Some(Engine::new(&store))
}

/// Fill the first open slot named `slot` with the option (or list entry) named `pick`.
fn choose(e: &Engine, s: &CharacterState, slot: &str, pick: &str) -> CharacterState {
    let snap = e.derive(s);
    let sl = snap
        .slots
        .iter()
        .find(|k| {
            k.name == slot
                && k.chosen.is_none()
                && k.list_choice.is_none()
                && k.rejected.is_none()
                && (k.options.iter().any(|o| o.name == pick)
                    || k.list_items.iter().any(|(_, t)| t == pick))
        })
        .unwrap_or_else(|| {
            let open: Vec<&str> = snap
                .slots
                .iter()
                .filter(|k| k.chosen.is_none())
                .map(|k| k.name.as_str())
                .collect();
            panic!("no open slot {slot} offering {pick}; open: {open:?}")
        });
    let choice = sl.options.iter().find(|o| o.name == pick).map_or_else(
        || {
            let id = sl.list_items.iter().find(|(_, t)| t == pick).unwrap().0;
            Choice::ListItem(id)
        },
        |o| Choice::Element(o.id.clone()),
    );
    commands::select(e, s, &sl.key, choice)
        .unwrap_or_else(|err| panic!("{slot}/{pick}: {err}"))
        .state
}

fn hero(e: &Engine, ruleset: Ruleset, picks: &[(&str, &str)], levels: u32) -> CharacterState {
    let mut s = commands::new_character(e, ruleset);
    for (slot, pick) in picks {
        s = choose(e, &s, slot, pick);
    }
    for _ in 1..levels {
        s = commands::level_up_main(e, &s, None).unwrap().state;
    }
    s
}

fn modifier(snap: &CharacterSnapshot, i: usize) -> i32 {
    snap.abilities[i].modifier
}

#[test]
fn species_lineage_spells_form_a_casting_block_with_their_chosen_ability() {
    let Some(e) = engine("srd-5.2.1") else { return };
    let s = hero(
        &e,
        Ruleset::Dnd2024,
        &[
            ("Race", "Gnome"),
            ("Gnomish Lineage", "Forest Gnome"),
            ("Lineage Spellcasting Ability", "Wisdom"),
            ("Class", "Wizard"),
        ],
        1,
    );
    let snap = e.derive(&s);
    let b = snap.caster("Forest Gnome").expect("Forest Gnome block");
    assert_eq!(b.ability, "Wisdom");
    assert_eq!(b.dc, 8 + snap.proficiency_bonus + modifier(&snap, 4));
    assert_eq!(b.attack, snap.proficiency_bonus + modifier(&snap, 4));
    let cantrips: Vec<&str> = b
        .spells
        .iter()
        .filter(|x| x.level == 0)
        .map(|x| x.name.as_str())
        .collect();
    assert_eq!(cantrips, vec!["Minor Illusion"]);
    let always: Vec<&str> = b
        .spells
        .iter()
        .filter(|x| x.always_prepared)
        .map(|x| x.name.as_str())
        .collect();
    assert_eq!(always, vec!["Speak with Animals"]);
}

#[test]
fn a_legacy_block_takes_the_traits_name_and_its_cantrips_get_attack_rows() {
    let Some(e) = engine("srd-5.2.1") else { return };
    let s = hero(
        &e,
        Ruleset::Dnd2024,
        &[
            ("Race", "Tiefling"),
            ("Fiendish Legacy", "Infernal"),
            ("Lineage Spellcasting Ability", "Charisma"),
            ("Class", "Fighter"),
        ],
        1,
    );
    let snap = e.derive(&s);
    let b = snap
        .caster("Infernal Legacy")
        .expect("Infernal Legacy block");
    let cantrips: Vec<&str> = b
        .spells
        .iter()
        .filter(|x| x.level == 0)
        .map(|x| x.name.as_str())
        .collect();
    assert_eq!(
        cantrips,
        vec!["Fire Bolt", "Thaumaturgy"],
        "Otherworldly Presence joins"
    );
    let row = snap
        .attacks
        .iter()
        .find(|a| a.name == "Fire Bolt")
        .expect("Fire Bolt row");
    assert_eq!(row.attack, b.attack);
    assert_eq!(row.damage, "1d10 fire");
}

#[test]
fn magic_initiate_has_a_block_per_spell_list_with_its_ability() {
    let Some(e) = engine("srd-5.2.1") else { return };
    let s = hero(
        &e,
        Ruleset::Dnd2024,
        &[
            ("Race", "Human"),
            ("Background", "Acolyte"),
            ("Class", "Fighter"),
            ("Spellcasting Ability", "Wisdom"),
        ],
        1,
    );
    let s = choose(&e, &s, "Magic Initiate Cantrips", "Sacred Flame");
    let s = choose(&e, &s, "Magic Initiate Level 1 Spell", "Bless");
    let snap = e.derive(&s);
    let b = snap
        .spellcasting
        .iter()
        .find(|b| b.name.starts_with("Magic Initiate"))
        .unwrap_or_else(|| {
            panic!(
                "no Magic Initiate block: {:?}",
                snap.spellcasting
                    .iter()
                    .map(|b| &b.name)
                    .collect::<Vec<_>>()
            )
        });
    assert_eq!(b.name, "Magic Initiate (Cleric)");
    assert_eq!(b.ability, "Wisdom");
    assert_eq!(b.dc, 8 + snap.proficiency_bonus + modifier(&snap, 4));
    assert!(b
        .spells
        .iter()
        .any(|x| x.name == "Bless" && x.always_prepared));
    assert!(b
        .spells
        .iter()
        .any(|x| x.name == "Sacred Flame" && !x.always_prepared));
    let row = snap
        .attacks
        .iter()
        .find(|a| a.name == "Sacred Flame")
        .expect("Sacred Flame row");
    assert_eq!(row.save.as_ref().map(|v| v.dc), Some(b.dc));
}

#[test]
fn breath_weapon_rows_in_both_editions() {
    if let Some(e) = engine("srd-5.2.1") {
        let s = hero(
            &e,
            Ruleset::Dnd2024,
            &[
                ("Race", "Dragonborn"),
                ("Draconic Ancestry", "Gold Dragon"),
                ("Class", "Fighter"),
            ],
            1,
        );
        let snap = e.derive(&s);
        let row = snap
            .attacks
            .iter()
            .find(|a| a.name == "Breath Weapon")
            .expect("2024 row");
        assert_eq!(row.damage, "1d10 fire");
        let save = row.save.as_ref().expect("save");
        assert_eq!(save.ability, "dexterity");
        assert_eq!(save.dc, 8 + snap.proficiency_bonus + modifier(&snap, 2));
    }
    if let Some(e) = engine("srd-5.1") {
        let s = hero(
            &e,
            Ruleset::Dnd2014,
            &[
                ("Race", "Dragonborn"),
                ("Draconic Ancestry", "Draconic Ancestry (Red)"),
                ("Class", "Fighter"),
            ],
            6,
        );
        let snap = e.derive(&s);
        let row = snap
            .attacks
            .iter()
            .find(|a| a.name == "Breath Weapon")
            .expect("2014 row");
        assert_eq!(row.damage, "3d6 fire", "2d6, 3d6 at 6th level");
        let save = row.save.as_ref().expect("save");
        assert_eq!(save.ability, "dexterity");
        assert_eq!(save.dc, 8 + snap.proficiency_bonus + modifier(&snap, 2));
    }
}

fn add(e: &Engine, s: &CharacterState, id: &str) -> (CharacterState, String) {
    let next = commands::add_item(e, s, id, 1).expect("add item").state;
    let uid = next.inventory.items.last().unwrap().uid.clone();
    (next, uid)
}

fn row<'a>(
    snap: &'a CharacterSnapshot,
    name: &str,
) -> &'a borealis_engine::snapshot::AttackSnapshot {
    snap.attacks
        .iter()
        .find(|a| a.name == name)
        .unwrap_or_else(|| panic!("no {name} row"))
}

#[test]
fn a_rogue_filter_proficiency_covers_martial_finesse_weapons_only() {
    let Some(e) = engine("srd-5.2.1") else { return };
    let s = hero(
        &e,
        Ruleset::Dnd2024,
        &[("Race", "Human"), ("Class", "Rogue")],
        1,
    );
    let snap = e.derive(&s);
    let mastery = snap
        .slots
        .iter()
        .find(|k| k.name == "Weapon Mastery")
        .expect("mastery slot");
    assert!(mastery.options.iter().any(|o| o.name == "Shortsword"));
    assert!(!mastery.options.iter().any(|o| o.name == "Longsword"));
    let (s, sword) = add(&e, &s, "ID_2024_WEAPON_SHORTSWORD");
    let s = commands::equip(&e, &s, &sword, Some(EquipSlot::PrimaryHand))
        .unwrap()
        .state;
    let snap = e.derive(&s);
    assert!(row(&snap, "Shortsword").proficient);
    let (s, long) = add(&e, &s, "ID_2024_WEAPON_LONGSWORD");
    let s = commands::equip(&e, &s, &long, Some(EquipSlot::PrimaryHand))
        .unwrap()
        .state;
    assert!(!row(&e.derive(&s), "Longsword").proficient);
}

#[test]
fn pact_of_the_blade_bonds_the_first_carried_melee_weapon() {
    let Some(e) = engine("srd-5.2.1") else { return };
    let s = hero(
        &e,
        Ruleset::Dnd2024,
        &[
            ("Race", "Human"),
            ("Class", "Warlock"),
            ("Eldritch Invocation (Warlock 1)", "Pact of the Blade"),
        ],
        1,
    );
    let (s, long) = add(&e, &s, "ID_2024_WEAPON_LONGSWORD");
    let (s, dagger) = add(&e, &s, "ID_2024_WEAPON_DAGGER");
    let bonded = commands::equip(&e, &s, &long, Some(EquipSlot::PrimaryHand))
        .unwrap()
        .state;
    let snap = e.derive(&bonded);
    let r = row(&snap, "Longsword");
    assert!(r.proficient, "the bond gives proficiency");
    if modifier(&snap, 5) > modifier(&snap, 0) {
        assert_eq!(r.ability, "charisma");
    }
    let other = commands::equip(&e, &s, &dagger, Some(EquipSlot::PrimaryHand))
        .unwrap()
        .state;
    let snap = e.derive(&other);
    assert_ne!(
        row(&snap, "Dagger").ability,
        "charisma",
        "one bonded weapon"
    );
}

fn feature_levels(snap: &CharacterSnapshot, name: &str) -> Vec<u32> {
    snap.features
        .iter()
        .filter(|f| f.name == name)
        .map(|f| f.level)
        .collect()
}

#[test]
fn feature_levels_follow_the_owner_and_repeated_feat_selects() {
    let Some(e) = engine("srd-5.2.1") else { return };
    let s = hero(
        &e,
        Ruleset::Dnd2024,
        &[("Race", "Human"), ("Class", "Paladin")],
        2,
    );
    let s = choose(&e, &s, "Fighting Style (Paladin)", "Defense");
    assert_eq!(feature_levels(&e.derive(&s), "Defense"), vec![2]);

    let s = hero(
        &e,
        Ruleset::Dnd2024,
        &[("Race", "Human"), ("Class", "Fighter")],
        6,
    );
    let levels: Vec<u32> = e
        .derive(&s)
        .features
        .iter()
        .filter(|f| f.name == "Ability Score Improvement" && f.ty == "Class Feature")
        .map(|f| f.level)
        .collect();
    assert_eq!(
        levels,
        vec![4, 6],
        "one entry per Ability Score Improvement level reached"
    );
}

#[test]
fn favored_enemy_casts_read_the_class_table() {
    let Some(e) = engine("srd-5.2.1") else { return };
    let s = hero(
        &e,
        Ruleset::Dnd2024,
        &[("Race", "Human"), ("Class", "Ranger")],
        5,
    );
    let snap = e.derive(&s);
    let fc = snap
        .free_casts
        .iter()
        .find(|f| f.spell == "ID_2024_SPELL_HUNTERS_MARK")
        .expect("Hunter's Mark free casts");
    assert_eq!(fc.uses, Some(3), "Favored Enemy column, Ranger 5");
}

#[test]
fn a_replaced_known_spell_may_be_of_any_level_with_slots() {
    let Some(e) = engine("srd-5.1") else { return };
    let mut s = hero(
        &e,
        Ruleset::Dnd2014,
        &[("Race", "Human"), ("Class", "Warlock")],
        1,
    );
    s = choose(&e, &s, "Spell (Warlock)", "Charm Person");
    s = choose(&e, &s, "Spell (Warlock)", "Comprehend Languages");
    for _ in 0..2 {
        s = commands::level_up_main(&e, &s, None).unwrap().state;
    }
    s = choose(&e, &s, "Spell (Warlock)", "Expeditious Retreat");
    s = choose(&e, &s, "Spell (Warlock)", "Hellish Rebuke");
    let snap = e.derive(&s);
    let open = |snap: &CharacterSnapshot| {
        snap.slots
            .iter()
            .any(|k| k.chosen.is_none() && k.options.iter().any(|o| o.name == "Hold Person"))
    };
    assert!(!open(&snap), "every known-spell slot is filled");
    let first = snap
        .slots
        .iter()
        .find(|k| k.chosen.as_ref().is_some_and(|c| c.name == "Charm Person"))
        .expect("Charm Person slot");
    assert!(
        !first.options.iter().any(|o| o.name == "Hold Person"),
        "authored filter: 1st level"
    );
    let reopened = commands::replace_spell(&e, &s, &first.key)
        .expect("replace")
        .state;
    assert!(
        open(&e.derive(&reopened)),
        "the reopened slot offers a 2nd-level spell"
    );
    let s2 = choose(&e, &reopened, "Spell (Warlock)", "Hold Person");
    assert!(e.derive(&s2).has_element("ID_PHB_SPELL_HOLD_PERSON"));
}
