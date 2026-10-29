//! Snapshot sections that slices 2 and 3 will edit (spells, inventory, details, attacks) plus
//! options, source restrictions and notices.

use crate::dto::{
    Attack, Attunement, CharacterDetails, CharacterOptions, ClassSpellcasting, Coins,
    InventoryItem, InventoryState, OptionDef, PactSlotRow, RolledCharges, SpellEntry, SpellSlotRow,
    SpellcastingState, WeaponInfo,
};
use crate::elements::plain_text;
use crate::gear_info;
use crate::ids::{ability_index, ability_key, encode_class_slot, encode_slot};
use crate::snapshot_map::MapInput;
use crate::state::ContentHub;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_character::{Character, Portrait};
use borealis_content::Ruleset;
use borealis_engine::ids as engine_ids;
use borealis_engine::snapshot::{CharacterSnapshot as Engine, SlotSnapshot, SpellcastingSnapshot};
use borealis_engine::state::EquipSlot;
use borealis_engine::ClassSlot;
use serde::{Deserialize, Serialize};

const UI_KEY: &str = "ui";
const COPPER_PER_GOLD: f64 = 100.0;
const BUILT_IN_OPTIONS: [&str; 3] = [
    engine_ids::OPT_FEATS,
    engine_ids::OPT_MULTICLASSING,
    engine_ids::OPT_AVERAGE_HP,
];

/// UI-only per-character preferences stored in the character file's `extra` map.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiExtras {
    /// House rule: lift the 8..15 point buy range.
    pub point_buy_unrestricted: bool,
    /// Exact standard-array assignment (`None` entries are unassigned abilities).
    pub array_assignment: Option<Vec<Option<i32>>>,
    /// Free-form text fields without a native home yet.
    pub alignment: String,
    /// Deity name.
    pub deity: String,
    /// Companion portrait as an image data URL.
    pub companion_portrait: Option<String>,
    /// Free-text additional treasure.
    pub treasure: String,
    /// Free-text quest items.
    pub quest_items: String,
}

/// Read the UI extras of a character.
#[must_use]
pub fn ui_extras(character: &Character) -> UiExtras {
    character
        .extra
        .get(UI_KEY)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

/// Store new UI extras on a character.
pub fn set_ui_extras(character: &mut Character, extras: &UiExtras) {
    if let Ok(v) = serde_json::to_value(extras) {
        character.extra.insert(UI_KEY.to_string(), v);
    }
}

/// The standard-array assignment: the stored one when it still matches the base scores, else
/// derived from the scores.
#[must_use]
pub fn array_assignment(character: &Character, array: [i32; 6]) -> [Option<i32>; 6] {
    let base = character.build.abilities.base;
    let low = array.iter().copied().min().unwrap_or(8);
    if let Some(stored) = ui_extras(character).array_assignment {
        let consistent = stored.len() == 6
            && stored
                .iter()
                .zip(base)
                .all(|(a, b)| a.map_or(b == low, |v| v == b));
        if consistent {
            let mut out = [None; 6];
            out.iter_mut().zip(stored).for_each(|(o, s)| *o = s);
            return out;
        }
    }
    crate::snapshot_map::standard_array_assignment(base, array)
}

fn mime_for(file_name: &str) -> &'static str {
    match file_name
        .rsplit('.')
        .next()
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => "image/png",
    }
}

/// The portrait as a `data:` URL.
#[must_use]
pub fn portrait_data_url(character: &Character) -> Option<String> {
    character
        .portrait
        .as_ref()
        .map(|p| format!("data:{};base64,{}", mime_for(&p.file_name), p.data_base64))
}

/// Parse a `data:image/...;base64,...` URL into a portrait.
///
/// # Errors
/// A message when the URL is not a base64 image data URL.
pub fn portrait_from_data_url(url: &str) -> Result<Portrait, String> {
    let rest = url
        .strip_prefix("data:image/")
        .ok_or("Not an image data URL.")?;
    let (kind, data) = rest
        .split_once(";base64,")
        .ok_or("Not a base64 data URL.")?;
    let ext = match kind {
        "png" => "png",
        "jpeg" | "jpg" => "jpg",
        "webp" => "webp",
        "gif" => "gif",
        _ => return Err("Unsupported image type (use PNG, JPEG, WebP or GIF).".into()),
    };
    // Bound the encoded length before decoding so a huge string is never allocated twice.
    if data.len() > MAX_PORTRAIT_BYTES.div_ceil(3) * 4 + 8 {
        return Err("The portrait is larger than 2 MiB.".into());
    }
    let bytes = STANDARD
        .decode(data)
        .map_err(|_| "The image data is not valid base64.".to_string())?;
    if bytes.len() > MAX_PORTRAIT_BYTES {
        return Err("The portrait is larger than 2 MiB.".into());
    }
    if !magic_matches(ext, &bytes) {
        return Err("The image data does not match its declared type.".into());
    }
    Ok(Portrait {
        file_name: format!("portrait.{ext}"),
        data_base64: data.to_string(),
    })
}

/// Largest accepted decoded portrait.
const MAX_PORTRAIT_BYTES: usize = 2 * 1024 * 1024;

fn magic_matches(ext: &str, b: &[u8]) -> bool {
    match ext {
        "png" => b.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]),
        "jpg" => b.starts_with(&[0xFF, 0xD8, 0xFF]),
        "gif" => b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a"),
        "webp" => b.len() >= 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP",
        _ => false,
    }
}

fn slot_rows(slots: &[i32; 9]) -> Vec<SpellSlotRow> {
    slots
        .iter()
        .enumerate()
        .filter(|(_, n)| **n > 0)
        .map(|(i, n)| SpellSlotRow {
            level: u32::try_from(i + 1).unwrap_or(1),
            total: *n,
        })
        .collect()
}

fn is_cantrip_slot(store: &borealis_content::ContentStore, slot: &SlotSnapshot) -> bool {
    let probe = slot
        .chosen
        .as_ref()
        .map(|c| c.id.as_str())
        .or_else(|| slot.options.first().map(|o| o.id.as_str()));
    probe
        .and_then(|id| store.element(id))
        .and_then(|e| e.setter_value("level"))
        .is_some_and(|l| l.trim() == "0")
}

/// Do the picks of this spell slot count toward its class's cantrips known (D-21)?
fn counts_toward_cantrips(store: &borealis_content::ContentStore, slot: &SlotSnapshot) -> bool {
    let rule = store
        .element(&slot.key.owner)
        .and_then(|e| e.rules.get(usize::try_from(slot.key.rule).ok()?));
    match rule {
        Some(borealis_content::Rule::Select(r)) => {
            borealis_engine::cantrip_count::counts_toward_cantrips_known(r)
        }
        _ => true,
    }
}

/// Class progression of the spellcasting entry named `name` (`main` when no class carries it).
fn caster_slot(snap: &Engine, name: &str) -> ClassSlot {
    snap.classes
        .iter()
        .find(|k| k.class.as_ref().is_some_and(|e| e.name == name))
        .map_or(ClassSlot::Main, |k| k.slot.clone())
}

/// `ClassSpellcasting.classSlotId` of the spellcasting entry a spell selection slot feeds, if
/// that entry is listed on the Magic page.
#[must_use]
pub fn slot_caster_id(snap: &Engine, slot: &SlotSnapshot) -> Option<String> {
    let name = slot.spellcasting.as_deref()?;
    snap.spellcasting
        .iter()
        .any(|c| c.name == name)
        .then(|| encode_class_slot(&caster_slot(snap, name)))
}

fn caster(input: &MapInput<'_>, snap: &Engine, c: &SpellcastingSnapshot) -> ClassSpellcasting {
    let store = &input.hub.store;
    let slot = caster_slot(snap, &c.name);
    let restricted = borealis_content::SourceRestriction::excluding(
        input.character.build.restricted_sources.iter().cloned(),
    );
    let own_slots = || {
        snap.slots
            .iter()
            .filter(|s| s.spellcasting.as_deref() == Some(c.name.as_str()) && s.rejected.is_none())
    };
    let spells: Vec<SpellEntry> = c
        .spells
        .iter()
        .filter(|s| s.level > 0 || s.known)
        .map(|s| {
            let el = store.element(&s.id);
            let flag = |name: &str| {
                el.and_then(|e| e.setter_value(name))
                    .is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
            };
            let own = own_slots().find(|k| k.chosen.as_ref().is_some_and(|ch| ch.id == s.id));
            let slot_id = own.map(|k| encode_slot(&k.key));
            let restricted_by_source = s.prepared
                && !s.always_prepared
                && el.is_some_and(|e| restricted.is_restricted_element(&e.source, &e.ty));
            SpellEntry {
                id: s.id.clone(),
                name: s.name.clone(),
                level: s.level,
                school: el
                    .and_then(|e| e.setter_value("school"))
                    .unwrap_or_default()
                    .to_string(),
                prepared: s.prepared,
                always_prepared: s.always_prepared,
                origin: "class".into(),
                can_replace: own.is_some_and(|k| k.replaceable),
                restricted_by_source,
                slot_id,
                concentration: flag("isConcentration"),
                ritual: flag("isRitual"),
                casting: el.and_then(crate::gear_facts::spell_facts),
                source_name: el
                    .map(|e| crate::elements::element_ref(e).source)
                    .unwrap_or_default(),
            }
        })
        .collect();
    // D-21: cantrips of a select open to any class's list (Pact of the Tome) are listed but
    // not counted toward the class's cantrips.
    let uncounted: Vec<&str> = own_slots()
        .filter(|s| !counts_toward_cantrips(store, s))
        .filter_map(|s| s.chosen.as_ref().map(|c| c.id.as_str()))
        .collect();
    let cantrips_known = u32::try_from(
        spells
            .iter()
            .filter(|s| s.level == 0 && !uncounted.contains(&s.id.as_str()))
            .count(),
    )
    .unwrap_or(0);
    let cantrip_slots = u32::try_from(
        own_slots()
            .filter(|s| is_cantrip_slot(store, s) && counts_toward_cantrips(store, s))
            .count(),
    )
    .unwrap_or(0);
    // `prepare_from_list`: the whole class list can be prepared (cleric, druid); otherwise the
    // prepared spells come from a spellbook of learned spells (wizard).
    let model = match (c.prepare, c.prepare_from_list) {
        (false, _) => "known",
        (true, true) => "prepared",
        (true, false) => "spellbook",
    };
    let pact = c.pact.map(|(total, level)| PactSlotRow {
        level: u32::try_from(level).unwrap_or(0),
        total,
    });
    ClassSpellcasting {
        class_slot_id: encode_class_slot(&slot),
        class_name: c.name.clone(),
        ability: ability_key(&c.ability),
        save_dc: c.dc,
        attack_bonus: c.attack,
        model: model.into(),
        cantrips_max: cantrip_slots.max(cantrips_known),
        cantrips_known,
        prepared_max: c.prepare_limit.map_or(0, |n| u32::try_from(n).unwrap_or(0)),
        prepared_count: c.prepared_count,
        max_spell_level: c
            .slots
            .iter()
            .rposition(|n| *n > 0)
            .map_or(0, |i| u32::try_from(i + 1).unwrap_or(0))
            .max(pact.map_or(0, |p| p.level)),
        pact,
        spells,
    }
}

/// Spellcasting section.
#[must_use]
pub fn spellcasting(input: &MapInput<'_>, snap: &Engine) -> SpellcastingState {
    // Without a pooled table at most one class has ordinary slots (Pact Magic is never in
    // `slots`), so the sum is that class's table.
    let single = (!snap.spellcasting.is_empty()).then(|| {
        let mut sum = [0; 9];
        for c in &snap.spellcasting {
            for (t, n) in sum.iter_mut().zip(c.slots) {
                *t += n;
            }
        }
        slot_rows(&sum)
    });
    let multi = snap.multiclass_slots.as_ref().map(|m| slot_rows(&m.slots));
    SpellcastingState {
        classes: snap
            .spellcasting
            .iter()
            .map(|c| caster(input, snap, c))
            .collect(),
        slots: multi.clone().or(single).unwrap_or_default(),
        multiclass_slots: multi.filter(|_| snap.spellcasting.len() > 1),
    }
}

fn equip_name(e: EquipSlot, both_hands: bool) -> &'static str {
    match e {
        EquipSlot::PrimaryHand if both_hands => "two-handed",
        EquipSlot::PrimaryHand => "primary",
        EquipSlot::SecondaryHand => "secondary",
        EquipSlot::Armor | EquipSlot::Worn => "worn",
    }
}

/// Weapon facts for the attack flow, read from the weapon's setters and tags.
fn weapon_info(el: Option<&borealis_content::Element>, snap: &Engine) -> WeaponInfo {
    let Some(el) = el else {
        return WeaponInfo::default();
    };
    let damage = el.setter("damage");
    let modifier = |key: &str| {
        ability_index(key)
            .and_then(|i| snap.abilities.get(i))
            .map_or(0, |a| a.modifier)
    };
    let ability = if gear_info::is_ranged(el)
        || (gear_info::is_finesse(el) && modifier("dex") > modifier("str"))
    {
        "dex"
    } else {
        "str"
    };
    WeaponInfo {
        damage: damage
            .map(|d| d.value.trim().to_string())
            .unwrap_or_default(),
        damage_type: damage
            .and_then(|d| d.attribute("type"))
            .unwrap_or_default()
            .to_string(),
        properties: gear_info::weapon_properties(el).join(", "),
        ability: ability.into(),
        mastery: el
            .setter_value("mastery")
            .map(|m| crate::ids::title_case(m.trim()))
            .filter(|m| !m.is_empty()),
        range: el
            .setter_value("range")
            .map(|r| r.trim().to_string())
            .filter(|r| !r.is_empty()),
    }
}

/// Inventory section.
#[must_use]
pub fn inventory(input: &MapInput<'_>, snap: &Engine) -> InventoryState {
    let state = &input.character.build;
    let store = &input.hub.store;
    let names = &state.inventory.storages;
    let items = snap
        .inventory
        .items
        .iter()
        .map(|i| {
            let el = store.element(&i.item.id);
            let line = state.inventory.items.iter().find(|x| x.uid == i.uid);
            let is_weapon = i.item.ty == "Weapon";
            let both_hands = line.is_some_and(|l| l.versatile)
                || el
                    .and_then(|e| e.setter_value("slot"))
                    .is_some_and(|s| s.trim().eq_ignore_ascii_case("twohand"));
            let contents: Vec<String> = el
                .and_then(|e| e.extract.as_ref())
                .map(|x| {
                    x.items
                        .iter()
                        .map(|p| {
                            let name = store
                                .element(&p.id)
                                .map_or(p.id.clone(), |e| e.name.clone());
                            match p.amount.unwrap_or(1) {
                                1 => name,
                                n => format!("{name} x{n}"),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            InventoryItem {
                id: i.uid.clone(),
                item_id: i.item.id.clone(),
                name: i.item.name.clone(),
                display_name: i.name.clone(),
                category: el.map_or_else(|| i.item.ty.clone(), gear_info::category),
                quantity: i.amount,
                weight: i.weight,
                value_cp: cp_from_gp(i.cost_gp),
                equipped: i.equipped.map(|e| equip_name(e, both_hands).to_string()),
                attuned: i.attuned,
                requires_attunement: i.requires_attunement,
                attunement_restriction: i.attunement_restriction.clone(),
                attunement_barred: i.attunement_barred,
                storage: i
                    .storage
                    .as_ref()
                    .and_then(|s| names.iter().position(|n| n == s))
                    .map_or(0, |p| u8::try_from(p + 1).unwrap_or(0)),
                notes: line.and_then(|l| l.notes.clone()).unwrap_or_default(),
                list_in_inventory: !line.is_some_and(|l| l.hidden),
                show_card: Some(i.show_card),
                is_pack: !contents.is_empty(),
                pack_contents: contents,
                is_weapon,
                equippable: i.equippable,
                weapon: is_weapon.then(|| weapon_info(el, snap)),
                armor: el.and_then(crate::gear_facts::armor_facts),
                rolled_charges: rolled_charges(snap, &i.uid),
                override_requirements: i.override_requirements,
                withheld_grants: i.withheld_grants.iter().map(|g| g.name.clone()).collect(),
            }
        })
        .collect();
    let c = snap.inventory.coins;
    InventoryState {
        items,
        coins: Coins {
            cp: c.cp,
            sp: c.sp,
            ep: c.ep,
            gp: c.gp,
            pp: c.pp,
        },
        total_gp: snap.inventory.gold_value,
        carried_weight: snap.inventory.weight,
        capacity: snap.inventory.capacity,
        attunement: Attunement {
            used: snap.inventory.attuned,
            max: snap.inventory.attunement_limit,
        },
        storage_names: [
            names.first().cloned().unwrap_or_else(|| "Storage 1".into()),
            names.get(1).cloned().unwrap_or_else(|| "Storage 2".into()),
        ],
        warnings: input.warnings.to_vec(),
    }
}

/// The dice-valued charge maximums held by inventory line `uid`.
fn rolled_charges(snap: &Engine, uid: &str) -> Vec<RolledCharges> {
    snap.resources
        .iter()
        .filter_map(|r| {
            let roll = r.rolled.as_ref()?;
            (roll.item_uid.as_deref() == Some(uid)).then(|| RolledCharges {
                resource_id: r.id.clone(),
                name: r.name.clone(),
                dice: roll.dice.clone(),
                value: r.max,
                rolled: roll.rolled,
                min: roll.min,
                max: roll.max,
            })
        })
        .collect()
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn cp_from_gp(gp: f64) -> u64 {
    (gp * COPPER_PER_GOLD).round().max(0.0) as u64
}

/// The free-text details of a character.
#[must_use]
pub fn details(c: &Character) -> CharacterDetails {
    let d = &c.details;
    let ui = ui_extras(c);
    CharacterDetails {
        gender: c.meta.gender.clone(),
        player_name: c.meta.player.clone(),
        alignment: ui.alignment,
        deity: ui.deity,
        age: d.appearance.age.clone(),
        height: d.appearance.height.clone(),
        weight: d.appearance.weight.clone(),
        eyes: d.appearance.eyes.clone(),
        skin: d.appearance.skin.clone(),
        hair: d.appearance.hair.clone(),
        features: d.additional_features.clone(),
        backstory: d.backstory.clone(),
        personality_traits: d.personality.traits.clone(),
        ideal: d.personality.ideals.clone(),
        bond: d.personality.bonds.clone(),
        flaw: d.personality.flaws.clone(),
        trinket: d.personality.trinket.clone(),
        notes_left: d.notes.left.clone(),
        notes_right: d.notes.right.clone(),
        organization: d.organization.name.clone(),
        allies: d.organization.allies.clone(),
        combat_notes: d.attacks_text.clone(),
        sheet_ac_note: d.conditional_ac.clone(),
        treasure: ui.treasure,
        quest_items: ui.quest_items,
    }
}

/// Automatic and manual attacks.
#[must_use]
pub fn attacks(c: &Character, snap: &Engine) -> Vec<Attack> {
    let auto = snap.attacks.iter().map(|a| Attack {
        id: a.uid.clone(),
        name: a.name.clone(),
        range: a.range.clone(),
        attack_bonus: format!("{:+}", a.attack),
        damage: a.damage.clone(),
        description: String::new(),
        ability: Some(ability_key(&a.ability)).filter(|k| !k.is_empty()),
        auto: true,
        show_in_combat: true,
        card: c
            .details
            .auto_attack_cards
            .get(&a.uid)
            .copied()
            .unwrap_or_else(|| {
                snap.inventory
                    .items
                    .iter()
                    .find(|i| i.uid == a.uid)
                    .is_none_or(|i| i.show_card)
            }),
        mastery: a.mastery.as_ref().map(|m| crate::play_dto::MasteryDto {
            name: m.name.clone(),
            text: m.text.clone(),
            mastered: m.mastered,
        }),
        crit_range: a.crit_range,
        die_floor: a.die_floor,
        offhand_damage: a.offhand_damage.clone(),
        ammunition: a.ammunition.clone(),
    });
    let manual = c.details.attacks.iter().enumerate().map(|(i, a)| Attack {
        id: format!("manual-{i}"),
        name: a.name.clone(),
        range: a.range.clone(),
        attack_bonus: a.attack.clone(),
        damage: a.damage.clone(),
        description: a.description.clone(),
        ability: Some(ability_key(&a.ability)).filter(|k| !k.is_empty()),
        auto: false,
        show_in_combat: a.displayed,
        card: a.as_card.unwrap_or(false),
        mastery: None,
        crit_range: 20,
        die_floor: None,
        offhand_damage: None,
        ammunition: None,
    });
    auto.chain(manual).collect()
}

/// Content option definitions the user may toggle (the three built-ins have dedicated switches).
#[must_use]
pub fn available_options(input: &MapInput<'_>) -> Vec<OptionDef> {
    let state = &input.character.build;
    let allow_other_edition = state.allow_cross_edition.unwrap_or_else(|| {
        borealis_engine::profile::profile_for(state.ruleset).default_allow_cross_edition()
    });
    content_options(
        input.hub,
        state.ruleset,
        allow_other_edition,
        &state.options,
    )
}

/// The character options of the content for `ruleset`, by name (Aurora's New Character and
/// Manage Character Options lists, `NewCharacterSliderViewModel.LoadCharacterOptions`): every
/// `Option` element except the three built-ins, with Aurora's card text and `default` setter.
///
/// An `Option` with supports is a feature choice a select picks (the SRD 5.2.1 invocations and
/// metamagic, the 5etools importer's background tool sets), not a character option: Aurora's
/// character options are registered directly and none of the 44 in the Aurora corpus has supports.
/// Aurora has no editions; here an option of the other edition is listed when the character
/// allows the other edition (`allow_other_edition`), like any other pick. An option the character
/// holds (`held`) is always listed, so one switched on through an older list can be switched off.
#[must_use]
pub fn content_options(
    hub: &ContentHub,
    ruleset: Ruleset,
    allow_other_edition: bool,
    held: &[String],
) -> Vec<OptionDef> {
    let is_held = |e: &borealis_content::Element| held.contains(&e.id);
    let mut defs: Vec<OptionDef> = hub
        .store
        .by_type("Option")
        .map(|s| &s.element)
        .filter(|e| e.supports.is_empty() || is_held(e))
        .filter(|e| !BUILT_IN_OPTIONS.contains(&e.id.as_str()))
        .filter(|e| {
            allow_other_edition
                || is_held(e)
                || e.ruleset.is_none_or(|r| r.compatible_with(ruleset))
        })
        .filter(|e| hub.engine.has_element(ruleset, &e.id))
        .map(|e| OptionDef {
            id: e.id.clone(),
            name: e.name.clone(),
            description: option_text(e),
            locked: false,
            default_on: e
                .setter_value("default")
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("true")),
        })
        .collect();
    defs.sort_by(|a, b| a.name.cmp(&b.name));
    defs
}

/// The text of an option card, as Aurora's `CharacterOption`: the `short` setter, else the first
/// sheet description, else the plain description.
fn option_text(e: &borealis_content::Element) -> String {
    if let Some(short) = e.setter_value("short").filter(|s| !s.trim().is_empty()) {
        return plain_text(short.trim());
    }
    e.sheet
        .as_ref()
        .and_then(|s| s.descriptions.first())
        .map(|d| d.text.trim())
        .filter(|t| !t.is_empty())
        .map_or_else(|| plain_text(&e.description), plain_text)
}

/// The character options as the UI edits them.
#[must_use]
pub fn options(input: &MapInput<'_>, snap: &Engine) -> CharacterOptions {
    let state = &input.character.build;
    CharacterOptions {
        multiclassing: state.has_option(engine_ids::OPT_MULTICLASSING),
        feats: state.has_option(engine_ids::OPT_FEATS),
        average_hp: state.has_option(engine_ids::OPT_AVERAGE_HP),
        point_buy_unrestricted: ui_extras(input.character).point_buy_unrestricted,
        attunement_limit: snap.inventory.attunement_limit,
        custom: available_options(input)
            .into_iter()
            .map(|d| {
                let on = state.has_option(&d.id);
                (d.id, on)
            })
            .collect(),
    }
}

/// Enabled sources (`None` = unrestricted).
#[must_use]
pub fn source_restrictions(input: &MapInput<'_>) -> Option<Vec<String>> {
    let restricted = &input.character.build.restricted_sources;
    if restricted.is_empty() {
        return None;
    }
    Some(
        input
            .hub
            .store
            .source_names()
            .into_iter()
            .filter(|s| {
                !restricted.iter().any(|r| r == s) && !borealis_content::is_unrestrictable_source(s)
            })
            .map(str::to_string)
            .collect(),
    )
}

/// Informational notices: engine diagnostics, import remarks, content load problems.
#[must_use]
pub fn notices(input: &MapInput<'_>, snap: &Engine) -> Vec<String> {
    let mut out: Vec<String> = snap.diagnostics.iter().map(|d| d.message.clone()).collect();
    if let Some(i) = &input.character.import {
        if i.missing_content > 0 {
            out.push(format!(
                "Imported from {} {}: {} content element(s) were not found.",
                i.source, i.source_version, i.missing_content
            ));
        }
    }
    out.extend(input.hub.load_warnings.iter().cloned());
    out.dedup();
    out
}

#[cfg(test)]
mod portrait_tests {
    use super::*;

    fn url(kind: &str, bytes: &[u8]) -> String {
        format!("data:image/{kind};base64,{}", STANDARD.encode(bytes))
    }

    #[test]
    fn accepts_valid_images() {
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0];
        assert_eq!(
            portrait_from_data_url(&url("png", &png)).unwrap().file_name,
            "portrait.png"
        );
        let jpg = [0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(
            portrait_from_data_url(&url("jpeg", &jpg))
                .unwrap()
                .file_name,
            "portrait.jpg"
        );
        assert!(portrait_from_data_url(&url("gif", b"GIF89a..")).is_ok());
        assert!(portrait_from_data_url(&url("webp", b"RIFF\0\0\0\0WEBPVP8 ")).is_ok());
    }

    #[test]
    fn rejects_bad_kind_magic_and_size() {
        assert!(portrait_from_data_url(&url("svg+xml", b"<svg/>")).is_err());
        assert!(portrait_from_data_url(&url("png/../x", b"abc")).is_err());
        assert!(portrait_from_data_url(&url("png", b"not a png")).is_err());
        assert!(portrait_from_data_url(&url("jpeg", &[0x89, b'P', b'N', b'G'])).is_err());
        let mut big = vec![0xFF, 0xD8, 0xFF];
        big.resize(MAX_PORTRAIT_BYTES + 1, 0);
        assert!(portrait_from_data_url(&url("jpeg", &big)).is_err());
    }
}
