//! Legal level-up options, computed by the engine, and the commands that apply them.
//!
//! An option is one decision for the next level: which class to level (`Take a level in ...`),
//! a choice in a newly opened slot (subclass, fighting style, spells, ...), or an ability score
//! improvement / feat. The model may only refer to these option ids.

use std::collections::BTreeSet;

use borealis_ai::retrieval::html_to_text;
use borealis_engine::commands::{self, Improvement};
use borealis_engine::ids::ABILITIES;
use borealis_engine::snapshot::{CharacterSnapshot, SlotSnapshot};
use borealis_engine::state::{ClassSlot, SlotKey};
use borealis_engine::{CharacterState, Choice, Engine};
use serde::{Deserialize, Serialize};

use crate::error::AssistError;

/// Slot name prefix of the ability score improvement / feat slot.
const IMPROVEMENT_PREFIX: &str = "Improvement Option (";
/// Slot name prefix of the "which class do you multiclass into" slot.
const MULTICLASS_PREFIX: &str = "Multiclass (Level";
/// At most this many options are listed for one open slot.
pub const MAX_OPTIONS_PER_SLOT: usize = 60;
/// Option summaries are cut to this many characters.
const SUMMARY_CHARS: usize = 160;
/// The character level cap.
const MAX_LEVEL: u32 = 20;

/// Which class gains the level.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LevelPath {
    /// The main class.
    Main,
    /// An existing multiclass, identified by its starting character level.
    Multi {
        /// Character level at which the multiclass started.
        starting_level: u32,
    },
    /// A new multiclass; `class` is a `Multiclass` element id from the engine's option list.
    NewMulticlass {
        /// The multiclass element id.
        class: String,
    },
}

impl LevelPath {
    /// Short stable key used in option ids.
    #[must_use]
    pub fn key(&self) -> String {
        match self {
            Self::Main => "main".to_string(),
            Self::Multi { starting_level } => format!("multi{starting_level}"),
            Self::NewMulticlass { class } => format!("new:{class}"),
        }
    }
}

/// An ability score improvement or feat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImprovementSpec {
    /// +2 to one ability.
    PlusTwo {
        /// Lower-case ability name (`strength`, ...).
        ability: String,
    },
    /// +1 to each of two different abilities.
    PlusOneEach {
        /// Two lower-case ability names.
        abilities: [String; 2],
    },
    /// A feat element id.
    Feat {
        /// The feat id.
        feat: String,
    },
}

impl ImprovementSpec {
    /// Resolve to the engine command argument.
    ///
    /// # Errors
    /// [`AssistError::Invalid`] for an unknown ability name.
    pub fn to_engine(&self) -> Result<Improvement, AssistError> {
        let idx = |name: &str| {
            ABILITIES
                .iter()
                .position(|a| a.eq_ignore_ascii_case(name.trim()))
                .ok_or_else(|| AssistError::Invalid(format!("unknown ability \"{name}\"")))
        };
        Ok(match self {
            Self::PlusTwo { ability } => Improvement::PlusTwo(idx(ability)?),
            Self::PlusOneEach { abilities } => {
                Improvement::PlusOneEach(idx(&abilities[0])?, idx(&abilities[1])?)
            }
            Self::Feat { feat } => Improvement::Feat(feat.clone()),
        })
    }

    fn id_part(&self) -> String {
        match self {
            Self::PlusTwo { ability } => format!("asi:plus2:{ability}"),
            Self::PlusOneEach { abilities } => {
                format!("asi:plus1:{},{}", abilities[0], abilities[1])
            }
            Self::Feat { feat } => format!("feat:{feat}"),
        }
    }
}

/// What the option does after the level is taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OptionAction {
    /// Only take the level (the class choice itself).
    TakeLevel,
    /// Choose `element` in a slot that the level opened.
    Select {
        /// The slot (valid in the state right after taking the level on this path).
        slot: SlotKey,
        /// The chosen element id.
        element: String,
    },
    /// Take an ability score improvement or feat.
    Improve {
        /// Class name as used in the slot title (`Fighter`).
        class: String,
        /// Class level of the improvement.
        level: u32,
        /// What is taken.
        what: ImprovementSpec,
    },
}

/// One legal decision for the next level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelUpOption {
    /// Stable id the model must use.
    pub id: String,
    /// The decision this option belongs to (slot title or `Which class to level`).
    pub group: String,
    /// Display label.
    pub label: String,
    /// Short plain-text description (may be empty).
    pub summary: String,
    /// The level-up path the option applies to.
    pub path: LevelPath,
    /// What to do after taking the level.
    pub action: OptionAction,
}

/// Result of applying an option.
#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    /// The new state.
    pub state: CharacterState,
    /// Human-readable list of the commands that ran.
    pub steps: Vec<String>,
}

/// Take a level on `path` (and choose the multiclass for a new one).
///
/// # Errors
/// The engine's refusal (level cap, multiclassing unavailable, class not allowed).
pub fn take_level(
    engine: &Engine,
    state: &CharacterState,
    path: &LevelPath,
) -> Result<Applied, AssistError> {
    match path {
        LevelPath::Main => Ok(Applied {
            state: commands::level_up_main(engine, state, None)?.state,
            steps: vec!["level up (main class)".to_string()],
        }),
        LevelPath::Multi { starting_level } => Ok(Applied {
            state: commands::level_up_multiclass(engine, state, *starting_level, None)?.state,
            steps: vec![format!("level up (multiclass from level {starting_level})")],
        }),
        LevelPath::NewMulticlass { class } => {
            let next = commands::level_up_new_multiclass(engine, state, None)?.state;
            let snap = engine.derive(&next);
            let slot = snap
                .slots
                .iter()
                .find(|s| s.name.starts_with(MULTICLASS_PREFIX) && s.chosen.is_none())
                .ok_or_else(|| AssistError::Invalid("no multiclass slot opened".into()))?;
            let chosen =
                commands::select(engine, &next, &slot.key, Choice::Element(class.clone()))?;
            Ok(Applied {
                state: chosen.state,
                steps: vec![
                    "level up (new multiclass)".to_string(),
                    format!("choose multiclass {class}"),
                ],
            })
        }
    }
}

/// Apply one option to `state`: take the level on its path, then its action.
///
/// # Errors
/// Any refusal from the engine commands.
pub fn apply_option(
    engine: &Engine,
    state: &CharacterState,
    option: &LevelUpOption,
) -> Result<Applied, AssistError> {
    let mut applied = take_level(engine, state, &option.path)?;
    match &option.action {
        OptionAction::TakeLevel => {}
        OptionAction::Select { slot, element } => {
            applied.state = commands::select(
                engine,
                &applied.state,
                slot,
                Choice::Element(element.clone()),
            )?
            .state;
            applied.steps.push(format!("{}: {element}", option.group));
        }
        OptionAction::Improve { class, level, what } => {
            applied.state = commands::choose_improvement(
                engine,
                &applied.state,
                class,
                *level,
                &what.to_engine()?,
            )?
            .state;
            applied
                .steps
                .push(format!("{}: {}", option.group, what.id_part()));
        }
    }
    Ok(applied)
}

/// Slots that offer a choice right now.
pub(crate) fn open_slots(snap: &CharacterSnapshot) -> Vec<&SlotSnapshot> {
    snap.slots
        .iter()
        .filter(|s| s.chosen.is_none() && s.list_choice.is_none() && s.rejected.is_none())
        .filter(|s| !s.options.is_empty())
        .collect()
}

/// Slots open in `after` that were not open in `before` (what a step newly asks for).
pub(crate) fn newly_open_slots<'a>(
    before: &CharacterSnapshot,
    after: &'a CharacterSnapshot,
) -> Vec<&'a SlotSnapshot> {
    let known: BTreeSet<&SlotKey> = open_slots(before).iter().map(|s| &s.key).collect();
    open_slots(after)
        .into_iter()
        .filter(|s| !known.contains(&s.key))
        .collect()
}

/// `Improvement Option (Fighter 4)` -> (`Fighter`, 4).
pub(crate) fn parse_improvement_slot(name: &str) -> Option<(String, u32)> {
    let inner = name.strip_prefix(IMPROVEMENT_PREFIX)?.strip_suffix(')')?;
    let (class, level) = inner.rsplit_once(' ')?;
    Some((class.to_string(), level.parse().ok()?))
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

fn summary_of(engine: &Engine, state: &CharacterState, id: &str) -> String {
    let Some(el) = engine.universe(state.ruleset).get(id) else {
        return String::new();
    };
    let text = html_to_text(&el.el.description);
    let mut flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > SUMMARY_CHARS {
        flat = flat.chars().take(SUMMARY_CHARS).collect::<String>() + "...";
    }
    flat
}

fn class_title(
    snap: &CharacterSnapshot,
    path: &LevelPath,
    engine: &Engine,
    state: &CharacterState,
) -> String {
    let slot = match path {
        LevelPath::Main => Some(ClassSlot::Main),
        LevelPath::Multi { starting_level } => Some(ClassSlot::Multi {
            starting_level: *starting_level,
        }),
        LevelPath::NewMulticlass { .. } => None,
    };
    if let Some(slot) = slot {
        if let Some(c) = snap.classes.iter().find(|c| c.slot == slot) {
            return c
                .class
                .as_ref()
                .map_or_else(|| "(no class)".to_string(), |e| e.name.clone());
        }
    }
    if let LevelPath::NewMulticlass { class } = path {
        return engine
            .universe(state.ruleset)
            .get(class)
            .map_or_else(|| class.clone(), |e| e.el.name.clone());
    }
    "(no class)".to_string()
}

/// All legal single decisions for the next character level.
///
/// For every level-up path (main class, existing multiclasses, each allowed new multiclass) the
/// engine levels a cloned state; the option list contains the class choice itself plus every
/// choice the level newly opens (for new multiclasses only the class choice is listed, further
/// choices come after it is taken). Ability score improvements and feats are expanded to the
/// concrete variants that the engine accepts and that change something.
///
/// # Errors
/// Never fails for engine refusals (an illegal path is just not listed).
pub fn legal_options(
    engine: &Engine,
    state: &CharacterState,
) -> Result<Vec<LevelUpOption>, AssistError> {
    if state.level() >= MAX_LEVEL || state.levels.is_empty() {
        return Ok(Vec::new());
    }
    let before = engine.derive(state);
    let mut paths = vec![LevelPath::Main];
    for l in &state.levels {
        if let ClassSlot::Multi { starting_level } = l {
            let p = LevelPath::Multi {
                starting_level: *starting_level,
            };
            if !paths.contains(&p) {
                paths.push(p);
            }
        }
    }
    let mut out = Vec::new();
    for path in &paths {
        let Ok(next) = take_level(engine, state, path) else {
            continue;
        };
        let after = engine.derive(&next.state);
        let class_name = class_title(&after, path, engine, state);
        out.push(LevelUpOption {
            id: format!("{}:level", path.key()),
            group: "Which class to level".to_string(),
            label: format!("Take the next level in {class_name}"),
            summary: String::new(),
            path: path.clone(),
            action: OptionAction::TakeLevel,
        });
        out.extend(slot_options(engine, &next.state, &before, &after, path));
    }
    out.extend(new_multiclass_options(engine, state, &before));
    Ok(out)
}

fn new_multiclass_options(
    engine: &Engine,
    state: &CharacterState,
    before: &CharacterSnapshot,
) -> Vec<LevelUpOption> {
    if !before.can_multiclass {
        return Vec::new();
    }
    let Ok(next) = commands::level_up_new_multiclass(engine, state, None) else {
        return Vec::new();
    };
    let snap = engine.derive(&next.state);
    let Some(slot) = snap
        .slots
        .iter()
        .find(|s| s.name.starts_with(MULTICLASS_PREFIX) && s.chosen.is_none())
    else {
        return Vec::new();
    };
    slot.options
        .iter()
        .take(MAX_OPTIONS_PER_SLOT)
        .map(|o| {
            let path = LevelPath::NewMulticlass {
                class: o.id.clone(),
            };
            LevelUpOption {
                id: format!("{}:level", path.key()),
                group: "Which class to level".to_string(),
                label: format!("Start multiclass: {}", o.name),
                summary: summary_of(engine, state, &o.id),
                path,
                action: OptionAction::TakeLevel,
            }
        })
        .collect()
}

fn slot_options(
    engine: &Engine,
    leveled: &CharacterState,
    before: &CharacterSnapshot,
    after: &CharacterSnapshot,
    path: &LevelPath,
) -> Vec<LevelUpOption> {
    let mut out = Vec::new();
    let mut seen_names: BTreeSet<&str> = BTreeSet::new();
    for slot in newly_open_slots(before, after) {
        if !seen_names.insert(slot.name.as_str()) {
            continue; // identical sibling slots (e.g. two skills): list the first
        }
        if let Some((class, level)) = parse_improvement_slot(&slot.name) {
            out.extend(improvement_options(
                engine, leveled, after, path, &slot.name, &class, level,
            ));
            continue;
        }
        for o in slot.options.iter().take(MAX_OPTIONS_PER_SLOT) {
            out.push(LevelUpOption {
                id: format!("{}:{}:{}", path.key(), slug(&slot.name), o.id),
                group: slot.name.clone(),
                label: o.name.clone(),
                summary: summary_of(engine, leveled, &o.id),
                path: path.clone(),
                action: OptionAction::Select {
                    slot: slot.key.clone(),
                    element: o.id.clone(),
                },
            });
        }
    }
    out
}

fn improvement_options(
    engine: &Engine,
    leveled: &CharacterState,
    after: &CharacterSnapshot,
    path: &LevelPath,
    group: &str,
    class: &str,
    level: u32,
) -> Vec<LevelUpOption> {
    let mut specs: Vec<(ImprovementSpec, String)> = Vec::new();
    for a in ABILITIES {
        specs.push((
            ImprovementSpec::PlusTwo {
                ability: a.to_string(),
            },
            format!("Increase {a} by 2"),
        ));
    }
    for (i, a) in ABILITIES.iter().enumerate() {
        for b in &ABILITIES[i + 1..] {
            specs.push((
                ImprovementSpec::PlusOneEach {
                    abilities: [(*a).to_string(), (*b).to_string()],
                },
                format!("Increase {a} and {b} by 1 each"),
            ));
        }
    }
    for f in engine.universe(leveled.ruleset).by_type("Feat") {
        specs.push((
            ImprovementSpec::Feat {
                feat: f.el.id.clone(),
            },
            format!("Feat: {}", f.el.name),
        ));
    }
    let base_scores: Vec<i32> = after.abilities.iter().map(|a| a.score).collect();
    let mut out = Vec::new();
    for (what, label) in specs {
        let Ok(engine_what) = what.to_engine() else {
            continue;
        };
        let Ok(done) = commands::choose_improvement(engine, leveled, class, level, &engine_what)
        else {
            continue;
        };
        if !matches!(what, ImprovementSpec::Feat { .. }) {
            let scores: Vec<i32> = engine
                .derive_with(&done.state, false)
                .abilities
                .iter()
                .map(|a| a.score)
                .collect();
            if scores == base_scores {
                continue; // at the ability maximum: nothing would change
            }
        }
        let summary = match &what {
            ImprovementSpec::Feat { feat } => summary_of(engine, leveled, feat),
            _ => String::new(),
        };
        out.push(LevelUpOption {
            id: format!("{}:{}", path.key(), what.id_part()),
            group: group.to_string(),
            label,
            summary,
            path: path.clone(),
            action: OptionAction::Improve {
                class: class.to_string(),
                level,
                what,
            },
        });
    }
    out
}
