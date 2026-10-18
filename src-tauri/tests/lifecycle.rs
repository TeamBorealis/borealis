//! IM4 acceptance stage 4 (specs/acceptance-imports-1.md lines 69-77): characters survive
//! restart, re-import, a pack update, a failed update and pack removal, per source.
//!
//! Sources: `a-user` (the committed invented A-user pack, always), `inv-folder` / `inv-index` /
//! `inv-5e` (invented v1 -> v2 pairs under `tests/fixtures/lifecycle/`, always; the index path
//! is served by a fake fetcher, det-src-3 style), `a-core` (`BOREALIS_LIVE_INDEX_URL`) and
//! `f-full` (`BOREALIS_5ETOOLS_DIR`), each skipping when its variable is unset.
//! Sample: every class of the source at levels 5 and 11 (the builder's Fighter and Wizard when
//! the source has no class) plus characters holding the source's origins and feats (all of them
//! for the invented packs, `BOREALIS_LIFECYCLE_PER_TYPE` per type for the corpora, default 1).
//! Output: gate `OUTCOME` lines `acceptance/<source>/s4/<check>` and
//! `METRIC\t<source>\ts4\t*\t<metric>\t<n>`; `BOREALIS_LIFECYCLE_STRICT=1` fails on a failed check.
#![allow(dead_code)]

#[path = "support/app.rs"]
mod app;
#[path = "support/files.rs"]
mod files;
#[path = "support/lifecycle.rs"]
mod lifecycle;
#[path = "support/lifecycle_sample.rs"]
mod lifecycle_sample;
#[path = "../../tools/regression-gate/outcome.rs"]
mod outcome;
#[path = "support/plan.rs"]
mod plan;
#[path = "support/prereq.rs"]
mod prereq;

use borealis_lib::content_dto::FiveToolsImportRequest;
use borealis_lib::content_index::{Fetcher, HttpsFetcher};
use borealis_lib::content_packs::NullSink;
use borealis_lib::content_preview::PreviewImportRequest;
use borealis_lib::picked_paths::PickPurpose;
use borealis_lib::state::AppState;
use lifecycle::{
    element_changes, elements, metric, opened, removal_checks, same, up_to_date, update,
    update_checks, Changes, Checks, FakeIndex,
};
use lifecycle_sample::{build, measure, ours, user_packs, Measure, Ours};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lifecycle")
}

/// Declared v1 -> v2 changes of the invented Aurora pair (`aurora-v1` / `aurora-v2`).
const AURORA_CHANGES: Changes = Changes {
    removed: &[
        "ID_LANTERNFALL_CLASS_ASHCALLER",
        "ID_LANTERNFALL_ARCHETYPE_OATH_OF_ASH",
        "ID_LANTERNFALL_SUB_RACE_ASH_WICKLING",
        "ID_LANTERNFALL_BACKGROUND_CHANDLER",
        "ID_LANTERNFALL_FEAT_SPARK_CATCHER",
    ],
    renamed: &[
        // Aurora has no rename declaration: a new id is a removal for a saved character
        (
            "ID_LANTERNFALL_FEAT_OLD_FLAME",
            "ID_LANTERNFALL_FEAT_NEW_FLAME",
            false,
        ),
        // v2 writes the id with an apostrophe: normalised back to the same id, the source id
        // kept as `renamed_from` (det-src-1)
        (
            "ID_LANTERNFALL_FEAT_KEEPER_S_VOW",
            "ID_LANTERNFALL_FEAT_KEEPER_S_VOW",
            true,
        ),
    ],
    modified: &[
        (
            "ID_LANTERNFALL_CLASS_FEATURE_WICK_GUARD",
            "/stats/armorClass",
            1,
        ),
        ("ID_LANTERNFALL_RACE_WICKLING", "/stats/speed", 5),
        ("ID_LANTERNFALL_FEAT_LANTERN_BEARER", "/stats/initiative", 2),
    ],
};

/// Declared v1 -> v2 changes of the invented 5etools pair (`5etools-v1` / `5etools-v2`).
const FIVE_CHANGES: Changes = Changes {
    removed: &[
        "ID_5ET_LNV_RACE_CINDERKIN",
        "ID_5ET_LNV_BACKGROUND_CHANDLER",
        "ID_5ET_LNV_FEAT_SPARK_CATCHER",
    ],
    renamed: &[
        // a new name is a new id: a removal for a saved character
        (
            "ID_5ET_LNV_FEAT_LAMP_OATH",
            "ID_5ET_LNV_FEAT_LANTERN_OATH",
            false,
        ),
        // the LNR book is now dated after the 2024 handbook: its race became a species that
        // keeps the race id as `renamed_from` (det-5e-4 / IM8)
        (
            "ID_5ET_LNR_RACE_EMBERFOLK",
            "ID_5ET_LNR_SPECIES_EMBERFOLK",
            true,
        ),
    ],
    modified: &[
        ("ID_5ET_LNV_RACE_WICKFOLK", "/stats/speed", 5),
        ("ID_5ET_LNV_FEAT_STEADY_FLAME", "/abilities/bonus/dex", 1),
        ("ID_5ET_LNV_FEAT_STEADY_FLAME", "/abilities/bonus/str", -1),
    ],
};

/// An update that fails partway: true when it was held (nothing installed).
type Partial<'a> = &'a dyn Fn(&AppState, &Measure, &Ours) -> bool;
/// v1 -> v2 and its declared changes.
type Update<'a> = (&'a dyn Fn(&AppState), &'a Changes);

/// How one source is installed, re-imported, updated and removed.
struct Source<'a> {
    name: &'a str,
    per_type: usize,
    install: &'a dyn Fn(&AppState),
    /// the same source again; checks "up to date" where the path has that notion
    reimport: &'a dyn Fn(&AppState, &mut Checks),
    /// an update that fails partway; true when it was held (nothing installed)
    partial: Option<Partial<'a>>,
    /// v1 -> v2 and its declared changes
    update: Option<Update<'a>>,
    remove: &'a dyn Fn(&AppState, &[String]),
    /// install the current version again after the removal
    reinstall: &'a dyn Fn(&AppState),
}

fn per_type() -> usize {
    std::env::var("BOREALIS_LIFECYCLE_PER_TYPE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
}

fn remove_packs(app: &AppState, packs: &[String]) {
    for p in packs {
        if let Err(e) = app.remove_pack(p, &NullSink) {
            eprintln!("  remove_pack {p}: {}", e.message);
        }
    }
}

fn sha(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `name:bytes-digest` of every file of the source's installed packs (pack-meta.json records the
/// install time and input name, so it is left out).
fn pack_files(app: &AppState, packs: &[String]) -> String {
    let lib = app.paths.library();
    packs
        .iter()
        .map(|p| {
            let entries = files::tree_entries(&lib.join(p), "", &|r| r != "pack-meta.json");
            let text: String = entries
                .iter()
                .map(|(n, b)| format!("{n}:{};", sha(b)))
                .collect();
            format!("{p}[{text}]")
        })
        .collect()
}

/// The whole stage-4 run of one source.
fn run(src: &Source<'_>) {
    let t0 = std::time::Instant::now();
    let mut c = Checks::new(src.name);
    let dir = tempfile::tempdir().expect("temp dir");
    let data = dir.path().join("data");
    let mut app = app::app_state_at(data.clone());
    let bundled = user_packs(&app);
    // ids the app has before the source: a source element that replaces one of them is not
    // missing once the source is removed
    let before: BTreeSet<String> = app
        .hub()
        .expect("hub")
        .store
        .iter()
        .map(|se| se.element.id.clone())
        .collect();
    (src.install)(&app);
    let packs: Vec<String> = user_packs(&app).difference(&bundled).cloned().collect();
    let v1 = ours(&app, &packs);
    metric(src.name, "elements", v1.ids.len() as i64);
    c.check(
        "installed",
        !v1.ids.is_empty(),
        &format!("{} packs", packs.len()),
    );
    let t = std::time::Instant::now();
    let sample = build(&app, &v1, src.per_type);
    metric(src.name, "sample_characters", sample.members.len() as i64);
    for level in lifecycle_sample::CLASS_LEVELS {
        let n = sample
            .members
            .iter()
            .filter(|m| m.label.ends_with(&format!("@{level}")))
            .count();
        metric(src.name, &format!("sample_class_l{level}"), n as i64);
    }
    metric(src.name, "sample_unplaced", sample.unplaced.len() as i64);
    metric(src.name, "sample_build_s", t.elapsed().as_secs() as i64);
    if !sample.unplaced.is_empty() {
        eprintln!("  unplaced: {:?}", sample.unplaced);
    }
    let base = measure(&app, &sample);
    opened(&mut c, "baseline", &base);

    // restart: a new AppState over the same data dir
    drop(app);
    app = app::app_state_at(data.clone());
    let m = measure(&app, &sample);
    same(&mut c, "restart", &base, &m);

    // re-import the same source
    let (e1, f1) = (elements(&app, &v1), pack_files(&app, &packs));
    (src.reimport)(&app, &mut c);
    let (e2, f2) = (elements(&app, &v1), pack_files(&app, &packs));
    let changed = element_changes(&e1, &e2);
    metric(src.name, "reimport_element_changes", changed as i64);
    c.check(
        "reimport/zero_element_changes",
        changed == 0,
        &format!("{changed}"),
    );
    c.check("reimport/pack_files_identical", f1 == f2, "");
    let m = measure(&app, &sample);
    same(&mut c, "reimport", &base, &m);

    if let Some(partial) = src.partial {
        let held = partial(&app, &base, &v1);
        c.check("partial/update_held", held, "");
        let m = measure(&app, &sample);
        same(&mut c, "partial", &base, &m);
    }

    let (current, v2) = match src.update {
        Some((apply, changes)) => {
            apply(&app);
            let v2 = ours(&app, &packs);
            (update_checks(&app, &mut c, &sample, &base, changes), v2)
        }
        None => (base.clone(), v1.clone()),
    };

    (src.remove)(&app, &packs);
    let left: BTreeSet<String> = user_packs(&app)
        .intersection(&packs.iter().cloned().collect())
        .cloned()
        .collect();
    c.check("remove/packs_gone", left.is_empty(), &format!("{left:?}"));
    // the characters were saved with v1 ids: every v1 or v2 id they hold is gone now
    let gone =
        |id: &str| (v1.ids.contains_key(id) || v2.ids.contains_key(id)) && !before.contains(id);
    removal_checks(&app, &mut c, &sample, &base, &gone);

    (src.reinstall)(&app);
    let m = measure(&app, &sample);
    same(&mut c, "reinstall", &current, &m);
    drop(app);
    c.finish(t0);
}

/// Run on a big stack (deep content recursion), propagating panics.
fn run_big<F: FnOnce() + Send + 'static>(f: F) {
    borealis_engine::invariants::enable_for_thread();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("lifecycle thread");
}

// ---------------------------------------------------------------- Aurora folder

fn folder_install(app: &AppState, dir: &Path) {
    let token = app
        .picked
        .grant(dir.to_path_buf(), PickPurpose::ContentFolder)
        .expect("grant")
        .token;
    let request = PreviewImportRequest {
        files: Vec::new(),
        folder: Some(token),
    };
    let preview = app.preview_import_pack(&request).expect("preview");
    app.install_import_preview(&preview.preview_id, &NullSink)
        .expect("install");
}

fn a_user_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/support/a-user")
}

#[test]
fn lifecycle_a_user() {
    run_big(|| {
        let dir = a_user_dir();
        let install = |app: &AppState| folder_install(app, &dir);
        let reimport = |app: &AppState, _: &mut Checks| folder_install(app, &dir);
        run(&Source {
            name: "a-user",
            per_type: usize::MAX,
            install: &install,
            reimport: &reimport,
            partial: None,
            update: None,
            remove: &remove_packs,
            reinstall: &install,
        });
    });
}

#[test]
fn lifecycle_inv_folder() {
    run_big(|| {
        let v1 = fixtures().join("aurora-v1/lanternfall");
        let v2 = fixtures().join("aurora-v2/lanternfall");
        let install = |app: &AppState| folder_install(app, &v1);
        let reimport = |app: &AppState, _: &mut Checks| folder_install(app, &v1);
        let to_v2 = |app: &AppState| folder_install(app, &v2);
        run(&Source {
            name: "inv-folder",
            per_type: usize::MAX,
            install: &install,
            reimport: &reimport,
            partial: None,
            update: Some((&to_v2, &AURORA_CHANGES)),
            remove: &remove_packs,
            reinstall: &to_v2,
        });
    });
}

// ---------------------------------------------------------------- Aurora index (fake fetcher)

const INV_BASE: &str = "https://example.invalid/lanternfall/";

fn inv_index_url() -> String {
    format!("{INV_BASE}lanternfall.index")
}

/// The invented pack `version` served as an index tree: top index -> the two XML files.
fn inv_index(version: &str, down: Option<&str>) -> FakeIndex {
    let dir = fixtures().join(format!("aurora-{version}/lanternfall"));
    let mut files = HashMap::new();
    let mut urls = Vec::new();
    for name in ["lanternfall-classes.xml", "lanternfall-origins.xml"] {
        let url = format!("{INV_BASE}{name}");
        files.insert(url.clone(), std::fs::read(dir.join(name)).expect("fixture"));
        urls.push(url);
    }
    files.insert(inv_index_url(), lifecycle::index_xml("Lanternfall", &urls));
    FakeIndex {
        files,
        down: down.map(|n| format!("{INV_BASE}{n}")),
    }
}

fn index_install(app: &AppState, url: &str, f: &dyn Fetcher) {
    app.add_index(url).expect("add index");
    let r = update(app, f);
    eprintln!(
        "  update {url}: updated {} failed {}",
        r.updated,
        r.failed.len()
    );
}

/// Forget the index, then remove its packs (both commands of the UI).
fn index_remove(app: &AppState, url: &str, packs: &[String]) {
    app.remove_index(url).expect("remove index");
    remove_packs(app, packs);
}

/// An update whose file `down` keeps failing: held = nothing installed, the index reports it.
fn held_update(app: &AppState, f: &dyn Fetcher) -> bool {
    let r = update(app, f);
    let problem = app.list_indexes().iter().any(|i| i.problem.is_some());
    eprintln!(
        "  partial update: updated {} failed {} problem {problem}",
        r.updated,
        r.failed.len()
    );
    r.updated == 0 && !r.reloaded && problem
}

#[test]
fn lifecycle_inv_index() {
    run_big(|| {
        let url = inv_index_url();
        let install = |app: &AppState| index_install(app, &url, &inv_index("v1", None));
        let reimport = |app: &AppState, c: &mut Checks| {
            let r = update(app, &inv_index("v1", None));
            c.check(
                "reimport/up_to_date",
                up_to_date(&r),
                &format!("updated {} reports {}", r.updated, r.reports.len()),
            );
        };
        let partial = |app: &AppState, _: &Measure, _: &Ours| {
            held_update(app, &inv_index("v2", Some("lanternfall-origins.xml")))
        };
        let to_v2 = |app: &AppState| {
            update(app, &inv_index("v2", None));
        };
        let remove = |app: &AppState, packs: &[String]| index_remove(app, &url, packs);
        let reinstall = |app: &AppState| index_install(app, &url, &inv_index("v2", None));
        run(&Source {
            name: "inv-index",
            per_type: usize::MAX,
            install: &install,
            reimport: &reimport,
            partial: Some(&partial),
            update: Some((&to_v2, &AURORA_CHANGES)),
            remove: &remove,
            reinstall: &reinstall,
        });
    });
}

// ---------------------------------------------------------------- 5etools

fn five_import(app: &AppState, data: &Path) -> borealis_lib::content_dto::FiveToolsImportReport {
    let token = app
        .picked
        .grant(data.to_path_buf(), PickPurpose::FivetoolsFolder)
        .expect("grant")
        .token;
    let sources = app
        .scan_5etools_folder(&token)
        .expect("scan")
        .sources
        .into_iter()
        .map(|s| s.abbr)
        .collect();
    let request = FiveToolsImportRequest {
        folder: token,
        sources,
        include_fluff: true,
    };
    app.import_5etools(&request, &NullSink).expect("import")
}

#[test]
fn lifecycle_inv_5e() {
    run_big(|| {
        let v1 = fixtures().join("5etools-v1");
        let v2 = fixtures().join("5etools-v2");
        let install = |app: &AppState| {
            five_import(app, &v1);
        };
        let reimport = |app: &AppState, _: &mut Checks| {
            five_import(app, &v1);
        };
        let to_v2 = |app: &AppState| {
            five_import(app, &v2);
        };
        run(&Source {
            name: "inv-5e",
            per_type: usize::MAX,
            install: &install,
            reimport: &reimport,
            partial: None,
            update: Some((&to_v2, &FIVE_CHANGES)),
            remove: &remove_packs,
            reinstall: &to_v2,
        });
    });
}

#[test]
fn lifecycle_f_full() {
    let Ok(dir) = std::env::var("BOREALIS_5ETOOLS_DIR") else {
        return eprintln!("SKIPPED f-full: BOREALIS_5ETOOLS_DIR not set");
    };
    run_big(move || {
        let data = PathBuf::from(dir);
        let install = |app: &AppState| {
            five_import(app, &data);
        };
        let reimport = |app: &AppState, _: &mut Checks| {
            five_import(app, &data);
        };
        run(&Source {
            name: "f-full",
            per_type: per_type(),
            install: &install,
            reimport: &reimport,
            partial: None,
            update: None,
            remove: &remove_packs,
            reinstall: &install,
        });
    });
}

// ---------------------------------------------------------------- A-core (live)

/// A live fetcher that keeps every file it downloaded (the partial-update fake serves them).
struct Recording {
    files: Mutex<HashMap<String, Vec<u8>>>,
}

impl Fetcher for Recording {
    fn get(&self, url: &str, max: usize) -> Result<Vec<u8>, String> {
        let r = HttpsFetcher.get(url, max);
        if let Ok(bytes) = &r {
            self.files
                .lock()
                .unwrap()
                .insert(url.to_string(), bytes.clone());
        }
        r
    }
}

/// The recorded A-core tree with the file defining a held id changed (that id renamed) and one
/// other content file failing: a held Update must leave every character as it was.
fn core_partial(
    files: &HashMap<String, Vec<u8>>,
    base: &Measure,
    ours: &Ours,
) -> Option<FakeIndex> {
    let held: BTreeSet<&String> = base
        .chars
        .values()
        .flat_map(|s| s.chosen.iter())
        .filter(|id| ours.ids.contains_key(*id))
        .collect();
    let mut files = files.clone();
    let mut urls: Vec<&String> = files.keys().collect();
    urls.sort();
    let (target, id) = urls.iter().find_map(|u| {
        let text = String::from_utf8_lossy(&files[*u]);
        held.iter()
            .find(|id| text.contains(&format!("id=\"{id}\"")))
            .map(|id| ((*u).clone(), (*id).clone()))
    })?;
    let down = urls
        .iter()
        .rev()
        .find(|u| u.ends_with(".xml") && **u != &target)
        .map(|u| (*u).clone())?;
    let text = String::from_utf8_lossy(&files[&target]).replace(
        &format!("id=\"{id}\""),
        &format!("id=\"{id}_LIFECYCLE_GONE\""),
    );
    files.insert(target, text.into_bytes());
    Some(FakeIndex {
        files,
        down: Some(down),
    })
}

#[test]
fn lifecycle_a_core() {
    let Ok(url) = std::env::var("BOREALIS_LIVE_INDEX_URL") else {
        return eprintln!("SKIPPED a-core: BOREALIS_LIVE_INDEX_URL not set");
    };
    run_big(move || {
        let rec = Recording {
            files: Mutex::new(HashMap::new()),
        };
        let install = |app: &AppState| index_install(app, &url, &rec);
        let reimport = |app: &AppState, c: &mut Checks| {
            let r = update(app, &HttpsFetcher);
            c.check(
                "reimport/up_to_date",
                up_to_date(&r),
                &format!(
                    "updated {} failed {} reports {}",
                    r.updated,
                    r.failed.len(),
                    r.reports.len()
                ),
            );
        };
        let partial = |app: &AppState, base: &Measure, ours: &Ours| {
            let files = rec.files.lock().unwrap().clone();
            match core_partial(&files, base, ours) {
                Some(fake) => held_update(app, &fake),
                None => {
                    eprintln!("  partial: no recorded file defines a held id (harness)");
                    false
                }
            }
        };
        let remove = |app: &AppState, packs: &[String]| index_remove(app, &url, packs);
        let reinstall = |app: &AppState| index_install(app, &url, &HttpsFetcher);
        run(&Source {
            name: "a-core",
            per_type: per_type(),
            install: &install,
            reimport: &reimport,
            partial: Some(&partial),
            update: None,
            remove: &remove,
            reinstall: &reinstall,
        });
    });
}
