//! Engine snapshot -> contract `CharacterSnapshot` (identity, levels, abilities, stats, slots).

use crate::dto::{
    AbilityRecord, AbilityState, CharacterSnapshot, CharacterStats, ClassLevelInfo, FeatureEntry,
    LevelState, PointBuyState, ProficiencyGroup, RollRecord, SaveEntry, SelectionProgress,
    SelectionSlot, SkillEntry, StandardArrayState,
};
use crate::extras::{self, ui_extras};
use crate::ids::{
    encode_class_slot, encode_slot, flow_priority, tab_for, title_case, ui_type, TABS,
};
use crate::settings::AppSettings;
use crate::state::ContentHub;
use borealis_character::Character;
use borealis_engine::levels::{average_die, point_cost};
use borealis_engine::profile::profile_for;
use borealis_engine::snapshot::{
    CharacterSnapshot as Engine, ClassSnapshot, ElementRef as EngineRef, SlotSnapshot,
};
use borealis_engine::state::AbilityMethod;
use borealis_engine::ClassSlot;
use std::collections::BTreeMap;

const MAX_LEVEL: u32 = 20;
const FEATURE_TYPES: [&str; 5] = [
    "Class Feature",
    "Archetype Feature",
    "Racial Trait",
    "Background Feature",
    "Feat",
];

/// Inputs of a snapshot build.
pub struct MapInput<'a> {
    /// Library id.
    pub id: &'a str,
    /// The character.
    pub character: &'a Character,
    /// Unsaved changes.
    pub dirty: bool,
    /// Display-only file path.
    pub display_path: String,
    /// Content + engine.
    pub hub: &'a ContentHub,
    /// App settings.
    pub settings: &'a AppSettings,
    /// Ability roll history.
    pub rolls: &'a [RollRecord],
    /// Transient warnings of the mutation that produced this snapshot.
    pub warnings: &'a [String],
}

/// Build the UI snapshot of a character.
#[must_use]
pub fn build_snapshot(input: &MapInput<'_>) -> CharacterSnapshot {
    let engine_snap = input.hub.engine.derive(&input.character.build);
    let slots_and_progress = slots(&engine_snap);
    let (slots, progress) = slots_and_progress;
    let ruleset = input.character.ruleset();
    CharacterSnapshot {
        id: input.id.to_string(),
        name: input.character.meta.name.clone(),
        ruleset,
        portrait: extras::portrait_data_url(input.character),
        group: input.character.meta.group.clone(),
        favorite: input.character.meta.favorite,
        dirty: input.dirty,
        display_path: input.display_path.clone(),
        level: level_state(input, &engine_snap),
        abilities: abilities(input, &engine_snap),
        stats: stats(&engine_snap),
        slots,
        progress,
        spellcasting: extras::spellcasting(input, &engine_snap),
        inventory: extras::inventory(input, &engine_snap),
        details: extras::details(input.character),
        attacks: extras::attacks(input.character, &engine_snap),
        companion: crate::companion::companion(input, &engine_snap),
        options: extras::options(input, &engine_snap),
        available_options: extras::available_options(input),
        source_restrictions: extras::source_restrictions(input),
        notices: extras::notices(input, &engine_snap),
        play: crate::play_dto::play_state(&engine_snap),
    }
}

fn level_state(input: &MapInput<'_>, snap: &Engine) -> LevelState {
    let ruleset = input.character.ruleset();
    let engine = &input.hub.engine;
    let can_multiclass = snap.can_multiclass;
    LevelState {
        total: snap.level,
        xp: snap.experience,
        next_level_xp: snap.next_level_xp,
        current_level_xp: engine.xp_threshold(ruleset, snap.level),
        level_up_available: snap.level_up_available,
        can_level_up: snap.level < MAX_LEVEL,
        can_level_down: snap.level > 1,
        can_multiclass,
        multiclass_blocked_reason: (!can_multiclass).then(|| multiclass_reason(input, snap)),
        classes: snap.classes.iter().map(|c| class_info(input, c)).collect(),
    }
}

fn multiclass_reason(input: &MapInput<'_>, snap: &Engine) -> String {
    if !input
        .character
        .build
        .has_option(borealis_engine::ids::OPT_MULTICLASSING)
    {
        return "Multiclassing is switched off for this character.".into();
    }
    match snap.classes.first().and_then(|c| c.class.as_ref()) {
        None => "Choose a class first.".into(),
        Some(_) => {
            let need = snap
                .classes
                .first()
                .map(|c| class_info(input, c))
                .and_then(|c| c.multiclass_prerequisite);
            need.map_or_else(
                || "The multiclassing requirements are not met.".into(),
                |p| format!("Multiclassing requires {p}."),
            )
        }
    }
}

fn class_info(input: &MapInput<'_>, c: &ClassSnapshot) -> ClassLevelInfo {
    let die = c.hit_die.unwrap_or(0);
    let stored = borealis_engine::levels::rolls_for(&input.character.build.hp_rolls, &c.slot);
    let hp_rolls = (0..c.level as usize)
        .map(|i| match stored.get(i) {
            Some(&v) if v > 0 => v,
            _ if i == 0 && c.slot == ClassSlot::Main => die,
            _ => average_die(die),
        })
        .collect();
    let prerequisite = c.class.as_ref().and_then(|cls| {
        let store = &input.hub.store;
        let sibling = cls.id.replace("_CLASS_", "_MULTICLASS_");
        [cls.id.as_str(), sibling.as_str()]
            .iter()
            .filter_map(|id| store.element(id))
            .find_map(|el| {
                el.multiclass
                    .as_ref()
                    .map(|m| m.prerequisite.clone())
                    .or_else(|| (el.ty == "Multiclass").then(|| el.prerequisite.clone()))
            })
            .filter(|p| !p.trim().is_empty())
    });
    ClassLevelInfo {
        slot_id: encode_class_slot(&c.slot),
        class_id: c.class.as_ref().map(|e| e.id.clone()),
        name: c.class.as_ref().map(|e| e.name.clone()),
        level: c.level,
        hit_die: die,
        is_main: c.slot == ClassSlot::Main,
        subclass: c.archetypes.first().map(engine_ref),
        hp_rolls,
        multiclass_prerequisite: prerequisite,
    }
}

fn engine_ref(e: &EngineRef) -> crate::dto::ElementRef {
    crate::dto::ElementRef {
        id: e.id.clone(),
        name: e.name.clone(),
        ty: ui_type(&e.ty).to_string(),
        source: e.source.clone(),
    }
}

/// Which standard-array values are assigned, derived from the stored base scores.
///
/// The lowest array value doubles as the "unassigned" placeholder, so it only counts as assigned
/// once every other value is placed.
#[must_use]
pub fn standard_array_assignment(base: [i32; 6], array: [i32; 6]) -> [Option<i32>; 6] {
    let low = array.iter().copied().min().unwrap_or(8);
    let mut assigned = [None; 6];
    for (i, v) in base.iter().enumerate() {
        let duplicate = assigned.contains(&Some(*v));
        if array.contains(v) && *v != low && !duplicate {
            assigned[i] = Some(*v);
        }
    }
    let placed = assigned.iter().flatten().count();
    if placed == array.len() - 1 {
        if let Some(i) = (0..6).find(|i| assigned[*i].is_none() && base[*i] == low) {
            assigned[i] = Some(low);
        }
    }
    assigned
}

fn abilities(input: &MapInput<'_>, snap: &Engine) -> AbilityState {
    let state = &input.character.build;
    let ruleset = input.character.ruleset();
    let generation = profile_for(ruleset).ability_generation();
    let unrestricted = ui_extras(input.character).point_buy_unrestricted;
    let get = |i: usize| snap.abilities.get(i);
    let method = state.abilities.method;
    let (min, max) = generation.point_buy_range;
    let point_buy = (method == AbilityMethod::PointBuy).then(|| {
        let pb = snap
            .point_buy
            .clone()
            .unwrap_or(borealis_engine::levels::PointBuy {
                budget: input.settings.point_buy_budget,
                spent: 0,
                remaining: input.settings.point_buy_budget,
            });
        PointBuyState {
            budget: pb.budget,
            spent: pb.spent,
            remaining: pb.remaining,
            costs: (min..=max).map(|s| (s, point_cost(s))).collect(),
            min,
            max,
            unrestricted,
        }
    });
    let standard_array = (method == AbilityMethod::StandardArray).then(|| {
        let assigned = extras::array_assignment(input.character, generation.standard_array);
        let used: Vec<i32> = assigned.iter().flatten().copied().collect();
        StandardArrayState {
            values: generation.standard_array.to_vec(),
            unassigned: generation
                .standard_array
                .iter()
                .copied()
                .filter(|v| !used.contains(v))
                .collect(),
            assigned: AbilityRecord::from_fn(|i| assigned[i]),
        }
    });
    let cap = snap.abilities.first().map_or(20, |a| a.maximum);
    let over_cap = snap
        .abilities
        .iter()
        .enumerate()
        .filter(|(_, a)| a.base + a.additional > a.maximum)
        .map(|(i, _)| crate::ids::ABILITY_KEYS[i].to_string())
        .collect();
    AbilityState {
        method: method.into(),
        base: AbilityRecord::from_fn(|i| state.abilities.base[i]),
        bonus: AbilityRecord::from_fn(|i| get(i).map_or(0, |a| a.additional)),
        bonus_sources: AbilityRecord::from_fn(|i| {
            get(i)
                .and_then(|a| snap.stats.get(&a.name))
                .map(|s| {
                    s.sources
                        .iter()
                        .filter(|(label, v)| *v != 0 && label != "Base")
                        .map(|(label, v)| format!("{label} {v:+}"))
                        .collect()
                })
                .unwrap_or_default()
        }),
        final_score: AbilityRecord::from_fn(|i| get(i).map_or(10, |a| a.score)),
        modifier: AbilityRecord::from_fn(|i| get(i).map_or(0, |a| a.modifier)),
        save: AbilityRecord::from_fn(|i| {
            get(i).map_or(SaveEntry::default(), |a| SaveEntry {
                bonus: a.save,
                proficient: a.save_proficient,
            })
        }),
        cap,
        over_cap,
        point_buy,
        standard_array,
        rolls: input.rolls.to_vec(),
    }
}

fn stats(snap: &Engine) -> CharacterStats {
    let skills: Vec<SkillEntry> = snap
        .skills
        .iter()
        .map(|s| SkillEntry {
            key: s.name.clone(),
            name: title_case(&s.name),
            ability: crate::ids::ability_key(&s.ability),
            bonus: s.total,
            proficiency: if s.expertise {
                "expertise"
            } else if s.proficient {
                "proficient"
            } else {
                "none"
            }
            .to_string(),
        })
        .collect();
    let passive = snap.skill("perception").map_or(10, |s| s.passive);
    let mut languages = Vec::new();
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut features = Vec::new();
    for reg in &snap.elements {
        let e = &reg.element;
        match e.ty.as_str() {
            "Language" => languages.push(e.name.clone()),
            "Proficiency" => {
                let (label, value) = e.name.split_once(" (").map_or_else(
                    || ("Proficiency".to_string(), e.name.clone()),
                    |(l, v)| (l.to_string(), v.trim_end_matches(')').to_string()),
                );
                groups.entry(label).or_default().push(value);
            }
            t if FEATURE_TYPES.contains(&t) => features.push(FeatureEntry {
                name: e.name.clone(),
                source: e.source.clone(),
            }),
            _ => {}
        }
    }
    CharacterStats {
        proficiency_bonus: snap.proficiency_bonus,
        armor_class: snap.armor_class,
        initiative: snap.initiative,
        speed: snap.speed.walk,
        hp_max: snap.hit_points,
        passive_perception: passive,
        skills,
        languages,
        proficiencies: groups
            .into_iter()
            .map(|(label, values)| ProficiencyGroup { label, values })
            .collect(),
        features,
    }
}

fn slot_tab(s: &SlotSnapshot) -> &'static str {
    if s.owner.id.contains("ABILITYSCOREIMPROVEMENT") || s.owner.ty == "Ability Score Improvement" {
        return "abilities";
    }
    match tab_for(&s.ty) {
        "class" if matches!(s.ty.as_str(), "Alignment" | "Deity") => "background",
        "class" => match s.owner.ty.as_str() {
            "Background" | "Background Feature" => "background",
            "Race" | "Sub Race" | "Species" | "Racial Trait" => "race",
            _ => "class",
        },
        other => other,
    }
}

fn slots(snap: &Engine) -> (Vec<SelectionSlot>, SelectionProgress) {
    let mut out = Vec::new();
    let mut ranked: Vec<(u8, usize)> = Vec::new();
    let mut by_tab: BTreeMap<String, usize> = TABS.iter().map(|t| ((*t).to_string(), 0)).collect();
    for s in snap.slots.iter().filter(|s| s.rejected.is_none()) {
        let tab = slot_tab(s);
        let required = !s.optional;
        let chosen = chosen_ref(s);
        let unfilled = required && chosen.is_none();
        if unfilled {
            if let Some(n) = by_tab.get_mut(tab) {
                *n += 1;
            }
            if !s.locked {
                ranked.push((flow_priority(&s.ty), out.len()));
            }
        }
        let (group_id, group_label) = if s.owner.ty == "Multiclass" {
            (Some(s.owner.id.clone()), Some(s.owner.name.clone()))
        } else {
            (None, None)
        };
        out.push(SelectionSlot {
            id: encode_slot(&s.key),
            tab: tab.to_string(),
            group_id,
            group_label,
            class_slot_id: extras::slot_caster_id(snap, s),
            title: s.name.clone(),
            element_type: ui_type(&s.ty).to_string(),
            ordinal: s.key.ordinal,
            of: s.number,
            required,
            locked: s.locked,
            chosen,
            required_level: (s.level > 1).then_some(s.level),
            allow_replace: s.replaceable,
            hint: None,
            gained_on_level: s.gained_level,
            replaced: s
                .replaced
                .iter()
                .map(|r| crate::dto::ReplacedPick {
                    id: r.pick.id.clone(),
                    name: r.pick.name.clone(),
                    level: r.level,
                })
                .collect(),
        });
    }
    ranked.sort();
    let next_slot_id = ranked.first().map(|(_, i)| out[*i].id.clone());
    let progress = SelectionProgress {
        total: out.len(),
        remaining: by_tab.values().sum(),
        next_slot_id,
        by_tab,
    };
    (out, progress)
}

/// The shown choice of a slot: the stored pick, or the only candidate of a locked slot.
fn chosen_ref(s: &SlotSnapshot) -> Option<crate::dto::ElementRef> {
    if let Some(c) = &s.chosen {
        return Some(engine_ref(c));
    }
    if let Some(id) = s.list_choice {
        let name = s
            .list_items
            .iter()
            .find(|(i, _)| *i == id)
            .map_or_else(|| id.to_string(), |(_, n)| n.clone());
        return Some(crate::dto::ElementRef {
            id: id.to_string(),
            name,
            ty: "list".into(),
            source: s.owner.source.clone(),
        });
    }
    match (s.locked, s.options.as_slice()) {
        (true, [only]) => Some(crate::dto::ElementRef {
            id: only.id.clone(),
            name: only.name.clone(),
            ty: ui_type(&s.ty).to_string(),
            source: only.source.clone(),
        }),
        _ => None,
    }
}
