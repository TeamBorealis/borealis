//! Items: weapons, armor, gear, magic items, item groups and magic variants.

use super::build::{grant, setter_with, stat, stat_bonus};
use super::mech::Mech;
use super::{append_fluff, drain_renderer, new_element, note_unmapped, Conv, Ent, Shared, State};
use crate::core_ids;
use crate::ids::IdKind;
use crate::model::{self, Item, ItemGroup};
use crate::srd::SrdKind;
use crate::text::{RefKind, Renderer};
use crate::util::{id_fragment, title_case};
use borealis_content::{Element, ExtractItem, Rule, Setter};
use serde_json::{json, Map, Value};

#[path = "item_mech.rs"]
mod item_mech;
#[path = "item_spells.rs"]
mod item_spells;

fn strip_source(code: &str) -> &str {
    code.split('|').next().unwrap_or(code)
}

fn type_code(it: &Item) -> String {
    it.item_type
        .as_deref()
        .map(strip_source)
        .unwrap_or("")
        .to_string()
}

fn flag(it: &Item, key: &str) -> bool {
    it.extra.get(key) == Some(&Value::Bool(true))
}

fn category_for(code: &str) -> &'static str {
    match code {
        "A" | "AF" => "Ammunition",
        "AT" => "Artisan's Tools",
        "EXP" => "Explosives",
        "FD" => "Food and Drink",
        "G" => "Adventuring Gear",
        "GS" => "Gaming Sets",
        "INS" => "Musical Instruments",
        "M" | "R" => "Weapons",
        "HA" | "MA" | "LA" | "S" => "Armor",
        "MNT" => "Mounts",
        "P" => "Potions",
        "RD" => "Rods",
        "RG" => "Rings",
        "SC" => "Scrolls",
        "SCF" => "Spellcasting Focuses",
        "ST" => "Staffs",
        "T" => "Tools",
        "TAH" => "Tack and Harness",
        "TG" => "Trade Goods",
        "VEH" | "SHP" | "AIR" | "SPC" => "Vehicles",
        "WD" => "Wands",
        "$C" | "$G" | "$A" => "Treasure",
        _ => "Other",
    }
}

fn magic_type_header(code: &str) -> String {
    match code {
        "P" => "Potion",
        "RG" => "Ring",
        "RD" => "Rod",
        "SC" => "Scroll",
        "ST" => "Staff",
        "WD" => "Wand",
        "M" | "R" => "Weapon",
        "HA" | "MA" | "LA" | "S" => "Armor",
        _ => "Wondrous item",
    }
    .to_string()
}

fn damage_type_name(code: &str) -> Option<&'static str> {
    Some(match code {
        "B" => "bludgeoning",
        "P" => "piercing",
        "S" => "slashing",
        "A" => "acid",
        "C" => "cold",
        "F" => "fire",
        "O" => "force",
        "L" => "lightning",
        "N" => "necrotic",
        "I" => "poison",
        "Y" => "psychic",
        "R" => "radiant",
        "T" => "thunder",
        _ => return None,
    })
}

fn property_id(code: &str) -> Option<&'static str> {
    Some(match code {
        "A" => "ID_INTERNAL_WEAPON_PROPERTY_AMMUNITION",
        "F" => "ID_INTERNAL_WEAPON_PROPERTY_FINESSE",
        "H" => "ID_INTERNAL_WEAPON_PROPERTY_HEAVY",
        "L" => "ID_INTERNAL_WEAPON_PROPERTY_LIGHT",
        "LD" => "ID_INTERNAL_WEAPON_PROPERTY_LOADING",
        "R" => "ID_INTERNAL_WEAPON_PROPERTY_REACH",
        "T" => "ID_INTERNAL_WEAPON_PROPERTY_THROWN",
        "2H" => "ID_INTERNAL_WEAPON_PROPERTY_TWOHANDED",
        "V" => "ID_INTERNAL_WEAPON_PROPERTY_VERSATILE",
        "S" => "ID_INTERNAL_WEAPON_PROPERTY_SPECIAL",
        "RLD" => "ID_INTERNAL_WEAPON_PROPERTY_FIREARM_RELOAD",
        "BF" => "ID_INTERNAL_WEAPON_PROPERTY_FIREARM_BURST_FIRE",
        "AF" => "ID_INTERNAL_WEAPON_PROPERTY_FIREARM_AMMUNITION",
        _ => return None,
    })
}

const GROUP_FLAGS: &[(&str, &str)] = &[
    ("sword", "ID_INTERNAL_WEAPON_GROUP_SWORDS"),
    ("axe", "ID_INTERNAL_WEAPON_GROUP_AXES"),
    ("mace", "ID_INTERNAL_WEAPON_GROUP_MACES"),
    ("hammer", "ID_INTERNAL_WEAPON_GROUP_HAMMERS"),
    ("spear", "ID_INTERNAL_WEAPON_GROUP_SPEARS"),
    ("bow", "ID_INTERNAL_WEAPON_GROUP_BOWS"),
    ("crossbow", "ID_INTERNAL_WEAPON_GROUP_CROSSBOWS"),
    ("club", "ID_INTERNAL_WEAPON_GROUP_CLUBS"),
];

fn number_text(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// `cost` setter for a value in copper pieces.
#[must_use]
pub fn cost_setter(value: f64) -> Option<Setter> {
    if value <= 0.0 {
        return None;
    }
    if value >= 100.0 && (value % 100.0).abs() < f64::EPSILON {
        Some(setter_with(
            "cost",
            &number_text(value / 100.0),
            &[("currency", "gp")],
        ))
    } else if value >= 10.0 && (value % 10.0).abs() < f64::EPSILON {
        Some(setter_with(
            "cost",
            &number_text(value / 10.0),
            &[("currency", "sp")],
        ))
    } else {
        Some(setter_with(
            "cost",
            &number_text(value),
            &[("currency", "cp")],
        ))
    }
}

fn armor_proficiency_id(name: &str) -> Option<String> {
    let lower = name.to_lowercase();
    let base = lower.trim_end_matches(" armor").trim();
    let frag = id_fragment(base);
    let known = [
        "PADDED",
        "LEATHER",
        "STUDDED_LEATHER",
        "HIDE",
        "CHAIN_SHIRT",
        "SCALE_MAIL",
        "BREASTPLATE",
        "HALF_PLATE",
        "RING_MAIL",
        "CHAIN_MAIL",
        "SPLINT",
        "PLATE",
        "SHIELD",
    ];
    known
        .contains(&frag.as_str())
        .then(|| format!("ID_PROFICIENCY_ARMOR_PROFICIENCY_{frag}"))
}

fn armor_group(code: &str) -> Option<(&'static str, &'static str)> {
    Some(match code {
        "LA" => ("Light", "ID_INTERNAL_ARMOR_GROUP_LIGHT"),
        "MA" => ("Medium", "ID_INTERNAL_ARMOR_GROUP_MEDIUM"),
        "HA" => ("Heavy", "ID_INTERNAL_ARMOR_GROUP_HEAVY"),
        "S" => ("Shield", "ID_INTERNAL_ARMOR_GROUP_SHIELD"),
        _ => return None,
    })
}

fn signed_value(text: &str) -> Option<i64> {
    text.trim().trim_start_matches('+').parse().ok()
}

fn is_magic(it: &Item) -> bool {
    let rarity = it.rarity.as_deref().unwrap_or("none");
    !matches!(rarity, "none" | "")
        || it.bonus_weapon.is_some()
        || it.bonus_ac.is_some()
        || flag(it, "wondrous")
}

fn element_type(it: &Item, code: &str) -> &'static str {
    let weapon = matches!(code, "M" | "R") || flag(it, "weapon");
    let armor = matches!(code, "LA" | "MA" | "HA" | "S") || flag(it, "armor");
    if weapon && it.dmg1.is_some() {
        "Weapon"
    } else if armor && it.ac.is_some() {
        "Armor"
    } else if is_magic(it) {
        "Magic Item"
    } else {
        "Item"
    }
}

/// Source 5etools assumes for an item reference that names none.
const DEFAULT_ITEM_SOURCE: &str = "DMG";

/// The imported element of a weapon's `baseItem` (`longsword|phb`; base items default to the
/// PHB), when the import has it: a `+1 Longsword` counts as a Longsword for Weapon Mastery.
fn base_weapon_id(sh: &Shared, it: &Item) -> Option<String> {
    let uid = it.extra.get("baseItem")?.as_str()?;
    let (name, source) = uid.split_once('|').unwrap_or((uid, ""));
    let source = if source.trim().is_empty() {
        "PHB"
    } else {
        source.trim()
    };
    sh.link(RefKind::Item, name.trim(), &source.to_uppercase())
}

fn extract_items(sh: &Shared, it: &Item) -> (Vec<ExtractItem>, Vec<String>) {
    let mut out = Vec::new();
    let mut partial = Vec::new();
    for c in &it.pack_contents {
        let (uid, qty) = match c {
            Value::String(s) => (s.clone(), 1u32),
            Value::Object(o) => match o.get("item").and_then(Value::as_str) {
                Some(i) => (
                    i.to_string(),
                    o.get("quantity")
                        .and_then(Value::as_u64)
                        .and_then(|q| u32::try_from(q).ok())
                        .unwrap_or(1),
                ),
                None => {
                    if let Some(special) = o.get("special").and_then(Value::as_str) {
                        partial.push(format!(
                            "pack content {special:?} is not an item and was not converted"
                        ));
                    }
                    continue;
                }
            },
            _ => continue,
        };
        let (name, source) = uid.split_once('|').unwrap_or((uid.as_str(), ""));
        // An item uid without a source means the DMG (5etools' default for items); fall back
        // to the containing item's own source.
        let id = if source.is_empty() {
            sh.link(RefKind::Item, name, DEFAULT_ITEM_SOURCE)
                .or_else(|| sh.link(RefKind::Item, name, &it.source))
                .unwrap_or_else(|| {
                    format!(
                        "ID_5ET_{}_ITEM_{}",
                        id_fragment(DEFAULT_ITEM_SOURCE),
                        id_fragment(name)
                    )
                })
        } else {
            sh.link(RefKind::Item, name, source).unwrap_or_else(|| {
                format!("ID_5ET_{}_ITEM_{}", id_fragment(source), id_fragment(name))
            })
        };
        out.push(ExtractItem {
            id,
            amount: (qty > 1).then_some(qty),
            ..ExtractItem::default()
        });
    }
    (out, partial)
}

/// Convert one item or base item.
pub fn convert(sh: &Shared, st: &mut State, ent: &Ent) -> Conv {
    let it: Item = match model::parse(&ent.raw) {
        Ok(i) => i,
        Err(e) => {
            st.report.skipped(
                &ent.file,
                "item",
                &ent.label(),
                &format!("invalid shape: {e}"),
            );
            return Conv::default();
        }
    };
    let resolver = |k: RefKind, n: &str, s: &str| sh.link(k, n, s);
    let mut renderer = Renderer::new(&resolver);
    let code = type_code(&it);
    let ty = element_type(&it, &code);
    let id = st.ids.named(IdKind::Item, &ent.source, &it.name);
    let mut el = new_element(
        &id,
        &it.name,
        ty,
        &sh.source_display(&ent.source),
        ent.ruleset,
    );
    let mut mech = Mech::new(sh, &it.name);
    let mut partial: Vec<String> = Vec::new();
    let base_name = it
        .extra
        .get("baseItem")
        .and_then(Value::as_str)
        .map(|b| strip_source(b).to_string())
        .unwrap_or_else(|| it.name.clone());

    // Common setters.
    let category = if it.pack_contents.is_empty() {
        category_for(&code)
    } else {
        "Equipment Packs"
    };
    let category = if category == "Other" && flag(&it, "wondrous") {
        "Wondrous Items"
    } else {
        category
    };
    el.setters.push(Setter::new("category", category));
    if let Some(v) = it.value {
        el.setters.extend(cost_setter(v));
    }
    if let Some(w) = it.weight {
        if w > 0.0 {
            el.setters.push(setter_with(
                "weight",
                &format!("{} lb.", number_text(w)),
                &[("lb", &number_text(w))],
            ));
        }
    }
    if flag(&it, "ammo") || code == "A" || code == "AF" {
        el.setters.push(Setter::new("stackable", "true"));
    }

    match ty {
        "Weapon" => weapon(&it, &code, &base_name, &mut el),
        "Armor" => armor(&it, &code, &base_name, &mut el, &mut mech),
        _ => {}
    }

    // Magic properties.
    if !matches!(it.rarity.as_deref(), None | Some("none") | Some("")) {
        el.setters.push(Setter::new(
            "rarity",
            title_case(it.rarity.as_deref().unwrap_or("")),
        ));
    }
    if ty == "Magic Item" {
        el.setters
            .push(Setter::new("type", magic_type_header(&code)));
    }
    match &it.req_attune {
        Some(Value::Bool(true)) => el.setters.push(Setter::new("attunement", "true")),
        Some(Value::String(s)) => {
            el.setters.push(setter_with(
                "attunement",
                "true",
                &[("addition", &crate::text::strip_tags(s))],
            ));
        }
        _ => {}
    }
    if let Some(bonus) = it.bonus_weapon.as_deref().and_then(signed_value) {
        el.setters
            .push(Setter::new("enhancement", bonus.to_string()));
    }
    if ty == "Weapon" {
        item_mech::weapon_bonus_rules(&it, &mut mech);
        item_mech::mastery_data(&mut st.ids, &it, &code, &mut el);
        if let Some(base) = base_weapon_id(sh, &it) {
            el.data.insert("base_weapon".into(), json!(base));
        }
    }
    magic_rules(&it, ty, &code, &mut mech, &mut el);
    item_mech::ability_bonus_rules(&it, &mut mech, &mut partial);
    item_mech::speed_rules(&it, &mut mech, &mut partial);
    let charges = item_mech::charges_rule(&it, &el, &mut partial);
    let has_charges = charges.is_some();
    mech.rules.extend(charges);
    if let Some(attached) = it.extra.get("attachedSpells") {
        mech.rules.extend(item_spells::attached_spell_rules(
            sh,
            &el.id,
            has_charges,
            attached,
            &mut partial,
        ));
    }

    // Equipment packs.
    if !it.pack_contents.is_empty() {
        let (items, p) = extract_items(sh, &it);
        partial.extend(p);
        el.extract = Some(borealis_content::Extract {
            items,
            ..borealis_content::Extract::default()
        });
    }

    // Unconverted details are kept as data.
    for key in [
        "charges",
        "recharge",
        "rechargeAmount",
        "attachedSpells",
        "bonusAbilityCheck",
        "bonusSpellAttack",
        "bonusSpellSaveDc",
        "containerCapacity",
        "tier",
        "lootTables",
    ] {
        if let Some(v) = it.extra.get(key) {
            el.data.insert(key.to_string(), v.clone());
        }
    }
    if !it.mastery.is_empty() {
        el.data
            .insert("mastery".into(), Value::Array(it.mastery.clone()));
    }
    el.rules.extend(std::mem::take(&mut mech.rules));
    for (k, vs) in &mech.unmapped {
        note_unmapped(&mut el, k, &vs.join("; "));
    }
    partial.extend(std::mem::take(&mut mech.partial));

    let mut description = renderer.entries(&it.entries);
    append_fluff(sh, "item", ent, &mut description, &mut renderer);
    el.description = description;
    el.maps_to = sh.maps_to(
        SrdKind::Item,
        ent.ruleset,
        &it.name,
        &it.reprinted_as,
        &|uid| sh.reprint(RefKind::Item, uid),
    );
    drain_renderer(st, &renderer, &ent.file, &it.name);
    partial.sort();
    partial.dedup();
    Conv {
        elements: vec![el],
        partial,
    }
}

fn weapon(it: &Item, code: &str, base_name: &str, el: &mut Element) {
    let melee = code != "R";
    let category = it.weapon_category.as_deref().unwrap_or("simple");
    el.supports.push(format!(
        "ID_INTERNAL_WEAPON_CATEGORY_{}_{}",
        category.to_uppercase(),
        if melee { "MELEE" } else { "RANGED" }
    ));
    let dmg_type = it.dmg_type.as_deref().and_then(damage_type_name);
    if let Some(d) = dmg_type {
        el.supports
            .push(format!("ID_INTERNAL_DAMAGE_TYPE_{}", d.to_uppercase()));
    }
    let mut two_handed = false;
    for p in &it.property {
        let Some(raw) = p.as_str() else { continue };
        let code = strip_source(raw);
        if code == "2H" {
            two_handed = true;
        }
        if let Some(id) = property_id(code) {
            el.supports.push(id.to_string());
        }
    }
    for (k, id) in GROUP_FLAGS {
        if flag(it, k) {
            el.supports.push((*id).to_string());
        }
    }
    for name in item_mech::mastery_names(it) {
        el.supports.push(format!("Mastery {name}"));
        el.setters.push(Setter::new("mastery", name));
    }
    el.setters.push(Setter::new(
        "slot",
        if two_handed { "twohand" } else { "onehand" },
    ));
    if let Some(d) = &it.dmg1 {
        let mut s = Setter::new("damage", d.clone());
        if let Some(t) = dmg_type {
            s.attributes.insert("type".into(), t.to_string());
        }
        el.setters.push(s);
    }
    if let Some(v) = &it.dmg2 {
        el.setters.push(Setter::new("versatile", v.clone()));
    }
    if let Some(r) = &it.range {
        el.setters.push(Setter::new("range", r.clone()));
    }
    // a weapon with no proficiency of its own (a non-core base, a named weapon without a
    // base item) is covered by its category; firearms have their own proficiency
    let prof = core_ids::weapon_prof_id(base_name).or_else(|| {
        (!flag(it, "firearm"))
            .then_some(it.weapon_category.as_deref())
            .flatten()
            .and_then(core_ids::weapon_prof_id)
    });
    if let Some(id) = prof {
        el.setters.push(Setter::new("proficiency", id));
    }
}

fn armor(it: &Item, code: &str, base_name: &str, el: &mut Element, mech: &mut Mech<'_>) {
    let ac = it.ac.unwrap_or(0.0);
    let ac_text = number_text(ac);
    let group = armor_group(code);
    if let Some((_, id)) = group {
        el.supports.push(id.to_string());
    }
    if code == "S" {
        el.setters.push(Setter::new("slot", "onehand,secondary"));
        el.setters.push(Setter::new("armor", "Shield"));
        el.setters
            .push(Setter::new("armorClass", format!("+{ac_text}")));
        mech.push(stat_bonus("ac:shield", ac_text.clone(), "shield"));
    } else {
        el.setters.push(Setter::new("slot", "body"));
        if let Some((label, _)) = group {
            el.setters.push(Setter::new("armor", label));
        }
        el.setters.push(Setter::new("armorClass", ac_text.clone()));
        mech.push(Rule::Stat(stat("ac:armored:armor", ac_text)));
    }
    if let Some(strength) = &it.strength {
        let text = match strength {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        if let Ok(n) = text.parse::<i64>() {
            el.setters.push(Setter::new("strength", text));
            mech.push(Rule::Stat(borealis_content::StatRule {
                bonus: Some("armor".into()),
                requirements: Some(format!("![str:{n}]")),
                ..stat("speed", -10)
            }));
        }
    }
    if it.stealth == Some(true) {
        el.setters.push(Setter::new("stealth", "Disadvantage"));
        mech.push(grant("ID_INTERNAL_GRANTS_STEALTH_DISADVANTAGE", "Grants"));
    }
    if let Some(id) = armor_proficiency_id(base_name) {
        el.setters.push(Setter::new("proficiency", id));
    }
}

fn magic_rules(it: &Item, ty: &str, code: &str, mech: &mut Mech<'_>, el: &mut Element) {
    if let Some(bonus) = it.bonus_ac.as_deref().and_then(signed_value) {
        if ty == "Armor" && code == "S" {
            // on the shield's own AC, which counts with and without body armor
            mech.push(stat_bonus("ac:shield", bonus, "enhancement:shield"));
        } else if ty == "Armor" {
            mech.push(stat_bonus("ac:armored:enhancement", bonus, "enhancement"));
        } else {
            mech.push(Rule::Stat(stat("ac:misc", bonus)));
        }
    }
    if let Some(bonus) = it.bonus_saving_throw.as_deref().and_then(signed_value) {
        for (_, name) in core_ids::ABILITIES {
            mech.push(Rule::Stat(stat(
                &format!("{}:save:misc", name.to_lowercase()),
                bonus,
            )));
        }
    }
    let list = |key: &str| -> Vec<Value> {
        it.extra
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let (resist, immune, vulnerable) = (list("resist"), list("immune"), list("vulnerable"));
    if !resist.is_empty() || !immune.is_empty() || !vulnerable.is_empty() {
        mech.resistances(&resist, &immune, &vulnerable, &[]);
    }
    if let Some(Value::Object(stat_map)) = it.extra.get("ability").and_then(|a| a.get("static")) {
        for (abbr, v) in stat_map {
            if let (Some(name), Some(n)) = (core_ids::ability_stat(abbr), v.as_i64()) {
                mech.push(Rule::Stat(stat(&format!("{name}:score:set"), n)));
            }
        }
    }
    for key in ["conditionImmune", "senses", "speed"] {
        if it.extra.contains_key(key) {
            note_unmapped(el, key, "item property kept in data only");
            el.data.insert(key.to_string(), it.extra[key].clone());
        }
    }
}

/// Convert an `itemGroup`.
pub fn convert_group(sh: &Shared, st: &mut State, ent: &Ent) -> Conv {
    let g: ItemGroup = match model::parse(&ent.raw) {
        Ok(g) => g,
        Err(e) => {
            st.report.skipped(
                &ent.file,
                "itemGroup",
                &ent.label(),
                &format!("invalid shape: {e}"),
            );
            return Conv::default();
        }
    };
    let resolver = |k: RefKind, n: &str, s: &str| sh.link(k, n, s);
    let mut renderer = Renderer::new(&resolver);
    let id = st.ids.named(IdKind::Item, &ent.source, &g.name);
    let mut el = new_element(
        &id,
        &g.name,
        "Item",
        &sh.source_display(&ent.source),
        ent.ruleset,
    );
    el.setters.push(Setter::new("category", "Item Groups"));
    el.setters.push(Setter::new("inventory-hidden", "true"));
    let mut items = Vec::new();
    for member in &g.items {
        let uid = match member {
            Value::String(s) => s.clone(),
            other => other
                .get("item")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        };
        let (name, source) = uid
            .split_once('|')
            .map_or((uid.as_str(), DEFAULT_ITEM_SOURCE), |(n, s)| (n, s));
        let mid = sh.link(RefKind::Item, name, source).unwrap_or_else(|| {
            format!("ID_5ET_{}_ITEM_{}", id_fragment(source), id_fragment(name))
        });
        items.push(ExtractItem {
            id: mid,
            ..ExtractItem::default()
        });
    }
    el.extract = Some(borealis_content::Extract {
        items,
        ..borealis_content::Extract::default()
    });
    el.description = renderer.entries(&g.entries);
    drain_renderer(st, &renderer, &ent.file, &g.name);
    Conv {
        elements: vec![el],
        partial: Vec::new(),
    }
}

/// Convert a weapon mastery property (2024).
pub fn convert_mastery(sh: &Shared, st: &mut State, ent: &Ent) -> Conv {
    let name = ent.raw.get("name").and_then(Value::as_str).unwrap_or("");
    let entries = ent
        .raw
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let resolver = |k: RefKind, n: &str, s: &str| sh.link(k, n, s);
    let mut renderer = Renderer::new(&resolver);
    let id = st.ids.id(
        IdKind::ItemProperty,
        &ent.source,
        &["Mastery", name],
        &format!("mastery|{name}"),
    );
    let mut el = new_element(
        &id,
        name,
        "Mastery Property",
        &sh.source_display(&ent.source),
        ent.ruleset,
    );
    el.supports = vec![format!("Mastery {name}")];
    el.description = renderer.entries(&entries);
    drain_renderer(st, &renderer, &ent.file, name);
    Conv {
        elements: vec![el],
        partial: Vec::new(),
    }
}

// ---------------------------------------------------------------------------------------
// Magic variants
// ---------------------------------------------------------------------------------------

fn requirement_matches(base: &Map<String, Value>, req: &Map<String, Value>) -> bool {
    req.iter().all(|(k, want)| {
        let have = base.get(k);
        match (k.as_str(), want) {
            ("type", Value::String(w)) => have
                .and_then(Value::as_str)
                .is_some_and(|h| strip_source(h) == strip_source(w)),
            ("property", Value::String(w)) => have.and_then(Value::as_array).is_some_and(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .any(|p| strip_source(p) == strip_source(w))
            }),
            (_, Value::String(w)) => have
                .and_then(Value::as_str)
                .is_some_and(|h| h.eq_ignore_ascii_case(w)),
            (_, Value::Bool(w)) => have.and_then(Value::as_bool).unwrap_or(false) == *w,
            (_, other) => have == Some(other),
        }
    })
}

fn replace_placeholders(value: &mut Value, item: &Map<String, Value>) {
    match value {
        Value::String(s) => {
            while let Some(start) = s.find("{=") {
                let Some(len) = s[start..].find('}') else {
                    break;
                };
                let repl = placeholder_text(&s[start + 2..start + len], item);
                s.replace_range(start..=start + len, &repl);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| replace_placeholders(x, item)),
        Value::Object(m) => {
            let snapshot = item.clone();
            m.values_mut()
                .for_each(|x| replace_placeholders(x, &snapshot));
        }
        _ => {}
    }
}

/// Text of a `{=key/modifiers}` placeholder: `baseName` is the base item's name, `dmgType` the
/// damage type in words; modifier `l` lowercases, `t` title-cases, `a` prefixes an article.
fn placeholder_text(spec: &str, item: &Map<String, Value>) -> String {
    let (key, mods) = spec.split_once('/').unwrap_or((spec, ""));
    let raw = match item.get(key) {
        Some(Value::String(s)) if key == "dmgType" => damage_type_name(s).unwrap_or(s).to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => return String::new(),
    };
    let mut text = if mods.contains('l') {
        raw.to_lowercase()
    } else if mods.contains('t') {
        crate::util::title_case(&raw)
    } else {
        raw
    };
    if mods.contains('a') {
        let vowel = text
            .chars()
            .next()
            .is_some_and(|c| "aeiouAEIOU".contains(c));
        text = format!("{} {text}", if vowel { "an" } else { "a" });
    }
    text
}

/// The generic item of each magic variant ("Weapon of Warning"), as 5etools lists it next to
/// the concrete items. Item groups and links point at these.
#[must_use]
pub fn generic_variant_items(variants: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    for variant in variants {
        let (Some(name), Some(inherits)) = (
            variant.get("name").and_then(Value::as_str),
            variant.get("inherits").and_then(Value::as_object),
        ) else {
            continue;
        };
        let mut item = Map::new();
        for (k, v) in inherits {
            if !matches!(
                k.as_str(),
                "namePrefix"
                    | "nameSuffix"
                    | "nameRemove"
                    | "propertyAdd"
                    | "propertyRemove"
                    | "valueExpression"
                    | "weightExpression"
                    | "valueMult"
                    | "weightMult"
                    | "barding"
                    | "_variables"
                    | "reprintedAs"
            ) {
                item.insert(k.clone(), v.clone());
            }
        }
        item.insert("name".into(), Value::String(name.to_string()));
        for k in ["type", "edition"] {
            if let Some(v) = variant.get(k) {
                item.insert(k.into(), v.clone());
            }
        }
        if variant.get("ammo") == Some(&Value::Bool(true)) {
            item.insert("ammo".into(), Value::Bool(true));
        }
        let requires = variant
            .get("requires")
            .map(Value::to_string)
            .unwrap_or_default();
        let base = if requires.contains("\"weapon\"") || requires.contains("weaponCategory") {
            "weapon"
        } else if requires.contains("\"armor\"") || requires.contains("\"type\":\"S") {
            "armor"
        } else {
            "item"
        };
        let mut snapshot = item.clone();
        snapshot.insert("baseName".into(), Value::String(base.into()));
        if let Some(entries) = item.get_mut("entries") {
            replace_placeholders(entries, &snapshot);
        }
        out.push(Value::Object(item));
    }
    out
}

/// Items produced by applying magic variants to base items.
#[must_use]
pub fn generate_variant_items(variants: &[Value], bases: &[Value]) -> Vec<Value> {
    const FLAGS: &[&str] = &[
        "srd",
        "srd52",
        "basicRules",
        "basicRules2024",
        "freeRules2024",
    ];
    let mut out = Vec::new();
    for variant in variants {
        let Some(inherits) = variant.get("inherits").and_then(Value::as_object) else {
            continue;
        };
        let requires: Vec<&Map<String, Value>> = variant
            .get("requires")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_object).collect())
            .unwrap_or_default();
        let excludes = variant.get("excludes").and_then(Value::as_object);
        let variant_source = inherits.get("source").and_then(Value::as_str).unwrap_or("");
        let variant_rs = crate::source::ruleset_for(
            variant_source,
            variant.get("edition").and_then(Value::as_str),
        );
        for base in bases {
            let Some(bm) = base.as_object() else { continue };
            // A 2014 variant applies to 2014 base items only, and a 2024 one to 2024 ones.
            if crate::source::ruleset_of_entry(base) != variant_rs {
                continue;
            }
            if !requires.iter().any(|r| requirement_matches(bm, r)) {
                continue;
            }
            if let Some(ex) = excludes {
                if ex.iter().any(|(k, v)| {
                    let mut one = Map::new();
                    one.insert(k.clone(), v.clone());
                    requirement_matches(bm, &one)
                }) {
                    continue;
                }
            }
            let mut item = bm.clone();
            for f in FLAGS.iter().chain(&[
                "reprintedAs",
                "page",
                "otherSources",
                "referenceSources",
                "hasFluff",
                "hasFluffImages",
                "edition",
            ]) {
                item.remove(*f);
            }
            let base_name = bm.get("name").and_then(Value::as_str).unwrap_or("");
            let base_source = bm.get("source").and_then(Value::as_str).unwrap_or("");
            item.insert(
                "baseItem".into(),
                Value::String(format!(
                    "{}|{}",
                    base_name.to_lowercase(),
                    base_source.to_lowercase()
                )),
            );
            for (k, v) in inherits {
                match k.as_str() {
                    "namePrefix" | "nameSuffix" | "nameRemove" | "propertyAdd"
                    | "propertyRemove" | "valueExpression" | "weightExpression" | "valueMult"
                    | "weightMult" | "barding" | "_variables" => {}
                    _ => {
                        item.insert(k.clone(), v.clone());
                    }
                }
            }
            let prefix = inherits
                .get("namePrefix")
                .and_then(Value::as_str)
                .unwrap_or("");
            let suffix = inherits
                .get("nameSuffix")
                .and_then(Value::as_str)
                .unwrap_or("");
            let mut name = base_name.to_string();
            if let Some(rm) = inherits.get("nameRemove").and_then(Value::as_str) {
                name = name.replace(rm, "");
            }
            item.insert(
                "name".into(),
                Value::String(format!("{prefix}{name}{suffix}")),
            );
            if !item.contains_key("source") {
                item.insert("source".into(), Value::String(base_source.to_string()));
            }
            if let Some(add) = inherits.get("propertyAdd").and_then(Value::as_array) {
                let props = item.entry("property").or_insert_with(|| json!([]));
                if let Value::Array(a) = props {
                    for p in add {
                        if !a.contains(p) {
                            a.push(p.clone());
                        }
                    }
                }
            }
            let mut snapshot = item.clone();
            snapshot.insert("baseName".into(), Value::String(base_name.to_string()));
            if let Some(entries) = item.get_mut("entries") {
                replace_placeholders(entries, &snapshot);
            }
            if let Some(Value::Bool(true)) = variant.get("ammo") {
                item.insert("ammo".into(), Value::Bool(true));
            }
            out.push(Value::Object(item));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::Ruleset;

    fn ent(raw: Value) -> Ent {
        Ent {
            name: raw["name"].as_str().unwrap().to_string(),
            source: raw["source"].as_str().unwrap().to_string(),
            ruleset: crate::source::ruleset_of_entry(&raw),
            is_srd: true,
            file: "items-base.json".into(),
            raw,
        }
    }

    fn setter<'a>(e: &'a Element, n: &str) -> &'a Setter {
        e.setter(n).unwrap_or_else(|| panic!("setter {n}"))
    }

    #[test]
    fn longsword_2014_and_2024() {
        let mut sh = Shared::default();
        sh.links.clear();
        let mut st = State::default();
        let classic = json!({"name": "Longsword", "source": "PHB", "type": "M", "rarity": "none", "weight": 3, "value": 1500,
            "weaponCategory": "martial", "property": ["V"], "dmg1": "1d8", "dmgType": "S", "dmg2": "1d10", "sword": true, "weapon": true, "edition": "classic"});
        let el = &convert(&sh, &mut st, &ent(classic)).elements[0];
        assert_eq!(el.ty, "Weapon");
        assert_eq!(el.id, "ID_5ET_PHB_ITEM_LONGSWORD");
        assert_eq!(
            el.supports,
            vec![
                "ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE",
                "ID_INTERNAL_DAMAGE_TYPE_SLASHING",
                "ID_INTERNAL_WEAPON_PROPERTY_VERSATILE",
                "ID_INTERNAL_WEAPON_GROUP_SWORDS"
            ]
        );
        assert_eq!(setter(el, "category").value, "Weapons");
        assert_eq!(setter(el, "cost").value, "15");
        assert_eq!(setter(el, "cost").attribute("currency"), Some("gp"));
        assert_eq!(setter(el, "weight").value, "3 lb.");
        assert_eq!(setter(el, "weight").attribute("lb"), Some("3"));
        assert_eq!(setter(el, "slot").value, "onehand");
        assert_eq!(setter(el, "damage").value, "1d8");
        assert_eq!(setter(el, "damage").attribute("type"), Some("slashing"));
        assert_eq!(setter(el, "versatile").value, "1d10");
        assert_eq!(
            setter(el, "proficiency").value,
            "ID_PROFICIENCY_WEAPON_PROFICIENCY_LONGSWORD"
        );
        assert!(el.setter("mastery").is_none());
        let modern = json!({"name": "Longsword", "source": "XPHB", "type": "M|XPHB", "rarity": "none", "weight": 3, "value": 1500,
            "weaponCategory": "martial", "property": ["V|XPHB"], "mastery": ["Sap|XPHB"], "dmg1": "1d8", "dmgType": "S", "dmg2": "1d10", "sword": true, "weapon": true, "edition": "one"});
        let el = &convert(&sh, &mut st, &ent(modern)).elements[0];
        assert_eq!(el.ruleset, Some(Ruleset::Dnd2024));
        assert_eq!(setter(el, "mastery").value, "Sap");
        assert!(el.supports.contains(&"Mastery Sap".to_string()));
    }

    #[test]
    fn armor_shield_and_gear() {
        let sh = Shared::default();
        let mut st = State::default();
        let chain = json!({"name": "Chain Mail", "source": "PHB", "type": "HA", "rarity": "none", "weight": 55, "value": 7500, "ac": 16, "strength": "13", "armor": true, "stealth": true});
        let el = &convert(&sh, &mut st, &ent(chain)).elements[0];
        assert_eq!(el.ty, "Armor");
        assert_eq!(el.supports, vec!["ID_INTERNAL_ARMOR_GROUP_HEAVY"]);
        assert_eq!(setter(el, "armor").value, "Heavy");
        assert_eq!(setter(el, "slot").value, "body");
        assert_eq!(setter(el, "armorClass").value, "16");
        assert_eq!(setter(el, "stealth").value, "Disadvantage");
        assert_eq!(
            setter(el, "proficiency").value,
            "ID_PROFICIENCY_ARMOR_PROFICIENCY_CHAIN_MAIL"
        );
        assert!(el.rules.iter().any(
            |r| matches!(r, Rule::Stat(s) if s.name == "ac:armored:armor" && s.value == "16")
        ));
        assert!(el.rules.iter().any(|r| matches!(r, Rule::Stat(s) if s.name == "speed" && s.value == "-10" && s.requirements.as_deref() == Some("![str:13]") && s.bonus.as_deref() == Some("armor"))));
        assert!(el.rules.iter().any(
            |r| matches!(r, Rule::Grant(g) if g.id == "ID_INTERNAL_GRANTS_STEALTH_DISADVANTAGE")
        ));
        let shield = json!({"name": "Shield", "source": "PHB", "type": "S", "rarity": "none", "weight": 6, "value": 1000, "ac": 2});
        let el = &convert(&sh, &mut st, &ent(shield)).elements[0];
        assert_eq!(el.ty, "Armor");
        assert!(el.rules.iter().any(|r| matches!(r, Rule::Stat(s) if s.name == "ac:shield" && s.value == "2" && s.bonus.as_deref() == Some("shield"))));
        assert_eq!(setter(el, "slot").value, "onehand,secondary");
        let rope = json!({"name": "Rope, hempen (50 feet)", "source": "PHB", "type": "G", "rarity": "none", "weight": 10, "value": 100});
        let el = &convert(&sh, &mut st, &ent(rope)).elements[0];
        assert_eq!(el.ty, "Item");
        assert_eq!(setter(el, "category").value, "Adventuring Gear");
        assert_eq!(setter(el, "cost").value, "1");
        let arrows = json!({"name": "Arrows (20)", "source": "PHB", "type": "A", "weight": 1, "value": 100, "arrow": true, "ammo": true});
        assert_eq!(
            setter(
                &convert(&sh, &mut st, &ent(arrows)).elements[0],
                "stackable"
            )
            .value,
            "true"
        );
    }

    #[test]
    fn magic_items_and_packs() {
        let mut sh = Shared::default();
        sh.links.insert(
            (RefKind::Item, "backpack".into(), "PHB".into()),
            "ID_5ET_PHB_ITEM_BACKPACK".into(),
        );
        sh.links.insert(
            (RefKind::Item, "dagger".into(), "PHB".into()),
            "ID_5ET_PHB_ITEM_DAGGER".into(),
        );
        let mut st = State::default();
        let ring = json!({"name": "Ring of Protection", "source": "DMG", "type": "RG|DMG", "rarity": "rare", "reqAttune": true, "bonusAc": "+1", "bonusSavingThrow": "+1", "entries": ["+1 AC."]});
        let el = &convert(&sh, &mut st, &ent(ring)).elements[0];
        assert_eq!(el.ty, "Magic Item");
        assert_eq!(setter(el, "rarity").value, "Rare");
        assert_eq!(setter(el, "type").value, "Ring");
        assert_eq!(setter(el, "attunement").value, "true");
        assert_eq!(setter(el, "category").value, "Rings");
        assert!(el
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Stat(s) if s.name == "ac:misc" && s.value == "1")));
        assert_eq!(
            el.rules
                .iter()
                .filter(|r| matches!(r, Rule::Stat(s) if s.name.ends_with(":save:misc")))
                .count(),
            6
        );
        let cloak = json!({"name": "Cloak of the Wise", "source": "ZZZ", "wondrous": true, "rarity": "very rare", "reqAttune": "by a spellcaster",
            "resist": ["fire"], "ability": {"static": {"int": 19}}, "charges": 3, "entries": ["x"]});
        let el = &convert(&sh, &mut st, &ent(cloak)).elements[0];
        assert_eq!(
            setter(el, "attunement").attribute("addition"),
            Some("by a spellcaster")
        );
        assert_eq!(setter(el, "type").value, "Wondrous item");
        assert_eq!(setter(el, "category").value, "Wondrous Items");
        assert!(el.rules.iter().any(|r| matches!(r, Rule::Grant(g) if g.id == "ID_INTERNAL_CONDITION_DAMAGE_RESISTANCE_FIRE")));
        assert!(el.rules.iter().any(
            |r| matches!(r, Rule::Stat(s) if s.name == "intelligence:score:set" && s.value == "19")
        ));
        assert_eq!(el.data["charges"], 3);
        let plus = json!({"name": "+1 Dagger", "source": "ZZZ", "type": "M", "rarity": "uncommon", "bonusWeapon": "+1", "weaponCategory": "simple", "dmg1": "1d4", "dmgType": "P", "baseItem": "dagger|phb"});
        let el = &convert(&sh, &mut st, &ent(plus)).elements[0];
        assert_eq!(el.ty, "Weapon");
        assert_eq!(setter(el, "enhancement").value, "1");
        assert_eq!(
            setter(el, "proficiency").value,
            "ID_PROFICIENCY_WEAPON_PROFICIENCY_DAGGER"
        );
        assert_eq!(
            el.data["base_weapon"], "ID_5ET_PHB_ITEM_DAGGER",
            "counts as a Dagger"
        );
        let wand = json!({"name": "Wand of Glow", "source": "ZZZ", "type": "WD", "rarity": "rare", "charges": 3,
            "recharge": "dawn", "attachedSpells": {"charges": {"2": ["glow orb|zzz"]}}});
        let el = &convert(&sh, &mut st, &ent(wand)).elements[0];
        let spell = el
            .rules
            .iter()
            .find_map(|r| match r {
                Rule::Custom(c) if c.kind == "grant_spell" => Some(c),
                _ => None,
            })
            .expect("attached spell");
        assert_eq!(
            spell.fields["resource_cost"],
            json!({"resource": el.id, "cost": 2}),
            "spends the wand's charges"
        );
        let pack = json!({"name": "Test Pack", "source": "PHB", "type": "G", "value": 200, "packContents": ["backpack|phb", {"item": "candle|phb", "quantity": 5}, {"special": "10 feet of string"}]});
        let conv = convert(&sh, &mut st, &ent(pack));
        let el = &conv.elements[0];
        assert_eq!(setter(el, "category").value, "Equipment Packs");
        let items = &el.extract.as_ref().unwrap().items;
        assert_eq!(items[0].id, "ID_5ET_PHB_ITEM_BACKPACK");
        assert_eq!(
            (items[1].id.as_str(), items[1].amount),
            ("ID_5ET_PHB_ITEM_CANDLE", Some(5))
        );
        assert_eq!(conv.partial.len(), 1);
    }

    #[test]
    fn cost_formats() {
        assert_eq!(cost_setter(0.0), None);
        assert_eq!(cost_setter(5000.0).unwrap().value, "50");
        assert_eq!(cost_setter(50.0).unwrap().attribute("currency"), Some("sp"));
        assert_eq!(cost_setter(5.0).unwrap().attribute("currency"), Some("cp"));
        assert_eq!(cost_setter(25.0).unwrap().value, "25");
        assert_eq!(cost_setter(25.0).unwrap().attribute("currency"), Some("cp"));
    }

    #[test]
    fn groups_masteries_and_bad_shapes() {
        let mut sh = Shared::default();
        sh.links.insert(
            (
                RefKind::Item,
                "potion of fire resistance".into(),
                "DMG".into(),
            ),
            "ID_5ET_DMG_ITEM_POTION_OF_FIRE_RESISTANCE".into(),
        );
        let mut st = State::default();
        let g = ent(
            json!({"name": "Potion of Resistance", "source": "DMG", "items": ["potion of fire resistance|dmg", "potion of cold resistance|dmg"], "entries": ["Pick one."]}),
        );
        let conv = convert_group(&sh, &mut st, &g);
        let items = &conv.elements[0].extract.as_ref().unwrap().items;
        assert_eq!(items[0].id, "ID_5ET_DMG_ITEM_POTION_OF_FIRE_RESISTANCE");
        assert_eq!(items[1].id, "ID_5ET_DMG_ITEM_POTION_OF_COLD_RESISTANCE");
        let m = ent(
            json!({"name": "Sap", "source": "XPHB", "entries": ["The target has {@variantrule Disadvantage}."], "edition": "one"}),
        );
        let el = &convert_mastery(&sh, &mut st, &m).elements[0];
        assert_eq!(el.ty, "Mastery Property");
        assert_eq!(el.id, "ID_5ET_XPHB_ITEMPROP_MASTERY_SAP");
        assert_eq!(el.supports, vec!["Mastery Sap"]);
        let bad = ent(json!({"name": "Bad", "source": "X", "weight": "heavy"}));
        assert!(convert(&sh, &mut st, &bad).elements.is_empty());
        let bad_g = ent(json!({"name": "BadG", "source": "X", "items": "no"}));
        assert!(convert_group(&sh, &mut st, &bad_g).elements.is_empty());
    }

    #[test]
    fn variants_only_apply_within_their_edition() {
        let bases = vec![
            json!({"name": "Longsword", "source": "PHB", "edition": "classic", "type": "M", "weapon": true}),
            json!({"name": "Longsword", "source": "XPHB", "edition": "one", "type": "M|XPHB", "weapon": true}),
        ];
        let classic = json!({"name": "+1 Weapon", "edition": "classic", "requires": [{"weapon": true}],
            "inherits": {"namePrefix": "+1 ", "source": "DMG"}});
        let one = json!({"name": "+1 Weapon", "requires": [{"weapon": true}],
            "inherits": {"namePrefix": "+1 ", "source": "XDMG"}});
        let items = generate_variant_items(&[classic, one], &bases);
        let made: Vec<(String, String)> = items
            .iter()
            .map(|i| {
                (
                    i["source"].as_str().unwrap().into(),
                    i["baseItem"].as_str().unwrap().into(),
                )
            })
            .collect();
        assert_eq!(
            made,
            vec![
                ("DMG".to_string(), "longsword|phb".to_string()),
                ("XDMG".to_string(), "longsword|xphb".to_string())
            ]
        );
    }

    #[test]
    fn variant_generation() {
        let bases = vec![
            json!({"name": "Longsword", "source": "PHB", "type": "M", "weaponCategory": "martial", "dmg1": "1d8", "dmgType": "S", "srd": true, "page": 5, "weapon": true, "sword": true}),
            json!({"name": "Dagger", "source": "PHB", "type": "M", "weaponCategory": "simple", "dmg1": "1d4", "dmgType": "P", "weapon": true}),
            json!({"name": "Shortbow", "source": "PHB", "type": "R", "weaponCategory": "simple", "dmg1": "1d6", "dmgType": "P", "weapon": true}),
            json!({"name": "Plate Armor", "source": "PHB", "type": "HA", "ac": 18, "armor": true}),
        ];
        let variants = vec![json!({
            "name": "+1 Weapon", "type": "GV|DMG",
            "requires": [{"type": "M"}, {"type": "R"}],
            "excludes": {"name": "Dagger"},
            "inherits": {"namePrefix": "+1 ", "source": "DMG", "srd": true, "rarity": "uncommon", "bonusWeapon": "+1",
                          "entries": ["You have a {=bonusWeapon} bonus to attack and damage rolls made with this {=name}."]}
        })];
        let items = generate_variant_items(&variants, &bases);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["name"], "+1 Longsword");
        assert_eq!(items[0]["source"], "DMG");
        assert_eq!(items[0]["baseItem"], "longsword|phb");
        assert_eq!(items[0]["rarity"], "uncommon");
        assert_eq!(items[0]["srd"], true);
        assert!(items[0].get("page").is_none());
        assert_eq!(
            items[0]["entries"][0],
            "You have a +1 bonus to attack and damage rolls made with this +1 Longsword."
        );
        assert_eq!(items[1]["name"], "+1 Shortbow");
        // generated items convert like any other
        let sh = Shared::default();
        let mut st = State::default();
        let e = Ent {
            name: "+1 Longsword".into(),
            source: "DMG".into(),
            ruleset: Ruleset::Dnd2014,
            is_srd: true,
            file: "magicvariants.json".into(),
            raw: items[0].clone(),
        };
        let el = &convert(&sh, &mut st, &e).elements[0];
        assert_eq!(el.ty, "Weapon");
        assert_eq!(setter(el, "enhancement").value, "1");
        assert_eq!(
            setter(el, "proficiency").value,
            "ID_PROFICIENCY_WEAPON_PROFICIENCY_LONGSWORD"
        );
        // property requirements and suffixes
        let v2 = vec![
            json!({"name": "Finesse", "requires": [{"property": "F"}], "inherits": {"nameSuffix": " of Finesse", "source": "ZZZ", "propertyAdd": ["L"]}}),
        ];
        let b2 = vec![
            json!({"name": "Rapier", "source": "PHB", "type": "M", "property": ["F|PHB"]}),
            json!({"name": "Mace", "source": "PHB", "type": "M", "property": []}),
        ];
        let items = generate_variant_items(&v2, &b2);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["name"], "Rapier of Finesse");
        assert_eq!(items[0]["property"], json!(["F|PHB", "L"]));
    }

    #[test]
    fn generic_variants_and_placeholders() {
        let variants = vec![json!({"name": "Keen Weapon", "type": "GV|ZZZ",
            "requires": [{"weapon": true}],
            "inherits": {"nameSuffix": " of Keenness", "source": "ZZZ", "rarity": "rare",
                "bonusWeapon": "+2", "reprintedAs": ["X|Y"],
                "entries": ["Your {=baseName/l} gets {=bonusWeapon}; it deals {=dmgType} damage. It is {=baseName/a}."]}})];
        let generic = generic_variant_items(&variants);
        assert_eq!(generic.len(), 1);
        let g = &generic[0];
        assert_eq!(g["name"], "Keen Weapon");
        assert_eq!(g["source"], "ZZZ");
        assert_eq!(g["type"], "GV|ZZZ");
        assert!(g.get("nameSuffix").is_none() && g.get("reprintedAs").is_none());
        assert_eq!(
            g["entries"][0],
            "Your weapon gets +2; it deals  damage. It is a weapon."
        );
        let bases = vec![
            json!({"name": "Axe", "source": "PHB", "type": "M", "weapon": true, "dmgType": "S"}),
        ];
        let items = generate_variant_items(&variants, &bases);
        assert_eq!(
            items[0]["entries"][0],
            "Your axe gets +2; it deals slashing damage. It is an Axe."
        );
    }
}
