//! Golden vectors for the 2024 rule kinds (defences, senses, advantages, expertise, crit range,
//! attack and skill bonuses, carry capacity). Numbers come from SRD 5.2.1.
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
    fn new(species: &str, class: &str, class_name: &str) -> Self {
        let array = engine()
            .universe(Ruleset::Dnd2024)
            .tables()
            .standard_array_for(class_name)
            .expect("class array");
        let mut h = Self {
            s: commands::new_character(engine(), Ruleset::Dnd2024),
        };
        h.pick(LEVEL1, "Race", 1, species)
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

    fn archetype(&mut self, id: &str) -> &mut Self {
        let snap = self.snap();
        let key = snap
            .slots
            .iter()
            .find(|s| s.ty == "Archetype")
            .expect("archetype slot")
            .key
            .clone();
        self.s = commands::select(engine(), &self.s, &key, Choice::Element(id.into()))
            .unwrap_or_else(|e| panic!("select {id}: {e}"))
            .state;
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

const DWARF: &str = "ID_2024_SPECIES_DWARF";

fn attack<'a>(
    snap: &'a CharacterSnapshot,
    name: &str,
) -> &'a borealis_engine::snapshot::AttackSnapshot {
    snap.attacks
        .iter()
        .find(|a| a.name == name)
        .unwrap_or_else(|| panic!("no attack {name}"))
}

/// SRD Champion: Improved Critical (level 3) crit on 19-20; Superior Critical (level 15) 18-20.
#[test]
fn c2_champion_critical_range_19_then_18() {
    let mut h = Hero::new(DWARF, "ID_2024_CLASS_FIGHTER", "Fighter");
    h.equip(GREATSWORD);
    assert_eq!(attack(&h.snap(), "Greatsword").crit_range, 20);
    h.level_to(3).archetype("ID_2024_SUBCLASS_FIGHTER_CHAMPION");
    let snap = h.snap();
    assert_eq!(attack(&snap, "Greatsword").crit_range, 19);
    assert_eq!(attack(&snap, "Unarmed Strike").crit_range, 19);
    h.level_to(15);
    assert_eq!(attack(&h.snap(), "Greatsword").crit_range, 18);
}

/// SRD Rogue: Expertise at level 1 (two skills), two more at level 6.
#[test]
fn c2_rogue_level1_expertise_two_skills() {
    let mut h = Hero::new(DWARF, "ID_2024_CLASS_ROGUE", "Rogue");
    let owner = "ID_2024_CLASS_FEATURE_ROGUE_EXPERTISE";
    let before = h.snap().skill("insight").unwrap().total;
    h.pick(owner, "Expertise", 1, "ID_EXPERTISE_SKILL_INSIGHT")
        .pick(owner, "Expertise", 2, "ID_EXPERTISE_SKILL_RELIGION");
    let snap = h.snap();
    let insight = snap.skill("insight").unwrap();
    assert!(insight.expertise && insight.proficient);
    assert_eq!(insight.total, before + 2, "proficiency +2 doubled");
    assert!(snap.skill("religion").unwrap().expertise);
    assert!(!snap.skill("stealth").unwrap().expertise);
    let open = |s: &CharacterSnapshot| {
        s.slots
            .iter()
            .filter(|x| x.owner.id == owner && x.name == "Expertise")
            .count()
    };
    assert_eq!(open(&snap), 2);
    h.level_to(6);
    assert_eq!(open(&h.snap()), 4, "two more at level 6");
}

/// SRD Barbarian: Rage gives resistance to Bludgeoning, Piercing and Slashing while active.
#[test]
fn c2_rage_resistance_only_while_toggled() {
    let mut h = Hero::new(DWARF, "ID_2024_CLASS_BARBARIAN", "Barbarian");
    let names = |s: &CharacterSnapshot, active: bool| {
        s.defenses
            .resistances
            .iter()
            .filter(|r| r.source == "Rage" && r.active == active)
            .map(|r| r.name.clone())
            .collect::<Vec<_>>()
    };
    let off = h.snap();
    assert_eq!(names(&off, false), ["bludgeoning", "piercing", "slashing"]);
    assert!(names(&off, true).is_empty());
    h.s = commands::set_toggle(engine(), &h.s, "rage", true)
        .expect("toggle")
        .state;
    let on = h.snap();
    assert_eq!(names(&on, true), ["bludgeoning", "piercing", "slashing"]);
    let adv = on
        .advantages
        .iter()
        .filter(|a| a.while_toggle.as_deref() == Some("rage"))
        .collect::<Vec<_>>();
    assert_eq!(adv.len(), 2);
    assert!(adv.iter().all(|a| a.active));
    assert!(adv
        .iter()
        .any(|a| a.text == "Advantage on Strength saving throws"));
}

/// Berserker Mindless Rage: immune to Charmed and Frightened while raging.
#[test]
fn c2_berserker_condition_immunity_while_raging() {
    let mut h = Hero::new(DWARF, "ID_2024_CLASS_BARBARIAN", "Barbarian");
    h.level_to(6)
        .archetype("ID_2024_SUBCLASS_BARBARIAN_PATH_OF_THE_BERSERKER");
    let imm = |s: &CharacterSnapshot| {
        s.defenses
            .condition_immunities
            .iter()
            .map(|c| (c.name.clone(), c.active))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        imm(&h.snap()),
        [("charmed".into(), false), ("frightened".into(), false)]
    );
    h.s = commands::set_toggle(engine(), &h.s, "rage", true)
        .expect("toggle")
        .state;
    assert_eq!(
        imm(&h.snap()),
        [("charmed".into(), true), ("frightened".into(), true)]
    );
}

/// SRD Dwarf: Darkvision 120 ft., Poison Resistance, Advantage on saves against Poisoned.
#[test]
fn c2_dwarf_poison_resistance_and_darkvision_120() {
    let snap = Hero::new(DWARF, "ID_2024_CLASS_FIGHTER", "Fighter").snap();
    let dv = snap
        .senses
        .iter()
        .find(|s| s.name == "darkvision")
        .expect("dv");
    assert_eq!((dv.range, dv.source.as_str()), (120, "Darkvision"));
    let poison = snap
        .defenses
        .resistances
        .iter()
        .find(|r| r.name == "poison")
        .expect("res");
    assert!(poison.active && poison.source == "Dwarven Resilience");
    assert!(snap.advantages.iter().any(|a| a.text.contains("Poisoned")));
}

/// SRD Halfling: Brave is Advantage on saves to avoid or end Frightened (surfaced).
#[test]
fn c2_halfling_brave_is_an_advantage_note() {
    let snap = Hero::new(
        "ID_2024_SPECIES_HALFLING",
        "ID_2024_CLASS_FIGHTER",
        "Fighter",
    )
    .snap();
    let brave = snap
        .advantages
        .iter()
        .find(|a| a.source == "Brave")
        .unwrap_or_else(|| panic!("{:?}", snap.advantages));
    assert!(brave
        .text
        .starts_with("Advantage on saving throws against being Frightened"));
    assert_eq!(brave.roll, "saving_throw");
    assert!(brave.active);
}

/// SRD Goliath Powerful Build: counts as one size larger (Medium 15 -> Large 30 x Str).
#[test]
fn c2_goliath_carry_capacity_counts_one_size_larger() {
    let goliath = Hero::new(
        "ID_2024_SPECIES_GOLIATH",
        "ID_2024_CLASS_FIGHTER",
        "Fighter",
    )
    .snap();
    let dwarf = Hero::new(DWARF, "ID_2024_CLASS_FIGHTER", "Fighter").snap();
    let strength = goliath.ability("strength").unwrap().score;
    let carry = goliath.carry.as_ref().expect("carry");
    assert_eq!(
        (carry.size.as_str(), carry.effective_size.as_str()),
        ("Medium", "Large")
    );
    assert!((carry.carry - f64::from(strength) * 30.0).abs() < 1e-9);
    assert!((carry.drag_lift_push - f64::from(strength) * 60.0).abs() < 1e-9);
    assert!((goliath.inventory.capacity - carry.carry).abs() < 1e-9);
    assert!(dwarf.carry.is_none());
    let dstr = dwarf.ability("strength").unwrap().score;
    assert!((dwarf.inventory.capacity - f64::from(dstr) * 15.0).abs() < 1e-9);
    assert!(goliath
        .advantages
        .iter()
        .any(|a| a.text.ends_with("end Grappled condition")));
}

/// SRD Archery: +2 to attack rolls with ranged weapons only.
#[test]
fn c2_archery_adds_two_to_ranged_attacks() {
    let style = |feat: &str, weapon: &str| {
        let mut h = Hero::new(DWARF, "ID_2024_CLASS_FIGHTER", "Fighter");
        h.pick(
            "ID_2024_CLASS_FEATURE_FIGHTER_FIGHTING_STYLE",
            "Fighting Style (Fighter)",
            1,
            feat,
        );
        h.equip(weapon);
        h.snap()
    };
    let (archery, defense) = ("ID_2024_FEAT_ARCHERY", "ID_2024_FEAT_DEFENSE");
    let bow = "ID_2024_WEAPON_LONGBOW";
    let (a, d) = (style(archery, bow), style(defense, bow));
    assert_eq!(
        attack(&a, "Longbow").attack,
        attack(&d, "Longbow").attack + 2
    );
    let (a, d) = (style(archery, GREATSWORD), style(defense, GREATSWORD));
    assert_eq!(
        attack(&a, "Greatsword").attack,
        attack(&d, "Greatsword").attack
    );
}

/// SRD Bard Jack of All Trades (level 2): half proficiency (rounded down) on unproficient skills.
#[test]
fn c2_jack_of_all_trades_adds_half_proficiency_to_unproficient_skills() {
    let mut h = Hero::new(DWARF, "ID_2024_CLASS_BARD", "Bard");
    let before = h.snap();
    h.level_to(2);
    let after = h.snap();
    let (b, a) = (
        before.skill("history").unwrap(),
        after.skill("history").unwrap(),
    );
    assert!(!a.proficient);
    assert_eq!(a.total, b.total + 1, "PB 2 -> +1");
    let p = after.skill("insight").unwrap();
    assert!(
        p.proficient,
        "Acolyte proficiency keeps the full bonus only"
    );
    assert_eq!(p.total, before.skill("insight").unwrap().total);
}

/// Surfaced notes: Sneak Attack dice from the class table, Evasion, Reliable Talent.
#[test]
fn c2_surfaced_notes_for_rogue() {
    let mut h = Hero::new(DWARF, "ID_2024_CLASS_ROGUE", "Rogue");
    let sneak = |s: &CharacterSnapshot| {
        s.notes
            .iter()
            .find(|n| n.kind == "extra_damage")
            .map(|n| n.text.clone())
            .expect("sneak attack note")
    };
    assert!(sneak(&h.snap()).starts_with("Sneak Attack: 1d6 weapon damage, once per turn"));
    h.level_to(7);
    let snap = h.snap();
    assert!(sneak(&snap).starts_with("Sneak Attack: 4d6"));
    assert!(snap.notes.iter().any(|n| n.kind == "evasion"));
}

/// SRD Great Weapon Fighting (floor 3 on two-handed melee) and Two-Weapon Fighting (modifier on
/// the Light weapon's extra attack).
#[test]
fn c2_fighting_style_damage_lines() {
    let make = |feat: &str, weapon: &str| {
        let mut h = Hero::new(DWARF, "ID_2024_CLASS_FIGHTER", "Fighter");
        h.pick(
            "ID_2024_CLASS_FEATURE_FIGHTER_FIGHTING_STYLE",
            "Fighting Style (Fighter)",
            1,
            feat,
        );
        h.equip(weapon);
        h.snap()
    };
    let gwf = make("ID_2024_FEAT_GREAT_WEAPON_FIGHTING", GREATSWORD);
    assert_eq!(attack(&gwf, "Greatsword").die_floor, Some(3));
    let none = make("ID_2024_FEAT_DEFENSE", GREATSWORD);
    assert_eq!(attack(&none, "Greatsword").die_floor, None);
    let dagger = "ID_2024_WEAPON_DAGGER";
    let twf = make("ID_2024_FEAT_TWO_WEAPON_FIGHTING", dagger);
    let plain = make("ID_2024_FEAT_DEFENSE", dagger);
    let (t, p) = (attack(&twf, "Dagger"), attack(&plain, "Dagger"));
    assert_eq!(t.offhand_damage, Some(t.damage.clone()));
    assert_eq!(p.offhand_damage.as_deref(), Some("1d4 piercing"));
}
