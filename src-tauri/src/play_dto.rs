//! 2024 play-state DTOs: the engine's resources, toggles, defences, senses, notes, Wild Shape,
//! packages, compatibility and feature blocks (camelCase JSON) plus the conversion preview.

use borealis_character::{Category, ConversionItem, ConversionPreview, KeyStats, Outcome};
use borealis_engine::snapshot::CharacterSnapshot as Engine;
use serde::Serialize;
use serde_json::{Map, Value};

/// Rewrite every object key of `v` from snake_case to camelCase.
#[must_use]
pub fn camel_keys(v: Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.into_iter()
                .map(|(k, v)| (camel(&k), camel_keys(v)))
                .collect::<Map<_, _>>(),
        ),
        Value::Array(a) => Value::Array(a.into_iter().map(camel_keys).collect()),
        other => other,
    }
}

fn camel(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    let mut up = false;
    for c in key.chars() {
        if c == '_' {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn dto<T: Serialize>(v: &T) -> Value {
    camel_keys(serde_json::to_value(v).unwrap_or(Value::Null))
}

/// Walking, flying, climbing, swimming and burrowing speeds in feet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpeedsDto {
    pub walk: i32,
    pub fly: i32,
    pub climb: i32,
    pub swim: i32,
    pub burrow: i32,
}

/// Everything a 2024 sheet needs beyond the builder view (`CharacterSnapshot.play`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayStateDto {
    pub speeds: SpeedsDto,
    pub resources: Value,
    pub resource_costs: Value,
    pub toggles: Value,
    pub exhaustion: Value,
    pub inspiration: bool,
    pub inspiration_sources: Value,
    pub martial_arts: Value,
    pub masteries: Value,
    pub defenses: Value,
    pub senses: Value,
    pub advantages: Value,
    pub initiative_advantage: bool,
    pub notes: Value,
    pub carry: Value,
    pub wild_shape: Value,
    pub equipment_packages: Value,
    pub compatibility: Value,
    pub features: Value,
    pub free_casts: Value,
    pub save_dcs: Value,
}

/// Map the engine's play blocks.
#[must_use]
pub fn play_state(s: &Engine) -> PlayStateDto {
    PlayStateDto {
        speeds: SpeedsDto {
            walk: s.speed.walk,
            fly: s.speed.fly,
            climb: s.speed.climb,
            swim: s.speed.swim,
            burrow: s.speed.burrow,
        },
        resources: dto(&s.resources),
        resource_costs: dto(&s.resource_costs),
        toggles: dto(&s.toggles),
        exhaustion: dto(&s.exhaustion),
        inspiration: s.inspiration,
        inspiration_sources: dto(&s.inspiration_sources),
        martial_arts: dto(&s.martial_arts),
        masteries: dto(&s.masteries),
        defenses: dto(&s.defenses),
        senses: dto(&s.senses),
        advantages: dto(&s.advantages),
        initiative_advantage: s.initiative_advantage,
        notes: dto(&s.notes),
        carry: dto(&s.carry),
        wild_shape: dto(&s.wild_shape),
        equipment_packages: dto(&s.equipment_packages),
        compatibility: dto(&s.compatibility),
        features: dto(&s.features),
        free_casts: dto(&s.free_casts),
        save_dcs: dto(&s.save_dcs),
    }
}

/// Mastery shown on an attack row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MasteryDto {
    pub name: String,
    pub text: String,
    pub mastered: bool,
}

/// One line of the conversion diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionItemDto {
    /// species, background, class, subclass, feat, spell, item, language, ability_score, choice.
    pub category: String,
    pub label: String,
    /// kept, mapped, dropped or needs_choice.
    pub outcome: String,
    /// The element before (mapped/kept/dropped).
    pub from_id: Option<String>,
    /// The element after (mapped/kept).
    pub to_id: Option<String>,
    /// Why it was dropped.
    pub reason: Option<String>,
    /// What the user must still choose.
    pub what: Option<String>,
    /// Kept as a 2014 element inside a 2024 character.
    pub cross_edition: bool,
    /// How an element was mapped (equivalent, approximate, ...).
    pub kind: Option<String>,
}

/// Headline numbers before or after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyStatsDto {
    pub level: u32,
    pub hit_points: i32,
    pub armor_class: i32,
    pub initiative: i32,
    pub proficiency_bonus: i32,
    pub speed: i32,
    pub ability_scores: [i32; 6],
    pub spell_save_dc: Option<i32>,
}

/// Counts per outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionCounts {
    pub kept: usize,
    pub mapped: usize,
    pub dropped: usize,
    pub needs_choice: usize,
}

/// `preview_convert_to_2024` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionPreviewDto {
    pub character_id: String,
    pub name: String,
    pub items: Vec<ConversionItemDto>,
    pub counts: ConversionCounts,
    pub before: KeyStatsDto,
    pub after: KeyStatsDto,
}

/// `apply_convert_to_2024` result.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionApplied {
    pub new_character_id: String,
    pub snapshot: crate::dto::CharacterSnapshot,
}

fn stats(k: &KeyStats) -> KeyStatsDto {
    KeyStatsDto {
        level: k.level,
        hit_points: k.hit_points,
        armor_class: k.armor_class,
        initiative: k.initiative,
        proficiency_bonus: k.proficiency_bonus,
        speed: k.speed,
        ability_scores: k.ability_scores,
        spell_save_dc: k.spell_save_dc,
    }
}

fn category(c: Category) -> String {
    dto(&c).as_str().unwrap_or_default().to_string()
}

fn item(i: &ConversionItem) -> ConversionItemDto {
    let mut out = ConversionItemDto {
        category: category(i.category),
        label: i.label.clone(),
        outcome: String::new(),
        from_id: None,
        to_id: None,
        reason: None,
        what: None,
        cross_edition: false,
        kind: None,
    };
    match &i.outcome {
        Outcome::Kept { id, cross_edition } => {
            out.outcome = "kept".into();
            out.from_id = Some(id.clone());
            out.to_id = Some(id.clone());
            out.cross_edition = *cross_edition;
        }
        Outcome::Mapped { from, to, kind } => {
            out.outcome = "mapped".into();
            out.from_id = Some(from.clone());
            out.to_id = Some(to.clone());
            out.kind = kind
                .as_ref()
                .and_then(|k| serde_json::to_value(k).ok())
                .and_then(|v| v.as_str().map(str::to_string));
        }
        Outcome::Dropped { id, reason } => {
            out.outcome = "dropped".into();
            out.from_id = Some(id.clone());
            out.reason = Some(reason.clone());
        }
        Outcome::NeedsChoice { what } => {
            out.outcome = "needs_choice".into();
            out.what = Some(what.clone());
        }
    }
    out
}

/// Map a conversion preview.
#[must_use]
pub fn conversion_preview(id: &str, p: &ConversionPreview) -> ConversionPreviewDto {
    let items: Vec<ConversionItemDto> = p.items.iter().map(item).collect();
    let count = |o: &str| items.iter().filter(|i| i.outcome == o).count();
    ConversionPreviewDto {
        character_id: id.to_string(),
        name: p.converted().meta.name.clone(),
        counts: ConversionCounts {
            kept: count("kept"),
            mapped: count("mapped"),
            dropped: count("dropped"),
            needs_choice: count("needs_choice"),
        },
        items,
        before: stats(&p.before),
        after: stats(&p.after),
    }
}
