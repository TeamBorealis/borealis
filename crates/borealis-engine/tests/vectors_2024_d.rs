//! 2024 golden vectors for engine slice D (Wild Shape forms, cantrip scaling, Circle of the Land,
//! starting-equipment packages, surfaced rules). Numbers come from SRD 5.2.1.
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
            .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1-monsters")))
            .expect("packs load")
            .build();
        Engine::new(&store)
    })
}

const LEVEL1: &str = "ID_LEVEL_1";
const ACOLYTE: &str = "ID_2024_BACKGROUND_ACOLYTE";

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
}

const DWARF: &str = "ID_2024_SPECIES_DWARF";
const DRUID: &str = "ID_2024_CLASS_DRUID";
const LAND: &str = "ID_2024_SUBCLASS_DRUID_CIRCLE_OF_THE_LAND";
const LAND_SPELLS: &str =
    "ID_2024_SUBCLASS_FEATURE_DRUID_CIRCLE_OF_THE_LAND_CIRCLE_OF_THE_LAND_SPELLS";

impl Hero {
    fn land_type(&mut self, land: &str) -> &mut Self {
        let snap = self.snap();
        let slot = snap
            .slots
            .iter()
            .find(|s| s.owner.id == LAND_SPELLS && s.name == "Land Type")
            .expect("Land Type slot");
        let item = slot
            .list_items
            .iter()
            .find(|(_, t)| t == land)
            .expect("land entry")
            .0;
        let key = slot.key.clone();
        self.s = commands::select(engine(), &self.s, &key, Choice::ListItem(item))
            .expect("land select")
            .state;
        self
    }
}

fn druid_spell<'a>(
    snap: &'a CharacterSnapshot,
    name: &str,
) -> Option<&'a borealis_engine::snapshot::SpellEntry> {
    snap.spellcasting
        .iter()
        .find(|c| c.name == "Druid")?
        .spells
        .iter()
        .find(|s| s.name == name)
}

/// SRD Druid table: Wild Shape knows 6 forms at level 4, maximum CR 1/2, no flying; lasts
/// level/2 hours and grants Temporary Hit Points equal to the Druid level.
#[test]
fn d_level4_druid_wild_shape_options_are_cr_half_without_flying() {
    let mut h = Hero::new(DWARF, DRUID, "Druid");
    h.level_to(4);
    let ws = h.snap().wild_shape.expect("wild shape");
    assert_eq!(
        (ws.druid_level, ws.max_forms, ws.max_cr.as_str()),
        (4, 6, "1/2")
    );
    assert!(!ws.fly_allowed);
    assert_eq!((ws.duration_hours, ws.temp_hp), (2, 4));
    assert!(!ws.options.is_empty());
    assert!(ws.options.iter().all(|o| !o.flying), "no flying forms");
    assert!(ws
        .options
        .iter()
        .all(|o| matches!(o.cr.as_str(), "0" | "1/8" | "1/4" | "1/2")));
    let names: Vec<&str> = ws.options.iter().map(|o| o.name.as_str()).collect();
    assert!(names.contains(&"Wolf"), "Wolf (CR 1/4) qualifies");
    assert!(
        !names.contains(&"Brown Bear"),
        "Brown Bear (CR 1) is too strong"
    );
    assert!(!names.contains(&"Giant Eagle"), "no flying forms");
    // learn the Wolf: the snapshot lists its stat summary
    let wolf = ws
        .options
        .iter()
        .find(|o| o.name == "Wolf")
        .unwrap()
        .id
        .clone();
    h.s = commands::set_wild_shape_forms(engine(), &h.s, std::slice::from_ref(&wolf))
        .expect("learn")
        .state;
    let known = h.snap().wild_shape.unwrap().known;
    assert_eq!(known.len(), 1);
    assert_eq!((known[0].ac, known[0].hp), (12, 11));
    assert_eq!(known[0].speed, "40 ft.");
    assert_eq!(known[0].abilities[0], 14);
    // a CR 1 beast is refused
    let bear = engine()
        .universe(Ruleset::Dnd2024)
        .by_type("Monster")
        .find(|m| m.el.name == "Brown Bear")
        .expect("bear")
        .el
        .id
        .clone();
    assert!(commands::set_wild_shape_forms(engine(), &h.s, &[bear]).is_err());
    // level 8 allows CR 1 and flying forms
    h.level_to(8);
    let ws = h.snap().wild_shape.unwrap();
    assert_eq!(
        (ws.max_forms, ws.max_cr.as_str(), ws.fly_allowed),
        (8, "1", true)
    );
    assert!(ws.options.iter().any(|o| o.name == "Brown Bear"));
    assert_eq!(ws.known.len(), 1, "the Wolf is still known");
}

/// SRD Circle of the Land (Arid): Blur, Burning Hands and Fire Bolt are always prepared at 3;
/// Fire Bolt grows 1d10 -> 2d10 at character level 5 -> 3d10 at 11.
#[test]
fn d_circle_of_the_land_arid_spells_and_fire_bolt_scaling() {
    let mut h = Hero::new(DWARF, DRUID, "Druid");
    h.level_to(3).archetype(LAND);
    assert!(
        druid_spell(&h.snap(), "Fire Bolt").is_none(),
        "no land chosen yet"
    );
    h.land_type("Arid");
    let snap = h.snap();
    for name in ["Blur", "Burning Hands", "Fire Bolt"] {
        let s = druid_spell(&snap, name).unwrap_or_else(|| panic!("{name}"));
        assert!(s.always_prepared, "{name} is always prepared");
    }
    assert!(
        druid_spell(&snap, "Fireball").is_none(),
        "Fireball needs level 5"
    );
    assert_eq!(
        druid_spell(&snap, "Fire Bolt").unwrap().damage.as_deref(),
        Some("1d10 Fire")
    );
    h.level_to(5);
    let snap = h.snap();
    assert!(druid_spell(&snap, "Fireball").is_some_and(|s| s.always_prepared));
    assert_eq!(
        druid_spell(&snap, "Fire Bolt").unwrap().damage.as_deref(),
        Some("2d10 Fire")
    );
    h.level_to(11);
    assert_eq!(
        druid_spell(&h.snap(), "Fire Bolt")
            .unwrap()
            .damage
            .as_deref(),
        Some("3d10 Fire")
    );
    // switching the land swaps the spells
    h.land_type("Polar");
    let snap = h.snap();
    assert!(druid_spell(&snap, "Fire Bolt").is_none());
    assert!(druid_spell(&snap, "Fog Cloud").is_some_and(|s| s.always_prepared));
}

/// SRD Fighter: starting equipment option A is Chain Mail, Greatsword, Flail, 8 Javelins,
/// Dungeoneer's Pack and 4 GP.
#[test]
fn d_fighter_starting_package_a_populates_inventory() {
    let mut h = Hero::new(DWARF, "ID_2024_CLASS_FIGHTER", "Fighter");
    let pkg = "ID_2024_EQUIPMENT_PACKAGE_CLASS_FIGHTER";
    let snap = h.snap();
    let offered = snap
        .equipment_packages
        .iter()
        .find(|p| p.package == pkg)
        .expect("fighter package");
    assert_eq!(offered.options.len(), 3);
    assert!(snap
        .equipment_packages
        .iter()
        .any(|p| p.package == "ID_2024_EQUIPMENT_PACKAGE_BACKGROUND_ACOLYTE"));
    let gp0 = h.s.inventory.coins.gp;
    h.s = commands::choose_equipment_package(engine(), &h.s, pkg, "A")
        .expect("choose A")
        .state;
    let items = &h.s.inventory.items;
    let has = |id: &str, n: u32| items.iter().any(|i| i.item == id && i.amount == n);
    assert!(has("ID_2024_ARMOR_CHAIN_MAIL", 1));
    assert!(has("ID_2024_WEAPON_GREATSWORD", 1));
    assert!(has("ID_2024_WEAPON_FLAIL", 1));
    assert!(has("ID_2024_WEAPON_JAVELIN", 8));
    assert!(has("ID_2024_ITEM_DUNGEONEERS_PACK", 1));
    assert_eq!(h.s.inventory.coins.gp, gp0 + 4);
    let snap = h.snap();
    assert_eq!(
        snap.equipment_packages
            .iter()
            .find(|p| p.package == pkg)
            .unwrap()
            .chosen
            .as_deref(),
        Some("A")
    );
    assert!(commands::choose_equipment_package(engine(), &h.s, pkg, "B").is_err());
}

/// Second Wind heals 1d10 + Fighter level; the note carries the computed number.
#[test]
fn d_second_wind_note_uses_fighter_level() {
    let mut h = Hero::new(DWARF, "ID_2024_CLASS_FIGHTER", "Fighter");
    h.level_to(3);
    let note = h
        .snap()
        .notes
        .into_iter()
        .find(|n| n.kind == "healing")
        .expect("healing note");
    assert_eq!(note.text, "Second Wind: 1d10 + 3 hit points (bonus action)");
}
