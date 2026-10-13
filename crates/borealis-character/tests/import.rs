mod common;
use borealis_character::{import_dnd5e, Character, ImportError};
use borealis_engine::state::{AbilityMethod, Choice, ClassSlot, EquipSlot};

const EXAMPLE: &str = include_str!("data/example.dnd5e");

#[test]
fn imports_the_synthetic_example() {
    let Some(engine) = common::engine() else {
        eprintln!("SKIPPED: fixtures/aurora-xml not found");
        return;
    };
    let out = import_dnd5e(engine, EXAMPLE).expect("import");
    let (c, r) = (&out.character, &out.report);
    assert_eq!(c.meta.name, "Test Hero");
    assert_eq!(
        (
            c.meta.group.as_str(),
            c.meta.favorite,
            c.meta.player.as_str()
        ),
        ("Example Group", true, "Player One")
    );
    assert_eq!(c.build.abilities.method, AbilityMethod::PointBuy);
    assert_eq!(c.build.abilities.base, [15, 14, 13, 12, 10, 8]);
    assert_eq!(c.build.experience, 900);
    assert_eq!(
        c.build.levels,
        vec![
            ClassSlot::Main,
            ClassSlot::Main,
            ClassSlot::Multi { starting_level: 3 }
        ]
    );
    assert_eq!(c.build.hp_rolls.len(), 2);
    assert_eq!(
        c.build.hp_rolls[1].class,
        ClassSlot::Multi { starting_level: 3 }
    );
    assert_eq!(c.build.inventory.coins.gp, 37);
    assert_eq!(c.build.inventory.storages, vec!["Saddlebags"]);
    assert_eq!(
        c.details.backstory,
        "Grew up near the docks.\nLearned to fight."
    );
    assert_eq!(
        c.details.attacks[0].item_uid.as_deref(),
        Some("3f2b8c1e-0000-4000-8000-000000000001")
    );
    assert_eq!(c.portrait.as_ref().unwrap().file_name, "hero.png");
    assert_eq!(
        c.details.background_feature.as_ref().unwrap().name,
        "Shelter of the Faithful"
    );
    assert_eq!(c.details.notes.left, "Session 1 notes.");
    assert_eq!(c.details.conditional_ac, "+1 AC while wielding a shield");
    assert!(c
        .build
        .prepared
        .iter()
        .any(|p| p.class == "Wizard" && p.spells == vec!["ID_PHB_SPELL_ALARM"]));
    // selections landed on stable slot keys
    assert!(c.build.selections.iter().any(|s| s.choice
        == Choice::Element("ID_WOTC_PHB_MULTICLASS_WIZARD".into())
        && s.slot.owner == "ID_LEVEL_3"));
    let skills: Vec<_> = c
        .build
        .selections
        .iter()
        .filter(|s| s.slot.owner == "ID_WOTC_PHB_CLASS_FIGHTER")
        .collect();
    assert_eq!(skills.len(), 2, "two skill slots: {skills:?}");
    assert_eq!((skills[0].slot.ordinal, skills[1].slot.ordinal), (1, 2));
    // inventory
    let inv = &c.build.inventory.items;
    assert_eq!(inv.len(), 4);
    assert_eq!(inv[0].equipped, Some(EquipSlot::PrimaryHand));
    assert_eq!(inv[1].equipped, Some(EquipSlot::Armor));
    assert_eq!(inv[2].item, "ID_WOTC_PHB_ITEM_TORCH", "legacy id fallback");
    assert_eq!(
        (inv[2].amount, inv[2].storage.as_deref()),
        (3, Some("Saddlebags"))
    );
    // missing content report
    let missing: Vec<_> = r
        .missing_content
        .iter()
        .map(|m| (m.id.as_str(), m.context.as_str()))
        .collect();
    assert!(missing.contains(&("ID_HOMEBREW_OPTION_GONE", "option")));
    assert!(missing.contains(&("ID_UNKNOWN_CANTRIP_FROM_A_BOOK_I_LACK", "selection")));
    assert!(missing.contains(&("ID_SOME_HOMEBREW_THING", "inventory")));
    assert!(missing.contains(&("ID_BOOK_I_DO_NOT_HAVE", "dependency")));
    assert_eq!(r.level, 3);
    assert_eq!(r.file_level, Some(3));
    assert!(r.selections_imported >= 8);
    // the engine can evaluate the result and reports what it ignores
    let snap = engine.derive(&c.build);
    assert_eq!(snap.level, 3);
    assert_eq!(snap.classes.len(), 2);
    assert_eq!(snap.classes[0].class.as_ref().unwrap().name, "Fighter");
    assert_eq!(snap.classes[1].class.as_ref().unwrap().name, "Wizard");
    assert!(snap.skill("acrobatics").unwrap().proficient);
    assert!(snap.skill("insight").unwrap().proficient);
    assert_eq!(snap.armor_class, 16 + 1, "chain mail 16 + Defense");
    let wizard = snap
        .caster("Wizard")
        .expect("wizard casting through the multiclass");
    assert!(wizard
        .spells
        .iter()
        .any(|s| s.id == "ID_PHB_SPELL_ACID_SPLASH"));
    assert!(snap
        .diagnostics
        .iter()
        .any(|d| d.code == "selection.missing_content"));
    assert_eq!(c.import.as_ref().unwrap().source, "aurora-dnd5e");
    assert_eq!(
        c.import.as_ref().unwrap().missing_content as usize,
        r.missing_content.len()
    );
}

#[test]
fn imported_character_round_trips_through_json() {
    let Some(engine) = common::engine() else {
        return;
    };
    let out = import_dnd5e(engine, EXAMPLE).unwrap();
    let text = out.character.to_json_string().unwrap();
    assert!(text.contains("borealis-character"));
    let back = Character::from_json_str(&text).unwrap();
    assert_eq!(back, out.character);
    assert_eq!(
        engine.derive(&back.build),
        engine.derive(&out.character.build)
    );
}

#[test]
fn rejects_things_that_are_not_characters() {
    let Some(engine) = common::engine() else {
        return;
    };
    assert!(matches!(
        import_dnd5e(engine, "<elements/>"),
        Err(ImportError::NotCharacter)
    ));
    assert!(matches!(
        import_dnd5e(engine, "<character><build/></character>"),
        Err(ImportError::BadVersion(_))
    ));
    assert!(matches!(
        import_dnd5e(engine, "<character version=\"1.0.3\"/>"),
        Err(ImportError::MissingSection("build"))
    ));
    assert!(matches!(
        import_dnd5e(engine, "<character version=\"1.0.3\"><build/></character>"),
        Err(ImportError::MissingSection("elements"))
    ));
    assert!(matches!(
        import_dnd5e(engine, "<character"),
        Err(ImportError::Xml(_))
    ));
    assert!(matches!(
        import_dnd5e(engine, "<character version=\"abc\"><build/></character>"),
        Err(ImportError::BadVersion(_))
    ));
}

#[test]
fn unmapped_slots_are_reported_and_kept() {
    let Some(engine) = common::engine() else {
        return;
    };
    let xml = EXAMPLE.replace(r#"type="Class Feature" name="Fighting Style" requiredLevel="1" checksum="25bf9bd9" registered="ID_WOTC_PHB_CLASS_FEATURE_FIGHTINGSTYLE_DEFENSE""#, r#"type="Bogus Type" name="Renamed Rule" requiredLevel="1" checksum="00000000" registered="ID_WOTC_PHB_CLASS_FEATURE_FIGHTINGSTYLE_DEFENSE""#);
    let out = import_dnd5e(engine, &xml).unwrap();
    assert_eq!(
        out.report.unmapped_slots.len(),
        1,
        "{:?}",
        out.report.unmapped_slots
    );
    assert_eq!(out.character.unresolved.len(), 1);
    assert_eq!(
        out.character.unresolved[0].chosen,
        "ID_WOTC_PHB_CLASS_FEATURE_FIGHTINGSTYLE_DEFENSE"
    );
}
