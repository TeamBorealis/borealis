//! Common-core builder of the typed sheet: everything both editions share, built once from the
//! engine snapshot and the character details.

use crate::model_text::{features, proficiencies};
use crate::model_types::{
    AbilityData, AttackKind, AttackRow, CasterData, ClassLevel, CombatData, Edition,
    ExhaustionData, ExtrasData, GearData, GearItem, HitDice, Identity, LimitedUse, PactSlots,
    PassiveData, PlayData, PooledSlots, SheetData, SkillData, SpellFactsMap, SpellRow,
    SpellcastingData, StoryData,
};
use crate::{model_2014, model_2024};
use borealis_character::Character;
use borealis_content::Ruleset;
use borealis_engine::snapshot::{RolledMaxSnapshot, SpellEntry, SpellcastingSnapshot};
use borealis_engine::state::EquipSlot;
use borealis_engine::CharacterSnapshot;

/// Condition tick boxes (the 15 SRD conditions minus Exhaustion, which has its own track).
pub(crate) const CONDITIONS: &[&str] = &[
    "Blinded",
    "Charmed",
    "Deafened",
    "Frightened",
    "Grappled",
    "Incapacitated",
    "Invisible",
    "Paralyzed",
    "Petrified",
    "Poisoned",
    "Prone",
    "Restrained",
    "Stunned",
    "Unconscious",
];
const DEFAULT_EXHAUSTION_LEVELS: u32 = 6;

/// Edition of a snapshot.
pub(crate) fn edition_of(snap: &CharacterSnapshot) -> Edition {
    if snap.ruleset == Ruleset::Dnd2024 {
        Edition::Dnd2024
    } else {
        Edition::Dnd2014
    }
}

/// Signed number with a true minus sign (U+2212).
pub(crate) fn signed_text(n: i32) -> String {
    if n >= 0 {
        format!("+{n}")
    } else {
        format!("\u{2212}{}", -n)
    }
}

fn title_case(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            if matches!(w, "of" | "the" | "and") {
                return w.to_string();
            }
            let mut c = w.chars();
            c.next().map_or(String::new(), |f| {
                f.to_uppercase().collect::<String>() + c.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn ability_key(name: &str) -> &'static str {
    match name.to_lowercase().get(..3).unwrap_or("") {
        "str" => "str",
        "dex" => "dex",
        "con" => "con",
        "int" => "int",
        "wis" => "wis",
        _ => "cha",
    }
}

fn origin(snap: &CharacterSnapshot, key: &str) -> String {
    snap.origin
        .get(key)
        .map(|e| e.name.clone())
        .unwrap_or_default()
}

/// Creature size: the chosen size, the carry table, or the one size the species text names.
fn size_of(snap: &CharacterSnapshot) -> Option<String> {
    snap.origin
        .get("Size")
        .map(|e| e.name.clone())
        .or_else(|| {
            snap.slots
                .iter()
                .find(|s| s.ty == "Size")?
                .chosen
                .as_ref()
                .map(|e| e.name.clone())
        })
        .or_else(|| snap.carry.as_ref().map(|c| c.size.clone()))
        .or_else(|| size_from_species_text(snap))
}

/// "Size: Medium (about 4-7 feet tall)." names one size; "Medium ... or Small ..." is a choice.
fn size_from_species_text(snap: &CharacterSnapshot) -> Option<String> {
    const SIZES: [&str; 6] = ["Tiny", "Small", "Medium", "Large", "Huge", "Gargantuan"];
    let species = snap
        .origin
        .get("Species")
        .or_else(|| snap.origin.get("Race"))?;
    let f = snap.features.iter().find(|f| f.id == species.id)?;
    let plain = f.description.replace(['<', '>'], " ");
    let sentence = plain.split_once("Size:")?.1.split('.').next()?;
    let words: Vec<&str> = sentence
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| SIZES.contains(w))
        .collect();
    let first = *words.first()?;
    words.iter().all(|w| *w == first).then(|| first.to_string())
}

fn identity(character: &Character, snap: &CharacterSnapshot) -> Identity {
    let classes: Vec<ClassLevel> = snap
        .classes
        .iter()
        .filter_map(|c| {
            let class = c.class.as_ref()?;
            let sub = c
                .archetypes
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Some(ClassLevel {
                class: class.name.clone(),
                level: c.level,
                subclass: (!sub.is_empty()).then_some(sub),
                hit_die: c.hit_die,
            })
        })
        .collect();
    let name = character.meta.name.trim();
    Identity {
        name: if name.is_empty() {
            "Unnamed character".into()
        } else {
            name.into()
        },
        player: character.meta.player.trim().into(),
        level: snap.level,
        class_label: classes
            .iter()
            .map(|c| format!("{} {}", c.class, c.level))
            .collect::<Vec<_>>()
            .join(" / "),
        subclass_label: classes
            .iter()
            .filter_map(|c| c.subclass.clone())
            .collect::<Vec<_>>()
            .join(", "),
        classes,
        background: origin(snap, "Background"),
        alignment: origin(snap, "Alignment"),
        experience: snap.experience,
        next_level_xp: snap.next_level_xp,
        size: size_of(snap),
        deity: origin(snap, "Deity"),
    }
}

fn abilities(snap: &CharacterSnapshot) -> Vec<AbilityData> {
    snap.abilities
        .iter()
        .map(|a| AbilityData {
            key: ability_key(&a.name),
            name: title_case(&a.name),
            score: a.score,
            modifier: a.modifier,
            save: a.save,
            save_proficient: a.save_proficient,
        })
        .collect()
}

fn skills(snap: &CharacterSnapshot) -> (Vec<SkillData>, Vec<PassiveData>) {
    let skills: Vec<SkillData> = snap
        .skills
        .iter()
        .map(|s| SkillData {
            key: s.name.replace(' ', "_"),
            name: title_case(&s.name),
            ability: ability_key(&s.ability),
            total: s.total,
            proficient: s.proficient,
            expertise: s.expertise,
            passive: s.passive,
        })
        .collect();
    // Perception always, any other passive only when it carries a bonus.
    let passives = snap
        .skills
        .iter()
        .filter(|s| s.name == "perception" || s.misc != 0)
        .map(|s| PassiveData {
            key: s.name.replace(' ', "_"),
            label: format!("Passive {}", title_case(&s.name)),
            value: s.passive,
        })
        .collect();
    (skills, passives)
}

fn hit_dice(snap: &CharacterSnapshot) -> Vec<HitDice> {
    let mut out: Vec<HitDice> = Vec::new();
    for c in &snap.classes {
        let Some(die) = c.hit_die else { continue };
        match out.iter_mut().find(|h| h.die == die) {
            Some(h) => h.count += c.level,
            None => out.push(HitDice {
                die,
                count: c.level,
            }),
        }
    }
    out.sort_by_key(|h| std::cmp::Reverse(h.die));
    out
}

fn attacks(
    character: &Character,
    snap: &CharacterSnapshot,
    casters: &[CasterData],
) -> Vec<AttackRow> {
    let mut rows: Vec<AttackRow> = snap
        .attacks
        .iter()
        .filter(|a| !is_spell_row(&a.uid))
        .map(|a| AttackRow {
            name: a.name.clone(),
            kind: AttackKind::Weapon,
            bonus: signed_text(a.attack),
            damage: a.damage.clone(),
            range: a.range.clone(),
            mastery: a
                .mastery
                .as_ref()
                .filter(|m| m.mastered)
                .map(|m| m.name.clone()),
            notes: a
                .offhand_damage
                .as_ref()
                .map(|d| format!("Off-hand {d}"))
                .unwrap_or_default(),
        })
        .collect();
    rows.extend(
        character
            .details
            .attacks
            .iter()
            .filter(|a| a.displayed && a.item_uid.is_none())
            .map(|a| AttackRow {
                name: a.name.clone(),
                kind: AttackKind::Manual,
                bonus: a.attack.clone(),
                damage: a.damage.clone(),
                range: a.range.clone(),
                mastery: None,
                notes: String::new(),
            }),
    );
    rows.extend(
        snap.attacks
            .iter()
            .filter(|a| is_spell_row(&a.uid))
            .map(|a| {
                let caster_of = casters.iter().find(|c| {
                    c.cantrips
                        .iter()
                        .any(|s| spell_row_id(&a.uid) == Some(s.id.as_str()))
                });
                engine_cantrip_row(a, caster_of)
            }),
    );
    for c in casters {
        for sp in c.cantrips.iter().filter(|s| s.damage.is_some()) {
            if rows
                .iter()
                .any(|r| r.kind == AttackKind::Cantrip && r.name == sp.name)
            {
                continue;
            }
            rows.push(AttackRow {
                name: sp.name.clone(),
                kind: AttackKind::Cantrip,
                bonus: cantrip_hit_dc(c, sp),
                damage: sp.damage.clone().unwrap_or_default(),
                range: sp
                    .facts
                    .as_ref()
                    .and_then(|f| f.range.clone())
                    .unwrap_or_default(),
                mastery: None,
                notes: format!("{} cantrip", c.class),
            });
        }
    }
    rows
}

/// The engine lists damaging cantrips among `snap.attacks` with uid `spell:<casting name>:<id>`.
fn is_spell_row(uid: &str) -> bool {
    uid.starts_with("spell:")
}

fn spell_row_id(uid: &str) -> Option<&str> {
    uid.rsplit_once(':').map(|(_, id)| id)
}

/// A damaging cantrip the engine already derived: its Hit / DC cell, damage (with Agonizing Blast,
/// Elemental Affinity and the like) and range are the engine's; the class comes from the casting block.
fn engine_cantrip_row(
    a: &borealis_engine::snapshot::AttackSnapshot,
    caster: Option<&CasterData>,
) -> AttackRow {
    let bonus = match &a.save {
        Some(save) => format!(
            "DC {} {}",
            save.dc,
            save.ability.get(..3).unwrap_or("").to_uppercase()
        ),
        None => signed_text(a.attack),
    };
    AttackRow {
        name: a.name.clone(),
        kind: AttackKind::Cantrip,
        bonus,
        damage: a.damage.clone(),
        range: a.range.clone(),
        mastery: None,
        notes: caster
            .map(|c| format!("{} cantrip", c.class))
            .unwrap_or_default(),
    }
}

/// Hit / DC cell of a damage cantrip: the spell attack bonus for attack rolls, "DC n ABL" for saves.
fn cantrip_hit_dc(caster: &CasterData, spell: &SpellRow) -> String {
    let facts = spell.facts.as_ref();
    if facts.is_some_and(|f| f.attack_roll) {
        return signed_text(caster.attack);
    }
    match facts.and_then(|f| f.save.as_deref()) {
        Some(save) => format!("DC {} {save}", caster.dc),
        None => format!("DC {}", caster.dc),
    }
}

fn combat(character: &Character, snap: &CharacterSnapshot, casters: &[CasterData]) -> CombatData {
    let s = &snap.speed;
    let mut speeds = vec![("walk".to_string(), s.walk)];
    speeds.extend(
        [
            ("fly", s.fly),
            ("climb", s.climb),
            ("swim", s.swim),
            ("burrow", s.burrow),
        ]
        .into_iter()
        .filter(|(_, v)| *v > 0)
        .map(|(k, v)| (k.to_string(), v)),
    );
    let shield_equipped = snap
        .inventory
        .items
        .iter()
        .any(|i| i.equipped.is_some() && i.item.id.to_uppercase().contains("SHIELD"));
    CombatData {
        armor_class: snap.armor_class,
        ac_note: character.details.conditional_ac.trim().to_string(),
        shield_equipped,
        initiative: snap.initiative,
        initiative_advantage: snap.initiative_advantage,
        speeds,
        hp_max: snap.hit_points,
        hit_dice: hit_dice(snap),
        attacks: attacks(character, snap, casters),
    }
}

fn recharge_label(triggers: &[String]) -> String {
    triggers
        .iter()
        .map(|t| title_case(&t.replace('_', " ")))
        .collect::<Vec<_>>()
        .join(" or ")
}

/// A rolled maximum as the app's inventory labels it: "7 (1d6 + 3)", or "6 (1d6 + 3, average)"
/// while no roll is stored (`ui/src/pages/equipment/RolledCharges.tsx` `chargesLabel`).
pub(crate) fn rolled_label(value: i32, r: &RolledMaxSnapshot) -> String {
    let average = if r.rolled { "" } else { ", average" };
    format!("{value} ({}{average})", r.dice)
}

fn limited_uses(snap: &CharacterSnapshot) -> Vec<LimitedUse> {
    let mut out: Vec<LimitedUse> = snap
        .resources
        .iter()
        .filter(|r| r.max > 0)
        .map(|r| LimitedUse {
            name: r.name.clone(),
            max: r.max,
            recharge: recharge_label(&r.recharge),
            rolled: r.rolled.as_ref().map(|d| rolled_label(r.max, d)),
        })
        .collect();
    // The engine lists every free cast with a known count as a Play resource of the same name.
    let extra: Vec<LimitedUse> = snap
        .free_casts
        .iter()
        .filter_map(|f| {
            let name = format!("{} (free cast)", f.name);
            if out.iter().any(|u| u.name == name) {
                return None;
            }
            Some(LimitedUse {
                name,
                max: f.uses?,
                recharge: recharge_label(std::slice::from_ref(&f.recharge)),
                rolled: None,
            })
        })
        .collect();
    out.extend(extra);
    out.retain(|u| u.max > 0);
    out
}

fn equip_label(slot: Option<EquipSlot>) -> &'static str {
    match slot {
        Some(EquipSlot::Armor | EquipSlot::Worn) => "worn",
        Some(EquipSlot::PrimaryHand) => "main hand",
        Some(EquipSlot::SecondaryHand) => "off hand",
        None => "",
    }
}

/// The gear list: every inventory line but those with "include in inventory" off (Aurora's
/// equipment page; its proxy items start that way, `specs/proxy-items.md` 1.5, IM13 D3).
fn gear(snap: &CharacterSnapshot, character: &Character) -> GearData {
    let inv = &snap.inventory;
    let c = &inv.coins;
    let hidden: Vec<&str> = character
        .build
        .inventory
        .items
        .iter()
        .filter(|i| i.hidden)
        .map(|i| i.uid.as_str())
        .collect();
    GearData {
        items: inv
            .items
            .iter()
            .filter(|i| !hidden.contains(&i.uid.as_str()))
            .map(|i| GearItem {
                name: i.name.clone(),
                quantity: i.amount,
                weight: i.weight,
                equipped: equip_label(i.equipped).into(),
                attuned: i.attuned,
                requires_attunement: i.requires_attunement,
            })
            .collect(),
        coins: [c.cp, c.sp, c.ep, c.gp, c.pp],
        weight: inv.weight,
        capacity: inv.capacity,
        attunement_limit: inv.attunement_limit,
        attuned: inv
            .items
            .iter()
            .filter(|i| i.attuned)
            .map(|i| i.name.clone())
            .collect(),
    }
}

fn spell_row(sp: &SpellEntry, facts: &SpellFactsMap) -> SpellRow {
    SpellRow {
        id: sp.id.clone(),
        name: sp.name.clone(),
        level: sp.level,
        prepared: sp.prepared,
        always_prepared: sp.always_prepared,
        known: sp.known,
        source: sp.granted_by.as_ref().map(|g| g.name.clone()),
        damage: sp.damage.clone(),
        facts: facts.get(&sp.id).cloned(),
    }
}

/// The casting ability's modifier. Names are matched case-insensitively on the three-letter
/// prefix; if the ability still cannot be found, derive it from the save DC.
fn caster_modifier(sc: &SpellcastingSnapshot, snap: &CharacterSnapshot) -> i32 {
    let key = ability_key(&sc.ability);
    let found = snap.abilities.iter().find(|a| {
        let n = a.name.trim().to_lowercase();
        n == key || n.starts_with(key)
    });
    if let Some(a) = found {
        return a.modifier;
    }
    debug_assert!(false, "casting ability {:?} not on the sheet", sc.ability);
    sc.dc - 8 - snap.proficiency_bonus
}

fn caster(
    sc: &SpellcastingSnapshot,
    snap: &CharacterSnapshot,
    facts: &SpellFactsMap,
    pooled: bool,
) -> CasterData {
    let mut rows: Vec<SpellRow> = Vec::new();
    for sp in &sc.spells {
        match rows.iter_mut().find(|r| r.id == sp.id) {
            Some(r) => {
                r.prepared |= sp.prepared;
                r.always_prepared |= sp.always_prepared;
                r.known |= sp.known;
            }
            None => rows.push(spell_row(sp, facts)),
        }
    }
    rows.sort_by(|a, b| (a.level, &a.name).cmp(&(b.level, &b.name)));
    let (cantrips, spells): (Vec<_>, Vec<_>) = rows.into_iter().partition(|r| r.level == 0);
    CasterData {
        class: sc.name.clone(),
        ability: ability_key(&sc.ability),
        ability_name: title_case(&sc.ability),
        ability_modifier: caster_modifier(sc, snap),
        dc: sc.dc,
        attack: sc.attack,
        prepares: sc.prepare,
        prepare_limit: sc.prepare_limit,
        prepared_count: sc.prepared_count,
        slots: sc.slots,
        slots_pooled: pooled && sc.pact.is_none(),
        pact: sc.pact.map(|(count, level)| PactSlots { count, level }),
        cantrips,
        spells,
    }
}

fn spellcasting(snap: &CharacterSnapshot, facts: &SpellFactsMap) -> SpellcastingData {
    let pooled = snap.multiclass_slots.as_ref().map(|m| PooledSlots {
        caster_level: m.caster_level,
        slots: m.slots,
    });
    SpellcastingData {
        casters: snap
            .spellcasting
            .iter()
            .map(|sc| caster(sc, snap, facts, pooled.is_some()))
            .collect(),
        pooled,
    }
}

fn play(snap: &CharacterSnapshot) -> PlayData {
    let e = &snap.exhaustion;
    PlayData {
        current_hp: None,
        temp_hp: None,
        inspiration: snap.inspiration,
        exhaustion: ExhaustionData {
            level: e.level,
            max_levels: if e.max_levels == 0 {
                DEFAULT_EXHAUSTION_LEVELS
            } else {
                e.max_levels
            },
            linear: e.linear,
        },
        conditions: CONDITIONS,
    }
}

fn story(character: &Character) -> StoryData {
    let d = &character.details;
    let (a, p) = (&d.appearance, &d.personality);
    let t = |s: &str| s.trim().to_string();
    let notes = [d.notes.left.trim(), d.notes.right.trim()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    StoryData {
        gender: t(&character.meta.gender),
        age: t(&a.age),
        height: t(&a.height),
        weight: t(&a.weight),
        eyes: t(&a.eyes),
        skin: t(&a.skin),
        hair: t(&a.hair),
        has_portrait: character.portrait.is_some(),
        traits: t(&p.traits),
        ideals: t(&p.ideals),
        bonds: t(&p.bonds),
        flaws: t(&p.flaws),
        trinket: t(&p.trinket),
        backstory: t(&d.backstory),
        organization: t(&d.organization.name),
        allies: t(&d.organization.allies),
        treasure: t(&d.treasure),
        quest: t(&d.quest),
        equipment_notes: t(&d.equipment_notes),
        additional_features: t(&d.additional_features),
        notes,
    }
}

fn extras(character: &Character, snap: &CharacterSnapshot) -> ExtrasData {
    let active = |v: &[borealis_engine::combat::DefenseEntry]| {
        let mut out: Vec<String> = Vec::new();
        for e in v.iter().filter(|e| e.active) {
            let n = title_case(&e.name);
            if !out.contains(&n) {
                out.push(n);
            }
        }
        out
    };
    ExtrasData {
        senses: snap
            .senses
            .iter()
            .map(|s| format!("{} {} ft", title_case(&s.name), s.range))
            .collect(),
        resistances: active(&snap.defenses.resistances),
        immunities: active(&snap.defenses.immunities),
        vulnerabilities: active(&snap.defenses.vulnerabilities),
        condition_immunities: active(&snap.defenses.condition_immunities),
        advantages: snap.advantages.iter().map(|a| a.text.clone()).collect(),
        companion_name: character.details.companion_name.trim().into(),
        companion: None,
        has_wild_shape: snap.wild_shape.is_some(),
    }
}

/// Build the typed sheet for `edition` (normally [`edition_of`] the snapshot).
pub(crate) fn build(
    character: &Character,
    snap: &CharacterSnapshot,
    facts: &SpellFactsMap,
    edition: Edition,
) -> SheetData {
    let (skills, passives) = skills(snap);
    let spellcasting = spellcasting(snap, facts);
    let proficiencies = proficiencies(snap);
    let edition_fields = match edition {
        Edition::Dnd2014 => crate::model_types::EditionFields::Y2014(model_2014::build(snap)),
        Edition::Dnd2024 => {
            crate::model_types::EditionFields::Y2024(model_2024::build(snap, &proficiencies))
        }
    };
    SheetData {
        edition,
        identity: identity(character, snap),
        abilities: abilities(snap),
        skills,
        passives,
        proficiency_bonus: snap.proficiency_bonus,
        combat: combat(character, snap, &spellcasting.casters),
        proficiencies,
        features: features(snap),
        limited_uses: limited_uses(snap),
        gear: gear(snap, character),
        spellcasting,
        play: play(snap),
        story: story(character),
        edition_fields,
        extras: extras(character, snap),
    }
}
