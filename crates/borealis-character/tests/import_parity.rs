//! `.dnd5e` import parity with Aurora's loader (det-char-1): equip locations, attack-list rows
//! of unequipped weapons, Aurora's proxy items and selects copied into a Multiclass element.
//! Every file here is synthetic, written from the format specification.

mod common;
use borealis_character::import_dnd5e;
use borealis_engine::state::{Choice, EquipSlot};

const FIGHTER: &str = r#"<element type="Class" name="Class" requiredLevel="1" checksum="33a7b15b" registered="ID_WOTC_PHB_CLASS_FIGHTER" />"#;

/// A level-1 human fighter with the given extra level nodes, items, attack rows and element
/// nodes (inventory and tree are written the way Aurora writes them).
fn character(levels: &str, items: &str, attacks: &str, tree: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<character version="1.19.309" preview="false">
  <display-properties><name>Parity</name><level>1</level></display-properties>
  <build>
    <input><name>Parity</name><attacks><description></description>{attacks}</attacks></input>
    <abilities><strength>16</strength><dexterity>14</dexterity><constitution>12</constitution>
      <intelligence>10</intelligence><wisdom>10</wisdom><charisma>8</charisma></abilities>
    <elements>
      <element type="Option" name="Multiclassing" id="ID_INTERNAL_OPTION_ALLOW_MULTICLASSING" />
      <element type="Level" name="1" id="ID_LEVEL_1">
        <element type="Race" name="Race" requiredLevel="1" checksum="1597ef76" registered="ID_RACE_HUMAN" />
        {FIGHTER}
      </element>
      {levels}
      {tree}
    </elements>
    <equipment>{items}</equipment>
    <sum element-count="0"></sum>
  </build>
</character>"#
    )
}

fn item(uid: &str, id: &str, equipped: Option<&str>) -> String {
    let eq = match equipped {
        None => String::new(),
        Some("") => "<equipped>true</equipped>".to_string(),
        Some(loc) => format!(r#"<equipped location="{loc}">true</equipped>"#),
    };
    format!(
        r#"<item identifier="{uid}" name="x" id="{id}">{eq}<details card="false"><name></name><notes></notes></details></item>"#
    )
}

fn attack_row(uid: &str, name: &str) -> String {
    format!(
        r#"<attack identifier="{uid}" name="{name}" range="5 ft" attack="+5" damage="1d12+3 slashing" displayed="true" ability="Strength"><description></description></attack>"#
    )
}

#[test]
fn every_equip_location_aurora_restores_is_mapped() {
    let Some(engine) = common::engine() else {
        eprintln!("SKIPPED: fixtures/aurora-xml not found");
        return;
    };
    let items = [
        item("a", "ID_WOTC_PHB_WEAPON_GREATAXE", Some("Two-Handed")),
        item(
            "b",
            "ID_WOTC_PHB_WEAPON_LONGSWORD",
            Some("Two-Handed (Versatile)"),
        ),
        item("c", "ID_WOTC_PHB_WEAPON_DAGGER", Some("offhand")),
        item("d", "ID_WOTC_PHB_WEAPON_HANDAXE", Some("One-Hand")),
        item("e", "ID_WOTC_ARMOR_HEAVY_CHAIN_MAIL", Some("body")),
        item("f", "ID_WOTC_PHB_WEAPON_MACE", Some("")),
        item("g", "ID_WOTC_PHB_WEAPON_CLUB", Some("Belt")),
    ]
    .concat();
    let out = import_dnd5e(engine, &character("", &items, "", "")).expect("import");
    let inv = &out.character.build.inventory.items;
    let slot = |uid: &str| {
        let it = inv.iter().find(|i| i.uid == uid).expect(uid);
        (it.equipped, it.versatile)
    };
    assert_eq!(slot("a"), (Some(EquipSlot::PrimaryHand), false));
    assert_eq!(slot("b"), (Some(EquipSlot::PrimaryHand), true));
    assert_eq!(slot("c"), (Some(EquipSlot::SecondaryHand), false));
    assert_eq!(slot("d"), (Some(EquipSlot::PrimaryHand), false));
    assert_eq!(slot("e"), (Some(EquipSlot::Armor), false));
    // no location: Aurora's plain equip, by the item's slot (a one-handed weapon: main hand)
    assert_eq!(slot("f"), (Some(EquipSlot::PrimaryHand), false));
    // a location Aurora's loader does not know leaves the item unequipped, and says so
    assert_eq!(slot("g"), (None, false));
    assert!(out.report.warnings.iter().any(|w| w.contains("\"Belt\"")));
}

#[test]
fn a_two_handed_weapon_keeps_its_attack() {
    let Some(engine) = common::engine() else {
        eprintln!("SKIPPED: fixtures/aurora-xml not found");
        return;
    };
    let items = item("axe", "ID_WOTC_PHB_WEAPON_GREATAXE", Some("Two-Handed"));
    let out = import_dnd5e(engine, &character("", &items, "", "")).expect("import");
    let snap = engine.derive(&out.character.build);
    let row = snap
        .attacks
        .iter()
        .find(|a| a.uid == "axe")
        .expect("attack");
    assert!(row.damage.starts_with("1d12+3"), "{}", row.damage);
}

#[test]
fn listed_attacks_of_unequipped_weapons_stay_on_the_sheet() {
    let Some(engine) = common::engine() else {
        eprintln!("SKIPPED: fixtures/aurora-xml not found");
        return;
    };
    let items = [
        item("listed", "ID_WOTC_PHB_WEAPON_GREATAXE", None),
        item("unlisted", "ID_WOTC_PHB_WEAPON_MACE", None),
    ]
    .concat();
    let attacks = attack_row("LISTED", "Greataxe");
    let out = import_dnd5e(engine, &character("", &items, &attacks, "")).expect("import");
    assert_eq!(out.character.build.inventory.listed_attacks, vec!["listed"]);
    let snap = engine.derive(&out.character.build);
    let uids: Vec<&str> = snap.attacks.iter().map(|a| a.uid.as_str()).collect();
    assert!(uids.contains(&"listed"), "{uids:?}");
    assert!(!uids.contains(&"unlisted"), "{uids:?}");
    let row = snap.attacks.iter().find(|a| a.uid == "listed").unwrap();
    // proficient fighter, STR 16: +2 +3
    assert_eq!(row.attack, 5);
    // removing the item drops it from the list
    let next =
        borealis_engine::commands::remove_item(engine, &out.character.build, "listed").unwrap();
    assert!(next.state.inventory.listed_attacks.is_empty());
}

#[test]
fn proxy_items_grant_their_element() {
    let Some(engine) = common::engine() else {
        eprintln!("SKIPPED: fixtures/aurora-xml not found");
        return;
    };
    let spell = "ID_PHB_INTERNAL_ITEM__SPELL_PROXY_SPELL_FOG_CLOUD";
    let skill = "ID_PHB_INTERNAL_ITEM_PROFICIENCY_PROXY_PROFICIENCY_SKILL_STEALTH";
    let feat = "ID_PHB_INTERNAL_ITEM_FEAT_PROXY_GRAPPLER";
    let gone = "ID_ZZZ_INTERNAL_ITEM_FEAT_PROXY_NOT_INSTALLED";
    let items = [
        item("p1", spell, Some("")),
        item("p2", skill, Some("")),
        item("p3", feat, Some("")),
        item("p4", gone, Some("")),
    ]
    .concat();
    let out = import_dnd5e(engine, &character("", &items, "", "")).expect("import");
    let ids: Vec<&str> = out
        .report
        .missing_content
        .iter()
        .map(|m| m.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec![gone],
        "only the proxy whose target is not installed"
    );
    assert_eq!(out.report.missing_content[0].context, "proxy item");
    let snap = engine.derive(&out.character.build);
    let has = |id: &str| snap.elements.iter().any(|r| r.element.id == id);
    assert!(has("ID_PHB_SPELL_FOG_CLOUD"));
    assert!(has("ID_PROFICIENCY_SKILL_STEALTH"));
    assert!(has("ID_PHB_FEAT_GRAPPLER"));
    // the proxy spell is an additional spell, not an item spell block
    assert!(snap
        .spellcasting
        .iter()
        .all(|b| b.spells.iter().all(|s| s.id != "ID_PHB_SPELL_FOG_CLOUD")));
    let proxy = engine
        .universe(borealis_content::Ruleset::Dnd2014)
        .get(spell)
        .unwrap();
    assert_eq!(proxy.el.name, "Additional Spell, Fog Cloud");
}

#[test]
fn a_class_spell_proxy_adds_to_that_class() {
    let Some(engine) = common::engine() else {
        eprintln!("SKIPPED: fixtures/aurora-xml not found");
        return;
    };
    let uni = engine.universe(borealis_content::Ruleset::Dnd2014);
    let p = uni
        .get("ID_PHB_INTERNAL_ITEM_WIZARD_SPELL_PROXY_SPELL_FOG_CLOUD")
        .expect("wizard proxy");
    assert_eq!(p.el.name, "Additional Wizard Spell, Fog Cloud");
    let borealis_content::Rule::Grant(g) = &p.el.rules[0] else {
        panic!("grant");
    };
    assert_eq!(g.spellcasting.as_deref(), Some("Wizard"));
    assert!(uni
        .get("ID_PHB_INTERNAL_ITEM_NOCLASS_SPELL_PROXY_SPELL_FOG_CLOUD")
        .is_none());
    // proxies are resolvable, never listed
    assert!(uni.by_type("Item").all(|i| !i.el.id.contains("_PROXY")));
}

#[test]
fn a_multiclass_select_lands_on_the_active_copy() {
    let Some(engine) = common::engine() else {
        eprintln!("SKIPPED: fixtures/aurora-xml not found");
        return;
    };
    let level2 = r#"<element type="Level" name="2" id="ID_LEVEL_2" multiclass="true" starting="true" class="ID_WOTC_PHB_MULTICLASS_ROGUE">
        <element type="Multiclass" name="Multiclass (Level 2)" requiredLevel="2" registered="ID_WOTC_PHB_MULTICLASS_ROGUE">
          <element type="Proficiency" name="Skill Proficiency (Rogue)" requiredLevel="1" registered="ID_PROFICIENCY_SKILL_SLEIGHTOFHAND" />
        </element>
      </element>"#;
    let out = import_dnd5e(engine, &character(level2, "", "", "")).expect("import");
    let sel = out
        .character
        .build
        .selections
        .iter()
        .find(|s| s.choice == Choice::Element("ID_PROFICIENCY_SKILL_SLEIGHTOFHAND".into()))
        .expect("selection");
    let snap = engine.derive(&out.character.build);
    assert!(
        snap.slots.iter().any(|s| s.key == sel.slot),
        "the choice sits in an open slot: {:?}",
        sel.slot
    );
    assert!(snap
        .elements
        .iter()
        .any(|r| r.element.id == "ID_PROFICIENCY_SKILL_SLEIGHTOFHAND"));
}

#[test]
fn a_missing_choice_keeps_its_slot_open_and_is_listed() {
    let Some(engine) = common::engine() else {
        eprintln!("SKIPPED: fixtures/aurora-xml not found");
        return;
    };
    let mut state = import_dnd5e(engine, &character("", "", "", ""))
        .expect("import")
        .character
        .build;
    let race = state
        .selections
        .iter_mut()
        .find(|s| s.choice == Choice::Element("ID_RACE_HUMAN".into()))
        .expect("race pick");
    race.choice = Choice::Element("ID_ZZ_RACE_GONE".into());
    let race_key = race.slot.clone();
    // a pick under the missing race: its slot cannot exist, the choice is still lost
    state.selections.push(borealis_engine::state::Selection {
        slot: borealis_engine::state::SlotKey {
            owner: "ID_ZZ_RACE_GONE".into(),
            rule: 0,
            ordinal: 1,
            instance: None,
        },
        choice: Choice::Element("ID_ZZ_SUB_RACE_GONE".into()),
        replace_level: None,
    });
    let snap = engine.derive(&state);
    let slot = snap.slots.iter().find(|s| s.key == race_key).expect("slot");
    assert!(slot.rejected.is_none() && slot.chosen.is_none());
    assert_eq!(slot.missing.as_deref(), Some("ID_ZZ_RACE_GONE"));
    assert!(slot.options.iter().any(|o| o.id == "ID_RACE_HUMAN"));
    for id in ["ID_ZZ_RACE_GONE", "ID_ZZ_SUB_RACE_GONE"] {
        assert!(
            snap.diagnostics
                .iter()
                .any(|d| d.message == format!("chosen element {id} is not installed")),
            "{id}"
        );
    }
    // a built character's re-check lists both too
    let mut ch =
        borealis_character::Character::with_ruleset("x", borealis_content::Ruleset::Dnd2014);
    ch.build = state;
    let out = borealis_character::recheck(engine, &mut ch);
    let ids: Vec<&str> = out.still_missing.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["ID_ZZ_RACE_GONE", "ID_ZZ_SUB_RACE_GONE"]);
}
