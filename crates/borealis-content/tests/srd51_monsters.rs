//! The bundled `srd-5.1-monsters` pack: schema, typed model, spot checks against the printed SRD stat blocks
//! and the CR to XP table.

use borealis_content::monster::{
    Ability, AttackKind, CreatureSize, CreatureType, DamageType, Skill, Usage,
};
use borealis_content::schema::{validate_document, DocumentKind};
use borealis_content::{
    ContentStore, ContentStoreBuilder, Cr, MonsterData, PackFileKind, PackManifest, Ruleset,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn pack_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/srd-5.1-monsters")
}

fn store() -> &'static ContentStore {
    static STORE: OnceLock<ContentStore> = OnceLock::new();
    STORE.get_or_init(|| {
        ContentStoreBuilder::new()
            .add_pack_dir(&pack_dir())
            .unwrap()
            .build()
    })
}

fn monster(name: &str) -> MonsterData {
    let e = store()
        .by_type("Monster")
        .find(|s| s.element.name == name)
        .unwrap_or_else(|| panic!("{name} missing"))
        .element
        .clone();
    MonsterData::from_element(&e).unwrap()
}

fn all() -> Vec<(String, MonsterData)> {
    store()
        .by_type("Monster")
        .map(|s| {
            (
                s.element.id.clone(),
                MonsterData::from_element(&s.element).unwrap(),
            )
        })
        .collect()
}

#[test]
fn pack_and_every_file_are_schema_valid() {
    let dir = pack_dir();
    let text = fs::read_to_string(dir.join("pack.json")).unwrap();
    let manifest_json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(validate_document(DocumentKind::Pack, &manifest_json)
        .unwrap()
        .is_empty());
    let manifest = PackManifest::from_json_str(&text).unwrap();
    assert_eq!(manifest.license.as_deref(), Some("CC-BY-4.0"));
    assert!(manifest
        .attribution
        .unwrap()
        .contains("System Reference Document 5.1"));
    assert_eq!(manifest.rulesets, vec![Ruleset::Dnd2014]);
    for f in &manifest.files {
        assert_eq!(f.kind, PackFileKind::Content);
        let doc: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join(&f.path)).unwrap()).unwrap();
        let violations = validate_document(DocumentKind::Content, &doc).unwrap();
        assert!(
            violations.is_empty(),
            "{}: {:?}",
            f.path,
            violations.iter().take(5).collect::<Vec<_>>()
        );
    }
}

#[test]
fn pack_loads_with_zero_diagnostics() {
    let s = store();
    assert!(s.diagnostics().is_empty(), "{:#?}", s.diagnostics());
    let issues = s.validate();
    assert!(
        issues.is_empty(),
        "{:#?}",
        issues.iter().take(10).collect::<Vec<_>>()
    );
    assert_eq!(s.by_type("Monster").count(), 317);
    assert!(s.undefined_sources().is_empty());
}

#[test]
fn every_monster_validates_and_round_trips() {
    for (id, m) in all() {
        assert!(id.starts_with("ID_SRD51_MONSTER_"), "{id}");
        assert_eq!(m.ruleset, Ruleset::Dnd2014, "{id}");
        assert_eq!(m.validate(), vec![], "{id}");
        assert_eq!(m.warnings(), vec![], "{id}");
        let e = m.to_element(&id, "x", "SRD 5.1 Monsters");
        assert_eq!(MonsterData::from_element(&e).unwrap(), m, "{id}");
    }
}

#[test]
fn soft_checks_hold_for_the_whole_pack() {
    // HP averages, passive Perception and attack damage averages all agree with the printed formulas.
    for (id, m) in all() {
        assert_eq!(m.soft_checks(), vec![], "{id}");
    }
}

#[test]
fn spot_check_goblin() {
    let m = monster("Goblin");
    assert_eq!(m.size, vec![CreatureSize::Small]);
    assert_eq!(m.creature_type, CreatureType::Humanoid);
    assert_eq!(m.tags, vec!["goblinoid"]);
    assert_eq!(m.alignment, "neutral evil");
    assert_eq!(
        (m.ac[0].value, m.ac[0].source.as_deref()),
        (15, Some("leather armor, shield"))
    );
    assert_eq!((m.hp.average, m.hp.formula.as_deref()), (7, Some("2d6")));
    assert_eq!(m.speed.walk, Some(30));
    assert_eq!(
        [
            m.abilities.str,
            m.abilities.dex,
            m.abilities.con,
            m.abilities.int,
            m.abilities.wis,
            m.abilities.cha
        ],
        [8, 14, 10, 10, 8, 8]
    );
    assert_eq!(m.skills, BTreeMap::from([(Skill::Stealth, 6)]));
    assert_eq!(
        (m.senses.darkvision, m.senses.passive_perception),
        (Some(60), 9)
    );
    assert_eq!(m.languages, vec!["Common", "Goblin"]);
    assert_eq!(
        (
            m.challenge_text().as_str(),
            m.xp.unwrap(),
            m.proficiency_bonus.unwrap()
        ),
        ("1/4", 50, 2)
    );
    assert_eq!(m.traits[0].name, "Nimble Escape");
    let scimitar = m.actions[0].attack.as_ref().unwrap();
    assert_eq!(
        (scimitar.kind, scimitar.to_hit, scimitar.reach),
        (AttackKind::MeleeWeapon, 4, Some(5))
    );
    assert_eq!(scimitar.damage[0].dice.as_deref(), Some("1d6+2"));
    let bow = m.actions[1].attack.as_ref().unwrap();
    assert_eq!((bow.range, bow.long_range), (Some(80), Some(320)));
}

#[test]
fn spot_check_owlbear() {
    let m = monster("Owlbear");
    assert_eq!(m.size, vec![CreatureSize::Large]);
    assert_eq!(m.creature_type, CreatureType::Monstrosity);
    assert_eq!(m.alignment, "unaligned");
    assert_eq!(
        (m.ac[0].value, m.ac[0].source.as_deref()),
        (13, Some("natural armor"))
    );
    assert_eq!(
        (m.hp.average, m.hp.formula.as_deref()),
        (59, Some("7d10+21"))
    );
    assert_eq!(m.speed.walk, Some(40));
    assert_eq!(
        [
            m.abilities.str,
            m.abilities.dex,
            m.abilities.con,
            m.abilities.int,
            m.abilities.wis,
            m.abilities.cha
        ],
        [20, 12, 17, 3, 12, 7]
    );
    assert_eq!(m.skills, BTreeMap::from([(Skill::Perception, 3)]));
    assert_eq!(
        (m.senses.darkvision, m.senses.passive_perception),
        (Some(60), 13)
    );
    assert!(m.languages.is_empty());
    assert_eq!(
        (
            m.challenge_text().as_str(),
            m.xp.unwrap(),
            m.proficiency_bonus.unwrap()
        ),
        ("3", 700, 2)
    );
    assert_eq!(
        m.traits.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        ["Keen Sight and Smell"]
    );
    assert_eq!(
        m.actions
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        ["Multiattack", "Beak", "Claws"]
    );
}

#[test]
fn spot_check_adult_red_dragon() {
    let m = monster("Adult Red Dragon");
    assert_eq!(m.size, vec![CreatureSize::Huge]);
    assert_eq!(m.creature_type, CreatureType::Dragon);
    assert_eq!(m.alignment, "chaotic evil");
    assert_eq!(
        (m.ac[0].value, m.ac[0].source.as_deref()),
        (19, Some("natural armor"))
    );
    assert_eq!(
        (m.hp.average, m.hp.formula.as_deref()),
        (256, Some("19d12+133"))
    );
    assert_eq!(
        (m.speed.walk, m.speed.climb, m.speed.fly),
        (Some(40), Some(40), Some(80))
    );
    assert_eq!(
        [
            m.abilities.str,
            m.abilities.dex,
            m.abilities.con,
            m.abilities.int,
            m.abilities.wis,
            m.abilities.cha
        ],
        [27, 10, 25, 16, 13, 21]
    );
    assert_eq!(
        m.saves,
        BTreeMap::from([
            (Ability::Dex, 6),
            (Ability::Con, 13),
            (Ability::Wis, 7),
            (Ability::Cha, 11)
        ])
    );
    assert_eq!(
        m.skills,
        BTreeMap::from([(Skill::Perception, 13), (Skill::Stealth, 6)])
    );
    assert_eq!(m.damage_immunities.len(), 1);
    assert_eq!(m.damage_immunities[0].types, vec![DamageType::Fire]);
    assert_eq!(
        (
            m.senses.blindsight,
            m.senses.darkvision,
            m.senses.passive_perception
        ),
        (Some(60), Some(120), 23)
    );
    assert_eq!(m.languages, vec!["Common", "Draconic"]);
    assert_eq!(
        (
            m.challenge_text().as_str(),
            m.xp.unwrap(),
            m.proficiency_bonus.unwrap()
        ),
        ("17", 18_000, 6)
    );
    assert_eq!(m.traits[0].name, "Legendary Resistance");
    assert_eq!(m.traits[0].usage, Some(Usage::PerDay(3)));
    let breath = m.actions.iter().find(|a| a.name == "Fire Breath").unwrap();
    assert_eq!(breath.usage, Some(Usage::Recharge { min: 5, max: 6 }));
    let legendary = m.legendary.as_ref().unwrap();
    assert_eq!(legendary.per_round, Some(3));
    let wing = legendary
        .actions
        .iter()
        .find(|a| a.name == "Wing Attack")
        .unwrap();
    assert_eq!(wing.cost, Some(2));
    let bite = m
        .actions
        .iter()
        .find(|a| a.name == "Bite")
        .unwrap()
        .attack
        .as_ref()
        .unwrap();
    assert_eq!(
        (bite.to_hit, bite.reach, bite.damage.len()),
        (14, Some(10), 2)
    );
    assert_eq!(bite.damage[1].damage_type, DamageType::Fire);
}

#[test]
fn spot_check_lich() {
    let m = monster("Lich");
    assert_eq!(m.size, vec![CreatureSize::Medium]);
    assert_eq!(m.creature_type, CreatureType::Undead);
    assert_eq!(m.alignment, "any evil alignment");
    assert_eq!(
        (m.ac[0].value, m.ac[0].source.as_deref()),
        (17, Some("natural armor"))
    );
    assert_eq!(
        (m.hp.average, m.hp.formula.as_deref()),
        (135, Some("18d8+54"))
    );
    assert_eq!(
        [
            m.abilities.str,
            m.abilities.dex,
            m.abilities.con,
            m.abilities.int,
            m.abilities.wis,
            m.abilities.cha
        ],
        [11, 16, 16, 20, 14, 16]
    );
    assert_eq!(
        m.saves,
        BTreeMap::from([(Ability::Con, 10), (Ability::Int, 12), (Ability::Wis, 9)])
    );
    assert_eq!(m.skills.len(), 4);
    assert_eq!(m.skills[&Skill::Arcana], 18);
    assert_eq!(
        m.damage_resistances[0].types,
        vec![
            DamageType::Cold,
            DamageType::Lightning,
            DamageType::Necrotic
        ]
    );
    assert_eq!(m.damage_immunities.len(), 2);
    assert_eq!(
        m.damage_immunities[1].types,
        vec![
            DamageType::Bludgeoning,
            DamageType::Piercing,
            DamageType::Slashing
        ]
    );
    assert!(m.damage_immunities[1]
        .text
        .as_deref()
        .unwrap()
        .ends_with("from nonmagical attacks"));
    assert_eq!(
        m.condition_immunities,
        vec![
            "charmed",
            "exhaustion",
            "frightened",
            "paralyzed",
            "poisoned"
        ]
    );
    assert_eq!(
        (m.senses.truesight, m.senses.passive_perception),
        (Some(120), 19)
    );
    assert_eq!(
        (
            m.challenge_text().as_str(),
            m.xp.unwrap(),
            m.proficiency_bonus.unwrap()
        ),
        ("21", 33_000, 7)
    );
    assert_eq!(m.legendary.as_ref().unwrap().per_round, Some(3));
    assert!(m
        .traits
        .iter()
        .any(|t| t.name == "Spellcasting"
            && t.text.contains("\n9th level (1 slot): power word kill")));
}

#[test]
fn spot_check_commoner() {
    let m = monster("Commoner");
    assert_eq!(m.size, vec![CreatureSize::Medium]);
    assert_eq!(m.creature_type, CreatureType::Humanoid);
    assert_eq!(m.tags, vec!["any race"]);
    assert_eq!(m.alignment, "any alignment");
    assert_eq!((m.ac[0].value, m.ac[0].source.clone()), (10, None));
    assert_eq!((m.hp.average, m.hp.formula.as_deref()), (4, Some("1d8")));
    assert_eq!(
        [
            m.abilities.str,
            m.abilities.dex,
            m.abilities.con,
            m.abilities.int,
            m.abilities.wis,
            m.abilities.cha
        ],
        [10; 6]
    );
    assert_eq!(m.senses.passive_perception, 10);
    assert_eq!(
        (
            m.challenge_text().as_str(),
            m.xp.unwrap(),
            m.proficiency_bonus.unwrap()
        ),
        ("0", 10, 2)
    );
    assert_eq!(m.actions.len(), 1);
    assert_eq!(m.actions[0].name, "Club");
}

#[test]
fn swarms_and_shapechangers_keep_their_structure() {
    let bats = monster("Swarm of Bats");
    assert_eq!(
        (bats.size.clone(), bats.swarm_of, bats.creature_type),
        (
            vec![CreatureSize::Medium],
            Some(CreatureSize::Tiny),
            CreatureType::Beast
        )
    );
    let werewolf = monster("Werewolf");
    assert_eq!(werewolf.ac.len(), 2);
    assert_eq!(
        werewolf.ac[0].condition.as_deref(),
        Some("in humanoid form")
    );
    assert_eq!(monster("Succubus/Incubus").tags, vec!["shapechanger"]);
}

/// The "Experience Points by Challenge Rating" table printed in the SRD 5.1 (CR 0 is "0 or 10").
#[test]
fn cr_to_xp_table_matches_the_srd() {
    let srd: [(&str, u32); 34] = [
        ("0", 10),
        ("1/8", 25),
        ("1/4", 50),
        ("1/2", 100),
        ("1", 200),
        ("2", 450),
        ("3", 700),
        ("4", 1_100),
        ("5", 1_800),
        ("6", 2_300),
        ("7", 2_900),
        ("8", 3_900),
        ("9", 5_000),
        ("10", 5_900),
        ("11", 7_200),
        ("12", 8_400),
        ("13", 10_000),
        ("14", 11_500),
        ("15", 13_000),
        ("16", 15_000),
        ("17", 18_000),
        ("18", 20_000),
        ("19", 22_000),
        ("20", 25_000),
        ("21", 33_000),
        ("22", 41_000),
        ("23", 50_000),
        ("24", 62_000),
        ("25", 75_000),
        ("26", 90_000),
        ("27", 105_000),
        ("28", 120_000),
        ("29", 135_000),
        ("30", 155_000),
    ];
    assert_eq!(Cr::XP_TABLE, srd);
    for (text, xp) in srd {
        let cr = Cr::parse(text).unwrap();
        assert_eq!(cr.standard_xp(), xp);
        assert!(cr.accepts_xp(xp));
        assert!(!cr.accepts_xp(xp + 1));
    }
    assert!(Cr::parse("0").unwrap().accepts_xp(0));
    assert!(Cr::parse("31").is_none() && Cr::parse("1/3").is_none() && Cr::parse("0.5").is_none());
    // proficiency bonus steps at 5, 9, 13, 17, 21, 25, 29
    let pb = |t: &str| Cr::parse(t).unwrap().proficiency_bonus();
    assert_eq!(
        [
            pb("0"),
            pb("1/8"),
            pb("4"),
            pb("5"),
            pb("8"),
            pb("9"),
            pb("16"),
            pb("17"),
            pb("29"),
            pb("30")
        ],
        [2, 2, 2, 3, 3, 4, 5, 6, 9, 9]
    );
    // every monster in the pack uses the table value (Frog and Sea Horse print 0 XP)
    for (id, m) in all() {
        assert!(m.cr.unwrap().accepts_xp(m.xp.unwrap()), "{id}");
    }
}

#[test]
fn cr_is_fraction_safe_in_json() {
    let cr: Cr = serde_json::from_str("\"1/8\"").unwrap();
    assert_eq!(serde_json::to_string(&cr).unwrap(), "\"1/8\"");
    assert_eq!(
        serde_json::from_str::<Cr>("5").unwrap(),
        Cr::parse("5").unwrap()
    );
    assert!(serde_json::from_str::<Cr>("0.125").is_err());
    assert!(serde_json::from_str::<Cr>("\"31\"").is_err());
}

#[test]
fn broken_monsters_are_reported_by_the_store_validator() {
    let (id, good) = all()
        .into_iter()
        .find(|(id, _)| id.ends_with("GOBLIN"))
        .unwrap();
    let mut bad = good.clone();
    bad.xp = Some(51);
    bad.proficiency_bonus = Some(3);
    bad.hp.formula = Some("two dice".into());
    bad.abilities.str = 0;
    let issues = bad.validate();
    let fields: Vec<&str> = issues.iter().map(|i| i.field.as_str()).collect();
    assert!(fields.contains(&"proficiency_bonus"), "{fields:?}");
    // A printed XP off the table is a warning (decision HB4), not a hard problem.
    assert!(!fields.contains(&"xp"), "{fields:?}");
    assert_eq!(
        bad.warnings()
            .iter()
            .map(|i| i.field.as_str())
            .collect::<Vec<_>>(),
        vec!["xp"]
    );
    assert!(
        fields.contains(&"hp.formula") && fields.contains(&"abilities.str"),
        "{fields:?}"
    );

    let element = bad.to_element(&id, "Goblin", "SRD 5.1 Monsters");
    let file = borealis_content::ContentFile {
        elements: vec![element],
        ..borealis_content::ContentFile::default()
    };
    let store = ContentStoreBuilder::new()
        .add_file("bad.json", borealis_content::PackOrigin::Custom, file)
        .build();
    let reported: Vec<_> = store
        .validate()
        .into_iter()
        .filter(|i| i.code == borealis_content::IssueCode::InvalidMonsterData)
        .collect();
    assert_eq!(reported.len(), issues.len());
}

#[test]
fn schema_rejects_malformed_monster_data_but_leaves_legacy_monsters_alone() {
    let (id, good) = all()
        .into_iter()
        .find(|(id, _)| id.ends_with("GOBLIN"))
        .unwrap();
    let element = |data: serde_json::Value| {
        serde_json::json!({"format":"borealis-content/1","elements":[
            {"id": id, "name":"Goblin","type":"Monster","source":"S","data": data}]})
    };
    let ok = serde_json::to_value(&good).unwrap();
    assert!(
        validate_document(DocumentKind::Content, &element(ok.clone()))
            .unwrap()
            .is_empty()
    );
    for (key, value) in [
        ("cr", serde_json::json!("1/3")),
        ("size", serde_json::json!(["colossal"])),
        ("creature_type", serde_json::json!("goblin")),
        ("surprise", serde_json::json!(1)),
    ] {
        let mut bad = ok.clone();
        bad[key] = value;
        assert!(
            !validate_document(DocumentKind::Content, &element(bad))
                .unwrap()
                .is_empty(),
            "{key}"
        );
    }
    // a legacy (XML-migrated) Monster without `data` is still accepted by the schema
    let legacy = serde_json::json!({"format":"borealis-content/1","elements":[
        {"id":"ID_X","name":"X","type":"Monster","source":"S","setters":[{"name":"size","value":"Small"}]}]});
    assert!(validate_document(DocumentKind::Content, &legacy)
        .unwrap()
        .is_empty());
}

#[test]
fn from_element_rejects_other_types_and_unknown_fields() {
    let other = borealis_content::Element::new("ID_A", "A", "Spell", "S");
    assert!(matches!(
        MonsterData::from_element(&other),
        Err(borealis_content::monster::MonsterError::NotAMonster(_))
    ));
    let mut bad = monster("Goblin").to_element("ID_G", "G", "S");
    bad.data.insert("wat".into(), serde_json::json!(1));
    assert!(MonsterData::from_element(&bad).is_err());
}
