//! Try it (spec 9): a test character that takes the open draft, at any level, without touching the user's
//! characters. `homebrew_trial_start` / `_update` / `_options` / `_end`.
//!
//! A trial holds a private content hub: the enabled packs with the draft's elements (the ids they would get now) in
//! its pack, and its own engine, so the unsaved element is never in the shared content store. A second private engine
//! over the content without the draft is the baseline of `changed`. The trial character is a `CharacterState` that is
//! never written to disk, never listed and never in the open-character sessions; it is dropped on
//! `homebrew_trial_end`, when more than [`MAX_TRIALS`] are open (the oldest), and after [`IDLE_LIMIT`] without a call
//! (the next call answers `not_found`, and the UI starts it again with the same setup).

use crate::builder::{clear_in, select_in, slot_options_page};
use crate::dto::{Page, Ruleset, SlotOption};
use crate::error::{ApiError, ApiResult};
use crate::homebrew_dto::{
    HomebrewProblem, HomebrewTrialChange, HomebrewTrialSetup, HomebrewTrialView,
};
use crate::homebrew_packs::CreatorPack;
use crate::homebrew_trial_build::{self as build, Note, Placement, Setup, TrialDraft, MAX_LEVEL};
use crate::homebrew_trial_view as view;
use crate::ids::decode_slot;
use crate::load_timing::LoadTimer;
use crate::session::{Ctx, Draft};
use crate::settings::AppSettings;
use crate::state::{AppState, ContentHub};
use borealis_authoring::check::messages::sweep_message;
use borealis_authoring::check::{sweep_engine, SweepFinding, SweepIssue, SweepTarget};
use borealis_authoring::form::{assign_ids, write_copy, CatalogBlockCodec, FormContext};
use borealis_authoring::pack::{pack_edition, with_elements, without_elements};
use borealis_character::Character;
use borealis_content::file::ContentFile;
use borealis_content::{ParsedPack, Rule};
use borealis_engine::{commands, CharacterState};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A trial nobody touched for this long is dropped (spec 9: "after 30 minutes idle").
pub const IDLE_LIMIT: Duration = Duration::from_secs(30 * 60);
/// Trials kept at once (each holds a private engine); starting one more drops the least recently used.
pub const MAX_TRIALS: usize = 2;
const SCORE_MIN: i32 = 1;
const SCORE_MAX: i32 = 30;
const SCORES_MESSAGE: &str = "Ability scores are six whole numbers from 1 to 30.";
const CLOSED_MESSAGE: &str = "The test character was closed. Open Try it again.";

/// The content a trial runs on: the draft in its pack (a private hub), the same content without it, and the draft.
struct TrialContent {
    hub: ContentHub,
    without: Without,
    draft: TrialDraft,
    /// The draft's `updatedAt` when this was made: a newer draft is loaded again.
    updated_at: String,
    /// The pack's edition (`any` for both).
    pack_edition: Ruleset,
}

/// The content without the draft. Only the view's `changed` reads it, so it is built on a thread of its own while the
/// test character is built on the trial content (an engine over a large library takes seconds on a phone).
enum Without {
    Building(std::thread::JoinHandle<Option<ContentHub>>),
    Ready(Box<ContentHub>),
    Failed,
}

const WITHOUT_FAILED: &str =
    "The content without the draft could not be prepared. Open Try it again.";

impl Without {
    /// Start building it on a thread of its own (here, when no thread can be started).
    fn start(build: impl FnOnce() -> ContentHub + Send + 'static) -> Self {
        let slot = Arc::new(Mutex::new(Some(build)));
        let theirs = Arc::clone(&slot);
        let spawned = std::thread::Builder::new()
            .name("try-it-without".into())
            .spawn(move || theirs.lock().ok().and_then(|mut b| b.take()).map(|b| b()));
        match spawned {
            Ok(handle) => Self::Building(handle),
            Err(e) => {
                tracing::warn!("Try it builds the content without the draft in line: {e}");
                let build = slot.lock().ok().and_then(|mut b| b.take());
                build.map_or(Self::Failed, |b| Self::Ready(Box::new(b())))
            }
        }
    }

    /// The content, once its thread is done.
    ///
    /// # Errors
    /// `internal` when its build failed.
    fn ready(&mut self) -> ApiResult<&ContentHub> {
        if matches!(self, Self::Building(_)) {
            if let Self::Building(handle) = std::mem::replace(self, Self::Failed) {
                match handle.join() {
                    Ok(Some(hub)) => *self = Self::Ready(Box::new(hub)),
                    Ok(None) | Err(_) => {
                        tracing::warn!("Try it: the content without the draft failed");
                    }
                }
            }
        }
        match self {
            Self::Ready(hub) => Ok(hub),
            Self::Building(_) | Self::Failed => Err(ApiError::internal(WITHOUT_FAILED)),
        }
    }
}

/// One test character.
struct TrialSession {
    id: String,
    draft_id: String,
    content: TrialContent,
    setup: Setup,
    level: u32,
    /// The character at level 20 with every choice (lower levels are levelled down from it).
    top: Character,
    /// The states of levels 1..=20.
    states: Vec<CharacterState>,
    notes: Vec<Note>,
    /// The sweep of a class or subclass draft, for the edition it ran in.
    sweep: Option<(Ruleset, Vec<SweepIssue>)>,
    ladder: Vec<crate::homebrew_dto::HomebrewLadderRow>,
    /// The first level the draft is on the character.
    placed_at: Option<u32>,
}

struct Entry {
    session: Arc<Mutex<TrialSession>>,
    used: Instant,
}

/// The open trials (in memory only).
#[derive(Default)]
pub(crate) struct CreatorTrials {
    open: Mutex<HashMap<String, Entry>>,
}

impl std::fmt::Debug for CreatorTrials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let open = self.open.lock().map_or(0, |m| m.len());
        f.debug_struct("CreatorTrials")
            .field("open", &open)
            .finish()
    }
}

fn drop_idle(open: &mut HashMap<String, Entry>, now: Instant) {
    open.retain(|_, e| now.saturating_duration_since(e.used) < IDLE_LIMIT);
}

impl CreatorTrials {
    fn insert(&self, session: TrialSession) -> ApiResult<()> {
        let mut open = self.open.lock()?;
        let now = Instant::now();
        drop_idle(&mut open, now);
        while open.len() >= MAX_TRIALS {
            let Some(oldest) = open
                .iter()
                .min_by_key(|(_, e)| e.used)
                .map(|(id, _)| id.clone())
            else {
                break;
            };
            open.remove(&oldest);
        }
        open.insert(
            session.id.clone(),
            Entry {
                session: Arc::new(Mutex::new(session)),
                used: now,
            },
        );
        Ok(())
    }

    fn get(&self, id: &str) -> ApiResult<Arc<Mutex<TrialSession>>> {
        let mut open = self.open.lock()?;
        let now = Instant::now();
        drop_idle(&mut open, now);
        let entry = open
            .get_mut(id)
            .ok_or_else(|| ApiError::not_found(CLOSED_MESSAGE))?;
        entry.used = now;
        Ok(Arc::clone(&entry.session))
    }

    fn remove(&self, id: &str) -> ApiResult<bool> {
        let mut open = self.open.lock()?;
        drop_idle(&mut open, Instant::now());
        Ok(open.remove(id).is_some())
    }

    /// Make a trial look idle for `idle` (tests of the expiry). `false` for an unknown trial.
    fn age(&self, id: &str, idle: Duration) -> ApiResult<bool> {
        let mut open = self.open.lock()?;
        let Some(entry) = open.get_mut(id) else {
            return Ok(false);
        };
        entry.used = Instant::now()
            .checked_sub(idle)
            .ok_or_else(|| ApiError::internal("the clock can't go back that far"))?;
        Ok(true)
    }
}

fn new_trial_id() -> String {
    format!("trial-{}", uuid::Uuid::new_v4().simple())
}

fn scores_of(v: Option<&Vec<i32>>) -> ApiResult<Option<[i32; 6]>> {
    let Some(v) = v else { return Ok(None) };
    let scores: [i32; 6] = v
        .as_slice()
        .try_into()
        .map_err(|_| ApiError::invalid(SCORES_MESSAGE))?;
    if scores.iter().any(|s| !(SCORE_MIN..=SCORE_MAX).contains(s)) {
        return Err(ApiError::invalid(SCORES_MESSAGE));
    }
    Ok(Some(scores))
}

/// A level the slider can show (as the mock: out-of-range levels are clamped).
fn clamp_level(level: u32) -> u32 {
    level.clamp(1, MAX_LEVEL)
}

/// The edition a trial starts in: the asked one, else the draft's, else the pack's, else 2024.
fn edition_for(asked: Option<Ruleset>, content: &TrialContent) -> Ruleset {
    [
        asked,
        content.draft.root.ruleset,
        Some(content.pack_edition),
    ]
    .into_iter()
    .flatten()
    .find(|r| *r != Ruleset::Any)
    .unwrap_or(Ruleset::Dnd2024)
}

/// A hub over the loaded packs with `content` as the creator pack's content (its load position kept).
fn hub_with(
    hub: &ContentHub,
    pack: &CreatorPack,
    content: ContentFile,
    settings: &AppSettings,
) -> ContentHub {
    let parsed = ParsedPack::from_parts(
        pack.disk.manifest.clone(),
        vec![(pack.disk.content_path.clone(), content)],
    );
    let builder = hub
        .builder_with_pack(parsed.clone())
        .unwrap_or_else(|| hub.builder.as_ref().clone().add_parsed_pack(parsed));
    let store = builder.clone().build_reusing(Some(&hub.store));
    ContentHub::from_built(builder, store, Vec::new(), settings)
}

/// A class whose multiclass section has the id another loaded class's section has (a copy keeps it) gets one of its
/// own in the trial, or the engine would hold one of the two entries only and the second-class check would try the
/// other class.
fn own_multiclass_ids(
    mut elements: Vec<borealis_content::Element>,
    hub: &ContentHub,
) -> Vec<borealis_content::Element> {
    for el in &mut elements {
        let own = el.id.clone();
        let Some(mc) = el.multiclass.as_mut() else {
            continue;
        };
        let taken = |id: &str| {
            hub.store.iter().any(|s| {
                s.element.id != own
                    && (s.element.id == id
                        || s.element.multiclass.as_ref().is_some_and(|m| m.id == id))
            })
        };
        if taken(&mc.id) {
            let base = format!("{own}_MULTICLASS");
            mc.id = (1..)
                .map(|n| {
                    if n == 1 {
                        base.clone()
                    } else {
                        format!("{base}_{n}")
                    }
                })
                .find(|id| !taken(id))
                .unwrap_or(base);
        }
    }
    elements
}

/// The sweep targets of a class or subclass draft (the features it grants must show at their levels).
fn sweep_targets(draft: &TrialDraft, class_id: Option<&str>) -> Vec<SweepTarget> {
    let features = draft
        .root
        .rules
        .iter()
        .filter_map(|r| match r {
            Rule::Grant(g) if draft.ids.contains(&g.id) => {
                Some((g.id.clone(), g.level.unwrap_or(1)))
            }
            _ => None,
        })
        .collect();
    let casts = draft.root.spellcasting.is_some();
    let target = |class_id: String, subclass_id: Option<String>| SweepTarget {
        class_id,
        subclass_id,
        features,
        root_id: draft.root.id.clone(),
        casts,
    };
    match (draft.placement, class_id) {
        (Placement::Class, _) => vec![target(draft.root.id.clone(), None)],
        (Placement::Subclass, Some(class)) => {
            vec![target(class.to_string(), Some(draft.root.id.clone()))]
        }
        _ => Vec::new(),
    }
}

/// A sweep finding in the creator's plain words (spec 8.2, WB-07's sentences), blamed on its element.
fn plain_issue(f: &SweepFinding, draft: &TrialDraft, hub: &ContentHub) -> SweepIssue {
    let ty = f
        .element_id
        .as_deref()
        .and_then(|id| hub.store.element(id))
        .map_or(draft.root.ty.as_str(), |e| e.ty.as_str());
    SweepIssue {
        level: f.level,
        element_id: f.element_id.clone(),
        message: sweep_message(f, ty, &*hub.store),
    }
}

impl TrialSession {
    fn ctx<'a>(&'a self, settings: &'a AppSettings, app: &'a AppState) -> Ctx<'a> {
        Ctx {
            hub: &self.content.hub,
            settings,
            app,
        }
    }

    /// The level shown (the chain may be shorter when the engine stopped below 20).
    fn current(&self) -> &CharacterState {
        let i = usize::try_from(self.level.saturating_sub(1))
            .unwrap_or(0)
            .min(self.states.len().saturating_sub(1));
        &self.states[i]
    }

    /// Build the character from the setup (a new trial, another edition, or a newer draft).
    fn rebuild(&mut self, settings: &AppSettings, app: &AppState) {
        let mut timer = LoadTimer::named("try it");
        let built = build::build(&self.ctx(settings, app), &self.content.draft, &self.setup);
        timer.step("character", "");
        self.top = built.character;
        self.notes = built.notes;
        self.refresh();
    }

    /// The level chain, the sweep (once per edition) and the ladder, after the top state changed.
    fn refresh(&mut self) {
        let mut timer = LoadTimer::named("try it");
        let hub = &self.content.hub;
        self.states = build::levels(hub, &self.top.build);
        crate::homebrew_trial_place::trim_added_spell(hub, &mut self.states, &self.content.draft);
        timer.step("levels", &self.states.len().to_string());
        let edition = self.setup.edition;
        if self.sweep.as_ref().is_none_or(|(r, _)| *r != edition) {
            let class_id = self
                .content
                .draft
                .parents
                .iter()
                .find(|id| hub.engine.has_element(edition, id))
                .map(String::as_str);
            let targets = sweep_targets(&self.content.draft, class_id);
            let issues = if targets.is_empty() {
                Vec::new()
            } else {
                sweep_engine(&hub.engine, edition, &self.content.draft.ids, &targets)
                    .iter()
                    .map(|f| plain_issue(f, &self.content.draft, hub))
                    .collect()
            };
            self.sweep = Some((edition, issues));
            timer.step("sweep", "");
        }
        let sweep = self.sweep.as_ref().map_or(&[][..], |(_, s)| s.as_slice());
        let (ladder, placed_at) =
            view::ladder(hub, &self.states, &self.content.draft, &self.notes, sweep);
        timer.step("ladder", "");
        self.ladder = ladder;
        self.placed_at = placed_at;
    }

    /// # Errors
    /// `internal` when the content without the draft could not be built.
    fn view(&mut self, settings: &AppSettings) -> ApiResult<HomebrewTrialView> {
        self.content.without.ready()?;
        let Without::Ready(without) = &self.content.without else {
            return Err(ApiError::internal(WITHOUT_FAILED));
        };
        let state = self.current();
        let draft = &self.content.draft;
        let snapshot = view::snapshot(&self.content.hub, settings, &self.id, state, draft);
        let without = view::snapshot(without.as_ref(), settings, &self.id, state, draft);
        let changed = view::changed(&snapshot, &without);
        Ok(HomebrewTrialView {
            trial_id: self.id.clone(),
            snapshot: Box::new(snapshot),
            problems: self
                .ladder
                .iter()
                .flat_map(|r| r.problems.iter().cloned())
                .collect(),
            ladder: self.ladder.clone(),
            changed,
        })
    }

    /// A choice on the trial character (spec 9 "Choices"), made on the level-20 state so higher levels keep theirs:
    /// `Some(id)` picks, `None` clears. A slot that holds the draft is fixed. Nothing changes on an error.
    fn choose(
        &mut self,
        settings: &AppSettings,
        app: &AppState,
        slot_id: &str,
        element_id: Option<&str>,
    ) -> ApiResult<()> {
        let key = decode_slot(slot_id)?;
        let (character, notes) = {
            let ctx = self.ctx(settings, app);
            let draft = &self.content.draft;
            let fixed =
                ctx.hub.derive(&self.top.build).slots.iter().any(|s| {
                    s.key == key && s.chosen.as_ref().is_some_and(|c| draft.is_draft(&c.id))
                });
            if fixed {
                return Err(ApiError::invalid(crate::builder::LOCKED_SLOT));
            }
            let mut d = Draft {
                character: self.top.clone(),
                rolls: Vec::new(),
                warnings: Vec::new(),
            };
            match element_id {
                Some(id) => select_in(&ctx, &mut d, &key, id)?,
                None => clear_in(&ctx, &mut d, &key)?,
            }
            let mut notes = self.notes.clone();
            crate::homebrew_trial_place::place(
                &ctx,
                &mut d,
                draft,
                self.setup.as_second_class,
                &mut notes,
            );
            (d.character, notes)
        };
        self.top = character;
        self.notes = notes;
        self.refresh();
        Ok(())
    }

    /// New ability scores on the built character (its choices kept).
    fn set_scores(&mut self, scores: [i32; 6]) -> ApiResult<()> {
        let out = commands::set_base_scores(&self.content.hub.engine, &self.top.build, scores)?;
        self.top.build = out.state;
        self.setup.scores = Some(scores);
        self.refresh();
        Ok(())
    }
}

impl AppState {
    /// The draft's content for a trial: its elements with the ids they would get now, loaded into a private hub.
    ///
    /// # Errors
    /// As `hb_pack`; `not_found` for an unknown draft; `invalid_argument` (with the problems in `details`) for a
    /// form that can't make its elements.
    fn hb_trial_content(&self, draft_id: &str) -> ApiResult<(TrialContent, String)> {
        let file = self.hb_drafts.get(&self.paths.data, draft_id)?;
        let pack = self.hb_pack(&file.pack_id)?;
        let hub = self.hub()?;
        let settings = self.settings()?;
        let codec = CatalogBlockCodec;
        let cx = FormContext::from_store(&hub.store, &codec);
        let mut ids = self.hb_ids(&hub, &pack)?;
        let form = assign_ids(&file.form, &mut ids);
        // as a save writes them: a copy's grants name its own copied features, not the originals (spec 11)
        let written = write_copy(&form, &file.copy, &cx, &mut ids).map_err(|ps| {
            let problems: Vec<HomebrewProblem> = ps
                .into_iter()
                .map(|p| HomebrewProblem {
                    severity: "error".into(),
                    message: p.message,
                    path: (!p.path.is_empty()).then_some(p.path),
                    element_id: file.element_id.clone(),
                    code: "form".into(),
                })
                .collect();
            let mut e = ApiError::invalid("Fix the problems marked in red, then try it again.");
            e.details = serde_json::to_value(&problems).ok();
            e
        })?;
        let elements = written.into_iter().map(|w| w.element).collect();
        let elements = own_multiclass_ids(elements, &hub);
        let trial_hub = hub_with(
            &hub,
            &pack,
            with_elements(&pack.disk.content, &elements),
            &settings,
        );
        let draft = TrialDraft::new(&elements, &trial_hub.store)
            .ok_or_else(|| ApiError::invalid("This form makes no element to try."))?;
        let without = {
            let (hub, settings, base_ids) =
                (Arc::clone(&hub), settings.clone(), file.base_ids.clone());
            let pack = pack.clone();
            Without::start(move || {
                if base_ids.is_empty() {
                    hub.with_settings(&settings)
                } else {
                    let base: HashSet<&str> = base_ids.iter().map(String::as_str).collect();
                    let (content, _) = without_elements(&pack.disk.content, &base);
                    hub_with(&hub, &pack, content, &settings)
                }
            })
        };
        let content = TrialContent {
            hub: trial_hub,
            without,
            draft,
            updated_at: file.updated_at.clone(),
            pack_edition: pack_edition(&pack.disk.manifest, &pack.disk.content),
        };
        Ok((content, file.draft_id))
    }

    /// Try it (spec 9): a test character that takes the draft. An empty `setup` takes the defaults per kind: the
    /// edition of the draft (else its pack), the SRD picks of that edition, the standard array by the class's main
    /// abilities and the level the draft is first on the character.
    ///
    /// # Errors
    /// As `hb_trial_content`; `invalid_argument` for bad ability scores.
    pub fn homebrew_trial_start(
        &self,
        draft_id: &str,
        setup: &HomebrewTrialSetup,
    ) -> ApiResult<HomebrewTrialView> {
        let scores = scores_of(setup.ability_scores.as_ref())?;
        let mut timer = LoadTimer::named("try it start");
        let (content, draft_id) = self.hb_trial_content(draft_id)?;
        timer.step("content", "");
        let settings = self.settings()?;
        let resolved = Setup {
            edition: edition_for(setup.edition, &content),
            scores,
            as_second_class: setup.as_second_class.unwrap_or(false),
            class_id: setup.class_id.clone(),
            subclass_id: setup.subclass_id.clone(),
            species_id: setup.species_id.clone(),
            background_id: setup.background_id.clone(),
        };
        let mut session = TrialSession {
            id: new_trial_id(),
            draft_id,
            top: Character::with_ruleset(build::TEST_NAME, resolved.edition),
            content,
            setup: resolved,
            level: 1,
            states: Vec::new(),
            notes: Vec::new(),
            sweep: None,
            ladder: Vec::new(),
            placed_at: None,
        };
        session.rebuild(&settings, self);
        session.level = match setup.level {
            Some(level) => clamp_level(level),
            None => session.placed_at.unwrap_or(1),
        };
        timer.step("character", "");
        let view = session.view(&settings)?;
        timer.step("view", "");
        self.hb_trials.insert(session)?;
        timer.total("");
        Ok(view)
    }

    /// Change the trial character (spec 9): the level, the setup (another edition or "Try as a second class" starts
    /// from that setup's picks; new ability scores keep the choices) or one choice (`slotId` with `elementId`, `null`
    /// clears). A draft edited since is loaded again first. Nothing changes on an error.
    ///
    /// # Errors
    /// `not_found` for an unknown or expired trial (and an unknown slot); `invalid_argument` for bad ability scores
    /// or a slot that holds the draft; the engine's refusal of a choice.
    pub fn homebrew_trial_update(
        &self,
        trial_id: &str,
        change: &HomebrewTrialChange,
    ) -> ApiResult<HomebrewTrialView> {
        let entry = self.hb_trials.get(trial_id)?;
        let mut t = entry.lock()?;
        let settings = self.settings()?;
        let setup = change.setup.clone().unwrap_or_default();
        let scores = scores_of(setup.ability_scores.as_ref())?;
        let newer = self
            .hb_drafts
            .get(&self.paths.data, &t.draft_id)
            .is_ok_and(|f| f.updated_at != t.content.updated_at);
        if newer {
            let (content, _) = self.hb_trial_content(&t.draft_id)?;
            t.content = content;
            t.sweep = None;
        }
        let next = Setup {
            edition: setup
                .edition
                .filter(|r| *r != Ruleset::Any)
                .unwrap_or(t.setup.edition),
            scores: scores.or(t.setup.scores),
            as_second_class: setup.as_second_class.unwrap_or(t.setup.as_second_class),
            class_id: setup.class_id.clone().or_else(|| t.setup.class_id.clone()),
            subclass_id: setup
                .subclass_id
                .clone()
                .or_else(|| t.setup.subclass_id.clone()),
            species_id: setup
                .species_id
                .clone()
                .or_else(|| t.setup.species_id.clone()),
            background_id: setup
                .background_id
                .clone()
                .or_else(|| t.setup.background_id.clone()),
        };
        let starts_over = Setup {
            scores: t.setup.scores,
            ..next.clone()
        } != t.setup;
        if starts_over || newer {
            // Another edition starts from its own picks and its own standard array.
            let scores = if next.edition == t.setup.edition {
                next.scores
            } else {
                scores
            };
            t.setup = Setup { scores, ..next };
            t.rebuild(&settings, self);
        } else if let Some(scores) = scores.filter(|s| Some(*s) != t.setup.scores) {
            if t.setup.as_second_class {
                // the multiclass check depends on the scores: the second class is taken again
                t.setup.scores = Some(scores);
                t.rebuild(&settings, self);
            } else {
                t.set_scores(scores)?;
            }
        }
        if let Some(slot_id) = change.slot_id.as_deref() {
            let element = change.element_id.as_ref().ok_or_else(|| {
                ApiError::invalid("Say what to choose, or null to clear the choice.")
            })?;
            t.choose(&settings, self, slot_id, element.as_deref())?;
        }
        if let Some(level) = change.level.or(setup.level) {
            t.level = clamp_level(level);
        }
        t.view(&settings)
    }

    /// The options of one of the trial character's choices (spec 9 "Choices"): the rows `get_slot_options` answers.
    ///
    /// # Errors
    /// `not_found` for an unknown or expired trial or slot; `invalid_argument` for a malformed slot id.
    pub fn homebrew_trial_options(
        &self,
        trial_id: &str,
        slot_id: &str,
        query: Option<&str>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> ApiResult<Page<SlotOption>> {
        let entry = self.hb_trials.get(trial_id)?;
        let t = entry.lock()?;
        let settings = self.settings()?;
        let ctx = t.ctx(&settings, self);
        slot_options_page(&ctx, t.current(), slot_id, query, limit, offset)
    }

    /// Drop the trial character (Close, Start over).
    ///
    /// # Errors
    /// `not_found` for an unknown or expired trial.
    pub fn homebrew_trial_end(&self, trial_id: &str) -> ApiResult<()> {
        if self.hb_trials.remove(trial_id)? {
            Ok(())
        } else {
            Err(ApiError::not_found(CLOSED_MESSAGE))
        }
    }

    /// Tests of the idle expiry only: make a trial look untouched for `idle`. `false` for an unknown trial.
    ///
    /// # Errors
    /// `internal` when a lock is poisoned.
    #[doc(hidden)]
    pub fn homebrew_trial_idle_for(&self, trial_id: &str, idle: Duration) -> ApiResult<bool> {
        self.hb_trials.age(trial_id, idle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_are_six_numbers_from_1_to_30() {
        assert_eq!(scores_of(None).unwrap(), None);
        assert_eq!(
            scores_of(Some(&vec![15, 14, 13, 12, 10, 8])).unwrap(),
            Some([15, 14, 13, 12, 10, 8])
        );
        for bad in [
            vec![15, 14],
            vec![0, 14, 13, 12, 10, 8],
            vec![31, 14, 13, 12, 10, 8],
        ] {
            assert!(scores_of(Some(&bad)).is_err());
        }
    }

    #[test]
    fn levels_are_clamped_to_the_ladder() {
        assert_eq!(clamp_level(0), 1);
        assert_eq!(clamp_level(7), 7);
        assert_eq!(clamp_level(99), MAX_LEVEL);
    }
}
