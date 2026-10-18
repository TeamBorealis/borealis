//! Stage 4 (specs/acceptance-imports-1.md lines 69-77): a source's characters survive restart,
//! re-import, a pack update (v1 -> v2), a failed update and removal + re-install. Each phase
//! compares [`Measure`]s of the same sample; every check is a gate `OUTCOME`
//! (`<source>/s4/<check>`) and a `METRIC` line, counts and ids only.
//!
//! Expected sets come from the fixtures' declared changes ([`Changes`]), never from the store
//! under test: "removed" are the ids characters hold that v2 drops or renames without an alias;
//! a rename with an alias (det-src-1 `renamed_from` of a normalised Aurora id, det-5e-4 races
//! that became species) must stay resolved.

use super::lifecycle_sample::{measure, Measure, Ours, Sample};
use borealis_lib::content_fetch_retry::FetchError;
use borealis_lib::content_index::Fetcher;
use borealis_lib::state::AppState;
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub const SUITE: &str = "acceptance";
pub const STAGE: &str = "s4";

/// Written straight to stderr (like `outcome::record`), so libtest's capture never swallows it.
pub fn metric(src: &str, name: &str, n: i64) {
    use std::io::Write as _;
    let line = format!("METRIC\t{src}\t{STAGE}\t*\t{name}\t{n}\n");
    let _ = std::io::stderr().lock().write_all(line.as_bytes());
}

/// The checks of one source.
pub struct Checks {
    pub src: String,
    pub failed: Vec<String>,
    pub passed: usize,
}

impl Checks {
    pub fn new(src: &str) -> Self {
        Self {
            src: src.to_string(),
            failed: Vec::new(),
            passed: 0,
        }
    }

    pub fn check(&mut self, name: &str, pass: bool, detail: &str) {
        crate::outcome::record(SUITE, &format!("{}/{STAGE}/{name}", self.src), pass, detail);
        metric(&self.src, name, i64::from(pass));
        eprintln!(
            "  [{}] {name}: {} {detail}",
            self.src,
            if pass { "PASS" } else { "FAIL" }
        );
        if pass {
            self.passed += 1;
        } else {
            self.failed.push(format!("{name} ({detail})"));
        }
    }

    /// Print the summary; panics on a failed check when `BOREALIS_LIFECYCLE_STRICT` is set.
    pub fn finish(self, t0: std::time::Instant) {
        metric(&self.src, "wall_s", t0.elapsed().as_secs() as i64);
        metric(&self.src, "checks_failed", self.failed.len() as i64);
        eprintln!(
            "LIFECYCLE {}: {} passed, {} failed, {} s",
            self.src,
            self.passed,
            self.failed.len(),
            t0.elapsed().as_secs()
        );
        for f in &self.failed {
            eprintln!("  FAILED {}: {f}", self.src);
        }
        if std::env::var_os("BOREALIS_LIFECYCLE_STRICT").is_some() {
            assert!(self.failed.is_empty(), "{}: {:?}", self.src, self.failed);
        }
    }
}

/// Ids (at most a few) for a detail string.
fn few(ids: &BTreeSet<String>) -> String {
    let v: Vec<&str> = ids.iter().take(4).map(String::as_str).collect();
    let more = ids.len().saturating_sub(v.len());
    if more > 0 {
        format!("{} +{more}", v.join(" "))
    } else {
        v.join(" ")
    }
}

/// Every character opens, none panics; returns (opened, panicked).
pub fn opened(c: &mut Checks, phase: &str, m: &Measure) {
    let total = m.chars.len();
    let open = m.chars.values().filter(|s| s.opened).count();
    let panics = m.chars.values().filter(|s| s.panicked).count();
    metric(&c.src, &format!("{phase}_opened"), open as i64);
    metric(&c.src, &format!("{phase}_panics"), panics as i64);
    c.check(
        &format!("{phase}/characters_open"),
        open == total && total > 0,
        &format!("{open}/{total}"),
    );
    c.check(
        &format!("{phase}/no_panic"),
        panics == 0,
        &format!("{panics}"),
    );
}

/// Snapshot digests and offered lists of `now` equal `then`.
pub fn same(c: &mut Checks, phase: &str, then: &Measure, now: &Measure) {
    let diff = |f: &dyn Fn(&super::lifecycle_sample::Seen) -> &String| {
        then.chars
            .iter()
            .filter(|(id, s)| {
                now.chars
                    .get(*id)
                    .map_or(true, |n| f(n) != f(s) || !n.opened)
            })
            .map(|(id, _)| id.clone())
            .collect::<BTreeSet<String>>()
    };
    let snap = diff(&|s| &s.snapshot);
    let offered = diff(&|s| &s.offered);
    metric(&c.src, &format!("{phase}_snapshot_diff"), snap.len() as i64);
    metric(
        &c.src,
        &format!("{phase}_offered_diff"),
        offered.len() as i64,
    );
    c.check(
        &format!("{phase}/snapshot_digests_identical"),
        snap.is_empty(),
        &format!("{} of {} differ", snap.len(), then.chars.len()),
    );
    c.check(
        &format!("{phase}/offered_identical"),
        offered.is_empty() && then.builder == now.builder,
        &format!(
            "{} characters, builder {}",
            offered.len(),
            if then.builder == now.builder {
                "same"
            } else {
                "differs"
            }
        ),
    );
}

/// Precision and recall of `listed` against `expected` (summed over characters).
pub fn exact(
    c: &mut Checks,
    name: &str,
    expected: &BTreeMap<String, BTreeSet<String>>,
    listed: &BTreeMap<String, BTreeSet<String>>,
) {
    let (mut tp, mut fp, mut fn_) = (0usize, BTreeSet::new(), BTreeSet::new());
    for id in expected
        .keys()
        .chain(listed.keys())
        .collect::<BTreeSet<_>>()
    {
        let e = expected.get(id).cloned().unwrap_or_default();
        let l = listed.get(id).cloned().unwrap_or_default();
        tp += e.intersection(&l).count();
        fp.extend(l.difference(&e).cloned());
        fn_.extend(e.difference(&l).cloned());
    }
    let pct = |a: usize, b: usize| (a * 100).checked_div(b).map_or(100, |v| v as i64);
    let n_fp: usize = listed.values().map(BTreeSet::len).sum::<usize>() - tp;
    let n_fn: usize = expected.values().map(BTreeSet::len).sum::<usize>() - tp;
    metric(&c.src, &format!("{name}_precision_pct"), pct(tp, tp + n_fp));
    metric(&c.src, &format!("{name}_recall_pct"), pct(tp, tp + n_fn));
    metric(&c.src, &format!("{name}_expected"), (tp + n_fn) as i64);
    c.check(
        name,
        n_fp == 0 && n_fn == 0,
        &format!(
            "tp {tp} fp {n_fp} fn {n_fn}; extra [{}] missed [{}]",
            few(&fp),
            few(&fn_)
        ),
    );
}

/// Per character: the ids of `gone` it held in `before`.
pub fn held(before: &Measure, gone: &dyn Fn(&str) -> bool) -> BTreeMap<String, BTreeSet<String>> {
    before
        .chars
        .iter()
        .map(|(id, s)| {
            let ids = s.chosen.iter().filter(|x| gone(x)).cloned().collect();
            (id.clone(), ids)
        })
        .filter(|(_, ids): &(String, BTreeSet<String>)| !ids.is_empty())
        .collect()
}

/// Per character: the gone ids it chose in a slot whose owner (the slot id up to the first
/// `:`) is still on the character in `after` (an owner of a slot or a chosen id there; a removed
/// race's sub race slot is owned by the race, which is gone). Also counts those slots that
/// `after` no longer shows at all (neither chosen nor open).
pub fn top_level(
    base: &Measure,
    after: &Measure,
    gone: &dyn Fn(&str) -> bool,
) -> (BTreeMap<String, BTreeSet<String>>, usize) {
    let slots_of = |m: &Measure, cid: &str| -> Vec<serde_json::Value> {
        m.chars
            .get(cid)
            .and_then(|s| s.value["slots"].as_array().cloned())
            .unwrap_or_default()
    };
    let owner = |sid: &str| sid.split(':').next().unwrap_or("").to_string();
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut hidden = 0;
    for cid in base.chars.keys() {
        let now = slots_of(after, cid);
        let ids: BTreeSet<String> = now
            .iter()
            .filter_map(|s| s["id"].as_str().map(str::to_string))
            .collect();
        let mut present: BTreeSet<String> = ids.iter().map(|s| owner(s)).collect();
        present.extend(
            now.iter()
                .filter_map(|s| s["chosen"]["id"].as_str().map(str::to_string)),
        );
        for slot in &slots_of(base, cid) {
            let (Some(id), Some(sid)) = (slot["chosen"]["id"].as_str(), slot["id"].as_str()) else {
                continue;
            };
            if gone(id) && present.contains(&owner(sid)) {
                out.entry(cid.clone()).or_default().insert(id.to_string());
                hidden += usize::from(!ids.contains(sid));
            }
        }
    }
    (out, hidden)
}

/// Print the slot of every expected id a character's notices leave out (first 40).
fn debug_misses(base: &Measure, after: &Measure, expected: &BTreeMap<String, BTreeSet<String>>) {
    let mut n = 0;
    for (cid, ids) in expected {
        let shown = after
            .chars
            .get(cid)
            .map(|s| s.listed_missing.clone())
            .unwrap_or_default();
        let slots = base.chars[cid].value["slots"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for slot in &slots {
            let id = slot["chosen"]["id"].as_str().unwrap_or("");
            if ids.contains(id) && !shown.contains(id) && n < 40 {
                n += 1;
                eprintln!(
                    "    miss {cid} slot {} type {} -> {id}",
                    slot["id"], slot["elementType"]
                );
            }
        }
    }
}

/// Per character: the ids its notices list as not installed.
pub fn listed(m: &Measure) -> BTreeMap<String, BTreeSet<String>> {
    m.chars
        .iter()
        .filter(|(_, s)| !s.listed_missing.is_empty())
        .map(|(id, s)| (id.clone(), s.listed_missing.clone()))
        .collect()
}

/// Per character: the ids `recheck_missing_content` reports as still missing.
pub fn rechecked(app: &AppState) -> BTreeMap<String, BTreeSet<String>> {
    let Ok(report) = app.recheck_missing_content() else {
        return BTreeMap::new();
    };
    report
        .outstanding
        .into_iter()
        .map(|e| {
            let ids = e.still_missing.into_iter().map(|m| m.id).collect();
            (e.character_id, ids)
        })
        .collect()
}

/// Declared changes of a v1 -> v2 fixture pair.
pub struct Changes {
    pub removed: &'static [&'static str],
    /// (v1 id, v2 id, resolved through an alias)
    pub renamed: &'static [(&'static str, &'static str, bool)],
    /// (element id, JSON pointer into the snapshot, v2 - v1)
    pub modified: &'static [(&'static str, &'static str, i64)],
}

/// The update checks: characters open, the removed ids are reported exactly (notices and
/// recheck), aliased renames stay resolved, modified values flow through.
pub fn update_checks(
    app: &AppState,
    c: &mut Checks,
    sample: &Sample,
    v1: &Measure,
    ch: &Changes,
) -> Measure {
    let v2 = measure(app, sample);
    opened(c, "update", &v2);
    let gone: BTreeSet<&str> = ch
        .removed
        .iter()
        .copied()
        .chain(ch.renamed.iter().filter(|r| !r.2).map(|r| r.0))
        .collect();
    let expected = held(v1, &|id| gone.contains(id));
    let removed_held: usize = expected.values().map(BTreeSet::len).sum();
    metric(&c.src, "update_removed_held", removed_held as i64);
    c.check(
        "update/sample_holds_removed",
        removed_held > 0,
        &format!("{removed_held} held removed ids"),
    );
    exact(c, "update/missing_listed_per_id", &expected, &listed(&v2));
    exact(
        c,
        "update/recheck_reports_removed",
        &expected,
        &rechecked(app),
    );
    for (old, new, alias) in ch.renamed.iter().filter(|r| r.2) {
        let holders: Vec<&String> = v1
            .chars
            .iter()
            .filter(|(_, s)| s.chosen.contains(*old))
            .map(|(id, _)| id)
            .collect();
        let followed = holders.iter().all(|id| {
            v2.chars
                .get(*id)
                .is_some_and(|s| s.chosen.contains(*new) && !s.listed_missing.contains(*old))
        });
        let _ = alias;
        c.check(
            &format!("update/alias_followed/{old}"),
            !holders.is_empty() && followed,
            &format!("{} holders", holders.len()),
        );
    }
    for (id, pointer, delta) in ch.modified {
        let mut holders = 0;
        let mut wrong = 0;
        for (cid, s) in &v1.chars {
            if !s.chosen.contains(*id) && !s.value.to_string().contains(*id) {
                continue;
            }
            holders += 1;
            let a = s.value.pointer(pointer).and_then(serde_json::Value::as_i64);
            let b = v2
                .chars
                .get(cid)
                .and_then(|n| n.value.pointer(pointer))
                .and_then(serde_json::Value::as_i64);
            if !matches!((a, b), (Some(a), Some(b)) if b - a == *delta) {
                wrong += 1;
                eprintln!("    {cid}: {pointer} {a:?} -> {b:?} (want {delta:+})");
            }
        }
        c.check(
            &format!("update/modified_flows/{id}{pointer}"),
            holders > 0 && wrong == 0,
            &format!("{holders} holders, {wrong} wrong"),
        );
    }
    v2
}

/// Removal: characters open, no panic, every id of the removed packs a character holds (as
/// saved: `base`, the measure before any update) is listed per id (notices) and by recheck.
pub fn removal_checks(
    app: &AppState,
    c: &mut Checks,
    sample: &Sample,
    base: &Measure,
    gone: &dyn Fn(&str) -> bool,
) -> Measure {
    let after = measure(app, sample);
    opened(c, "remove", &after);
    let expected = held(base, gone);
    exact(
        c,
        "remove/missing_listed_per_id",
        &expected,
        &listed(&after),
    );
    // the same, limited to choices whose slot owner is still on the character (a removed race's
    // sub race slot belongs to the race): tells the two kinds of misses apart
    let (top, hidden) = top_level(base, &after, gone);
    // not a stage-4 criterion: a slot whose choice is missing is not shown (no open slot to
    // choose a replacement in); counted for the report
    metric(&c.src, "remove_hidden_slots", hidden as i64);
    let shown: BTreeMap<String, BTreeSet<String>> = listed(&after)
        .into_iter()
        .map(|(cid, ids)| {
            let keep = top.get(&cid).cloned().unwrap_or_default();
            (cid, ids.intersection(&keep).cloned().collect())
        })
        .collect();
    exact(c, "remove/missing_listed_owner_present", &top, &shown);
    if std::env::var_os("BOREALIS_LIFECYCLE_DEBUG").is_some() {
        debug_misses(base, &after, &top);
    }
    exact(
        c,
        "remove/recheck_reports_removed",
        &expected,
        &rechecked(app),
    );
    after
}

// ---------------------------------------------------------------- fake index (det-src-3 style)

/// An Aurora index tree served from memory; `down` answers 503 every time (a passing failure
/// that outlasts the retries, so Update holds the index).
pub struct FakeIndex {
    pub files: HashMap<String, Vec<u8>>,
    pub down: Option<String>,
}

impl Fetcher for FakeIndex {
    fn get(&self, url: &str, max: usize) -> Result<Vec<u8>, String> {
        self.get_classified(url, max).map_err(|e| e.wire())
    }
    fn get_classified(&self, url: &str, _max: usize) -> Result<Vec<u8>, FetchError> {
        if self.down.as_deref() == Some(url) {
            return Err(FetchError::http(503, "Service Unavailable", None));
        }
        self.files
            .get(url)
            .cloned()
            .ok_or_else(|| FetchError::http(404, "Not Found", None))
    }
}

/// `<index>` XML listing `files` (absolute URLs).
pub fn index_xml(name: &str, files: &[String]) -> Vec<u8> {
    let rows: String = files
        .iter()
        .map(|u| {
            let f = u.rsplit('/').next().unwrap_or(u);
            format!("    <file name=\"{f}\" url=\"{u}\" />\n")
        })
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<index>\n  <info><name>{name}</name></info>\n  <files>\n{rows}  </files>\n</index>"
    )
    .into_bytes()
}

/// Update indexes without real pauses (retries still run).
pub fn update(app: &AppState, f: &dyn Fetcher) -> borealis_lib::content_dto::UpdateIndexesResult {
    app.update_indexes_with(
        f,
        &borealis_lib::content_packs::NullSink,
        borealis_lib::content_fetch_retry::RetryPolicy::default(),
        &|_| {},
    )
    .expect("update indexes")
}

/// "Up to date": nothing downloaded, failed, changed or reported.
pub fn up_to_date(r: &borealis_lib::content_dto::UpdateIndexesResult) -> bool {
    r.updated == 0 && r.failed.is_empty() && r.changed.is_empty() && r.reports.is_empty()
}

/// Every element of the source: id -> JSON (element-level diff of a re-import).
pub fn elements(app: &AppState, ours: &Ours) -> BTreeMap<String, String> {
    let hub = app.hub().expect("hub");
    ours.ids
        .keys()
        .filter_map(|id| {
            let e = hub.store.element(id)?;
            Some((id.clone(), serde_json::to_string(e).unwrap_or_default()))
        })
        .collect()
}

/// Number of elements added, removed or changed between two [`elements`] maps.
pub fn element_changes(a: &BTreeMap<String, String>, b: &BTreeMap<String, String>) -> usize {
    let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    keys.into_iter().filter(|k| a.get(*k) != b.get(*k)).count()
}
