//! Mechanics carried by equipped items reach the sheet, in both editions: a cantrip an attuned
//! item teaches is listed in a spell block named after the item, a skill-check advantage and a
//! condition immunity show (whatever spelling the pack uses for the roll, skill or condition),
//! a magic weapon used on its own gives an attack line, and a magic armor's `+2 AC` written with
//! a loose "equipped" condition still counts. None of it applies while the item is not equipped.

use borealis_content::{ContentFile, ContentStore, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::Engine;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::OnceLock;

/// Edition-specific ids the synthetic items refer to.
struct Ed {
    ruleset: Ruleset,
    tag: &'static str,
    fighter: &'static str,
    light: &'static str,
    frightened: &'static str,
    longsword: &'static str,
    hide: &'static str,
}

const EDITIONS: [Ed; 2] = [
    Ed {
        ruleset: Ruleset::Dnd2014,
        tag: "dnd2014",
        fighter: "ID_WOTC_PHB_CLASS_FIGHTER",
        light: "ID_PHB_SPELL_LIGHT",
        frightened: "ID_SRD51_CONDITION_FRIGHTENED",
        longsword: "ID_WOTC_PHB_WEAPON_LONGSWORD",
        hide: "ID_WOTC_ARMOR_MEDIUM_HIDE_ARMOR",
    },
    Ed {
        ruleset: Ruleset::Dnd2024,
        tag: "dnd2024",
        fighter: "ID_2024_CLASS_FIGHTER",
        light: "ID_2024_SPELL_LIGHT",
        frightened: "ID_2024_CONDITION_FRIGHTENED",
        longsword: "ID_2024_WEAPON_LONGSWORD",
        hide: "ID_2024_ARMOR_HIDE_ARMOR",
    },
];

fn magic_item(id: String, name: &str, setters: Value, rules: Value) -> Value {
    json!({"id": id, "name": name, "type": "Magic Item", "source": "Test",
        "setters": setters, "rules": rules})
}

/// Synthetic items of one edition (ids carry the edition tag).
fn items(ed: &Ed) -> ContentFile {
    let id = |n: &str| format!("ID_T_{}_{n}", ed.tag.to_uppercase());
    let wondrous = json!([{"name": "category", "value": "Wondrous Items"},
        {"name": "type", "value": "Wondrous item"}, {"name": "attunement", "value": "true"},
        {"name": "rarity", "value": "Rare"}]);
    let elements = vec![
        magic_item(
            id("CIRCLET"),
            "Test Circlet",
            wondrous.clone(),
            json!([
                {"kind": "grant_spell", "spell": ed.light, "mode": "known_cantrip"},
                {"kind": "roll_advantage", "roll": "skill_check", "skill": "stealth",
                 "requirements": "attuned"},
                {"kind": "condition_immunity", "conditions": [ed.frightened]}
            ]),
        ),
        magic_item(
            id("TORC"),
            "Test Torc",
            wondrous,
            json!([{"kind": "grant_spell", "spell": ed.light, "mode": "known_cantrip",
                    "ability": "wisdom"}]),
        ),
        magic_item(
            id("BLADE"),
            "Test Blade",
            json!([{"name": "category", "value": "Magic Weapons"},
                {"name": "type", "value": "Weapon", "attributes": {"addition": "longsword"}},
                {"name": "rarity", "value": "Rare"}, {"name": "enhancement", "value": "1"},
                {"name": "weapon", "value": "Longsword"}]),
            json!([]),
        ),
        magic_item(
            id("SHELL"),
            "Test Shell",
            json!([{"name": "category", "value": "Magic Armor"},
                {"name": "type", "value": "Armor", "attributes": {"addition": "hide"}},
                {"name": "rarity", "value": "Rare"}]),
            json!([{"kind": "stat", "name": "ac", "value": "2", "equipped": "true"}]),
        ),
    ];
    serde_json::from_value(json!({"format": "borealis-content/1", "ruleset": ed.tag,
        "elements": elements}))
    .expect("synthetic content")
}

fn store() -> &'static ContentStore {
    static STORE: OnceLock<ContentStore> = OnceLock::new();
    STORE.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let mut b = ContentStoreBuilder::new();
        for pack in ["core", "srd-5.1", "srd-5.2.1"] {
            b = b.add_pack_dir(&root.join(pack)).expect("bundled pack");
        }
        for ed in &EDITIONS {
            b = b.add_file(
                format!("items-{}.json", ed.tag),
                PackOrigin::Custom,
                items(ed),
            );
        }
        b.build()
    })
}

fn engine() -> Engine {
    borealis_engine::invariants::enable_for_thread();
    Engine::new(store())
}

fn item_id(ed: &Ed, n: &str) -> String {
    format!("ID_T_{}_{n}", ed.tag.to_uppercase())
}

fn fighter(e: &Engine, ed: &Ed) -> CharacterState {
    let s = commands::new_character(e, ed.ruleset);
    let key = slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    let s = commands::select(e, &s, &key, Choice::Element(ed.fighter.into()))
        .expect("fighter")
        .state;
    commands::set_base_scores(e, &s, [16, 10, 14, 10, 14, 8])
        .expect("scores")
        .state
}

/// Add `item` (optionally adorned) and equip it when `wear`.
fn carry(
    e: &Engine,
    s: &CharacterState,
    item: &str,
    adorner: Option<&str>,
    wear: bool,
) -> CharacterState {
    let mut s = commands::add_item(e, s, item, 1).expect("adds").state;
    let uid = s.inventory.items.last().expect("added").uid.clone();
    if adorner.is_some() {
        s = commands::set_adorner(e, &s, &uid, adorner)
            .expect("adorns")
            .state;
    }
    if wear {
        s = commands::equip(e, &s, &uid, None).expect("equips").state;
    }
    s
}

fn block_spells(snap: &CharacterSnapshot, block: &str) -> Vec<String> {
    snap.spellcasting
        .iter()
        .filter(|b| b.name == block)
        .flat_map(|b| b.spells.iter().map(|s| s.name.clone()))
        .collect()
}

#[test]
fn an_attuned_items_cantrip_advantage_and_immunity_reach_the_sheet() {
    let e = engine();
    for ed in &EDITIONS {
        let base = fighter(&e, ed);
        let circlet = item_id(ed, "CIRCLET");
        let worn = e.derive(&carry(&e, &base, &circlet, None, true));
        assert_eq!(worn.inventory.attuned, 1, "{}: equipping attunes", ed.tag);
        assert_eq!(block_spells(&worn, "Test Circlet"), ["Light"], "{}", ed.tag);
        assert!(
            worn.advantages.iter().any(|a| a.roll == "ability_check"
                && a.text == "Advantage on Stealth checks"
                && a.source == "Test Circlet"),
            "{}: {:?}",
            ed.tag,
            worn.advantages
        );
        assert!(
            worn.defenses
                .condition_immunities
                .iter()
                .any(|c| c.name == "frightened" && c.source == "Test Circlet"),
            "{}: {:?}",
            ed.tag,
            worn.defenses.condition_immunities
        );
        let carried = e.derive(&carry(&e, &base, &circlet, None, false));
        assert!(
            carried.spellcasting.is_empty(),
            "{}: only while worn",
            ed.tag
        );
        assert!(carried
            .advantages
            .iter()
            .all(|a| a.source != "Test Circlet"));
        assert!(carried.defenses.condition_immunities.is_empty());
    }
}

#[test]
fn an_item_spell_block_uses_the_ability_its_grant_names() {
    let e = engine();
    for ed in &EDITIONS {
        let snap = e.derive(&carry(
            &e,
            &fighter(&e, ed),
            &item_id(ed, "TORC"),
            None,
            true,
        ));
        let block = snap
            .spellcasting
            .iter()
            .find(|b| b.name == "Test Torc")
            .unwrap_or_else(|| panic!("{}: no item block", ed.tag));
        assert_eq!(block.ability, "Wisdom");
        // proficiency 2 + Wisdom 14 (+2)
        assert_eq!((block.dc, block.attack), (12, 4), "{}", ed.tag);
        assert_eq!(block.slots, [0; 9]);
    }
}

#[test]
fn a_magic_weapon_used_on_its_own_attacks_as_its_base_weapon() {
    let e = engine();
    for ed in &EDITIONS {
        let base = fighter(&e, ed);
        let blade = item_id(ed, "BLADE");
        let alone = e.derive(&carry(&e, &base, &blade, None, true));
        let line = alone.attacks.iter().find(|a| a.name == "Test Blade");
        let line =
            line.unwrap_or_else(|| panic!("{}: no attack line: {:?}", ed.tag, alone.attacks));
        // proficiency 2 + Strength 16 (+3) + enhancement 1
        assert_eq!(line.attack, 6, "{}", ed.tag);
        assert!(
            line.damage.starts_with("1d8+4"),
            "{}: {}",
            ed.tag,
            line.damage
        );
        assert!(line.proficient);
        let adorned = e.derive(&carry(&e, &base, ed.longsword, Some(&blade), true));
        let line = adorned
            .attacks
            .iter()
            .find(|a| a.name == "Longsword")
            .expect("adorned line");
        assert_eq!(line.attack, 6, "{}: the adorner path is unchanged", ed.tag);
    }
}

#[test]
fn magic_armor_ac_written_with_a_loose_equipped_condition_counts() {
    let e = engine();
    for ed in &EDITIONS {
        let base = fighter(&e, ed);
        let shell = item_id(ed, "SHELL");
        let plain = e.derive(&carry(&e, &base, ed.hide, None, true)).armor_class;
        assert_eq!(plain, 12, "{}: hide armor, Dex 10", ed.tag);
        let adorned = e.derive(&carry(&e, &base, ed.hide, Some(&shell), true));
        assert_eq!(adorned.armor_class, plain + 2, "{}", ed.tag);
        let alone = e.derive(&carry(&e, &base, &shell, None, true));
        assert_eq!(
            alone.armor_class,
            plain + 2,
            "{}: used as its base armor",
            ed.tag
        );
        let carried = e.derive(&carry(&e, &base, &shell, None, false));
        assert_eq!(carried.armor_class, 10, "{}: not while carried", ed.tag);
    }
}

#[test]
fn official_magic_weapons_used_alone_attack_and_adorned_ones_are_unchanged() {
    let e = engine();
    let ed = &EDITIONS[0];
    let base = fighter(&e, ed);
    let sun = "ID_WOTC_DMG_MAGIC_ITEM_SUN_BLADE";
    let alone = e.derive(&carry(&e, &base, sun, None, true));
    let line = alone
        .attacks
        .iter()
        .find(|a| a.name == "Sun Blade")
        .expect("attack line");
    // proficiency 2 + Strength +3 + enhancement 2
    assert_eq!(line.attack, 7);
    assert_eq!(alone.inventory.attuned, 1);
    let adorned = e.derive(&carry(&e, &base, ed.longsword, Some(sun), true));
    let line = adorned
        .attacks
        .iter()
        .find(|a| a.name == "Longsword")
        .expect("line");
    assert_eq!(line.attack, 7);
    // a weapon naming a group has no single base and still needs one
    let group = "ID_WOTC_DMG_MAGIC_ITEM_FLAME_TONGUE";
    if e.has_element(ed.ruleset, group) {
        let snap = e.derive(&carry(&e, &base, group, None, true));
        assert!(snap.attacks.iter().all(|a| a.name != "Flame Tongue"));
    }
}
