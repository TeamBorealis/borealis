//! Try it's test character (spec 9): built from scratch on the trial's private engine at level 20, with the draft in
//! its slot, then levelled down one level at a time so every level 1-20 has its own state (the ladder, and the level
//! the user looks at). Deterministic: the same draft and setup always build the same character.
//!
//! How the draft is taken, by its element type: a class, species or background is the test character's own pick at
//! level 1 ("Try as a second class": Fighter 1, then levels 2-20 in the draft through the `Multiclass` slot); a
//! subclass, sub-species, feat, spell or anything else goes into the first choice that offers it (the parent class or
//! species is chosen so that one does); a feat that no choice offers is taken at the first ability score improvement
//! (as `check::load_into_engine` takes feats); an item is added, equipped and attuned; whatever is still not taken is
//! granted as a test grant (registered like a character option). Other choices get the edition's SRD picks (Fighter,
//! Human, Soldier, the first SRD subclass) and their defaults; everything else stays open for the user. Where the
//! draft goes is `homebrew_trial_place`.

use crate::builder::{apply, apply_filled, select_in};
use crate::session::{fill_defaults, Ctx, Draft};
use crate::state::{ContentHub, BUNDLED_PACKS};
use borealis_character::Character;
use borealis_content::{ContentStore, Element, Rule, Ruleset};
use borealis_engine::commands;
use borealis_engine::ids as engine_ids;
use borealis_engine::profile::profile_for;
use borealis_engine::snapshot::OptionRef;
use borealis_engine::state::AbilityMethod;
use borealis_engine::{CharacterState, ClassSlot};
use std::collections::HashSet;

/// Highest level of the ladder.
pub(crate) const MAX_LEVEL: u32 = 20;
/// The level a second class starts at (Fighter 1, then the draft).
const SECOND_CLASS_LEVEL: u32 = 2;
/// The trial character's name (never shown in the Character Collection: it is never saved).
pub(crate) const TEST_NAME: &str = "Test character";
/// Rounds of subclass picks (one per class of a multiclassed character, with room to spare).
const SUBCLASS_PASSES: usize = 4;
/// The slot types of the level-1 picks.
const CLASS_SLOT: &[&str] = &["Class"];
const SPECIES_SLOT: &[&str] = &["Race", "Species"];
const BACKGROUND_SLOT: &[&str] = &["Background"];
const SUBCLASS_SLOT: &[&str] = &["Archetype"];
const MULTICLASS_SLOT: &[&str] = &["Multiclass"];

/// How the test character takes the draft (spec 9 "plus the draft in its slot").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Placement {
    Class,
    Subclass,
    Species,
    SubSpecies,
    Background,
    Feat,
    Spell,
    Item,
    Creature,
    Grant,
}

impl Placement {
    /// The placement of an element type.
    pub(crate) fn of(ty: &str) -> Self {
        match ty {
            "Class" => Self::Class,
            "Archetype" => Self::Subclass,
            "Race" | "Species" => Self::Species,
            "Sub Race" | "Race Variant" => Self::SubSpecies,
            "Background" => Self::Background,
            "Feat" => Self::Feat,
            "Spell" => Self::Spell,
            "Item" | "Weapon" | "Armor" | "Magic Item" | "Item Pack" => Self::Item,
            "Monster" => Self::Creature,
            _ => Self::Grant,
        }
    }
}

/// A problem the build met, at the level it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Note {
    pub(crate) level: u32,
    /// `error`, `warning` or `info`.
    pub(crate) severity: &'static str,
    pub(crate) message: String,
}

impl Note {
    pub(crate) fn new(level: u32, severity: &'static str, message: impl Into<String>) -> Self {
        Self {
            level,
            severity,
            message: message.into(),
        }
    }
}

/// The draft as the trial takes it: its elements with the ids they would get now.
#[derive(Debug, Clone)]
pub(crate) struct TrialDraft {
    /// The element the form makes first (the class, the feat...).
    pub(crate) root: Element,
    /// Every element id of the draft (the root and its children).
    pub(crate) ids: HashSet<String>,
    pub(crate) placement: Placement,
    /// A subclass's classes, a sub-species's species: the elements whose choice offers it.
    pub(crate) parents: Vec<String>,
}

impl TrialDraft {
    /// `None` when the form made no element.
    pub(crate) fn new(elements: &[Element], store: &ContentStore) -> Option<Self> {
        let root = elements.first()?.clone();
        let placement = Placement::of(&root.ty);
        let parents = match placement {
            Placement::Subclass | Placement::SubSpecies => parents_of(&root, store),
            _ => Vec::new(),
        };
        Some(Self {
            ids: elements.iter().map(|e| e.id.clone()).collect(),
            root,
            placement,
            parents,
        })
    }

    pub(crate) fn name(&self) -> &str {
        let name = self.root.name.trim();
        if name.is_empty() {
            "This element"
        } else {
            name
        }
    }

    /// The engine's multiclass entry of a class draft (its `multiclass` section's id).
    pub(crate) fn multiclass_id(&self) -> Option<&str> {
        self.root.multiclass.as_ref().map(|m| m.id.as_str())
    }

    /// Is `id` the draft (one of its elements, or a class draft's multiclass entry)? A slot that holds it is fixed.
    pub(crate) fn is_draft(&self, id: &str) -> bool {
        self.ids.contains(id) || self.multiclass_id() == Some(id)
    }
}

/// The setup a trial is built from (spec 9 "Setup"). Picks left out take the edition's SRD defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Setup {
    pub(crate) edition: Ruleset,
    /// STR..CHA; `None` = the standard array by the class's main abilities.
    pub(crate) scores: Option<[i32; 6]>,
    pub(crate) as_second_class: bool,
    pub(crate) class_id: Option<String>,
    pub(crate) subclass_id: Option<String>,
    pub(crate) species_id: Option<String>,
    pub(crate) background_id: Option<String>,
}

/// The built level-20 character and the problems met.
pub(crate) struct Built {
    pub(crate) character: Character,
    pub(crate) notes: Vec<Note>,
}

/// Which option of a slot to take.
enum Want<'a> {
    Id(&'a str),
    /// This id even when its prerequisites are not met (offered only because a house rule ignores them).
    Unmet(&'a str),
    /// An option of this name, SRD first.
    Name(&'a str),
    /// The first SRD option.
    Bundled,
}

fn bundled(hub: &ContentHub, id: &str) -> bool {
    hub.store.get(id).is_some_and(|s| {
        let pack = s.file.split('/').next().unwrap_or_default();
        BUNDLED_PACKS.contains(&pack)
    })
}

impl Want<'_> {
    fn pick(&self, options: &[OptionRef], hub: &ContentHub) -> Option<String> {
        let usable = options.iter().filter(|o| !o.prerequisites_unmet);
        let found = match self {
            Want::Id(id) => usable.clone().find(|o| o.id == *id),
            Want::Unmet(id) => options.iter().find(|o| o.id == *id),
            Want::Name(name) => {
                let named: Vec<&OptionRef> = usable
                    .clone()
                    .filter(|o| o.name.eq_ignore_ascii_case(name))
                    .collect();
                named
                    .iter()
                    .find(|o| bundled(hub, &o.id))
                    .or_else(|| named.first())
                    .copied()
            }
            Want::Bundled => usable.clone().find(|o| bundled(hub, &o.id)),
        };
        found.map(|o| o.id.clone())
    }
}

/// Does select `supports` (`A||B`, `A&&B`, `A,B`) name one of `supports`?
fn supports_match(select: Option<&str>, supports: &[String]) -> bool {
    select.is_some_and(|sel| {
        sel.split(['|', '&', ','])
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .any(|t| supports.iter().any(|s| s.trim().eq_ignore_ascii_case(t)))
    })
}

/// The classes or species whose choice offers `root` (a subclass, a sub-species): the owners of a select of its type
/// that its `supports` match, or the classes and species that grant such an owner (a class feature).
fn parents_of(root: &Element, store: &ContentStore) -> Vec<String> {
    let is_parent = |e: &Element| matches!(e.ty.as_str(), "Class" | "Race" | "Species");
    let offers = |e: &Element| {
        e.rules.iter().any(|r| {
            matches!(r, Rule::Select(s) if s.ty == root.ty && supports_match(s.supports.as_deref(), &root.supports))
        })
    };
    let mut out: Vec<String> = Vec::new();
    for owner in store
        .iter()
        .map(|s| s.element.as_ref())
        .filter(|e| offers(e))
    {
        if is_parent(owner) {
            out.push(owner.id.clone());
            continue;
        }
        out.extend(
            store
                .iter()
                .map(|s| s.element.as_ref())
                .filter(|p| is_parent(p))
                .filter(|p| {
                    p.rules
                        .iter()
                        .any(|r| matches!(r, Rule::Grant(g) if g.id == owner.id))
                })
                .map(|p| p.id.clone()),
        );
    }
    let mut seen = HashSet::new();
    out.retain(|id| seen.insert(id.clone()));
    out
}

/// Take a `want` in the lowest open slot of one of `types`. `Ok(None)` when there is no such slot or it offers none
/// of them.
fn choose(
    ctx: &Ctx<'_>,
    d: &mut Draft,
    types: &[&str],
    wants: &[Want<'_>],
) -> Result<Option<String>, String> {
    let snap = ctx.hub.derive(&d.character.build);
    let Some(slot) = snap
        .slots
        .iter()
        .filter(|s| types.contains(&s.ty.as_str()) && s.chosen.is_none() && s.rejected.is_none())
        .min_by_key(|s| s.level)
    else {
        return Ok(None);
    };
    let key = slot.key.clone();
    let Some(id) = wants.iter().find_map(|w| w.pick(&slot.options, ctx.hub)) else {
        return Ok(None);
    };
    select_in(ctx, d, &key, &id)
        .map(|()| Some(id))
        .map_err(|e| e.message)
}

/// Build the trial character at level 20 (or as high as the engine lets it go).
pub(crate) fn build(ctx: &Ctx<'_>, draft: &TrialDraft, setup: &Setup) -> Built {
    let engine = &ctx.hub.engine;
    let mut state = commands::set_ability_method(
        &commands::new_character(engine, setup.edition),
        AbilityMethod::Manual,
    );
    for opt in [engine_ids::OPT_FEATS, engine_ids::OPT_MULTICLASSING] {
        if !state.has_option(opt) && engine.has_element(setup.edition, opt) {
            if let Ok(o) = commands::set_option(engine, &state, opt, true) {
                state = o.state;
            }
        }
    }
    let mut character = Character::with_ruleset(TEST_NAME, setup.edition);
    character.build = fill_defaults(ctx.hub, &HashSet::new(), &state);
    let mut d = Draft {
        character,
        rolls: Vec::new(),
        warnings: Vec::new(),
    };
    let mut notes = Vec::new();
    take_origin(ctx, &mut d, draft, setup, &mut notes);
    set_scores(ctx, &mut d, setup, &mut notes);
    level_up(ctx, &mut d, draft, setup, &mut notes);
    pick_subclasses(ctx, &mut d, draft, setup);
    crate::homebrew_trial_place::place(ctx, &mut d, draft, setup.as_second_class, &mut notes);
    Built {
        character: d.character,
        notes,
    }
}

fn push_note(notes: &mut Vec<Note>, r: Result<Option<String>, String>, what: &str) {
    if let Err(e) = r {
        notes.push(Note::new(
            1,
            "warning",
            format!("The test character can't take its {what}: {e}"),
        ));
    }
}

/// The class, species and background at level 1.
fn take_origin(
    ctx: &Ctx<'_>,
    d: &mut Draft,
    draft: &TrialDraft,
    setup: &Setup,
    notes: &mut Vec<Note>,
) {
    let root = draft.root.id.as_str();
    let p = draft.placement;
    let parents = |on: bool| -> Vec<Want<'_>> {
        if on {
            draft
                .parents
                .iter()
                .map(|id| Want::Id(id.as_str()))
                .collect()
        } else {
            Vec::new()
        }
    };
    let mut class: Vec<Want<'_>> = Vec::new();
    if p == Placement::Class && !setup.as_second_class {
        class.push(Want::Id(root));
    } else {
        class.extend(setup.class_id.as_deref().map(Want::Id));
        class.extend(parents(p == Placement::Subclass));
        if p == Placement::Spell {
            class.extend(draft.root.supports.iter().map(|s| Want::Name(s.as_str())));
            class.push(Want::Name("Wizard"));
        }
        class.extend([Want::Name("Fighter"), Want::Bundled]);
    }
    push_note(notes, choose(ctx, d, CLASS_SLOT, &class), "class");
    let mut species: Vec<Want<'_>> = Vec::new();
    if p == Placement::Species {
        species.push(Want::Id(root));
    } else {
        species.extend(setup.species_id.as_deref().map(Want::Id));
        species.extend(parents(p == Placement::SubSpecies));
        species.extend([Want::Name("Human"), Want::Bundled]);
    }
    push_note(notes, choose(ctx, d, SPECIES_SLOT, &species), "species");
    let mut background: Vec<Want<'_>> = Vec::new();
    if p == Placement::Background {
        background.push(Want::Id(root));
    } else {
        background.extend(setup.background_id.as_deref().map(Want::Id));
        background.extend([Want::Name("Soldier"), Want::Name("Acolyte"), Want::Bundled]);
    }
    push_note(
        notes,
        choose(ctx, d, BACKGROUND_SLOT, &background),
        "background",
    );
}

/// The setup's scores, else the standard array by the class's main abilities (spec 9).
fn set_scores(ctx: &Ctx<'_>, d: &mut Draft, setup: &Setup, notes: &mut Vec<Note>) {
    let engine = &ctx.hub.engine;
    let scores = setup
        .scores
        .unwrap_or_else(|| standard_array(ctx.hub, &d.character.build));
    match commands::set_base_scores(engine, &d.character.build, scores) {
        Ok(o) => apply(d, o),
        Err(e) => notes.push(Note::new(
            1,
            "warning",
            format!("The ability scores can't be set: {e}"),
        )),
    }
}

/// The standard array as the main class's main abilities take it; a class the tables don't know takes the
/// Fighter's order.
pub(crate) fn standard_array(hub: &ContentHub, state: &CharacterState) -> [i32; 6] {
    let tables = hub.engine.universe(state.ruleset).tables();
    let snap = hub.engine.derive_with(state, false);
    let class = snap
        .classes
        .iter()
        .find(|c| c.slot == ClassSlot::Main)
        .and_then(|c| c.class.as_ref())
        .map(|c| c.name.clone())
        .unwrap_or_default();
    tables
        .standard_array_for(&class)
        .or_else(|| tables.standard_array_for("Fighter"))
        .unwrap_or_else(|| {
            profile_for(state.ruleset)
                .ability_generation()
                .standard_array
        })
}

/// Levels 2-20: the main class, or for "Try as a second class" the draft from level 2.
fn level_up(
    ctx: &Ctx<'_>,
    d: &mut Draft,
    draft: &TrialDraft,
    setup: &Setup,
    notes: &mut Vec<Note>,
) {
    let engine = &ctx.hub.engine;
    let mut second = setup.as_second_class && draft.placement == Placement::Class;
    for level in 2..=MAX_LEVEL {
        if second && level == SECOND_CLASS_LEVEL {
            second = second_class(ctx, d, draft, notes);
            if second {
                continue;
            }
        }
        let state = &d.character.build;
        let out = if second {
            commands::level_up_multiclass(engine, state, SECOND_CLASS_LEVEL, None)
        } else {
            commands::level_up_main_derived(engine, state, None)
        };
        match out {
            Ok(o) => apply_filled(ctx, d, o),
            Err(e) => {
                notes.push(Note::new(
                    level,
                    "error",
                    format!("The test character can't reach level {level}: {e}"),
                ));
                return;
            }
        }
    }
}

fn main_class_name(hub: &ContentHub, state: &CharacterState) -> String {
    hub.engine
        .derive_with(state, false)
        .classes
        .iter()
        .find(|c| c.slot == ClassSlot::Main)
        .and_then(|c| c.class.as_ref())
        .map_or_else(|| "Fighter".to_string(), |c| c.name.clone())
}

/// Level 2 in the class draft through the `Multiclass` slot (spec 9 "Multiclass check"): its multiclass
/// prerequisite and proficiencies are checked; a prerequisite the test character misses is reported and the class is
/// taken anyway (as with "Ignore multiclass prerequisites"), so the levels can still be tried. `false` when it can't
/// be taken at all (the levels go to the main class then).
fn second_class(ctx: &Ctx<'_>, d: &mut Draft, draft: &TrialDraft, notes: &mut Vec<Note>) -> bool {
    let engine = &ctx.hub.engine;
    let name = draft.name().to_string();
    let Some(mc) = draft.root.multiclass.clone() else {
        notes.push(Note::new(
            SECOND_CLASS_LEVEL,
            "error",
            format!(
                "{name} has no multiclass section, so no character can take it as a second class."
            ),
        ));
        return false;
    };
    let main = main_class_name(ctx.hub, &d.character.build);
    let opened = match commands::level_up_new_multiclass(engine, &d.character.build, None) {
        Ok(o) => Ok(o),
        Err(_) => {
            notes.push(Note::new(
                SECOND_CLASS_LEVEL,
                "warning",
                format!("The test {main} doesn't meet its own class's multiclass prerequisite; Try it ignores it."),
            ));
            d.character
                .build
                .house_rules
                .ignore_multiclass_prerequisites = true;
            commands::level_up_new_multiclass(engine, &d.character.build, None)
        }
    };
    match opened {
        Ok(o) => apply_filled(ctx, d, o),
        Err(e) => {
            notes.push(Note::new(
                SECOND_CLASS_LEVEL,
                "error",
                format!("The test character can't start a second class: {e}"),
            ));
            return false;
        }
    }
    let wants = [Want::Id(mc.id.as_str())];
    if matches!(choose(ctx, d, MULTICLASS_SLOT, &wants), Ok(Some(_))) {
        check_proficiencies(&mc, &name, notes);
        return true;
    }
    let prerequisite = mc.prerequisite.trim();
    let detail = if prerequisite.is_empty() {
        String::new()
    } else {
        format!(" ({prerequisite})")
    };
    notes.push(Note::new(
        SECOND_CLASS_LEVEL,
        "warning",
        format!(
            "A {main} with these ability scores can't take {name} as a second class: it misses the multiclass \
             prerequisite{detail}. Try it takes it anyway, as with \"Ignore multiclass prerequisites\"."
        ),
    ));
    d.character
        .build
        .house_rules
        .ignore_multiclass_prerequisites = true;
    let wants = [Want::Unmet(mc.id.as_str())];
    let taken = choose(ctx, d, MULTICLASS_SLOT, &wants);
    match taken {
        Ok(Some(_)) => {
            check_proficiencies(&mc, &name, notes);
            true
        }
        other => {
            let why = other.err().unwrap_or_else(|| "it isn't offered".into());
            notes.push(Note::new(
                SECOND_CLASS_LEVEL,
                "error",
                format!("The test character can't take {name} as a second class: {why}"),
            ));
            // the empty second-class level goes again: the levels go to the main class
            match commands::level_down(engine, &d.character.build) {
                Ok(o) => apply(d, o),
                Err(e) => tracing::warn!("removing the empty Try it multiclass level failed: {e}"),
            }
            false
        }
    }
}

/// A multiclass section that gives no proficiencies is legal (some classes give none), so it is only noted.
fn check_proficiencies(mc: &borealis_content::Multiclass, name: &str, notes: &mut Vec<Note>) {
    let gives = mc.rules.iter().any(|r| match r {
        Rule::Grant(g) => g.ty.as_deref() == Some("Proficiency"),
        Rule::Select(s) => s.ty == "Proficiency",
        _ => false,
    });
    if !gives {
        notes.push(Note::new(
            SECOND_CLASS_LEVEL,
            "info",
            format!("As a second class, {name} gives no proficiencies (its multiclass section grants none)."),
        ));
    }
}

/// Every open subclass choice takes the setup's subclass, else the first SRD one (the draft's own choice is left to
/// [`crate::homebrew_trial_place::place`]).
fn pick_subclasses(ctx: &Ctx<'_>, d: &mut Draft, draft: &TrialDraft, setup: &Setup) {
    if draft.placement == Placement::Subclass {
        return;
    }
    let mut wants: Vec<Want<'_>> = Vec::new();
    wants.extend(setup.subclass_id.as_deref().map(Want::Id));
    wants.push(Want::Bundled);
    for _ in 0..SUBCLASS_PASSES {
        if !matches!(choose(ctx, d, SUBCLASS_SLOT, &wants), Ok(Some(_))) {
            break;
        }
    }
}

/// The states of levels 1..=top: the top state levelled down one level at a time.
pub(crate) fn levels(hub: &ContentHub, top: &CharacterState) -> Vec<CharacterState> {
    let mut out = vec![top.clone()];
    while let Some(last) = out.last().filter(|s| s.level() > 1) {
        match commands::level_down(&hub.engine, last) {
            Ok(o) => out.push(o.state),
            Err(e) => {
                tracing::warn!("levelling the Try it character down failed: {e}");
                break;
            }
        }
    }
    out.reverse();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_types_map_to_their_placement() {
        assert_eq!(Placement::of("Class"), Placement::Class);
        assert_eq!(Placement::of("Archetype"), Placement::Subclass);
        assert_eq!(Placement::of("Species"), Placement::Species);
        assert_eq!(Placement::of("Sub Race"), Placement::SubSpecies);
        assert_eq!(Placement::of("Magic Item"), Placement::Item);
        assert_eq!(Placement::of("Monster"), Placement::Creature);
        assert_eq!(Placement::of("Language"), Placement::Grant);
    }

    #[test]
    fn a_select_supports_matches_any_of_its_terms() {
        let supports = vec!["Fighter Subclass".to_string()];
        assert!(supports_match(Some("Fighter Subclass"), &supports));
        assert!(supports_match(
            Some("Rogue Subclass||fighter subclass"),
            &supports
        ));
        assert!(!supports_match(Some("Rogue Subclass"), &supports));
        assert!(!supports_match(None, &supports));
    }
}
