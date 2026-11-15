//! Spec 18.5 performance budgets of the homebrew creator (WB-27a), measured through the real `AppState` on a library of
//! about 20,000 elements (the bundled packs plus synthetic book packs shaped like a 5etools import) with a 20-level
//! class draft of 40 features (a deep copy of the SRD Monk plus 18 copied features):
//!
//! | what | budget (desktop) |
//! |---|---|
//! | `homebrew_update_draft` (an edit of the class) | p95 < 150 ms |
//! | `homebrew_trial_update` (Try it: a level or setup change) | p95 < 300 ms |
//! | `homebrew_save_draft` (write the pack, reload it) | < 2 s each |
//!
//! Budgets mean something only in an optimised build, so the measuring test is ignored by default and asserts only in
//! release (a debug run prints the times and says so):
//!
//! `cargo test --release -p borealis --test homebrew_budgets -- --ignored --nocapture`
//!
//! It is made reliable by measuring many calls after warm-up calls (p95 of 40 edits and 30 trial updates; each save is
//! measured once, three times) on a quiet machine; the times are printed so a slow run shows how close it came. The
//! phone budget (save + reload < 4 s) is measured on devices (WB-27b). `the_budget_scenario_runs` runs the same steps
//! on the bundled library alone, in every `cargo test`, so the scenario keeps working.

#[path = "support/synthetic_library.rs"]
mod synthetic_library;

use borealis_authoring::form::FormState;
use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::{
    HomebrewDraftView, HomebrewPackRequest, HomebrewTrialChange, HomebrewTrialSetup,
};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::json;
use std::time::{Duration, Instant};

const UPDATE_P95: Duration = Duration::from_millis(150);
const TRIAL_P95: Duration = Duration::from_millis(300);
const SAVE_MAX: Duration = Duration::from_secs(2);
/// Elements of the bundled packs (about).
const BUNDLED: usize = 4_000;
/// Elements of the measured library (bundled + synthetic).
const LIBRARY: usize = 20_000;
/// Features of the measured class.
const FEATURES: usize = 40;
const EDITS: usize = 40;
const TRIAL_UPDATES: usize = 30;
const SAVES: usize = 3;
const WARM_UP: usize = 3;

/// The class copied (22 features) and the features copied into it to reach [`FEATURES`].
const CLASS: &str = "ID_2024_CLASS_MONK";
const EXTRA_FEATURES: &[&str] = &[
    "ID_2024_CLASS_FEATURE_FIGHTER_EXTRA_ATTACK",
    "ID_2024_CLASS_FEATURE_FIGHTER_SECOND_WIND",
    "ID_2024_CLASS_FEATURE_FIGHTER_ACTION_SURGE",
    "ID_2024_CLASS_FEATURE_FIGHTER_INDOMITABLE",
    "ID_2024_CLASS_FEATURE_BARBARIAN_RAGE",
    "ID_2024_CLASS_FEATURE_BARBARIAN_RECKLESS_ATTACK",
    "ID_2024_CLASS_FEATURE_BARBARIAN_DANGER_SENSE",
    "ID_2024_CLASS_FEATURE_BARBARIAN_FERAL_INSTINCT",
    "ID_2024_CLASS_FEATURE_ROGUE_SNEAK_ATTACK",
    "ID_2024_CLASS_FEATURE_ROGUE_CUNNING_ACTION",
    "ID_2024_CLASS_FEATURE_ROGUE_UNCANNY_DODGE",
    "ID_2024_CLASS_FEATURE_ROGUE_EVASION",
    "ID_2024_CLASS_FEATURE_ROGUE_RELIABLE_TALENT",
    "ID_2024_CLASS_FEATURE_PALADIN_LAY_ON_HANDS",
    "ID_2024_CLASS_FEATURE_PALADIN_AURA_OF_PROTECTION",
    "ID_2024_CLASS_FEATURE_RANGER_FAVORED_ENEMY",
    "ID_2024_CLASS_FEATURE_BARD_JACK_OF_ALL_TRADES",
    "ID_2024_CLASS_FEATURE_WARLOCK_MAGICAL_CUNNING",
    "ID_2024_CLASS_FEATURE_SORCERER_FONT_OF_MAGIC",
    "ID_2024_CLASS_FEATURE_WIZARD_RITUAL_ADEPT",
];

struct NoEvents;
impl EventSink for NoEvents {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

struct Scenario {
    _data: tempfile::TempDir,
    state: AppState,
    pack: String,
    elements: usize,
}

/// The app on a data folder whose library holds synthetic book packs until the store has `library` elements (0: the
/// bundled packs only), with a 2024 homebrew pack.
fn scenario(library: usize) -> Scenario {
    let data = tempfile::tempdir().unwrap();
    let paths = AppPaths {
        content_root: synthetic_library::content_root(),
        data: data.path().to_path_buf(),
    };
    if library > 0 {
        // the bundled packs hold about 4,000 elements; each synthetic pack copies one bundled SRD pack
        let mut packs = 4;
        while synthetic_library::write_library(&paths.library(), packs, 1) + BUNDLED < library {
            packs += 4;
        }
    }
    let state = AppState::new(paths).unwrap();
    let elements = state.hub().unwrap().store.len();
    let pack = state
        .homebrew_create_pack(
            &HomebrewPackRequest {
                name: "Budget Isles".into(),
                code: "BI".into(),
                edition: Ruleset::Dnd2024,
                author: None,
                description: None,
                license: None,
            },
            &NoEvents,
        )
        .unwrap()
        .id;
    Scenario {
        _data: data,
        state,
        pack,
        elements,
    }
}

/// A deep copy of the SRD Monk with copied features added until it has [`FEATURES`].
fn class_draft(s: &Scenario) -> HomebrewDraftView {
    let mut v = s
        .state
        .homebrew_new_draft(&s.pack, "class", Some(CLASS), None, Some(true))
        .unwrap();
    let features = |v: &HomebrewDraftView| {
        v.form
            .children
            .iter()
            .filter(|c| c.kind == "classFeature")
            .count()
    };
    for id in EXTRA_FEATURES {
        if features(&v) >= FEATURES {
            break;
        }
        v = s
            .state
            .homebrew_new_draft(&s.pack, "classFeature", Some(id), Some(&v.draft_id), None)
            .unwrap();
    }
    assert!(
        features(&v) >= FEATURES,
        "the class has {} features",
        features(&v)
    );
    v
}

fn edited(form: &FormState, i: usize) -> FormState {
    form.with_field("name", json!(format!("Monk of the Budget {i}")))
}

fn p95(mut times: Vec<Duration>) -> Duration {
    times.sort_unstable();
    let at = (times.len() * 95).div_ceil(100).max(1) - 1;
    times[at.min(times.len() - 1)]
}

fn timed<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let started = Instant::now();
    let out = f();
    (out, started.elapsed())
}

struct Measured {
    /// Try it's start (its private engine with the draft): printed, no budget.
    trial_start: Duration,
    update: Duration,
    trial: Duration,
    saves: Vec<Duration>,
}

/// Run the scenario: `edits` draft updates, `trial_updates` Try it updates, `saves` edit + save rounds; the warm-up
/// calls are not counted.
fn measure(s: &Scenario, edits: usize, trial_updates: usize, saves: usize) -> Measured {
    let draft = class_draft(s);
    let mut form = draft.form.clone();
    let mut updates = Vec::new();
    for i in 0..WARM_UP + edits {
        form = edited(&form, i);
        let (view, t) = timed(|| {
            s.state
                .homebrew_update_draft(&draft.draft_id, &form)
                .unwrap()
        });
        assert_eq!(view.form.fields["name"], form.fields["name"]);
        form = view.form;
        if i >= WARM_UP {
            updates.push(t);
        }
    }

    let (trial, trial_start) = timed(|| {
        s.state
            .homebrew_trial_start(&draft.draft_id, &HomebrewTrialSetup::default())
            .unwrap()
    });
    assert_eq!(trial.ladder.len(), 20, "the class ladder");
    let mut trials = Vec::new();
    for i in 0..WARM_UP + trial_updates {
        let change = if i % 5 == 4 {
            HomebrewTrialChange {
                setup: Some(HomebrewTrialSetup {
                    ability_scores: Some(vec![15, 14, 13, 12, 10, 8]),
                    ..HomebrewTrialSetup::default()
                }),
                ..HomebrewTrialChange::default()
            }
        } else {
            HomebrewTrialChange {
                level: Some(u32::try_from(1 + (i * 7) % 20).unwrap()),
                ..HomebrewTrialChange::default()
            }
        };
        let (view, t) = timed(|| {
            s.state
                .homebrew_trial_update(&trial.trial_id, &change)
                .unwrap()
        });
        assert_eq!(view.ladder.len(), 20);
        if i >= WARM_UP {
            trials.push(t);
        }
    }
    s.state.homebrew_trial_end(&trial.trial_id).unwrap();

    let mut save_times = Vec::new();
    let mut draft_id = draft.draft_id.clone();
    for i in 0..saves {
        if i > 0 {
            let v = s
                .state
                .homebrew_open_draft(&s.pack, Some(&saved_id(s)), None)
                .unwrap();
            let f = edited(&v.form, 100 + i);
            s.state.homebrew_update_draft(&v.draft_id, &f).unwrap();
            draft_id = v.draft_id;
        }
        let (saved, t) = timed(|| s.state.homebrew_save_draft(&draft_id, &NoEvents).unwrap());
        assert!(
            saved.element_ids.len() > FEATURES,
            "the class and its features are saved"
        );
        save_times.push(t);
    }
    Measured {
        trial_start,
        update: p95(updates),
        trial: p95(trials),
        saves: save_times,
    }
}

/// The saved class's id.
fn saved_id(s: &Scenario) -> String {
    let hub = s.state.hub().unwrap();
    let id = hub
        .store
        .by_type("Class")
        .find(|e| e.element.name.starts_with("Monk of the Budget"))
        .map(|e| e.element.id.clone());
    id.expect("the saved class is loaded")
}

#[test]
fn the_budget_scenario_runs() {
    let s = scenario(0);
    let m = measure(&s, 2, 2, 2);
    eprintln!(
        "bundled library ({} elements): update p95 {:?}, trial start {:?}, trial p95 {:?}, saves {:?}",
        s.elements, m.update, m.trial_start, m.trial, m.saves
    );
}

#[test]
#[ignore = "benchmark: cargo test --release -p borealis --test homebrew_budgets -- --ignored --nocapture"]
fn the_creator_meets_its_desktop_budgets() {
    let s = scenario(LIBRARY);
    assert!(
        s.elements >= LIBRARY,
        "the library has {} elements",
        s.elements
    );
    let m = measure(&s, EDITS, TRIAL_UPDATES, SAVES);
    eprintln!(
        "library {} elements, class of {FEATURES} features:\n  update p95 {:?} (budget {UPDATE_P95:?})\n  trial \
         update p95 {:?} (budget {TRIAL_P95:?}; start {:?})\n  save + reload {:?} (budget {SAVE_MAX:?} each)",
        s.elements, m.update, m.trial, m.trial_start, m.saves
    );
    if cfg!(debug_assertions) {
        eprintln!("a debug build: the budgets are asserted in release builds only");
        return;
    }
    assert!(m.update < UPDATE_P95, "update p95 {:?}", m.update);
    assert!(m.trial < TRIAL_P95, "trial update p95 {:?}", m.trial);
    for t in &m.saves {
        assert!(*t < SAVE_MAX, "save + reload {t:?}");
    }
}
