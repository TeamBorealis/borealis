//! Golden vectors for the 2024 rule kinds (resources, toggles, weapon mastery, martial arts,
//! exhaustion, Heroic Inspiration). Numbers come from the SRD 5.2.1 class tables.
use borealis_content::{ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::{AbilityMethod, CharacterState};
use borealis_engine::{Choice, Engine};
use std::path::Path;
use std::sync::OnceLock;

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let store = ContentStoreBuilder::new()
            .add_pack_dir(&root.join("core"))
            .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1")))
            .expect("packs load")
            .build();
        Engine::new(&store)
    })
}

const LEVEL1: &str = "ID_LEVEL_1";
const ACOLYTE: &str = "ID_2024_BACKGROUND_ACOLYTE";
const GREATSWORD: &str = "ID_2024_WEAPON_GREATSWORD";

struct Hero {
    s: CharacterState,
}

impl Hero {
    fn new(class: &str, class_name: &str) -> Self {
        let array = engine()
            .universe(Ruleset::Dnd2024)
            .tables()
            .standard_array_for(class_name)
            .expect("class array");
        let mut h = Self {
            s: commands::new_character(engine(), Ruleset::Dnd2024),
        };
        h.pick(LEVEL1, "Race", 1, "ID_2024_SPECIES_DWARF")
            .pick(LEVEL1, "Class", 1, class)
            .pick(LEVEL1, "Background", 1, ACOLYTE);
        h.s = commands::set_ability_method(&h.s, AbilityMethod::StandardArray);
        h.s = commands::set_base_scores(engine(), &h.s, array)
            .expect("scores")
            .state;
        let snap = h.snap();
        let slot = snap
            .slots
            .iter()
            .find(|s| s.owner.id == ACOLYTE && s.list_choice.is_none() && !s.list_items.is_empty())
            .expect("ability slot");
        let item = slot
            .list_items
            .iter()
            .find(|(_, t)| t == "Wisdom +2, Charisma +1")
            .expect("entry")
            .0;
        let key = slot.key.clone();
        h.s = commands::select(engine(), &h.s, &key, Choice::ListItem(item))
            .expect("list select")
            .state;
        h
    }

    fn snap(&self) -> CharacterSnapshot {
        engine().derive(&self.s)
    }

    fn pick(&mut self, owner: &str, name: &str, ordinal: u32, id: &str) -> &mut Self {
        let snap = self.snap();
        let key = slot_key(&snap, owner, name, ordinal)
            .unwrap_or_else(|| panic!("no open slot {owner}/{name}#{ordinal}"));
        self.s = commands::select(engine(), &self.s, &key, Choice::Element(id.into()))
            .unwrap_or_else(|e| panic!("select {id}: {e}"))
            .state;
        self
    }

    fn level_to(&mut self, n: u32) -> &mut Self {
        while self.s.level() < n {
            self.s = commands::level_up_main(engine(), &self.s, None)
                .expect("level up")
                .state;
        }
        self
    }

    fn equip(&mut self, item: &str) -> &mut Self {
        self.s = commands::add_item(engine(), &self.s, item, 1)
            .expect("add item")
            .state;
        let uid = self.s.inventory.items.last().expect("item").uid.clone();
        self.s = commands::equip(engine(), &self.s, &uid, None)
            .expect("equip")
            .state;
        self
    }
}

fn res<'a>(
    snap: &'a CharacterSnapshot,
    id: &str,
) -> &'a borealis_engine::snapshot::ResourceSnapshot {
    snap.resources
        .iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("no resource {id}: {:?}", snap.resources))
}

/// Barbarian 5 (SRD table: Rages 3, Rage Damage +2). Dwarf Barbarian STR 15 (+2), greatsword 2d6.
/// Rage on adds the +2 Rage Damage to a Strength melee attack.
#[test]
fn c1_level5_barbarian_rages_rage_damage_and_toggle() {
    let mut h = Hero::new("ID_2024_CLASS_BARBARIAN", "Barbarian");
    h.level_to(5).equip(GREATSWORD);
    let snap = h.snap();
    let rage = res(&snap, "rage");
    assert_eq!((rage.max, rage.current), (3, 3));
    assert_eq!(rage.short_rest_regain, Some(1));
    assert!(rage.long_rest_regain_all);
    let toggle = snap.toggles.iter().find(|t| t.id == "rage").expect("rage");
    assert!(!toggle.on);
    assert!(
        toggle.effects.iter().any(|e| e.contains("+2")),
        "{:?}",
        toggle.effects
    );
    let off = snap
        .attacks
        .iter()
        .find(|a| a.name == "Greatsword")
        .expect("atk");
    assert_eq!(off.damage, "2d6+2 slashing");

    h.s = commands::spend_resource(engine(), &h.s, "rage", 1)
        .expect("spend")
        .state;
    h.s = commands::set_toggle(engine(), &h.s, "rage", true)
        .expect("toggle")
        .state;
    let snap = h.snap();
    assert_eq!(res(&snap, "rage").current, 2);
    let on = snap
        .attacks
        .iter()
        .find(|a| a.name == "Greatsword")
        .expect("atk");
    assert_eq!(on.damage, "2d6+4 slashing");
    let unarmed = snap
        .attacks
        .iter()
        .find(|a| a.uid == "unarmed")
        .expect("unarmed");
    assert!(
        unarmed.damage.starts_with("5 "),
        "1 + STR 2 + Rage 2: {}",
        unarmed.damage
    );
    // a long rest ends the Rage and refills the uses
    let rested = commands::long_rest(engine(), &h.s).expect("rest").state;
    let snap = engine().derive(&rested);
    assert_eq!(res(&snap, "rage").current, 3);
    assert!(!snap.toggles[0].on);
    // spending more than is left is refused
    assert!(commands::spend_resource(engine(), &rested, "rage", 4).is_err());
}

/// Monk 5 (SRD table: Martial Arts 1d8, Focus Points 5, proficiency +3). Dwarf Monk DEX 15 (+2).
#[test]
fn c1_level5_monk_focus_points_martial_arts_and_unarmed_strike() {
    let mut h = Hero::new("ID_2024_CLASS_MONK", "Monk");
    h.level_to(5);
    let snap = h.snap();
    assert_eq!(res(&snap, "focus points").max, 5);
    assert_eq!(res(&snap, "focus points").short_rest_regain, Some(5));
    let ma = snap.martial_arts.as_ref().expect("martial arts");
    assert_eq!(ma.die, "1d8");
    assert!(ma.active && ma.bonus_unarmed_strike);
    let dex = snap.abilities[1].modifier;
    let str_ = snap.abilities[0].modifier;
    assert!(dex > str_, "Monk array favours Dexterity");
    let u = snap
        .attacks
        .iter()
        .find(|a| a.uid == "unarmed")
        .expect("unarmed");
    assert_eq!(u.ability, "dexterity");
    assert_eq!(u.attack, 3 + dex);
    assert_eq!(u.damage, format!("1d8+{dex} bludgeoning"));
    // wearing armor switches Martial Arts off
    h.equip("ID_2024_ARMOR_CHAIN_MAIL");
    let ma = h.snap().martial_arts.expect("martial arts");
    assert!(!ma.active);
}

/// Fighter 1 (SRD table: Weapon Mastery 3). Greatsword has the Graze property.
#[test]
fn c1_level1_fighter_three_masteries_and_graze_on_a_greatsword() {
    let mut h = Hero::new("ID_2024_CLASS_FIGHTER", "Fighter");
    h.equip(GREATSWORD);
    let snap = h.snap();
    let slots: Vec<_> = snap
        .slots
        .iter()
        .filter(|s| s.name == "Weapon Mastery")
        .collect();
    assert_eq!(slots.len(), 3);
    let a = snap
        .attacks
        .iter()
        .find(|a| a.name == "Greatsword")
        .expect("atk");
    assert_eq!(
        a.mastery.as_ref().map(|m| (m.name.as_str(), m.mastered)),
        Some(("Graze", false))
    );
    h.pick(
        "ID_2024_CLASS_FEATURE_FIGHTER_WEAPON_MASTERY",
        "Weapon Mastery",
        1,
        GREATSWORD,
    );
    let snap = h.snap();
    let a = snap
        .attacks
        .iter()
        .find(|a| a.name == "Greatsword")
        .expect("atk");
    let m = a.mastery.as_ref().expect("mastery");
    assert!(m.mastered && m.text.contains("misses"), "{m:?}");
    assert_eq!(snap.masteries.len(), 1);
    assert_eq!(snap.masteries[0].property, "Graze");
    // level 4 grows the table to 4 slots
    h.level_to(4);
    let n = h
        .snap()
        .slots
        .iter()
        .filter(|s| s.name == "Weapon Mastery")
        .count();
    assert_eq!(n, 4);
}

/// Regression: every Weapon Mastery slot counts, not only the first (the slot index of a chosen
/// weapon was read as a rule index, so slots 2 and 3 were dropped from `snapshot.masteries`).
#[test]
fn c1_fighter_all_three_mastery_picks_are_listed() {
    let mut h = Hero::new("ID_2024_CLASS_FIGHTER", "Fighter");
    let picks = [
        "ID_2024_WEAPON_LONGSWORD",
        "ID_2024_WEAPON_DAGGER",
        GREATSWORD,
    ];
    for (i, w) in picks.iter().enumerate() {
        h.pick(
            "ID_2024_CLASS_FEATURE_FIGHTER_WEAPON_MASTERY",
            "Weapon Mastery",
            u32::try_from(i + 1).expect("ordinal"),
            w,
        );
    }
    let props: Vec<_> = h
        .snap()
        .masteries
        .iter()
        .map(|m| (m.weapon.clone(), m.property.clone()))
        .collect();
    assert_eq!(
        props,
        [
            ("ID_2024_WEAPON_LONGSWORD".to_string(), "Sap".to_string()),
            ("ID_2024_WEAPON_DAGGER".to_string(), "Nick".to_string()),
            (GREATSWORD.to_string(), "Graze".to_string()),
        ]
    );
}

/// A Wizard who multiclasses into Fighter 1 gets the Fighter's level 1 Weapon Mastery (table
/// value 3 at Fighter level 1), and all three picks are listed.
#[test]
fn c1_wizard_multiclass_fighter_1_gets_three_masteries() {
    let mut h = Hero::new("ID_2024_CLASS_WIZARD", "Wizard");
    h.s = commands::set_base_scores(engine(), &h.s, [13, 14, 12, 15, 10, 8])
        .expect("scores")
        .state;
    h.level_to(2);
    h.s = commands::level_up_new_multiclass(engine(), &h.s, None)
        .expect("new multiclass level")
        .state;
    let lvl = h.s.level();
    h.pick(
        &format!("ID_LEVEL_{lvl}"),
        &format!("Multiclass (Level {lvl})"),
        1,
        "ID_2024_MULTICLASS_FIGHTER",
    );
    let owner = "ID_2024_CLASS_FEATURE_FIGHTER_WEAPON_MASTERY";
    let n = h
        .snap()
        .slots
        .iter()
        .filter(|s| s.name == "Weapon Mastery")
        .count();
    assert_eq!(n, 3);
    for (i, w) in [
        "ID_2024_WEAPON_DAGGER",
        "ID_2024_WEAPON_LONGSWORD",
        GREATSWORD,
    ]
    .iter()
    .enumerate()
    {
        h.pick(
            owner,
            "Weapon Mastery",
            u32::try_from(i + 1).expect("ordinal"),
            w,
        );
    }
    assert_eq!(h.snap().masteries.len(), 3);
}

/// Exhaustion 3: D20 Tests -6, Speed -15 (dwarf 30 ft -> 15), death at 6, long rest -1.
#[test]
fn c1_exhaustion_is_linear_and_kills_at_six() {
    let h = Hero::new("ID_2024_CLASS_FIGHTER", "Fighter");
    let base = h.snap();
    assert_eq!(base.exhaustion.level, 0);
    let s3 = commands::set_exhaustion(engine(), &h.s, 3)
        .expect("set")
        .state;
    let snap = engine().derive(&s3);
    assert_eq!(snap.exhaustion.d20_penalty, -6);
    assert_eq!(snap.exhaustion.speed_penalty, -15);
    assert_eq!(snap.speed.walk, base.speed.walk - 15);
    assert_eq!(snap.initiative, base.initiative - 6);
    assert_eq!(snap.abilities[0].save, base.abilities[0].save - 6);
    assert_eq!(snap.skills[0].total, base.skills[0].total - 6);
    assert!(!snap.exhaustion.dead);
    let s6 = commands::set_exhaustion(engine(), &h.s, 6)
        .expect("set")
        .state;
    let dead = engine().derive(&s6);
    assert!(dead.exhaustion.dead);
    assert_eq!(dead.speed.walk, 0);
    assert!(commands::set_exhaustion(engine(), &h.s, 7).is_err());
    let rested = commands::long_rest(engine(), &s3).expect("rest").state;
    assert_eq!(engine().derive(&rested).exhaustion.level, 2);
}

/// Cleric 5 (SRD table: Channel Divinity 2). A Short Rest returns one use, a Long Rest all.
#[test]
fn c1_short_rest_restores_one_channel_divinity() {
    let mut h = Hero::new("ID_2024_CLASS_CLERIC", "Cleric");
    h.level_to(5);
    let id = "channel divinity cleric";
    let snap = h.snap();
    assert_eq!(res(&snap, id).max, 2);
    h.s = commands::spend_resource(engine(), &h.s, id, 2)
        .expect("spend")
        .state;
    assert_eq!(res(&h.snap(), id).current, 0);
    let short = commands::short_rest(engine(), &h.s).expect("short").state;
    assert_eq!(res(&engine().derive(&short), id).current, 1);
    let long = commands::long_rest(engine(), &short).expect("long").state;
    assert_eq!(res(&engine().derive(&long), id).current, 2);
}

/// Heroic Inspiration is a flag; Human Resourceful grants it on a Long Rest.
#[test]
fn c1_heroic_inspiration_flag_and_command() {
    let h = Hero::new("ID_2024_CLASS_FIGHTER", "Fighter");
    assert!(!h.snap().inspiration);
    let s = commands::set_inspiration(&h.s, true).state;
    assert!(engine().derive(&s).inspiration);
    let s = commands::set_inspiration(&s, false).state;
    assert!(!engine().derive(&s).inspiration);
    assert!(engine()
        .derive(&h.s)
        .diagnostics
        .iter()
        .all(|d| d.code != "profile.unimplemented"));
}

/// Rage requires not wearing Heavy armor: the switch stays on but has no effect.
#[test]
fn c1_heavy_armor_blocks_the_rage_effects() {
    let mut h = Hero::new("ID_2024_CLASS_BARBARIAN", "Barbarian");
    h.equip(GREATSWORD);
    h.s = commands::set_toggle(engine(), &h.s, "rage", true)
        .expect("toggle")
        .state;
    assert!(h.snap().toggles[0].active);
    h.equip("ID_2024_ARMOR_PLATE_ARMOR");
    let snap = h.snap();
    let t = &snap.toggles[0];
    assert!(t.on && !t.active && t.blocked_by.is_some(), "{t:?}");
    let a = snap
        .attacks
        .iter()
        .find(|a| a.name == "Greatsword")
        .expect("atk");
    assert_eq!(a.damage, "2d6+2 slashing");
}

/// The Monk `martial arts:dice` stat must agree with the SRD 5.2.1 table (1d6/1d8/1d10/1d12 at
/// levels 1/5/11/17) that the engine's `martial_arts` path reads.
#[test]
fn c1_monk_martial_arts_dice_stat_matches_the_class_table() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../content/srd-5.2.1/classes/monk.json"
    );
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut stats: Vec<(u64, String)> = Vec::new();
    let mut stack = vec![&doc];
    while let Some(v) = stack.pop() {
        match v {
            serde_json::Value::Object(m) => {
                if m.get("name").and_then(|n| n.as_str()) == Some("martial arts:dice") {
                    stats.push((
                        m.get("level").and_then(|l| l.as_u64()).unwrap_or(1),
                        m["value"].as_str().unwrap().to_string(),
                    ));
                }
                stack.extend(m.values());
            }
            serde_json::Value::Array(a) => stack.extend(a.iter()),
            _ => {}
        }
    }
    stats.sort();
    let expected = [(1, "6"), (5, "8"), (11, "10"), (17, "12")].map(|(l, v)| (l, v.to_string()));
    assert_eq!(stats, expected);
    for (level, die) in [(1, "1d6"), (5, "1d8"), (11, "1d10"), (17, "1d12")] {
        let mut h = Hero::new("ID_2024_CLASS_MONK", "Monk");
        h.level_to(level);
        assert_eq!(h.snap().martial_arts.expect("martial arts").die, die);
    }
}
