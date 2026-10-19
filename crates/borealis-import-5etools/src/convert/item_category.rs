//! The equipment-screen category of an imported item, in Aurora's names (`specs/handoffs/
//! ui-cat-1.md`): Aurora lists its 22 fixed categories first and every other name after them, so
//! a 5etools type whose meaning matches an Aurora category takes Aurora's name ("Spellcasting
//! Focuses" is Aurora's "Spellcasting Focus"; artisan's tools and gaming sets are Aurora "Tools";
//! mounts, tack and vehicles are Aurora's "Mounts & Vehicles"). A 5etools group Aurora has no
//! name for (Explosives, Food and Drink, Item Groups) keeps its own.

use super::{flag, is_magic, requirement_matches, strip_source};
use crate::model::Item;
use serde_json::{Map, Value};

/// Key a generic magic variant item carries with the category its base items share.
pub(super) const GENERIC_CATEGORY: &str = "borealisCategory";

const GEAR: &str = "Adventuring Gear";
const WONDROUS: &str = "Wondrous Items";
const PACKS: &str = "Equipment Packs";
const OTHER: &str = "Other";
/// 5etools type code of a generic magic variant.
const GENERIC_VARIANT: &str = "GV";
const AMMUNITION: &str = "Ammunition";
const FOCUS: &str = "Spellcasting Focus";
const MAGIC_WEAPONS: &str = "Magic Weapons";
const MAGIC_ARMOR: &str = "Magic Armor";
const MAGIC_ITEM: &str = "Magic Item";
const MOUNTS: &str = "Mounts & Vehicles";

/// Aurora's category of a 5etools item type code (mundane form).
pub(super) fn category_for(code: &str) -> &'static str {
    match code {
        "A" | "AF" => AMMUNITION,
        "AT" | "GS" | "T" => "Tools",
        "EXP" => "Explosives",
        "FD" => "Food and Drink",
        "G" => GEAR,
        "INS" => "Musical Instruments",
        "M" | "R" => "Weapons",
        "HA" | "MA" | "LA" | "S" => "Armor",
        "MNT" | "TAH" | "VEH" | "SHP" | "AIR" | "SPC" => MOUNTS,
        "P" => "Potions",
        "RD" => "Rods",
        "RG" => "Rings",
        "SC" => "Scrolls",
        "SCF" => FOCUS,
        "ST" => "Staffs",
        "TG" | "TB" => "Trade Goods",
        "WD" => "Wands",
        "$C" | "$G" | "$A" => "Treasure",
        _ => OTHER,
    }
}

/// Aurora's category of a magic item of a type code: a weapon or armor that is not a Weapon or
/// Armor element is a Magic Weapon / Magic Armor, as Aurora files them.
fn magic_category_for(code: &str) -> &'static str {
    match category_for(code) {
        "Weapons" => MAGIC_WEAPONS,
        "Armor" => MAGIC_ARMOR,
        c => c,
    }
}

/// A pack: a mundane gear bundle of more than one kind of thing (a Burglar's Pack). A bag of
/// one thing (Ball Bearings, Arrows (20)) is filed as that thing.
fn is_pack(it: &Item, mundane: &str) -> bool {
    let mut kinds: Vec<String> = it.pack_contents.iter().map(Value::to_string).collect();
    kinds.sort();
    kinds.dedup();
    kinds.len() > 1 && matches!(mundane, GEAR | OTHER) && !is_magic(it)
}

/// The `category` setter of an item converted to an element of type `ty`.
pub(super) fn item_category<'a>(it: &'a Item, code: &str, ty: &str) -> &'a str {
    if let Some(hint) = it.extra.get(GENERIC_CATEGORY).and_then(Value::as_str) {
        return match hint {
            MAGIC_WEAPONS if ty != MAGIC_ITEM => "Weapons",
            MAGIC_ARMOR if ty != MAGIC_ITEM => "Armor",
            h => h,
        };
    }
    if code == GENERIC_VARIANT {
        // a variant whose base items differ in kind ("+1 Weapon or Ammunition") keeps 5etools'
        return OTHER;
    }
    let mundane = category_for(code);
    if matches!(ty, "Weapon" | "Armor") {
        return mundane;
    }
    if is_pack(it, mundane) {
        return PACKS;
    }
    if flag(it, "poison") {
        return "Poison";
    }
    let magic = ty == MAGIC_ITEM;
    if magic && flag(it, "staff") && code != "SCF" {
        return "Staffs";
    }
    match (mundane, magic) {
        (OTHER, true) => WONDROUS,
        (OTHER, false) => GEAR,
        (_, true) => magic_category_for(code),
        (c, false) => c,
    }
}

/// Category of a base item as the magic form of a variant would be filed.
fn base_category(base: &Map<String, Value>) -> &'static str {
    magic_category_for(
        base.get("type")
            .and_then(Value::as_str)
            .map_or("", strip_source),
    )
}

/// A base item a weapon variant can apply to: a weapon, or anything else with a weapon category
/// but ammunition (the arcane-focus Staff is a simple weapon).
fn is_weapon_base(base: &Map<String, Value>) -> bool {
    match base_category(base) {
        MAGIC_WEAPONS => true,
        AMMUNITION => false,
        _ => base.contains_key("weaponCategory"),
    }
}

/// The category every base item a magic variant applies to shares ("+1 Armor": Magic Armor,
/// "Walloping Ammunition": Ammunition), or `None` when they differ or none applies.
pub(super) fn generic_category(variant: &Value, bases: &[Value]) -> Option<&'static str> {
    let inherits = variant.get("inherits");
    if inherits.and_then(|i| i.get("barding")) == Some(&Value::Bool(true)) {
        return Some(MOUNTS);
    }
    let requires: Vec<&Map<String, Value>> = variant
        .get("requires")?
        .as_array()?
        .iter()
        .filter_map(Value::as_object)
        .collect();
    let excludes = variant.get("excludes").and_then(Value::as_object);
    let excluded = |bm: &Map<String, Value>| {
        excludes.is_some_and(|ex| {
            ex.iter().any(|(k, v)| {
                let mut one = Map::new();
                one.insert(k.clone(), v.clone());
                requirement_matches(bm, &one)
            })
        })
    };
    let applies: Vec<&Map<String, Value>> = bases
        .iter()
        .filter_map(Value::as_object)
        .filter(|bm| requires.iter().any(|r| requirement_matches(bm, r)) && !excluded(bm))
        .collect();
    let (first, rest) = applies.split_first()?;
    let first = base_category(first);
    // a variant of every arcane focus is a focus; one of every weapon (the focus Staff too) a weapon
    let found = if rest.iter().all(|b| base_category(b) == first) {
        Some(first)
    } else if applies.iter().all(|b| is_weapon_base(b)) {
        Some(MAGIC_WEAPONS)
    } else {
        None
    };
    found.filter(|c| *c != OTHER)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model;
    use serde_json::json;

    fn cat(raw: &Value, ty: &str) -> String {
        let it: Item = model::parse(raw).unwrap();
        let code = it.item_type.as_deref().map_or("", strip_source).to_string();
        item_category(&it, &code, ty).to_string()
    }

    #[test]
    fn five_etools_types_take_auroras_names() {
        for (code, want) in [
            ("SCF|XPHB", FOCUS),
            ("AT", "Tools"),
            ("GS", "Tools"),
            ("T", "Tools"),
            ("MNT", "Mounts & Vehicles"),
            ("TAH", "Mounts & Vehicles"),
            ("VEH", "Mounts & Vehicles"),
            ("SHP", "Mounts & Vehicles"),
            ("TB|XDMG", "Trade Goods"),
            ("EXP", "Explosives"),
            ("FD", "Food and Drink"),
        ] {
            let raw = json!({"name": "X", "source": "ZZZ", "type": code, "rarity": "none"});
            assert_eq!(cat(&raw, "Item"), want, "{code}");
        }
    }

    #[test]
    fn untyped_items_fall_back_as_aurora_places_them() {
        let gear = json!({"name": "Grenade", "source": "ZZZ", "type": "OTH", "rarity": "none"});
        assert_eq!(cat(&gear, "Item"), GEAR);
        let magic = json!({"name": "Circlet", "source": "ZZZ", "type": "OTH", "rarity": "rare"});
        assert_eq!(cat(&magic, "Magic Item"), WONDROUS);
        let poison = json!({"name": "Toxin", "source": "ZZZ", "poison": true, "rarity": "none"});
        assert_eq!(cat(&poison, "Item"), "Poison");
        let typed_poison = json!({"name": "Venom", "source": "ZZZ", "type": "G", "poison": true});
        assert_eq!(cat(&typed_poison, "Item"), "Poison");
    }

    #[test]
    fn magic_weapons_armor_and_staffs() {
        let staff = json!({"name": "Staff of Z", "source": "ZZZ", "type": "M", "staff": true, "rarity": "rare"});
        assert_eq!(cat(&staff, "Magic Item"), "Staffs");
        let untyped_staff =
            json!({"name": "Staff of Y", "source": "ZZZ", "staff": true, "rarity": "rare"});
        assert_eq!(cat(&untyped_staff, "Magic Item"), "Staffs");
        let hammer = json!({"name": "Hammer", "source": "ZZZ", "type": "M", "rarity": "legendary"});
        assert_eq!(cat(&hammer, "Magic Item"), MAGIC_WEAPONS);
        assert_eq!(cat(&hammer, "Weapon"), "Weapons");
        let mail = json!({"name": "Mail", "source": "ZZZ", "type": "HA", "rarity": "rare"});
        assert_eq!(cat(&mail, "Magic Item"), MAGIC_ARMOR);
        assert_eq!(cat(&mail, "Armor"), "Armor");
        let focus = json!({"name": "Hat", "source": "ZZZ", "type": "SCF", "staff": true, "wondrous": true, "rarity": "common"});
        assert_eq!(cat(&focus, "Magic Item"), FOCUS);
    }

    #[test]
    fn only_a_mundane_mixed_bundle_is_a_pack() {
        let pack = json!({"name": "Test Pack", "source": "ZZZ", "type": "G",
            "packContents": ["backpack|phb", {"item": "candle|phb", "quantity": 5}]});
        assert_eq!(cat(&pack, "Item"), PACKS);
        let bag = json!({"name": "Ball Bearings (bag of 1,000)", "source": "ZZZ", "type": "G",
            "packContents": [{"item": "ball bearing|phb", "quantity": 1000}]});
        assert_eq!(cat(&bag, "Item"), GEAR);
        let arrows = json!({"name": "+1 Arrows (20)", "source": "ZZZ", "type": "A", "rarity": "uncommon",
            "packContents": [{"item": "+1 arrow|zzz", "quantity": 20}]});
        assert_eq!(cat(&arrows, "Magic Item"), AMMUNITION);
        let deck = json!({"name": "Deck", "source": "ZZZ", "wondrous": true, "rarity": "legendary",
            "packContents": ["a|zzz", "b|zzz"]});
        assert_eq!(cat(&deck, "Magic Item"), WONDROUS);
    }

    #[test]
    fn a_generic_variant_without_a_shared_kind_stays_other() {
        let mixed =
            json!({"name": "+1 Thing", "source": "ZZZ", "type": "GV|DMG", "rarity": "rare"});
        assert_eq!(cat(&mixed, "Magic Item"), OTHER);
        let hinted = json!({"name": "+1 Armor", "source": "ZZZ", "type": "GV|DMG", "rarity": "rare",
            GENERIC_CATEGORY: MAGIC_ARMOR});
        assert_eq!(cat(&hinted, "Magic Item"), MAGIC_ARMOR);
        assert_eq!(
            cat(&hinted, "Item"),
            "Armor",
            "a mundane variant is not magic armor"
        );
    }

    #[test]
    fn a_generic_variant_takes_the_category_its_bases_share() {
        let bases = vec![
            json!({"name": "Longsword", "source": "PHB", "type": "M", "weapon": true, "sword": true}),
            json!({"name": "Longbow", "source": "PHB", "type": "R", "weapon": true}),
            json!({"name": "Plate Armor", "source": "PHB", "type": "HA", "armor": true}),
            json!({"name": "Shield", "source": "PHB", "type": "S", "armor": true}),
            json!({"name": "Arrow", "source": "PHB", "type": "A"}),
            json!({"name": "Club", "source": "PHB", "type": "M", "weaponCategory": "simple"}),
            json!({"name": "Wand", "source": "PHB", "type": "SCF"}),
            json!({"name": "Staff", "source": "PHB", "type": "SCF", "weaponCategory": "simple", "staff": true}),
        ];
        let v = |req: Value| json!({"name": "V", "type": "GV|DMG", "requires": req});
        assert_eq!(
            generic_category(&v(json!([{"weapon": true}])), &bases),
            Some(MAGIC_WEAPONS)
        );
        assert_eq!(
            generic_category(
                &v(json!([{"name": "Longsword"}, {"name": "Longbow"}])),
                &bases
            ),
            Some(MAGIC_WEAPONS)
        );
        assert_eq!(
            generic_category(&v(json!([{"armor": true}])), &bases),
            Some(MAGIC_ARMOR)
        );
        assert_eq!(
            generic_category(&v(json!([{"type": "A"}])), &bases),
            Some(AMMUNITION)
        );
        assert_eq!(
            generic_category(&v(json!([{"type": "A"}, {"type": "M"}])), &bases),
            None
        );
        let no_shield =
            json!({"name": "V", "requires": [{"armor": true}], "excludes": {"type": "HA"}});
        assert_eq!(generic_category(&no_shield, &bases), Some(MAGIC_ARMOR));
        assert_eq!(generic_category(&v(json!([{"type": "INS"}])), &bases), None);
        let focus = v(json!([{"type": "SCF"}]));
        assert_eq!(generic_category(&focus, &bases), Some(FOCUS));
        let simple = v(json!([{"weaponCategory": "simple"}]));
        assert_eq!(
            generic_category(&simple, &bases),
            Some(MAGIC_WEAPONS),
            "a focus staff is a weapon"
        );
        let barding = json!({"name": "Barding", "requires": [{"armor": true}], "inherits": {"barding": true}});
        assert_eq!(generic_category(&barding, &bases), Some(MOUNTS));
    }
}
