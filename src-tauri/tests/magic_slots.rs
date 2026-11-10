//! Magic page wiring of selection slots (regression): slot ids are opaque
//! `owner:rule:ordinal`, so a spell slot names its spellcasting class in `class_slot_id`, and a
//! list-rule slot is typed `list` (its options are list item numbers, not element ids).

use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::error::ErrorCode;
use borealis_lib::state::{AppPaths, AppState};
use std::path::PathBuf;

fn app() -> (tempfile::TempDir, AppState) {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    (dir, AppState::new(paths).expect("state"))
}

fn slot_id(snap: &CharacterSnapshot, title: &str) -> String {
    snap.slots
        .iter()
        .find(|s| s.title == title)
        .unwrap_or_else(|| panic!("no slot {title}"))
        .id
        .clone()
}

/// A 2024 Elf Wizard 1 with the Sage background (Seraphine Vale).
fn sage_wizard(app: &AppState) -> CharacterSnapshot {
    let request = NewCharacterRequest {
        name: "Seraphine".into(),
        ruleset: Ruleset::Dnd2024,
        starting_level: 1,
        gender: "f".into(),
        ability_method: AbilityMethodDto::PointBuy,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    let s = app
        .create_character(Ruleset::Dnd2024, &request)
        .expect("create");
    let id = s.id.clone();
    let s = app
        .apply_selection(&id, &slot_id(&s, "Race"), "ID_2024_SPECIES_ELF")
        .expect("species");
    let s = app
        .apply_selection(&id, &slot_id(&s, "Class"), "ID_2024_CLASS_WIZARD")
        .expect("class");
    app.apply_selection(&id, &slot_id(&s, "Background"), "ID_2024_BACKGROUND_SAGE")
        .expect("background")
}

#[test]
fn wizard_spell_slots_name_their_class_and_other_slots_do_not() {
    let (_dir, app) = app();
    let snap = sage_wizard(&app);
    let wizard = snap
        .spellcasting
        .classes
        .iter()
        .find(|c| c.class_name == "Wizard")
        .expect("wizard spellcasting");
    let cantrips: Vec<_> = snap
        .slots
        .iter()
        .filter(|s| s.title.starts_with("Cantrip (Wizard"))
        .collect();
    assert!(!cantrips.is_empty(), "wizard cantrip slots");
    for s in &cantrips {
        assert!(
            !s.id.starts_with(&format!("{}:", wizard.class_slot_id)),
            "slot ids are owner:rule:ordinal, not class-prefixed: {}",
            s.id
        );
        assert_eq!(
            s.class_slot_id.as_deref(),
            Some(wizard.class_slot_id.as_str()),
            "{}",
            s.id
        );
    }
    for s in snap.slots.iter().filter(|s| s.tab != "spells") {
        assert_eq!(s.class_slot_id, None, "non-spell slot {}", s.id);
    }
    // Magic Initiate (from Sage) feeds no class: the Magic page shows its catalogue in a block of its own (B1).
    for s in snap
        .slots
        .iter()
        .filter(|s| s.title.starts_with("Magic Initiate"))
    {
        assert_eq!(s.class_slot_id, None, "{}", s.id);
    }
}

/// B1 (tester video 2026-11-02): a 2024 Dragonborn Barbarian with the Acolyte background gets
/// Magic Initiate (Cleric) and no spellcasting class. Its three spell slots sit on the Magic tab,
/// feed no class, and name their source so the Magic page can list them and their catalogue.
#[test]
fn acolyte_barbarian_magic_initiate_slots_carry_a_source_label() {
    let (_dir, app) = app();
    let snap = acolyte_barbarian(&app);
    let id = snap.id.clone();
    assert!(
        snap.spellcasting.classes.is_empty(),
        "a Barbarian casts nothing"
    );
    let spells: Vec<_> = snap.slots.iter().filter(|s| s.tab == "spells").collect();
    let titles: Vec<&str> = spells.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "Magic Initiate Cantrips",
            "Magic Initiate Cantrips",
            "Magic Initiate Level 1 Spell"
        ]
    );
    for s in &spells {
        assert_eq!(s.class_slot_id, None, "{}", s.id);
        assert_eq!(
            s.group_label.as_deref(),
            Some("Magic Initiate (Cleric)"),
            "{}",
            s.id
        );
        assert!(s.chosen.is_none() && s.required, "{}", s.id);
    }
    let count = |slot: &str| {
        app.get_slot_options(&id, slot, None, Some(500), None)
            .expect("options")
            .items
            .len()
    };
    assert_eq!(count(&spells[0].id), 7, "Cleric cantrips");
    assert_eq!(count(&spells[2].id), 15, "Cleric level-1 spells");

    // Once the spells and the ability are chosen, the block the label names exists, so the Magic
    // page shows those slots' catalogue in it.
    let mut snap = snap;
    for slot in [
        "Magic Initiate Cantrips",
        "Magic Initiate Level 1 Spell",
        "Spellcasting Ability",
    ] {
        let open = snap
            .slots
            .iter()
            .find(|s| s.title == slot && s.chosen.is_none())
            .unwrap_or_else(|| panic!("open {slot}"))
            .id
            .clone();
        let options = app
            .get_slot_options(&id, &open, None, Some(500), None)
            .expect("options");
        let pick = options
            .items
            .first()
            .expect("an option")
            .base
            .summary
            .id
            .clone();
        snap = app.apply_selection(&id, &open, &pick).expect("pick");
    }
    let open = snap
        .slots
        .iter()
        .find(|s| s.title == "Magic Initiate Cantrips" && s.chosen.is_none())
        .expect("second cantrip")
        .id
        .clone();
    let options = app
        .get_slot_options(&id, &open, None, Some(500), None)
        .expect("options");
    let chosen: Vec<String> = snap
        .slots
        .iter()
        .filter_map(|s| s.chosen.as_ref().map(|c| c.id.clone()))
        .collect();
    let pick = options
        .items
        .iter()
        .map(|o| o.base.summary.id.clone())
        .find(|o| !chosen.contains(o))
        .expect("another cantrip");
    let snap = app.apply_selection(&id, &open, &pick).expect("pick");
    assert!(
        snap.spellcasting
            .classes
            .iter()
            .any(|c| c.class_name == "Magic Initiate (Cleric)"),
        "{:?}",
        snap.spellcasting
            .classes
            .iter()
            .map(|c| &c.class_name)
            .collect::<Vec<_>>()
    );
    assert!(snap
        .slots
        .iter()
        .filter(|s| s.tab == "spells")
        .all(|s| s.chosen.is_some()));
}

/// A 2024 Dragonborn Barbarian 1 with the Acolyte background: Magic Initiate (Cleric), no
/// spellcasting class (tester video 2026-11-02).
fn acolyte_barbarian(app: &AppState) -> CharacterSnapshot {
    let request = NewCharacterRequest {
        name: "Torrin".into(),
        ruleset: Ruleset::Dnd2024,
        starting_level: 1,
        gender: "m".into(),
        ability_method: AbilityMethodDto::PointBuy,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    let s = app
        .create_character(Ruleset::Dnd2024, &request)
        .expect("create");
    let id = s.id.clone();
    let s = app
        .apply_selection(&id, &slot_id(&s, "Race"), "ID_2024_SPECIES_DRAGONBORN")
        .expect("species");
    let s = app
        .apply_selection(&id, &slot_id(&s, "Class"), "ID_2024_CLASS_BARBARIAN")
        .expect("class");
    app.apply_selection(
        &id,
        &slot_id(&s, "Background"),
        "ID_2024_BACKGROUND_ACOLYTE",
    )
    .expect("background")
}

/// Picks the first option of the slot `progress.nextSlotId` names that no other slot holds.
fn pick_next(app: &AppState, snap: &CharacterSnapshot) -> CharacterSnapshot {
    let next = snap.progress.next_slot_id.clone().expect("a next slot");
    let held: Vec<String> = snap
        .slots
        .iter()
        .filter_map(|s| s.chosen.as_ref().map(|c| c.id.clone()))
        .collect();
    let options = app
        .get_slot_options(&snap.id, &next, None, Some(500), None)
        .expect("options");
    let pick = options
        .items
        .iter()
        .map(|o| o.base.summary.id.clone())
        .find(|o| !held.contains(o))
        .unwrap_or_else(|| panic!("an option for {next}"));
    app.apply_selection(&snap.id, &next, &pick).expect("pick")
}

fn next_slot(snap: &CharacterSnapshot) -> Option<&borealis_lib::dto::SelectionSlot> {
    let next = snap.progress.next_slot_id.as_deref()?;
    snap.slots.iter().find(|s| s.id == next)
}

/// B2 (tester video 2026-11-02): Magic Initiate's "Spellcasting Ability" is the feat's own choice,
/// so it sits on Build > Feats under the feat's name, and Next selection asks it before the spells
/// (the Magic Initiate block can't exist until it is chosen).
#[test]
fn magic_initiate_ability_is_on_feats_and_asked_before_its_spells() {
    let (_dir, app) = app();
    let mut snap = acolyte_barbarian(&app);
    let ability = snap
        .slots
        .iter()
        .find(|s| s.title == "Spellcasting Ability")
        .expect("ability slot")
        .clone();
    assert_eq!(ability.tab, "feats");
    assert_eq!(ability.element_type, "list");
    assert_eq!(
        ability.group_label.as_deref(),
        Some("Magic Initiate (Cleric)")
    );
    assert!(ability.required && !ability.locked && ability.chosen.is_none());

    // Next selection works through the earlier tabs (languages, proficiencies, ...) and reaches
    // the ability before any spell slot.
    for _ in 0..40 {
        match next_slot(&snap) {
            Some(s) if s.tab != "feats" && s.tab != "spells" => snap = pick_next(&app, &snap),
            _ => break,
        }
    }
    assert_eq!(
        snap.progress.next_slot_id.as_deref(),
        Some(ability.id.as_str()),
        "next is {:?}",
        next_slot(&snap).map(|s| (&s.tab, &s.title))
    );
    let spells: Vec<_> = snap
        .slots
        .iter()
        .filter(|s| s.title.starts_with("Magic Initiate"))
        .collect();
    assert_eq!(spells.len(), 3);
    assert!(spells.iter().all(|s| s.chosen.is_none()));

    // The ability, then the three spells, each through Next selection: the block then exists.
    snap = pick_next(&app, &snap);
    for _ in 0..3 {
        let next = next_slot(&snap).expect("a spell slot next");
        assert!(next.title.starts_with("Magic Initiate"), "{}", next.title);
        snap = pick_next(&app, &snap);
    }
    assert!(
        snap.spellcasting
            .classes
            .iter()
            .any(|c| c.class_name == "Magic Initiate (Cleric)"),
        "{:?}",
        snap.spellcasting
            .classes
            .iter()
            .map(|c| &c.class_name)
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_cantrip_picked_through_a_real_slot_id_joins_the_wizard_spell_list() {
    let (_dir, app) = app();
    let snap = sage_wizard(&app);
    let slot = snap
        .slots
        .iter()
        .find(|s| s.title.starts_with("Cantrip (Wizard") && s.chosen.is_none())
        .expect("open cantrip slot");
    let options = app
        .get_slot_options(&snap.id, &slot.id, None, Some(1), None)
        .expect("options");
    let pick = &options
        .items
        .first()
        .expect("a cantrip option")
        .base
        .summary;
    let after = app
        .apply_selection(&snap.id, &slot.id, &pick.id)
        .expect("pick cantrip");
    let wizard = after
        .spellcasting
        .classes
        .iter()
        .find(|c| c.class_name == "Wizard")
        .expect("wizard");
    let entry = wizard
        .spells
        .iter()
        .find(|s| s.id == pick.id)
        .expect("picked cantrip listed");
    assert_eq!(entry.slot_id.as_deref(), Some(slot.id.as_str()));
}

#[test]
fn sage_ability_scores_is_a_list_slot_whose_options_are_not_elements() {
    let (_dir, app) = app();
    let snap = sage_wizard(&app);
    let slot = snap
        .slots
        .iter()
        .find(|s| s.title == "Ability Scores (Sage)")
        .expect("sage ability slot");
    assert_eq!(slot.element_type, "list");
    let options = app
        .get_slot_options(&snap.id, &slot.id, None, None, None)
        .expect("options");
    let first = &options.items.first().expect("an entry").base.summary;
    assert_eq!(first.ty, "list");
    assert!(
        app.get_element(&first.id).is_err(),
        "list entry ids are not element ids"
    );
    let after = app
        .apply_selection(&snap.id, &slot.id, &first.id)
        .expect("pick entry");
    let chosen = after
        .slots
        .iter()
        .find(|s| s.id == slot.id)
        .and_then(|s| s.chosen.clone())
        .expect("chosen");
    assert_eq!(chosen.ty, "list");
}

/// A Sage Wizard 1 with every Magic Initiate pick made (the ability and the three spells), so the
/// feat's own spellcasting block exists next to the Wizard's.
fn sage_wizard_with_magic_initiate(app: &AppState) -> CharacterSnapshot {
    let mut snap = sage_wizard(app);
    for title in [
        "Spellcasting Ability",
        "Magic Initiate Cantrips",
        "Magic Initiate Cantrips",
        "Magic Initiate Level 1 Spell",
    ] {
        let open = snap
            .slots
            .iter()
            .find(|s| s.title == title && s.chosen.is_none())
            .unwrap_or_else(|| panic!("open {title}"))
            .id
            .clone();
        let held: Vec<String> = snap
            .slots
            .iter()
            .filter_map(|s| s.chosen.as_ref().map(|c| c.id.clone()))
            .collect();
        let pick = app
            .get_slot_options(&snap.id, &open, None, Some(500), None)
            .expect("options")
            .items
            .iter()
            .map(|o| o.base.summary.id.clone())
            .find(|o| !held.contains(o))
            .unwrap_or_else(|| panic!("an option for {title}"));
        snap = app.apply_selection(&snap.id, &open, &pick).expect("pick");
    }
    snap
}

/// A feat's spellcasting block (Magic Initiate) has an id of its own: the Magic page keys its
/// tabs by `classSlotId`, so a feat block sharing the Wizard's `main` showed the Wizard's spells
/// on the feat's tab, and a prepare tick there addressed the Wizard.
#[test]
fn a_feat_spellcasting_block_has_its_own_id_next_to_the_class() {
    let (_dir, app) = app();
    let snap = sage_wizard_with_magic_initiate(&app);
    let blocks: Vec<(&str, &str)> = snap
        .spellcasting
        .classes
        .iter()
        .map(|c| (c.class_name.as_str(), c.class_slot_id.as_str()))
        .collect();
    let wizard = snap
        .spellcasting
        .classes
        .iter()
        .find(|c| c.class_name == "Wizard")
        .expect("wizard block");
    let feat = snap
        .spellcasting
        .classes
        .iter()
        .find(|c| c.class_name.starts_with("Magic Initiate"))
        .unwrap_or_else(|| panic!("feat block in {blocks:?}"));
    assert_eq!(wizard.class_slot_id, "main", "class ids are unchanged");
    let mut ids: Vec<&str> = blocks.iter().map(|b| b.1).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(
        ids.len(),
        blocks.len(),
        "every block has its own id: {blocks:?}"
    );
    assert!(
        feat.class_slot_id != "main" && !feat.class_slot_id.starts_with("multi-"),
        "{blocks:?}"
    );
    assert!(
        !feat.class_slot_id.contains(char::is_whitespace),
        "the id is used in HTML ids: {}",
        feat.class_slot_id
    );

    // The same feat block keeps its id across derives.
    let again = app.get_snapshot(&snap.id).expect("snapshot");
    assert!(again
        .spellcasting
        .classes
        .iter()
        .any(|c| c.class_name == feat.class_name && c.class_slot_id == feat.class_slot_id));

    // Preparing addresses the block the id names: the feat's spells are not prepared spells, and
    // the Wizard's list is untouched.
    let spell = feat
        .spells
        .iter()
        .find(|s| s.level > 0)
        .expect("the feat's level 1 spell");
    let err = app
        .set_spell_prepared(&snap.id, &feat.class_slot_id, &spell.id, true)
        .expect_err("a feat's spell is not prepared");
    assert_eq!(err.code, ErrorCode::RequirementFailed, "{err:?}");
    let after = app.get_snapshot(&snap.id).expect("snapshot");
    let wizard_after = after
        .spellcasting
        .classes
        .iter()
        .find(|c| c.class_name == "Wizard")
        .expect("wizard");
    assert_eq!(wizard_after.prepared_count, wizard.prepared_count);
    let err = app
        .set_spell_prepared(&snap.id, "caster-no-such-block", &spell.id, true)
        .expect_err("unknown block");
    assert_eq!(err.code, ErrorCode::NotFound, "{err:?}");
}

/// A Sacred Oath whose spells name the Paladin's spellcasting in lower and upper case, as
/// community Aurora content writes it (`spellcasting="paladin"`).
fn install_lowercase_oath(app: &AppState) {
    let xml = r#"<elements><info><name>Ward Oath</name><update version="0.0.1"><file name="ward.xml" url="https://example.invalid/ward.xml"/></update></info>
<element name="Oath of the Test Ward" type="Archetype" source="Ward Oath" id="ID_T_MS_ARCHETYPE_OATH_TEST_WARD"><supports>Sacred Oath</supports><description><p>Synthetic.</p></description><rules><grant type="Archetype Feature" id="ID_T_MS_ARCHETYPE_FEATURE_WARD_SPELLS" level="3" /></rules></element>
<element name="Ward Spells" type="Archetype Feature" source="Ward Oath" id="ID_T_MS_ARCHETYPE_FEATURE_WARD_SPELLS"><description><p>Synthetic.</p></description><rules><grant type="Spell" id="ID_PHB_SPELL_SANCTUARY" spellcasting="paladin" prepared="true" /><select type="Spell" name="Ward Pick" supports="Cleric,1" spellcasting="PALADIN" /></rules></element>
</elements>"#;
    use base64::Engine as _;
    let req = borealis_lib::content_dto::ImportPackRequest {
        file_name: "ward.xml".into(),
        content_base64: base64::engine::general_purpose::STANDARD.encode(xml),
        format: None,
    };
    app.import_pack(&req, &borealis_lib::content_packs::NullSink)
        .expect("install");
}

/// Aurora matches a spell's `spellcasting` name to the class ignoring case: the oath's spells
/// join the Paladin block (with its save DC), its spell slot feeds the Paladin's Magic tab, and
/// no "Other Spells" block is made for them.
#[test]
fn a_lowercase_spellcasting_name_feeds_the_paladin_block_and_its_magic_tab() {
    let (_dir, app) = app();
    install_lowercase_oath(&app);
    let request = NewCharacterRequest {
        name: "Ward".into(),
        ruleset: Ruleset::Dnd2014,
        starting_level: 3,
        gender: String::new(),
        ability_method: AbilityMethodDto::PointBuy,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    let s = app
        .create_character(Ruleset::Dnd2014, &request)
        .expect("create");
    let id = s.id.clone();
    let s = app
        .apply_selection(&id, &slot_id(&s, "Race"), "ID_RACE_HUMAN")
        .expect("race");
    let s = app
        .apply_selection(&id, &slot_id(&s, "Class"), "ID_WOTC_PHB_CLASS_PALADIN")
        .expect("class");
    let s = app
        .apply_selection(
            &id,
            &slot_id(&s, "Sacred Oath"),
            "ID_T_MS_ARCHETYPE_OATH_TEST_WARD",
        )
        .expect("oath");
    let s = app
        .apply_selection(&id, &slot_id(&s, "Ward Pick"), "ID_PHB_SPELL_BLESS")
        .expect("ward pick");
    let names: Vec<&str> = s
        .spellcasting
        .classes
        .iter()
        .map(|c| c.class_name.as_str())
        .collect();
    assert_eq!(names, ["Paladin"], "no Other Spells block");
    let paladin = &s.spellcasting.classes[0];
    assert!(paladin.save_dc >= 8, "a real save DC: {}", paladin.save_dc);
    for spell in ["Sanctuary", "Bless"] {
        assert!(
            paladin.spells.iter().any(|x| x.name == spell),
            "{spell} in the Paladin block"
        );
    }
    let pick = s
        .slots
        .iter()
        .find(|x| x.title == "Ward Pick")
        .expect("Ward Pick slot");
    assert_eq!(
        pick.class_slot_id.as_deref(),
        Some(paladin.class_slot_id.as_str())
    );
}
