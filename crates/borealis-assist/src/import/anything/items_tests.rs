use super::*;

#[test]
fn a_magic_item_without_a_type_setter_gets_one_from_its_subtitle_or_category() {
    let plan: ImportPlan = serde_json::from_value(json!({
        "edition": "2024", "groups": [], "elements": [], "notes": []}))
    .unwrap();
    let type_of = |el: &Value| {
        let s = setter(el, "type").cloned().unwrap_or(Value::Null);
        (
            s.get("value").and_then(Value::as_str).map(str::to_string),
            s.pointer("/attributes/addition")
                .and_then(Value::as_str)
                .map(str::to_string),
        )
    };
    let mut wondrous = json!({"description": "<p><em>Wondrous item, rare (requires attunement)</em></p><p>A lamp.</p>",
        "setters": [{"name": "rarity", "value": "Rare"}]});
    magic_item_type(&mut wondrous, "Moth Lamp", &plan);
    assert_eq!(type_of(&wondrous), (Some("Wondrous item".into()), None));
    let mut blade = json!({"description": "<p>Weapon (longsword), very rare</p><p>It hums.</p>"});
    magic_item_type(&mut blade, "Hum Blade", &plan);
    assert_eq!(
        type_of(&blade),
        (Some("Weapon".into()), Some("longsword".into()))
    );
    let mut ring = json!({"description": "<p>A ring of reeds.</p>",
        "setters": [{"name": "category", "value": "Rings"}]});
    magic_item_type(&mut ring, "Reed Ring", &plan);
    assert_eq!(type_of(&ring), (Some("Ring".into()), None));
    // The model's own setter stays.
    let mut kept = json!({"description": "<p>Wand, rare</p>",
        "setters": [{"name": "type", "value": "Rod"}]});
    magic_item_type(&mut kept, "Odd Rod", &plan);
    assert_eq!(type_of(&kept), (Some("Rod".into()), None));
}

#[test]
fn property_names_match_bundled_tags_with_details() {
    assert_eq!(
        known_property("Versatile (1d10)"),
        Some("ID_INTERNAL_WEAPON_PROPERTY_VERSATILE")
    );
    assert_eq!(known_property("two handed"), Some(TWO_HANDED));
    assert_eq!(known_property("Sparkburst"), None);
    assert_eq!(
        known_property("Ammunition (range 80/320)"),
        Some("ID_INTERNAL_WEAPON_PROPERTY_AMMUNITION")
    );
    assert_eq!(
        known_property("Special (Net)"),
        Some("ID_INTERNAL_WEAPON_PROPERTY_SPECIAL_NET")
    );
    // Every core weapon property is known by its own name, nothing else is built in.
    for (name, id) in core_properties() {
        assert!(known_property(name).is_some(), "{name} ({id})");
    }
    assert!(core_properties().len() >= 15);
    assert_eq!(
        property_detail("Ammunition (range 80/320)").unwrap(),
        "range 80/320"
    );
}

#[test]
fn a_table_row_weapon_without_prose_keeps_its_properties_cell() {
    let mut el = json!({"description": "", "data": {"weapon_category": "Martial Ranged",
        "range": "40/120", "properties": ["Ammunition", "Recoil (11)", "Loud"]}});
    assert!(keep_properties(&mut el).is_some());
    let d = el["description"].as_str().unwrap().to_string();
    assert_eq!(
        d,
        "<p><strong>Properties:</strong> Ammunition (range 40/120), Recoil (11), Loud</p>"
    );
    assert!(keep_properties(&mut el).is_none(), "idempotent");
    assert!(super::super::fidelity::pipeline_paragraph(&d));
    let mut prose = json!({"description": "<p>A carved stock of pale ash.</p>",
        "data": {"properties": ["Loud"]}});
    assert!(keep_properties(&mut prose).is_none());
}

#[test]
fn a_table_row_weapon_keeps_its_printed_properties_cell() {
    // Arrange: the model normalised the names; the row prints details with them.
    let cell = "Ammunition (range 30/90), clatter (12), loud, reload (4 shots)";
    let data = json!({"weapon_category": "Martial Ranged", "range": "30/90",
        "properties": ["Ammunition", "Clatter", "Loud", "Reload"],
        "columns": {"Properties": cell}});
    let mut fresh = json!({"description": "", "data": data.clone()});
    // An older draft already holds the normalised line.
    let mut old = json!({"description": "<p><strong>Properties:</strong> Ammunition (range \
        30/90), Clatter, Loud, Reload</p>", "data": data});
    // Act / Assert
    for el in [&mut fresh, &mut old] {
        assert!(keep_properties(el).is_some());
        let want = format!("<p><strong>Properties:</strong> {cell}</p>");
        assert_eq!(el["description"], want.as_str());
        assert!(keep_properties(el).is_none(), "idempotent");
        keep_columns(el, &WEAPON_KEYS);
        assert_eq!(el["description"], want.as_str(), "the cell is not repeated");
    }
    let mut prose = json!({"description": "<p>A clattering brass tube.</p>",
        "data": {"properties": ["Loud"], "columns": {"Properties": "loud"}}});
    assert!(
        keep_properties(&mut prose).is_none(),
        "printed prose is kept"
    );
}

#[test]
fn unmapped_table_columns_stay_in_the_description() {
    let mut el = json!({"description": "<p>A cudgel.</p>", "data": {
        "weapon_category": "Simple Melee", "damage": {"dice": "1d4", "type": "bludgeoning"},
        "cost": {"value": 1, "currency": "sp"}, "craft_dc": 10,
        "columns": {"Material": "Oak/Iron", "Notes": ""}}});
    keep_columns(&mut el, &WEAPON_KEYS);
    let d = el["description"].as_str().unwrap().to_string();
    assert!(d.contains("<strong>Material:</strong> Oak/Iron"), "{d}");
    assert!(d.contains("<strong>Craft dc:</strong> 10"), "{d}");
    assert!(!d.contains("Notes"), "{d}");
    assert!(!d.contains("Weapon category"), "{d}");
    // Idempotent: a second pass adds nothing.
    keep_columns(&mut el, &WEAPON_KEYS);
    assert_eq!(el["description"].as_str().unwrap(), d);
}

#[test]
fn armor_gets_srd_ac_rules() {
    let mut el = json!({"data": {"armor_category": "Medium", "base_ac": 13, "dex_cap": 2,
        "cost": "40 gp", "weight": 18}});
    normalize_armor(&mut el);
    assert_eq!(setter_value(&el, "armorClass").unwrap(), "13 + Dex (max 2)");
    assert_eq!(setter_value(&el, "slot").unwrap(), "body");
    assert_eq!(setter_value(&el, "cost").unwrap(), "40");
    let rules = el["rules"].as_array().unwrap();
    assert!(rules.iter().any(|r| r["name"] == "ac:armored:armor"));
    // Dex and its cap come from the core rules by group; never a second time from the item.
    assert!(!rules.iter().any(|r| r["name"]
        .as_str()
        .unwrap_or("")
        .starts_with("ac:armored:dexterity")));
    assert_eq!(el["supports"], json!(["ID_INTERNAL_ARMOR_GROUP_MEDIUM"]));
}

fn planned_row(name: &str, line: usize) -> PlannedElement {
    serde_json::from_value(json!({"id": "ID_T_X", "ty": "Weapon", "name": name,
        "role": "standalone", "level": null, "owner": null, "brief": "",
        "excerpt": {"start_line": line, "end_line": line, "snippet": ""}}))
    .unwrap()
}

/// A crafting table that prints no properties column (1-based lines 5..).
const CRAFT_TABLE: &str = "## Gear Stats\n\n#### Simple Weapons\n\nName | Cost | DC | Materials\n\n\
Knobstick | 1sp | 10 | 1 Wood/Metal\n\nThrowing Reed (2) | 10sp | 25 | 1 Wood\n\n#### Ammunition\n\n\
Name | Cost | DC | Materials\n\nPellets (5) | 1gp | 40 | 1 Metal\n";

#[test]
fn a_table_row_item_gets_only_the_cells_its_row_prints() {
    // Arrange: the model normalised core properties for a row that prints none.
    let row = row::printed_row(CRAFT_TABLE, &planned_row("Knobstick", 7)).expect("row");
    assert_eq!(row.cells.len(), 4);
    let mut club = json!({"type": "Weapon", "name": "Knobstick", "description": "",
        "data": {"properties": ["Light"], "cost": {"value": 1, "currency": "sp"},
            "columns": {"DC": "10", "Materials": "1 Wood/Metal"}}});
    keep_properties(&mut club);
    keep_columns(&mut club, &WEAPON_KEYS);
    assert!(
        club["description"].as_str().unwrap().contains("Light"),
        "{club}"
    );
    // Act
    let notes = row::keep_printed_row(&mut club, "Knobstick", &WEAPON_KEYS, &row);
    // Assert: the printed cells, once, and nothing the row does not print.
    assert_eq!(
        club["description"],
        "<p><strong>DC:</strong> 10</p><p><strong>Materials:</strong> 1 Wood/Metal</p>"
    );
    assert!(
        notes.iter().any(|n| n.contains("Properties left out")),
        "{notes:?}"
    );
    assert!(
        row::keep_printed_row(&mut club, "Knobstick", &WEAPON_KEYS, &row).is_empty(),
        "idempotent"
    );
    // A name cell that prints more than the name ("(2)") is kept.
    let reed_row = row::printed_row(CRAFT_TABLE, &planned_row("Throwing Reed", 9)).unwrap();
    let mut reed = json!({"type": "Weapon", "name": "Throwing Reed", "description": "",
        "data": {"cost": {"value": 10, "currency": "sp"}}});
    row::keep_printed_row(&mut reed, "Throwing Reed", &WEAPON_KEYS, &reed_row);
    assert!(reed["description"]
        .as_str()
        .unwrap()
        .starts_with("<p><strong>Name:</strong> Throwing Reed (2)</p>"));
}

#[test]
fn a_model_cell_that_disagrees_with_the_printed_row_is_replaced_not_repeated() {
    // Arrange: the model wrote the row as a table (DC as printed) and a wrong DC column.
    let row = row::printed_row(CRAFT_TABLE, &planned_row("Pellets", 15)).expect("row");
    let table = "<table><tr><th>Cost</th><th>DC</th><th>Materials</th></tr><tr><td>1 gp</td>\
        <td>40</td><td>1 Metal</td></tr></table>";
    let mut el = json!({"type": "Item", "name": "Pellets", "description": table,
        "data": {"ammunition": true, "cost": {"value": 1, "currency": "gp"},
            "columns": {"DC": "20", "Materials": "1 Metal"}}});
    keep_columns(&mut el, &GEAR_KEYS);
    // Act
    let notes = row::keep_printed_row(&mut el, "Pellets", &GEAR_KEYS, &row);
    // Assert: one DC, the printed one (and the name cell, which prints the bundle size).
    assert_eq!(
        el["description"],
        format!("{table}<p><strong>Name:</strong> Pellets (5)</p>")
    );
    assert_eq!(el["data"]["columns"]["DC"], "40");
    assert!(
        notes.iter().any(|n| n.contains("(40) kept, not 20")),
        "{notes:?}"
    );
}

#[test]
fn a_weapon_row_keeps_its_printed_properties_cell_without_model_columns() {
    // Arrange: a header with a properties column; the model sent no data.columns, and one
    // row also has run-in prose before the normalised line.
    let source = "Some lead-in prose ends here.\n\nName | Cost | Damage | Weight | Properties\n\
        Martial Ranged Weapons\nBlunderpipe, short | 85 gp | 1d8 piercing | 3 lb. | Ammunition \
        (range 60/180), clatter (12), loud, reload (6 shots)\n\nBlunderpipe, long | 85 gp | 1d10 \
        piercing | 7 lb. | Ammunition (range 10/20), clatter (14), loud, reload (2 shots)\n";
    let data = json!({"cost": {"value": 85, "currency": "gp"}, "range": "60/180",
        "damage": {"dice": "1d8", "type": "piercing"}, "weight": 3,
        "properties": ["Ammunition", "Clatter", "Loud", "Reload"]});
    let mut short = json!({"type": "Weapon", "name": "Blunderpipe, short", "description": "",
        "data": data});
    keep_properties(&mut short);
    let mut long = json!({"type": "Weapon", "name": "Blunderpipe, long",
        "description": "<p>A long one roars.</p><p><strong>Properties:</strong> Ammunition \
        (range 10/20), Clatter, Loud, Reload</p>", "data": data});
    // Act
    for (el, line) in [(&mut short, 5), (&mut long, 7)] {
        let name = el["name"].as_str().unwrap().to_string();
        let p = planned_row(&name, line);
        printed_row_fixes(el, Some(&p), source);
    }
    // Assert
    assert_eq!(
        short["description"],
        "<p><strong>Properties:</strong> Ammunition (range 60/180), clatter (12), loud, reload \
         (6 shots)</p>"
    );
    assert_eq!(
        long["description"],
        "<p>A long one roars.</p><p><strong>Properties:</strong> Ammunition (range 10/20), \
         clatter (14), loud, reload (2 shots)</p>"
    );
    // Prose is no table row: the fallback stays.
    let p = planned_row("Blunderpipe, short", 1);
    assert!(row::printed_row(source, &p).is_none());
}

#[test]
fn a_magic_armors_plain_ac_bonus_becomes_its_enhancement() {
    let armor = |addition: &str, rules: Value| {
        json!({"type": "Magic Item", "rules": rules, "setters": [
            {"name": "type", "value": "Armor", "attributes": {"addition": addition}}]})
    };
    let mut hide = armor(
        "hide",
        json!([{"kind": "stat", "name": "ac", "value": "2"}]),
    );
    assert!(armor_enhancement(&mut hide).is_some());
    assert_eq!(
        hide["rules"][0],
        json!({"kind": "stat", "name": "ac:armored:enhancement", "value": "2",
            "bonus": "enhancement"})
    );
    assert!(armor_enhancement(&mut hide).is_none(), "idempotent");
    let mut shield = armor(
        "shield",
        json!([{"kind": "stat", "name": "ac", "value": "+1"}]),
    );
    armor_enhancement(&mut shield);
    assert_eq!(shield["rules"][0]["bonus"], "enhancement:shield");
    assert_eq!(shield["rules"][0]["value"], "1");
    // A conditional bonus, or one on a non-armor item, is left to the model.
    let mut cond = armor(
        "hide",
        json!([{"kind": "stat", "name": "ac", "value": "1", "equipped": "[armor:none]"}]),
    );
    assert!(armor_enhancement(&mut cond).is_none());
    let mut ring = json!({"type": "Magic Item", "rules": [{"kind": "stat", "name": "ac",
        "value": "1"}], "setters": [{"name": "type", "value": "Ring"}]});
    assert!(armor_enhancement(&mut ring).is_none());
}

/// A synthetic item with a roll table of its versions (the roll is the first cell).
const VERSION_TABLE: &str = "<table><thead><tr><th>d6</th><th>Animal</th></tr></thead><tbody>\
    <tr><td>1-2</td><td>Otter</td></tr><tr><td>3-4</td><td>Heron</td></tr>\
    <tr><td>5-6</td><td>Marten*</td></tr></tbody></table>";

fn version(name: &str, desc: &str) -> Value {
    json!({"id": "x", "name": name, "type": "Magic Item", "description": desc})
}

#[test]
fn a_version_keeps_only_its_own_row_of_the_items_roll_table() {
    // Arrange: the item prints the whole table; one version copied it whole.
    let item_desc = format!("<p>A carved charm.</p><p>Roll on the table.</p><h5>Charm animals</h5>{VERSION_TABLE}<p>Use it once.</p>");
    let raw = vec![
        version("Charm of the Wild", &item_desc),
        version("Charm of the Wild, Heron", &item_desc),
    ];
    let items = variant::item_descriptions(&raw);
    let mut el = raw[1].clone();
    // Act
    let note = variant::own_row_fix(&mut el, &items);
    // Assert: header + the Heron row, the other rows gone, the rest of the text unchanged.
    let desc = el["description"].as_str().unwrap();
    assert!(note.is_some_and(|n| n.contains("2 other row")));
    assert!(desc.contains("<th>d6</th><th>Animal</th>"));
    assert!(desc.contains("<td>3-4</td><td>Heron</td>"));
    assert!(!desc.contains("Otter") && !desc.contains("Marten"));
    assert!(desc.starts_with("<p>A carved charm.</p>") && desc.ends_with("<p>Use it once.</p>"));
    // The item itself keeps every row; a version that names no row is untouched.
    let mut item = raw[0].clone();
    assert_eq!(variant::own_row_fix(&mut item, &items), None);
    let mut other = version("Charm of the Wild, Lynx", &item_desc);
    assert_eq!(variant::own_row_fix(&mut other, &items), None);
}

#[test]
fn a_version_without_its_row_gets_it_from_the_items_table_after_the_shared_paragraph() {
    // Arrange: the version printed the item's text but no table (a footnote marker on the cell).
    let item_desc = format!("<p>A carved charm.</p><p>Roll on the table.</p><h5>Charm animals</h5>{VERSION_TABLE}<p>Use it once.</p>");
    let raw = vec![version("Charm of the Wild", &item_desc)];
    let items = variant::item_descriptions(&raw);
    let mut placed = version(
        "Charm of the Wild, Marten",
        "<p>A carved charm.</p><p>Roll on the table.</p><p>Use it once.</p>",
    );
    let mut appended = version("Charm of the Wild, Otter", "<p>A carved charm.</p>");
    // Act
    let placed_note = variant::own_row_fix(&mut placed, &items);
    let appended_note = variant::own_row_fix(&mut appended, &items);
    // Assert: caption + header + its own row, after the paragraph the item prints before them.
    assert!(placed_note.is_some_and(|n| n.contains("added")));
    assert!(appended_note.is_some());
    let placed = placed["description"].as_str().unwrap();
    assert!(placed.starts_with(
        "<p>A carved charm.</p><p>Roll on the table.</p><h5>Charm animals</h5><table>"
    ));
    assert!(placed.contains("<td>5-6</td><td>Marten*</td>") && !placed.contains("Heron"));
    assert!(placed.ends_with("</table><p>Use it once.</p>"));
    let appended = appended["description"].as_str().unwrap();
    assert!(appended.starts_with("<p>A carved charm.</p><h5>Charm animals</h5>"));
    assert!(appended.contains("Otter") && !appended.contains("Heron"));
}

#[test]
fn a_version_whose_table_already_prints_only_its_row_is_left_alone() {
    // Arrange: the version names itself in the FIRST cell of its own one-row table.
    let item_desc = format!("<p>A carved charm.</p>{VERSION_TABLE}");
    let raw = vec![version("Charm of the Wild", &item_desc)];
    let items = variant::item_descriptions(&raw);
    let own = "<p>A carved charm.</p><table><thead><tr><th>Animal</th></tr></thead><tbody>\
        <tr><td>Heron</td></tr></tbody></table>";
    let mut el = version("Charm of the Wild, Heron", own);
    // Act + Assert
    assert_eq!(variant::own_row_fix(&mut el, &items), None);
    assert_eq!(el["description"], own);
}

#[test]
fn a_magic_armors_enhancement_setter_without_an_ac_rule_becomes_its_ac_rule() {
    let item = |addition: &str, n: &str, rules: Value| {
        json!({"type": "Magic Item", "rules": rules, "setters": [
            {"name": "type", "value": "Armor", "attributes": {"addition": addition}},
            {"name": "enhancement", "value": n}]})
    };
    let mut armor = item("breastplate", "2", json!([{"kind": "resource", "id": "x"}]));
    assert!(armor_enhancement(&mut armor).is_some());
    assert_eq!(
        armor["rules"][1],
        json!({"kind": "stat", "name": "ac:armored:enhancement", "value": "2",
            "bonus": "enhancement"})
    );
    assert!(armor_enhancement(&mut armor).is_none(), "idempotent");
    let mut shield = json!({"type": "Magic Item", "setters": [
        {"name": "type", "value": "Armor"}, {"name": "armor", "value": "Shield"},
        {"name": "enhancement", "value": "+1"}]});
    armor_enhancement(&mut shield);
    assert_eq!(shield["rules"][0]["bonus"], "enhancement:shield");
    assert_eq!(shield["rules"][0]["value"], "1");
    // An AC rule the model wrote (conditional or not) is left alone; so are other items.
    let mut cond = item(
        "hide",
        "1",
        json!([{"kind": "stat", "name": "ac", "value": "1", "equipped": "[armor:none]"}]),
    );
    assert!(armor_enhancement(&mut cond).is_none());
    let mut zero = item("hide", "0", json!([]));
    assert!(armor_enhancement(&mut zero).is_none());
    let mut weapon = json!({"type": "Magic Item", "setters": [
        {"name": "type", "value": "Weapon"}, {"name": "enhancement", "value": "1"}]});
    assert!(armor_enhancement(&mut weapon).is_none());
}

/// The sheet of a level-1 fighter of `ruleset` (base scores `scores`) with `extra` content
/// loaded next to the bundled packs, wearing each of `wear` (ids).
pub(crate) fn fighter_sheet(
    ruleset: borealis_content::Ruleset,
    extra: &[Value],
    wear: &[&str],
    scores: [i32; 6],
) -> borealis_engine::snapshot::CharacterSnapshot {
    use borealis_content::{file::ContentFile, file::PackOrigin, ContentStoreBuilder};
    use borealis_engine::{commands, Choice, Engine};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut b = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        b = b.add_pack_dir(&root.join(pack)).expect("pack loads");
    }
    let doc = json!({"format": borealis_content::file::ContentFormat::ID, "elements": extra});
    let file = ContentFile::from_json_str(&doc.to_string()).expect("items load");
    let store = b.add_file("t", PackOrigin::Custom, file).build();
    let engine = Engine::new(&store);
    let fighter = if ruleset == borealis_content::Ruleset::Dnd2014 {
        "ID_WOTC_PHB_CLASS_FIGHTER"
    } else {
        "ID_2024_CLASS_FIGHTER"
    };
    let mut s = commands::new_character(&engine, ruleset);
    let key = commands::slot_key(&engine.derive(&s), "ID_LEVEL_1", "Class", 1).expect("slot");
    s = commands::select(&engine, &s, &key, Choice::Element(fighter.into()))
        .expect("fighter")
        .state;
    s = commands::set_base_scores(&engine, &s, scores)
        .expect("scores")
        .state;
    for id in wear {
        s = commands::add_item(&engine, &s, id, 1).expect("adds").state;
        let uid = s.inventory.items.last().expect("added").uid.clone();
        s = commands::equip(&engine, &s, &uid, None)
            .expect("equips")
            .state;
    }
    engine.derive(&s)
}

/// Armor class of a level-1 fighter (Dex 10) wearing each of `wear`.
fn fighter_ac(ruleset: borealis_content::Ruleset, extra: &[Value], wear: &[&str]) -> i32 {
    fighter_sheet(ruleset, extra, wear, [16, 10, 14, 10, 14, 8]).armor_class
}

#[test]
fn a_fighter_wearing_setter_only_magic_armor_and_shield_gets_their_ac() {
    use borealis_content::Ruleset;
    for (ruleset, tag, hide, shield) in [
        (
            Ruleset::Dnd2014,
            "dnd2014",
            "ID_WOTC_ARMOR_MEDIUM_HIDE_ARMOR",
            "ID_WOTC_GEAR_SHIELD",
        ),
        (
            Ruleset::Dnd2024,
            "dnd2024",
            "ID_2024_ARMOR_HIDE_ARMOR",
            "ID_2024_ARMOR_SHIELD",
        ),
    ] {
        let magic = |id: &str, name: &str, base: &str, n: &str| {
            let mut el = json!({"id": id, "name": name, "type": "Magic Item",
                "source": "Test", "ruleset": tag, "rules": [], "setters": [
                {"name": "category", "value": "Magic Armor"},
                {"name": "type", "value": "Armor", "attributes": {"addition": base}},
                {"name": "rarity", "value": "Rare"}, {"name": "armor", "value": base},
                {"name": "enhancement", "value": n}]});
            armor_enhancement(&mut el).expect("gets its AC rule");
            el
        };
        let items = [
            magic("ID_T_MAGIC_ITEM_MOSS_HIDE", "Moss Hide", "Hide", "2"),
            magic(
                "ID_T_MAGIC_ITEM_BARK_BUCKLER",
                "Bark Buckler",
                "Shield",
                "1",
            ),
        ];
        let plain = fighter_ac(ruleset, &items, &[hide, shield]);
        assert_eq!(plain, 14, "{tag}: hide 12 + shield 2");
        let magic_ac = fighter_ac(
            ruleset,
            &items,
            &["ID_T_MAGIC_ITEM_MOSS_HIDE", "ID_T_MAGIC_ITEM_BARK_BUCKLER"],
        );
        assert_eq!(magic_ac, plain + 3, "{tag}: +2 armor and +1 shield");
    }
}

#[test]
fn a_magic_weapon_printing_one_bonus_gets_its_enhancement_setter() {
    let weapon = |desc: &str| {
        json!({"type": "Magic Item", "name": "Ember Blade", "description": desc,
            "setters": [{"name": "type", "value": "Weapon"}]})
    };
    let mut one =
        weapon("<p>You have a +1 bonus to attack and damage rolls made with this weapon.</p>");
    assert!(weapon_enhancement(&mut one).is_some());
    assert_eq!(
        setter(&one, "enhancement").and_then(|s| int_of(s.get("value"))),
        Some(1)
    );
    assert!(weapon_enhancement(&mut one).is_none(), "idempotent");
    // A family printing several tiers in one item, and an item printing none, get nothing.
    let mut tiers = weapon("<p>+1 bonus to attack and damage rolls (uncommon), +2 bonus to attack and damage rolls (rare).</p>");
    assert!(weapon_enhancement(&mut tiers).is_none());
    let mut none = weapon("<p>It glows.</p>");
    assert!(weapon_enhancement(&mut none).is_none());
    // Not a weapon: nothing.
    let mut ring = json!({"type": "Magic Item", "description": "<p>+1 bonus to attack and damage rolls.</p>",
        "setters": [{"name": "type", "value": "Ring"}]});
    assert!(weapon_enhancement(&mut ring).is_none());
}
