//! What the homebrew creator's saves cost on a large imported library (item `Isq-Y`, spec 18.5 phone budget): time and
//! heap of New pack, a spell save and a deep-copied class save through the real `AppState`, on a library folder the
//! device suite installs (`src-tauri/tests/device_library.rs` writes one). Memory is the process's resident size
//! (sampled with `ps`): after each step and its peak during it, next to the size of one content snapshot (builder,
//! store and engine), so a step that holds two snapshots at once shows as a peak of two.
//!
//! Not a check (the numbers depend on the library and the machine), so it is `#[ignore]` and runs on demand:
//!
//! ```sh
//! BOREALIS_PROFILE_LIBRARY=../target/device-library-5e \
//! cargo test -p borealis --test library_profile -- --ignored --nocapture
//! ```
//!
//! `BOREALIS_PROFILE_LIBRARY` is a data folder with `library/` and `content-prefs.json`; it is copied to a temporary
//! folder first, so the profile never changes it.

use borealis_authoring::form::FormState;
use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::{HomebrewPackRequest, HomebrewTrialChange, HomebrewTrialSetup};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

/// The process's resident memory, sampled by a background thread (`ps`, every [`SAMPLE`]): the current value and the
/// peak since the last [`Rss::reset`].
struct Rss {
    now: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
}

/// How often the resident memory is sampled.
const SAMPLE: Duration = Duration::from_millis(20);

fn rss_kb() -> usize {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok();
    out.and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

impl Rss {
    fn start() -> Self {
        let now = Arc::new(AtomicUsize::new(rss_kb()));
        let peak = Arc::new(AtomicUsize::new(now.load(Ordering::Relaxed)));
        let (n, p) = (Arc::clone(&now), Arc::clone(&peak));
        std::thread::spawn(move || loop {
            let kb = rss_kb();
            n.store(kb, Ordering::Relaxed);
            p.fetch_max(kb, Ordering::Relaxed);
            std::thread::sleep(SAMPLE);
        });
        Self { now, peak }
    }

    /// Sample now, and start a new peak there.
    fn reset(&self) -> usize {
        let kb = rss_kb();
        self.now.store(kb, Ordering::Relaxed);
        self.peak.store(kb, Ordering::Relaxed);
        kb
    }

    fn sample(&self) -> (usize, usize) {
        let kb = rss_kb();
        self.peak.fetch_max(kb, Ordering::Relaxed);
        (kb, self.peak.load(Ordering::Relaxed))
    }
}

static RSS: OnceLock<Rss> = OnceLock::new();

const MB: f64 = 1024.0;
/// Longest wait for the background search index.
const SEARCH_WAIT: Duration = Duration::from_secs(600);
const SAVES: usize = 3;

#[allow(clippy::cast_precision_loss)]
fn mb(kb: usize) -> f64 {
    kb as f64 / MB
}

struct NoEvents;
impl EventSink for NoEvents {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

/// Time and memory of one step: its time, the resident memory after it, the peak during it and how far above the start.
fn step<T>(what: &str, f: impl FnOnce() -> T) -> T {
    let rss = RSS.get_or_init(Rss::start);
    let before = rss.reset();
    let started = Instant::now();
    let out = f();
    let took = started.elapsed();
    let (after, peak) = rss.sample();
    eprintln!(
        "{what:<34} {:>8} ms   rss {:>7.1} MB   peak {:>7.1} MB (+{:.1} MB over the start)",
        took.as_millis(),
        mb(after),
        mb(peak),
        mb(peak.saturating_sub(before)),
    );
    out
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

fn content_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content")
}

fn wait_search(state: &AppState) {
    assert!(
        state.search.wait_idle(SEARCH_WAIT),
        "the search index did not finish"
    );
}

fn renamed(form: &FormState, name: &str) -> FormState {
    form.with_field("name", json!(name))
}

#[test]
#[ignore = "profile: BOREALIS_PROFILE_LIBRARY=<data folder> cargo test -p borealis --test library_profile -- --ignored --nocapture"]
fn creator_saves_on_a_large_library() {
    let Some(library) = std::env::var_os("BOREALIS_PROFILE_LIBRARY").map(PathBuf::from) else {
        eprintln!("BOREALIS_PROFILE_LIBRARY is not set: nothing to profile");
        return;
    };
    // the app's own step lines (`content load: ...`, `homebrew save: ...`, `search: ...`) go to stderr with the profile's
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_writer(std::io::stderr)
        .without_time()
        .with_target(false)
        .try_init();
    let data = tempfile::tempdir().unwrap();
    copy_dir(&library.join("library"), &data.path().join("library"));
    std::fs::copy(
        library.join("content-prefs.json"),
        data.path().join("content-prefs.json"),
    )
    .unwrap();
    let paths = AppPaths {
        content_root: content_root(),
        data: data.path().to_path_buf(),
    };
    eprintln!(
        "{} (debug build: {})",
        library.display(),
        cfg!(debug_assertions)
    );
    // a first start writes the content cache; the profiled start reads it, as every later start of the app does
    step("first start (no cache)", || {
        let state = AppState::new(paths.clone()).unwrap();
        state.hub().unwrap().wait_for_cache_write();
    });
    let state = step("start (cache)", || AppState::new(paths.clone()).unwrap());
    let elements = state.hub().unwrap().store.len();
    eprintln!("{elements} elements");
    {
        // one content snapshot: the builder the hub keeps, the store built from it, the engine over the store
        let hub = state.hub().unwrap();
        let settings = state.settings().unwrap().engine_settings();
        let builder = step("  one builder (a copy)", || hub.builder.as_ref().clone());
        let store = step("  one store (built)", || builder.clone().build());
        let engine = step("  one engine", || {
            borealis_engine::Engine::with_settings(&store, settings)
        });
        drop((engine, store, builder));
    }
    step("search index (startup)", || {
        state.start_search_index().unwrap();
        wait_search(&state);
    });
    let pack = step("New pack", || {
        state
            .homebrew_create_pack(
                &HomebrewPackRequest {
                    name: "Ember Isles".into(),
                    code: "EI".into(),
                    edition: Ruleset::Dnd2024,
                    author: None,
                    description: None,
                    license: None,
                },
                &NoEvents,
            )
            .unwrap()
            .id
    });
    step("  search re-sync after New pack", || wait_search(&state));

    let spell = state
        .homebrew_new_draft(&pack, "spell", Some("ID_2024_SPELL_BLESS"), None, None)
        .unwrap();
    let mut draft = spell.draft_id.clone();
    let mut form = spell.form;
    for i in 1..=SAVES {
        state
            .homebrew_update_draft(&draft, &renamed(&form, &format!("Ember Lance {i}")))
            .unwrap();
        let saved = step(&format!("spell save {i}"), || {
            state.homebrew_save_draft(&draft, &NoEvents).unwrap()
        });
        step("  search re-sync", || wait_search(&state));
        let reopened = step("  open the saved spell", || {
            state
                .homebrew_open_draft(&pack, saved.element_ids.first().map(String::as_str), None)
                .unwrap()
        });
        draft = reopened.draft_id;
        form = reopened.form;
        if i == 1 {
            // flow 1's Try it: the spell on a test character
            let trial = step("Try it (start)", || {
                state
                    .homebrew_trial_start(&draft, &HomebrewTrialSetup::default())
                    .unwrap()
            });
            step("  Try it (level 3)", || {
                state
                    .homebrew_trial_update(
                        &trial.trial_id,
                        &HomebrewTrialChange {
                            level: Some(3),
                            ..HomebrewTrialChange::default()
                        },
                    )
                    .unwrap()
            });
            state.homebrew_trial_end(&trial.trial_id).unwrap();
        }
    }

    let class = state
        .homebrew_new_draft(
            &pack,
            "class",
            Some("ID_2024_CLASS_WIZARD"),
            None,
            Some(true),
        )
        .unwrap();
    let mut draft = class.draft_id.clone();
    let mut form = class.form;
    for i in 1..=SAVES {
        state
            .homebrew_update_draft(&draft, &renamed(&form, &format!("Ember Wizard {i}")))
            .unwrap();
        let saved = step(&format!("deep-copied Wizard save {i}"), || {
            state.homebrew_save_draft(&draft, &NoEvents).unwrap()
        });
        step("  search re-sync", || wait_search(&state));
        let reopened = state
            .homebrew_open_draft(&pack, saved.element_ids.first().map(String::as_str), None)
            .unwrap();
        draft = reopened.draft_id;
        form = reopened.form;
    }
}
