//! 2024 golden vectors for engine slice A (creation / origin flow). Every number is derived by
//! hand from SRD 5.2.1 (page references in the comments) and then pinned.
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

struct Hero {
    s: CharacterState,
}

impl Hero {
    fn new() -> Self {
        borealis_engine::invariants::enable_for_thread();
        Self {
            s: commands::new_character(engine(), Ruleset::Dnd2024),
        }
    }

    fn snap(&self) -> CharacterSnapshot {
        engine().derive(&self.s)
    }

    fn pick(&mut self, owner: &str, name: &str, ordinal: u32, id: &str) -> &mut Self {
        let snap = self.snap();
        let key = slot_key(&snap, owner, name, ordinal).unwrap_or_else(|| {
            let open: Vec<_> = snap
                .slots
                .iter()
                .map(|s| format!("{}:{}#{}", s.owner.id, s.name, s.key.ordinal))
                .collect();
            panic!("no open slot {owner}/{name}#{ordinal}; open: {open:?}")
        });
        self.s = commands::select(engine(), &self.s, &key, Choice::Element(id.into()))
            .unwrap_or_else(|e| panic!("select {id}: {e}"))
            .state;
        self
    }

    /// Choose the list entry with this text in the first still empty slot `owner`/`name`.
    fn pick_text(&mut self, owner: &str, name: &str, text: &str) -> &mut Self {
        let snap = self.snap();
        let slot = snap
            .slots
            .iter()
            .find(|s| s.owner.id == owner && s.name == name && s.list_choice.is_none())
            .unwrap_or_else(|| panic!("no empty slot {owner}/{name}"));
        let item = slot
            .list_items
            .iter()
            .find(|(_, t)| t == text)
            .unwrap_or_else(|| panic!("no entry {text:?} in {:?}", slot.list_items));
        let key = slot.key.clone();
        self.s = commands::select(engine(), &self.s, &key, Choice::ListItem(item.0))
            .expect("list select")
            .state;
        self
    }

    fn scores(&mut self, method: AbilityMethod, base: [i32; 6]) -> &mut Self {
        self.s = commands::set_ability_method(&self.s, method);
        self.s = commands::set_base_scores(engine(), &self.s, base)
            .expect("legal scores")
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

const SOLDIER: &str = "ID_2024_BACKGROUND_SOLDIER";
const ACOLYTE: &str = "ID_2024_BACKGROUND_ACOLYTE";
const FIGHTER: &str = "ID_2024_CLASS_FIGHTER";
const CLERIC: &str = "ID_2024_CLASS_CLERIC";
const ASI_FEAT: &str = "ID_2024_FEAT_ABILITY_SCORE_IMPROVEMENT";
const FIGHTER_ASI: &str = "ID_2024_CLASS_FEATURE_FIGHTER_ABILITY_SCORE_IMPROVEMENT";
const ASI_PICK: &str = "Ability Score Increase";

fn skill(name: &str) -> String {
    format!(
        "ID_PROFICIENCY_SKILL_{}",
        name.to_uppercase().replace(' ', "")
    )
}

/// Vector 1 (SRD 5.2.1 "Character Origins"/"Fighter"/"Equipment"): level 1 Human Fighter, Soldier
/// background (+2 STR, +1 CON, Savage Attacker), point buy 15/13/14/8/12/10 (costs 9+5+7+0+4+2 =
/// 27), Defense fighting style, chain mail.
///
/// * STR 15+2 = 17 (+3), DEX 13 (+1), CON 14+1 = 15 (+2), INT 8 (-1), WIS 12 (+1), CHA 10 (+0).
/// * Proficiency +2; Fighter saves STR and CON: STR +3+2 = 5, CON +2+2 = 4, DEX +1.
/// * HP: d10 maximum 10 + CON 2 = 12.
/// * AC: chain mail 16 (no DEX) + Defense 1 = 17; unarmored 10 + DEX 1 = 11.
/// * Skills: Athletics (Soldier) 3+2 = 5, Intimidation (Soldier) 0+2 = 2, Perception and
///   Survival (Fighter picks) 1+2 = 3, Stealth (Human Skillful) 1+2 = 3, Acrobatics unproficient 1.
#[test]
fn v1_level1_human_fighter_soldier_point_buy() {
    let mut h = Hero::new();
    h.pick(LEVEL1, "Race", 1, "ID_2024_SPECIES_HUMAN")
        .pick(LEVEL1, "Class", 1, FIGHTER)
        .pick(LEVEL1, "Background", 1, SOLDIER)
        .scores(AbilityMethod::PointBuy, [15, 13, 14, 8, 12, 10]);
    let snap = h.snap();
    let pb = snap.point_buy.as_ref().expect("point buy summary");
    assert_eq!((pb.budget, pb.spent, pb.remaining), (27, 27, 0));
    assert!(snap.slot(SOLDIER, "Ability Scores (Soldier)").is_some());
    assert_eq!(
        snap.slot(SOLDIER, "Ability Scores (Soldier)")
            .unwrap()
            .list_items
            .len(),
        7,
        "six +2/+1 assignments and the +1/+1/+1 triple"
    );
    h.pick_text(
        SOLDIER,
        "Ability Scores (Soldier)",
        "Strength +2, Constitution +1",
    )
    .pick(
        FIGHTER,
        "Skill Proficiency (Fighter)",
        1,
        &skill("Perception"),
    )
    .pick(
        FIGHTER,
        "Skill Proficiency (Fighter)",
        2,
        &skill("Survival"),
    )
    .pick(
        "ID_2024_TRAIT_HUMAN_SKILLFUL",
        "Skillful",
        1,
        &skill("Stealth"),
    )
    .pick(
        "ID_2024_CLASS_FEATURE_FIGHTER_FIGHTING_STYLE",
        "Fighting Style (Fighter)",
        1,
        "ID_2024_FEAT_DEFENSE",
    )
    .pick(
        "ID_INTERNAL_CREATION_LANGUAGES",
        "Languages",
        1,
        "ID_2024_LANGUAGE_DWARVISH",
    )
    .pick(
        "ID_INTERNAL_CREATION_LANGUAGES",
        "Languages",
        2,
        "ID_2024_LANGUAGE_ELVISH",
    );
    let snap = h.snap();
    let scores: Vec<i32> = snap.abilities.iter().map(|a| a.score).collect();
    assert_eq!(scores, [17, 13, 15, 8, 12, 10]);
    let mods: Vec<i32> = snap.abilities.iter().map(|a| a.modifier).collect();
    assert_eq!(mods, [3, 1, 2, -1, 1, 0]);
    assert_eq!(snap.proficiency_bonus, 2);
    let saves: Vec<i32> = snap.abilities.iter().map(|a| a.save).collect();
    assert_eq!(saves, [5, 1, 4, -1, 1, 0]);
    assert!(snap.abilities[0].save_proficient && snap.abilities[2].save_proficient);
    assert_eq!(snap.hit_points, 12);
    assert_eq!(snap.armor_class, 11, "unarmored 10 + DEX");
    for (name, total) in [
        ("athletics", 5),
        ("intimidation", 2),
        ("perception", 3),
        ("survival", 3),
        ("stealth", 3),
        ("acrobatics", 1),
    ] {
        assert_eq!(snap.skill(name).map(|s| s.total), Some(total), "{name}");
    }
    // the origin feat the background grants
    assert!(snap.has_element("ID_2024_FEAT_SAVAGE_ATTACKER"));
    // Common plus two Standard languages
    for lang in ["COMMON", "DWARVISH", "ELVISH"] {
        assert!(
            snap.has_element(&format!("ID_2024_LANGUAGE_{lang}")),
            "{lang}"
        );
    }
    assert!(snap
        .slots
        .iter()
        .filter(|s| s.owner.id == "ID_INTERNAL_CREATION_LANGUAGES")
        .all(|s| s.chosen.is_some()));
    h.equip("ID_2024_ARMOR_CHAIN_MAIL");
    let snap = h.snap();
    assert_eq!(snap.armor_class, 17, "chain mail 16 + Defense");
    assert!(snap.has_element("ID_2024_FEAT_DEFENSE"));
    // the 2024 profile no longer reports feat categories as unimplemented
    assert!(!snap
        .diagnostics
        .iter()
        .any(|d| d.code == "profile.unimplemented" && d.message.contains("feat_categories")));
}

#[test]
fn feat_slots_are_filtered_by_category_and_prerequisites() {
    let mut h = Hero::new();
    h.pick(LEVEL1, "Race", 1, "ID_2024_SPECIES_HUMAN")
        .pick(LEVEL1, "Class", 1, FIGHTER)
        .pick(LEVEL1, "Background", 1, SOLDIER);
    let snap = h.snap();
    let ids = |owner: &str, name: &str| -> Vec<String> {
        let mut v: Vec<String> = snap
            .slot(owner, name)
            .unwrap_or_else(|| panic!("{owner}/{name}"))
            .options
            .iter()
            .map(|o| o.id.clone())
            .collect();
        v.sort();
        v
    };
    // Versatile is an Origin slot; Savage Attacker is granted by Soldier so it is not offered again
    let origin = ids("ID_2024_TRAIT_HUMAN_VERSATILE", "Versatile");
    assert_eq!(
        origin,
        [
            "ID_2024_FEAT_ALERT",
            "ID_2024_FEAT_MAGIC_INITIATE",
            "ID_2024_FEAT_SKILLED"
        ]
    );
    let styles = ids(
        "ID_2024_CLASS_FEATURE_FIGHTER_FIGHTING_STYLE",
        "Fighting Style (Fighter)",
    );
    assert_eq!(styles.len(), 4, "{styles:?}");
    assert!(styles.iter().all(|s| s.starts_with("ID_2024_FEAT_")));
    // a level 1 character may not take General or Epic Boon feats anywhere
    assert!(snap.slots.iter().all(|s| s
        .options
        .iter()
        .all(|o| o.id != ASI_FEAT && !o.id.contains("FEAT_BOON_OF"))));
}

/// Vector 2: level 4 Dwarf Cleric, Acolyte (+2 WIS, +1 CHA, Magic Initiate (Cleric)), Cleric standard
/// array (STR 14, DEX 8, CON 13, INT 10, WIS 15, CHA 12), Ability Score Improvement feat at 4
/// (+2 WIS).
///
/// * WIS 15+2+2 = 19 (+4), CHA 12+1 = 13 (+1), CON 13 (+1), DEX 8 (-1).
/// * Proficiency +2 (levels 1-4); Cleric saves WIS and CHA: WIS 4+2 = 6, CHA 1+2 = 3.
/// * HP: d8 max 8 + 3 levels x 5 (fixed table) + 4 x CON 1 + Dwarven Toughness 4 (1 + 1 per level
///   gained: 1 at level 1, +1 for each of the 3 later levels) = 8+15+4+4 = 31.
/// * AC unarmored 10 + DEX -1 = 9.
#[test]
fn v2_level4_dwarf_cleric_takes_the_asi_feat() {
    let tables = engine().universe(Ruleset::Dnd2024).tables();
    let array = tables.standard_array_for("Cleric").expect("class array");
    assert_eq!(array, [14, 8, 13, 10, 15, 12]);
    let mut h = Hero::new();
    h.pick(LEVEL1, "Race", 1, "ID_2024_SPECIES_DWARF")
        .pick(LEVEL1, "Class", 1, CLERIC)
        .pick(LEVEL1, "Background", 1, ACOLYTE)
        .scores(AbilityMethod::StandardArray, array)
        .pick_text(
            ACOLYTE,
            "Ability Scores (Acolyte)",
            "Wisdom +2, Charisma +1",
        );
    h.level_to(4);
    let snap = h.snap();
    let asi_slot = "Ability Score Improvement (Cleric 4)";
    let owner = "ID_2024_CLASS_FEATURE_CLERIC_ABILITY_SCORE_IMPROVEMENT";
    let slot = snap.slot(owner, asi_slot).expect("ASI slot at level 4");
    assert_eq!(slot.ty, "Feat");
    assert_eq!(slot.default.as_deref(), Some(ASI_FEAT));
    assert!(slot.options.iter().any(|o| o.id == ASI_FEAT));
    assert!(
        slot.options.iter().all(|o| !o.id.contains("FEAT_BOON_OF")),
        "Epic Boons need level 19"
    );
    h.pick(owner, asi_slot, 1, ASI_FEAT)
        .pick_text(ASI_FEAT, ASI_PICK, "Wisdom +2");
    let snap = h.snap();
    assert_eq!(snap.level, 4);
    let scores: Vec<i32> = snap.abilities.iter().map(|a| a.score).collect();
    assert_eq!(scores, [14, 8, 13, 10, 19, 13]);
    assert_eq!(snap.proficiency_bonus, 2);
    assert_eq!(snap.abilities[4].save, 6);
    assert_eq!(snap.abilities[5].save, 3);
    assert_eq!(snap.hit_points, 31);
    assert_eq!(snap.armor_class, 9);
    assert!(snap.has_element("ID_2024_FEAT_MAGIC_INITIATE"));
    // the ASI feat cannot raise a score above 20: a second +2 at level 8 stops at 20
    h.level_to(8);
    h.pick(owner, "Ability Score Improvement (Cleric 8)", 1, ASI_FEAT)
        .pick_text(ASI_FEAT, ASI_PICK, "Wisdom +2");
    let snap = h.snap();
    assert_eq!(snap.abilities[4].score, 20);
    assert_eq!(snap.abilities[4].maximum, 20);
}

/// Vector 3: level 19 Fighter, Soldier (+2 STR, +1 CON), Fighter standard array
/// (15/14/13/8/10/12), ASIs: STR +2 (4), STR +2 (6, stops at 20), CON +2 (8), CON +2 (12),
/// DEX +2 (14), DEX +2 (16), then Boon of Combat Prowess (+1 STR, maximum 30) at 19.
///
/// * STR 15+2 = 17, +2 = 19, +2 -> 20 (capped, one point lost), boon +1 = 21 (+5).
/// * DEX 14+2+2 = 18 (+4). CON 13+1+2+2 = 18 (+4). INT 8, WIS 10, CHA 12.
/// * Proficiency +6; saves STR 5+6 = 11, CON 4+6 = 10.
/// * HP: 10 + 18 x 6 (fixed table) + 19 x CON 4 = 118 + 76 = 194.
#[test]
fn v3_level19_epic_boon_exceeds_twenty() {
    let array = engine()
        .universe(Ruleset::Dnd2024)
        .tables()
        .standard_array_for("Fighter")
        .expect("class array");
    assert_eq!(array, [15, 14, 13, 8, 10, 12]);
    let mut h = Hero::new();
    h.pick(LEVEL1, "Race", 1, "ID_2024_SPECIES_HUMAN")
        .pick(LEVEL1, "Class", 1, FIGHTER)
        .pick(LEVEL1, "Background", 1, SOLDIER)
        .scores(AbilityMethod::StandardArray, array)
        .pick_text(
            SOLDIER,
            "Ability Scores (Soldier)",
            "Strength +2, Constitution +1",
        );
    let plan = [
        (4, "Strength +2"),
        (6, "Strength +2"),
        (8, "Constitution +2"),
        (12, "Constitution +2"),
        (14, "Dexterity +2"),
        (16, "Dexterity +2"),
    ];
    for (level, pick) in plan {
        h.level_to(level);
        h.pick(
            FIGHTER_ASI,
            &format!("Ability Score Improvement (Fighter {level})"),
            1,
            ASI_FEAT,
        )
        .pick_text(ASI_FEAT, ASI_PICK, pick);
        let score = h.snap().abilities[0].score;
        assert!(score <= 20, "level {level}: STR {score}");
    }
    h.level_to(19);
    let snap = h.snap();
    assert_eq!(
        snap.abilities[0].score, 20,
        "the sixth ASI point is lost at the cap"
    );
    let boon_slot = snap
        .slot(
            "ID_2024_CLASS_FEATURE_FIGHTER_EPIC_BOON",
            "Epic Boon (Fighter)",
        )
        .expect("Epic Boon slot at 19");
    let offered: Vec<&str> = boon_slot.options.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(
        offered.len(),
        6,
        "seven boons minus Boon of Spell Recall (no Spellcasting): {offered:?}"
    );
    assert!(!offered.contains(&"ID_2024_FEAT_BOON_OF_SPELL_RECALL"));
    assert!(!offered.contains(&ASI_FEAT));
    h.pick(
        "ID_2024_CLASS_FEATURE_FIGHTER_EPIC_BOON",
        "Epic Boon (Fighter)",
        1,
        "ID_2024_FEAT_BOON_OF_COMBAT_PROWESS",
    )
    .pick_text(
        "ID_2024_FEAT_BOON_OF_COMBAT_PROWESS",
        ASI_PICK,
        "Strength +1",
    );
    let snap = h.snap();
    let scores: Vec<i32> = snap.abilities.iter().map(|a| a.score).collect();
    assert_eq!(scores, [21, 18, 18, 8, 10, 12]);
    assert_eq!(snap.abilities[0].maximum, 30);
    assert_eq!(snap.abilities[0].modifier, 5);
    assert_eq!(snap.proficiency_bonus, 6);
    assert_eq!(snap.abilities[0].save, 11);
    assert_eq!(snap.abilities[2].save, 10);
    assert_eq!(snap.hit_points, 194);
    // the explain trace stays consistent with the value
    let trace = engine().explain(&h.s, "strength:score");
    assert_eq!(trace.value, 21);
}

#[test]
fn epic_boon_with_two_abilities_offers_only_the_listed_ones() {
    let mut h = Hero::new();
    h.pick(LEVEL1, "Race", 1, "ID_2024_SPECIES_HUMAN")
        .pick(LEVEL1, "Class", 1, FIGHTER)
        .pick(LEVEL1, "Background", 1, SOLDIER)
        .level_to(19)
        .pick(
            "ID_2024_CLASS_FEATURE_FIGHTER_EPIC_BOON",
            "Epic Boon (Fighter)",
            1,
            "ID_2024_FEAT_BOON_OF_IRRESISTIBLE_OFFENSE",
        );
    let snap = h.snap();
    let slot = snap
        .slots
        .iter()
        .find(|s| s.owner.id == "ID_2024_FEAT_BOON_OF_IRRESISTIBLE_OFFENSE" && s.name == ASI_PICK)
        .expect("ability choice of the boon");
    let texts: Vec<&str> = slot.list_items.iter().map(|(_, t)| t.as_str()).collect();
    assert_eq!(texts, ["Strength +1", "Dexterity +1"]);
}

/// Vector 4: Magic Initiate (Cleric) granted by Acolyte offers exactly the Cleric list
/// (SRD: 7 Cleric cantrips; 15 Cleric spells of level 1) through `$(param:Spell List)`.
#[test]
fn v4_magic_initiate_cleric_offers_only_cleric_spells() {
    let mut h = Hero::new();
    h.pick(LEVEL1, "Race", 1, "ID_2024_SPECIES_HUMAN")
        .pick(LEVEL1, "Class", 1, FIGHTER)
        .pick(LEVEL1, "Background", 1, ACOLYTE);
    let snap = h.snap();
    let mi = "ID_2024_FEAT_MAGIC_INITIATE";
    // the list is preset by the background: no Spell List slot, but an ability choice remains
    assert!(snap.slot(mi, "Spell List").is_none());
    assert!(snap.slot(mi, "Spellcasting Ability").is_some());
    let uni = engine().universe(Ruleset::Dnd2024);
    let cantrips = snap
        .slot(mi, "Magic Initiate Cantrips")
        .expect("cantrip slots");
    assert_eq!(cantrips.options.len(), 7);
    assert!(cantrips.options.iter().all(|o| {
        let e = uni.get(&o.id).expect("option element");
        e.el.supports.iter().any(|s| s == "Cleric") && o.level == Some(0)
    }));
    let names: Vec<&str> = cantrips.options.iter().map(|o| o.name.as_str()).collect();
    for n in ["Guidance", "Sacred Flame", "Thaumaturgy"] {
        assert!(names.contains(&n), "{n} in {names:?}");
    }
    let first = snap
        .slot(mi, "Magic Initiate Level 1 Spell")
        .expect("level 1 slot");
    assert_eq!(first.options.len(), 15);
    assert!(first.options.iter().all(|o| o.level == Some(1)));
    // picking is accepted and stored under the feat's own slot key
    let key = slot_key(&snap, mi, "Magic Initiate Cantrips", 1).unwrap();
    h.s = commands::select(
        engine(),
        &h.s,
        &key,
        Choice::Element("ID_2024_SPELL_SACRED_FLAME".into()),
    )
    .expect("sacred flame")
    .state;
    assert!(h.snap().has_element("ID_2024_SPELL_SACRED_FLAME"));
    // a Wizard cantrip is refused
    let wizard_only = uni
        .iter()
        .find(|e| {
            e.el.ty == "Spell"
                && e.spell_level() == Some(0)
                && e.el.supports.iter().any(|s| s == "Wizard")
                && !e.el.supports.iter().any(|s| s == "Cleric")
        })
        .expect("a wizard-only cantrip")
        .el
        .id
        .clone();
    let key2 = slot_key(&h.snap(), mi, "Magic Initiate Cantrips", 2).unwrap();
    assert!(commands::select(engine(), &h.s, &key2, Choice::Element(wizard_only)).is_err());
}

/// A chosen `select_param` drives the same token, and `distinct_per_instance` hides a list another
/// Magic Initiate already took.
#[test]
fn magic_initiate_param_choice_and_distinct_lists() {
    let mut h = Hero::new();
    h.pick(LEVEL1, "Race", 1, "ID_2024_SPECIES_HUMAN")
        .pick(LEVEL1, "Class", 1, FIGHTER)
        .pick(LEVEL1, "Background", 1, ACOLYTE)
        .pick(
            "ID_2024_TRAIT_HUMAN_VERSATILE",
            "Versatile",
            1,
            "ID_2024_FEAT_MAGIC_INITIATE",
        );
    let snap = h.snap();
    // two instances of the feat now exist; the second one asks for its list, without Cleric
    let lists: Vec<_> = snap
        .slots
        .iter()
        .filter(|s| s.name == "Spell List")
        .collect();
    assert_eq!(lists.len(), 1);
    let texts: Vec<&str> = lists[0]
        .list_items
        .iter()
        .map(|(_, t)| t.as_str())
        .collect();
    assert_eq!(
        texts,
        ["Druid", "Wizard"],
        "Cleric is taken by the Acolyte's instance"
    );
    assert!(
        lists[0].key.instance.is_some(),
        "repeatable feat instance keys"
    );
    h.pick_text("ID_2024_FEAT_MAGIC_INITIATE", "Spell List", "Wizard");
    let snap = h.snap();
    let wizard = snap
        .slots
        .iter()
        .find(|s| s.name == "Magic Initiate Cantrips" && s.key.instance == lists[0].key.instance)
        .expect("wizard instance cantrips");
    assert_eq!(wizard.options.len(), 15, "SRD Wizard cantrips");
    let cleric = snap
        .slots
        .iter()
        .find(|s| s.name == "Magic Initiate Cantrips" && s.key.instance != lists[0].key.instance)
        .expect("cleric instance cantrips");
    assert_eq!(cleric.options.len(), 7);
}

#[test]
fn species_size_choice_is_a_size_element() {
    let mut h = Hero::new();
    h.pick(LEVEL1, "Race", 1, "ID_2024_SPECIES_HUMAN");
    let snap = h.snap();
    let size = snap
        .slot("ID_2024_SPECIES_HUMAN", "Size")
        .expect("size slot");
    let ids: Vec<&str> = size.options.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, ["ID_SIZE_MEDIUM", "ID_SIZE_SMALL"]);
    h.pick("ID_2024_SPECIES_HUMAN", "Size", 1, "ID_SIZE_SMALL");
    assert!(h.snap().has_element("ID_SIZE_SMALL"));
}

/// Profile tables come from `ID_2024_PROFILE_TABLES`, not from code.
#[test]
fn profile_tables_are_read_from_the_content() {
    let uni = engine().universe(Ruleset::Dnd2024);
    let t = uni.tables();
    assert_eq!(t.xp.map(|x| x[19]), Some(355_000));
    assert_eq!(t.proficiency_bonus(13), Some(5));
    assert_eq!(t.point_buy_budget, Some(27));
    assert_eq!(t.point_buy_costs.get(&14), Some(&7));
    assert_eq!(t.standard_array, Some([15, 14, 13, 12, 10, 8]));
    assert_eq!(t.fixed_hp("Barbarian"), Some(7));
    assert_eq!(t.fixed_hp("Wizard"), Some(4));
    assert_eq!(engine().xp_threshold(Ruleset::Dnd2024, 5), 6_500);
    let snap = Hero::new().snap();
    assert_eq!(snap.profile.standard_array, [15, 14, 13, 12, 10, 8]);
    assert_eq!(snap.profile.languages_at_creation, 2);
    // the 2014 universe has none
    assert!(engine().universe(Ruleset::Dnd2014).tables().xp.is_none());
}

/// `Class.data.asi_levels` agrees with the `select_feat` slots the classes emit, and every class
/// has its Epic Boon at 19.
#[test]
fn asi_levels_match_the_emitted_feat_slots() {
    let uni = engine().universe(Ruleset::Dnd2024);
    for class in uni.by_type("Class") {
        let want: Vec<u64> = class.el.data["asi_levels"]
            .as_array()
            .expect("asi_levels")
            .iter()
            .filter_map(serde_json::Value::as_u64)
            .collect();
        let mut general = Vec::new();
        let mut boon = Vec::new();
        for feature in uni.by_type("Class Feature") {
            if feature.el.data.get("class").and_then(|c| c.as_str()) != Some(class.el.name.as_str())
            {
                continue;
            }
            for rule in &feature.el.rules {
                let borealis_content::Rule::Select(s) = rule else {
                    continue;
                };
                if s.ty != "Feat" {
                    continue;
                }
                let level = u64::from(s.level.unwrap_or(1));
                match s.supports.as_deref() {
                    Some("General Feat") => general.push(level),
                    Some("Epic Boon Feat") => boon.push(level),
                    _ => {}
                }
            }
        }
        general.sort_unstable();
        assert_eq!(general, want, "{} ASI slots", class.el.name);
        assert_eq!(boon, [19], "{} Epic Boon", class.el.name);
    }
}

/// `select_feat.alternatives`: a Paladin's Fighting Style slot offers the Fighting Style feats and
/// the Blessed Warrior option, and accepts the option.
#[test]
fn feat_slot_alternatives_offer_non_feat_options() {
    let mut h = Hero::new();
    h.pick(LEVEL1, "Race", 1, "ID_2024_SPECIES_HUMAN")
        .pick(LEVEL1, "Class", 1, "ID_2024_CLASS_PALADIN")
        .pick(LEVEL1, "Background", 1, SOLDIER)
        .level_to(2);
    let owner = "ID_2024_CLASS_FEATURE_PALADIN_FIGHTING_STYLE";
    let snap = h.snap();
    let slot = snap
        .slots
        .iter()
        .find(|s| s.owner.id == owner)
        .expect("fighting style slot");
    let ids: Vec<&str> = slot.options.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids.len(), 5, "{ids:?}");
    assert!(ids.contains(&"ID_2024_OPTION_FIGHTING_STYLE_BLESSED_WARRIOR"));
    let name = slot.name.clone();
    h.pick(
        owner,
        &name,
        1,
        "ID_2024_OPTION_FIGHTING_STYLE_BLESSED_WARRIOR",
    );
    assert!(h
        .snap()
        .has_element("ID_2024_OPTION_FIGHTING_STYLE_BLESSED_WARRIOR"));
}

// ---- Slice B1: spellcasting core -------------------------------------------------------------

const WIZARD: &str = "ID_2024_CLASS_WIZARD";

/// Dwarf with the Acolyte background and the class standard array; `bonus` is the Acolyte list entry.
fn caster(class: &str, class_name: &str, bonus: &str) -> Hero {
    let array = engine()
        .universe(Ruleset::Dnd2024)
        .tables()
        .standard_array_for(class_name)
        .expect("class array");
    let mut h = Hero::new();
    h.pick(LEVEL1, "Race", 1, "ID_2024_SPECIES_DWARF")
        .pick(LEVEL1, "Class", 1, class)
        .pick(LEVEL1, "Background", 1, ACOLYTE)
        .scores(AbilityMethod::StandardArray, array)
        .pick_text(ACOLYTE, "Ability Scores (Acolyte)", bonus);
    h
}

fn sc<'a>(
    snap: &'a CharacterSnapshot,
    class: &str,
) -> &'a borealis_engine::snapshot::SpellcastingSnapshot {
    snap.caster(class)
        .unwrap_or_else(|| panic!("no {class} spellcasting"))
}

/// Vector B1-1 (SRD 5.2.1 Cleric Features, Life Domain): level 5 Dwarf Cleric, Acolyte WIS +2 / CHA +1.
///
/// * WIS 15+2 = 17 (+3), proficiency +3: spell save DC 8+3+3 = 14, spell attack 3+3 = 6.
/// * Table row 5: 4 cantrips, 9 prepared spells, slots 4/3/2.
/// * Life Domain: Aid, Bless, Cure Wounds, Lesser Restoration (level 3) and Mass Healing Word,
///   Revivify (level 5) are always prepared and do not count against the 9.
#[test]
fn b1_level5_life_cleric_prepared_slots_dc_and_domain_spells() {
    let mut h = caster(CLERIC, "Cleric", "Wisdom +2, Charisma +1");
    h.level_to(3);
    h.pick(
        "ID_2024_CLASS_FEATURE_CLERIC_CLERIC_SUBCLASS",
        "Divine Domain",
        1,
        "ID_2024_SUBCLASS_CLERIC_LIFE_DOMAIN",
    );
    h.level_to(5);
    let snap = h.snap();
    assert_eq!(snap.proficiency_bonus, 3);
    let c = sc(&snap, "Cleric");
    assert_eq!((c.dc, c.attack), (14, 6));
    assert_eq!(c.slots, [4, 3, 2, 0, 0, 0, 0, 0, 0]);
    assert_eq!(c.prepare_limit, Some(9));
    assert_eq!(c.prepare_source, "class_list");
    assert_eq!(c.pact, None);
    assert_eq!(c.prepared_change, Some(("long_rest".into(), "any".into())));
    let mut always: Vec<&str> = c
        .spells
        .iter()
        .filter(|s| s.always_prepared)
        .map(|s| s.id.as_str())
        .collect();
    always.sort_unstable();
    assert_eq!(
        always,
        [
            "ID_2024_SPELL_AID",
            "ID_2024_SPELL_BLESS",
            "ID_2024_SPELL_CURE_WOUNDS",
            "ID_2024_SPELL_LESSER_RESTORATION",
            "ID_2024_SPELL_MASS_HEALING_WORD",
            "ID_2024_SPELL_REVIVIFY",
        ]
    );
    assert_eq!(c.prepared_count, 0, "domain spells do not count");
    let cd = snap
        .save_dcs
        .iter()
        .find(|d| d.id == "channel divinity")
        .expect("Channel Divinity DC");
    assert_eq!(
        (cd.formula.as_str(), cd.value),
        ("spell save DC (Cleric)", Some(14))
    );
    // the spell_preparation diagnostic is gone
    assert!(!snap
        .diagnostics
        .iter()
        .any(|d| d.message.contains("spell_preparation")));
    // 9 ordinary spells can be prepared, the tenth is refused
    let picks: Vec<String> = c
        .spells
        .iter()
        .filter(|s| !s.always_prepared && s.level > 0 && s.level <= 3)
        .take(10)
        .map(|s| s.id.clone())
        .collect();
    for id in &picks[..9] {
        h.s = commands::set_prepared(engine(), &h.s, "Cleric", id, true)
            .expect("prepare")
            .state;
    }
    assert!(commands::set_prepared(engine(), &h.s, "Cleric", &picks[9], true).is_err());
    assert_eq!(sc(&h.snap(), "Cleric").prepared_count, 9);
}

/// Vector B1-2 (SRD 5.2.1 Wizard Features, Spellbook): level 5 Wizard, Acolyte INT +2 / WIS +1.
///
/// * INT 15+2 = 17 (+3): DC 14, attack 6; slots 4/3/2; 9 prepared spells.
/// * Spellbook: six spells at level 1, two more per level after 1: 6 + 2 x 4 = 14.
/// * Wizards prepare from the spellbook only; Ritual Adept casts rituals from it.
#[test]
fn b1_level5_wizard_spellbook_and_prepared_from_book() {
    let mut h = caster(WIZARD, "Wizard", "Intelligence +2, Wisdom +1");
    h.level_to(5);
    let snap = h.snap();
    let c = sc(&snap, "Wizard");
    assert_eq!((c.dc, c.attack), (14, 6));
    assert_eq!(c.slots, [4, 3, 2, 0, 0, 0, 0, 0, 0]);
    assert_eq!(c.prepare_limit, Some(9));
    assert_eq!(c.prepare_source, "spellbook");
    assert_eq!(c.ritual_source.as_deref(), Some("spellbook"));
    let book = c.spellbook.as_ref().expect("spellbook");
    assert_eq!((book.capacity, book.spells.len()), (14, 0));
    assert_eq!(
        (book.copy_cost_gp_per_level, book.copy_hours_per_level),
        (50, 2)
    );
    assert!(book.candidates.iter().all(|s| (1..=3).contains(&s.level)));
    assert!(book.candidates.len() > 14);
    // nothing is preparable until it is in the book
    let ids: Vec<String> = book.candidates.iter().map(|s| s.id.clone()).collect();
    assert!(commands::set_prepared(engine(), &h.s, "Wizard", &ids[0], true).is_err());
    for id in &ids[..14] {
        h.s = commands::set_spellbook(engine(), &h.s, "Wizard", id, true)
            .expect("copy into book")
            .state;
    }
    assert!(commands::set_spellbook(engine(), &h.s, "Wizard", &ids[14], true).is_err());
    let snap = h.snap();
    assert_eq!(
        sc(&snap, "Wizard")
            .spellbook
            .as_ref()
            .map(|b| b.spells.len()),
        Some(14)
    );
    for id in &ids[..9] {
        h.s = commands::set_prepared(engine(), &h.s, "Wizard", id, true)
            .expect("prepare from book")
            .state;
    }
    assert!(commands::set_prepared(engine(), &h.s, "Wizard", &ids[9], true).is_err());
    assert_eq!(sc(&h.snap(), "Wizard").prepared_count, 9);
    // removing a spell from the book unprepares it
    h.s = commands::set_spellbook(engine(), &h.s, "Wizard", &ids[0], false)
        .expect("remove")
        .state;
    let snap = h.snap();
    assert_eq!(sc(&snap, "Wizard").prepared_count, 8);
    assert_eq!(
        sc(&snap, "Wizard")
            .spellbook
            .as_ref()
            .map(|b| b.spells.len()),
        Some(13)
    );
}

/// Vector B1-3 (SRD 5.2.1 Warlock Features): level 11 Warlock.
///
/// * Table row 11: 4 cantrips, 11 prepared spells, 3 pact slots of level 5.
/// * Pact slots are a separate pool: no ordinary slots, no multiclass pool.
/// * Mystic Arcanum: the level 6 spell is unlocked (gained at 11), levels 7-9 are not yet.
#[test]
fn b1_level11_warlock_pact_slots_and_mystic_arcanum() {
    let mut h = caster("ID_2024_CLASS_WARLOCK", "Warlock", "Wisdom +1, Charisma +2");
    h.level_to(11);
    let snap = h.snap();
    let c = sc(&snap, "Warlock");
    assert_eq!(c.pact, Some((3, 5)));
    assert_eq!(c.slots, [0; 9], "pact slots are separate");
    assert_eq!(c.prepare_limit, Some(11));
    assert_eq!(c.prepared_change, Some(("gain_level".into(), "one".into())));
    assert!(snap.multiclass_slots.is_none());
    let arcanum: Vec<(u32, u32, bool)> = c
        .arcanum
        .iter()
        .map(|a| (a.spell_level, a.class_level, a.unlocked))
        .collect();
    assert_eq!(
        arcanum,
        [
            (6, 11, true),
            (7, 13, false),
            (8, 15, false),
            (9, 17, false)
        ]
    );
    assert_eq!(c.arcanum[0].recharge, "long_rest");
    // choose the level 6 arcanum spell
    let owner = "ID_2024_CLASS_FEATURE_WARLOCK_MYSTIC_ARCANUM";
    let slot = snap
        .slot(owner, "Mystic Arcanum (level 6)")
        .expect("arcanum slot");
    let pick = slot.options.first().expect("level 6 option").id.clone();
    h.pick(owner, "Mystic Arcanum (level 6)", 1, &pick);
    let snap = h.snap();
    let c = sc(&snap, "Warlock");
    assert_eq!(c.arcanum[0].spell.as_deref(), Some(pick.as_str()));
    assert_eq!(
        c.prepared_count, 0,
        "arcanum spells are not prepared spells"
    );
}

/// Vector B1-4 (SRD 5.2.1 Ranger Features): level 3 Ranger is a half-caster with its own table.
///
/// * Row 3: 4 prepared spells, 3 level 1 slots. WIS 15+2 = 17 (+3), proficiency +2 at level 3:
///   DC 8+2+3 = 13, attack 2+3 = 5.
#[test]
fn b1_level3_ranger_half_caster_slots() {
    let mut h = caster("ID_2024_CLASS_RANGER", "Ranger", "Wisdom +2, Charisma +1");
    h.level_to(3);
    let snap = h.snap();
    let c = sc(&snap, "Ranger");
    assert_eq!(c.slots, [3, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(c.prepare_limit, Some(4));
    assert_eq!((c.dc, c.attack), (13, 5));
    assert!(snap.multiclass_slots.is_none(), "one caster class: no pool");
    // Hunter's Mark: free casts come from the Favored Enemy column of the class table (2 at 3)
    let mark = snap
        .free_casts
        .iter()
        .find(|f| f.spell == "ID_2024_SPELL_HUNTERS_MARK")
        .expect("Hunter's Mark free casts");
    assert_eq!(
        (mark.uses, mark.uses_rule.as_str(), mark.recharge.as_str()),
        (Some(2), "table:favored_enemy", "long_rest")
    );
}

/// Level 3 Paladin (SRD 5.2.1 Paladin Features): Divine Smite is always prepared with one free cast
/// per Long Rest (Find Steed follows at level 5), and the Channel Divinity DC is the Paladin spell save DC
/// (spell save DC).
///
/// * Acolyte CHA +2 on the class array (CHA 12): CHA 14 (+2), proficiency +2: DC 8+2+2 = 12.
#[test]
fn b1_level3_paladin_free_casts_and_channel_divinity_dc() {
    let mut h = caster("ID_2024_CLASS_PALADIN", "Paladin", "Wisdom +1, Charisma +2");
    h.level_to(3);
    let snap = h.snap();
    let c = sc(&snap, "Paladin");
    let id = "ID_2024_SPELL_DIVINE_SMITE";
    let e = c.spells.iter().find(|s| s.id == id).expect(id);
    assert!(e.always_prepared && e.prepared, "{id}");
    let f = snap.free_casts.iter().find(|f| f.spell == id).expect(id);
    assert_eq!((f.uses, f.recharge.as_str()), (Some(1), "long_rest"));
    assert!(f.also_with_slots);
    let cd = snap
        .save_dcs
        .iter()
        .find(|d| d.id == "channel divinity")
        .expect("Channel Divinity DC");
    assert_eq!(cd.value, Some(c.dc));
}

/// Vector B1-5 (SRD 5.2.1 multiclassing): Paladin 3 / Sorcerer 3.
///
/// * 2024 rounds half-caster levels up: Paladin 3 -> 2, plus Sorcerer 3 = caster level 5, which is
///   4/3/2 slots (the 2014 rule would give level 4: 4/3).
/// * Each class keeps its own prepared count from its table: Paladin 4, Sorcerer 6.
#[test]
fn b1_paladin3_sorcerer3_rounds_half_caster_up() {
    let mut h = caster("ID_2024_CLASS_PALADIN", "Paladin", "Wisdom +2, Charisma +1");
    h.level_to(3);
    h.s = commands::level_up_new_multiclass(engine(), &h.s, None)
        .expect("new multiclass level")
        .state;
    let start = h.s.level();
    h.pick(
        &format!("ID_LEVEL_{start}"),
        &format!("Multiclass (Level {start})"),
        1,
        "ID_2024_MULTICLASS_SORCERER",
    );
    for _ in 0..2 {
        h.s = commands::level_up_multiclass(engine(), &h.s, start, None)
            .expect("multiclass level")
            .state;
    }
    let snap = h.snap();
    assert_eq!(snap.level, 6);
    let pool = snap.multiclass_slots.as_ref().expect("pooled slots");
    assert_eq!(pool.caster_level, 5);
    assert_eq!(pool.slots, [4, 3, 2, 0, 0, 0, 0, 0, 0]);
    assert_eq!(sc(&snap, "Paladin").prepare_limit, Some(4));
    assert_eq!(sc(&snap, "Paladin").slots[0], 3);
    assert_eq!(sc(&snap, "Sorcerer").prepare_limit, Some(6));
    assert_eq!(sc(&snap, "Sorcerer").slots[..2], [4, 2]);
}

/// Vector B1-6 (SRD 5.2.1 Pact Magic and multiclassing): Warlock 3 / Wizard 2.
///
/// * Warlock table row 3: 2 pact slots of level 2, a separate pool (no ordinary Warlock slots).
/// * Pact Magic is not a multiclass slot source: one Spellcasting class (Wizard 2) keeps its own
///   table, 3 level 1 slots, and there is no pooled table.
#[test]
fn b1_warlock3_wizard2_pact_slots_stay_separate() {
    let mut h = caster("ID_2024_CLASS_WARLOCK", "Warlock", "Wisdom +1, Charisma +2");
    h.scores(AbilityMethod::Manual, [8, 14, 12, 13, 10, 15]);
    h.level_to(3);
    h.s = commands::level_up_new_multiclass(engine(), &h.s, None)
        .expect("new multiclass level")
        .state;
    let start = h.s.level();
    h.pick(
        &format!("ID_LEVEL_{start}"),
        &format!("Multiclass (Level {start})"),
        1,
        "ID_2024_MULTICLASS_WIZARD",
    );
    h.s = commands::level_up_multiclass(engine(), &h.s, start, None)
        .expect("multiclass level")
        .state;
    let snap = h.snap();
    assert_eq!(snap.level, 5);
    let w = sc(&snap, "Warlock");
    assert_eq!((w.pact, w.slots), (Some((2, 2)), [0; 9]));
    assert_eq!(sc(&snap, "Wizard").slots[..2], [3, 0]);
    assert!(snap.multiclass_slots.is_none(), "pact magic does not pool");
}
