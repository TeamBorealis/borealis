//! IM4 acceptance harness, stages 1 and 2 (specs/acceptance-imports-1.md), driven through the
//! real app state exactly as the UI drives it.
//!
//! Stage 1: every source element converts or gets a precise warning. `S` comes from independent
//! counters (`support/count.rs`), `I` (clean), `W` (warned) and `K` (skipped by design, closed
//! list) from the import report the install command returns; `U = S - I - W - K` must be 0. Two
//! installs and folder vs zip must give byte-identical packs.
//! Stage 2: every imported element is offered where it belongs, via `get_slot_options` (plus the
//! spellcasting block and `search_items`) on hosts built with the builder commands; elements of
//! types no select ever offers are declared non-offerable with a reason.
//!
//! Sources (each skips when its variable is unset; `BOREALIS_ACCEPT_STRICT=1` fails on a skip):
//! `a-user` the committed invented pack (always), `a-core` `BOREALIS_LIVE_INDEX_URL`,
//! `a-comm` `BOREALIS_CORPUS_DIR`, `f-full` `BOREALIS_5ETOOLS_DIR`, `f-url`
//! `BOREALIS_LIVE_5ETOOLS_URL`. `BOREALIS_ACCEPT_LIMIT` caps the class/subclass/feat hosts per
//! source (quick runs). Output: `METRIC\t<source>\t<stage>\t<type>\t<metric>\t<n>` and gate
//! `OUTCOME` lines on stderr; counts and ids only, never content text.

#[path = "../../tools/regression-gate/outcome.rs"]
mod outcome;
mod support;

use base64::Engine as _;
use borealis_content::{ContentStore, ContentStoreBuilder, Rule, Ruleset};
use borealis_lib::content_dto::{FiveToolsImportRequest, PackKind};
use borealis_lib::content_index::{Fetcher, HttpsFetcher};
use borealis_lib::content_packs::NullSink;
use borealis_lib::content_preview::{ImportFileInput, PreviewImportRequest};
use borealis_lib::picked_paths::PickPurpose;
use borealis_lib::state::AppState;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use support::app::{app_state, Driver, Offers, CASTER_SCORES, MAX_LEVEL, TEST_SCORES};
use support::count::{self, AuroraCount};
use support::{files, prereq};

const SUITE: &str = "acceptance";
const BUNDLED: [&str; 3] = ["core", "srd-5.1", "srd-5.2.1"];
/// Element types placed by a dedicated host (spec type -> host table).
const HOSTED: &[&str] = &[
    "Class",
    "Archetype",
    "Race",
    "Sub Race",
    "Race Variant",
    "Species",
    "Background",
    "Feat",
    "Spell",
    "Item",
    "Weapon",
    "Armor",
    "Magic Item",
];
/// Types that are never offered by a select, by design (spec: Rule, Grants, Internal, ...).
const NON_OFFERABLE: &[(&str, &str)] = &[
    ("Grants", "Grants"),
    ("Internal", "Internal"),
    ("Support", "Support"),
    ("Source", "Source"),
    ("Information", "Information"),
    ("Rule", "Rule"),
];
/// 5etools arrays the rules import leaves out by decision (IM5: rules-only; monsters, adventures
/// and site data are not character content). Anything else unconverted counts in `U`.
const FIVE_OUT_OF_SCOPE: &[&str] = &[
    "monster",
    "legendaryGroup",
    "vehicle",
    "vehicleUpgrade",
    "object",
    "trap",
    "hazard",
    "table",
    "tableGroup",
    "encounter",
    "name",
    "deck",
    "card",
    "recipe",
    "reward",
    "cult",
    "boon",
    "deity",
    "psionic",
    "variantrule",
    "action",
    "facility",
    "makebrewCreatureTrait",
    "makebrewCreatureAction",
    "monsterfeatures",
    "adventure",
    "book",
    "crochetPattern",
    "languageScript",
    "itemTypeAdditionalEntries",
];
/// 5etools arrays the importer reports under another kind.
const FIVE_KIND_OF: &[(&str, &str)] = &[("baseitem", "item"), ("magicvariant", "item")];
/// Skip reasons that are by design (digit-free prefixes of the importer's skip notes).
const FIVE_SKIP_BY_DESIGN: &[&str] = &[
    "descriptive catalog entry covered by the core pack",
    "defined by the core pack",
    "duplicate of an entry that was already imported",
];

/// Written straight to stderr (like `outcome::record`), so libtest's capture never swallows it.
fn metric(src: &str, stage: &str, ty: &str, name: &str, n: i64) {
    use std::io::Write as _;
    let line = format!("METRIC\t{src}\t{stage}\t{ty}\t{name}\t{n}\n");
    let _ = std::io::stderr().lock().write_all(line.as_bytes());
}

fn limit() -> usize {
    std::env::var("BOREALIS_ACCEPT_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX)
}

fn skip(src: &str, why: &str) {
    assert!(
        std::env::var_os("BOREALIS_ACCEPT_STRICT").is_none(),
        "{src}: {why} (BOREALIS_ACCEPT_STRICT set: a skip fails)"
    );
    eprintln!("SKIPPED {src}: {why}");
}

fn content_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content")
}

/// Installed pack directories under the app data dir: id -> (dir, single ruleset).
fn pack_dirs(data: &Path) -> BTreeMap<String, (PathBuf, Option<Ruleset>)> {
    let mut out = BTreeMap::new();
    let mut stack = vec![data.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().is_some_and(|n| n == "pack.json") {
                let v: serde_json::Value = std::fs::read_to_string(&p)
                    .ok()
                    .and_then(|t| serde_json::from_str(&t).ok())
                    .unwrap_or_default();
                let rs = v["rulesets"]
                    .as_array()
                    .filter(|a| a.len() == 1)
                    .and_then(|a| serde_json::from_value(a[0].clone()).ok());
                if let Some(id) = v["id"].as_str() {
                    out.insert(id.to_string(), (dir.clone(), rs));
                }
            }
        }
    }
    out
}

/// What a source installed: the full store (bundled + every installed pack) and its own elements.
struct Installed {
    store: ContentStore,
    ours: Vec<(String, String, Ruleset)>,
    digest: String,
}

fn installed(app_data: &Path, wanted: &dyn Fn(&str) -> bool) -> Installed {
    let mut b = ContentStoreBuilder::new();
    for p in BUNDLED {
        b = b
            .add_pack_dir(&content_dir().join(p))
            .expect("bundled pack");
    }
    let dirs = pack_dirs(app_data);
    let mut mine = Vec::new();
    let mut digest = String::new();
    for (id, (dir, rs)) in &dirs {
        b = b.add_pack_dir(dir).expect("installed pack loads");
        if wanted(id) {
            mine.push((format!("{id}/"), *rs));
            digest.push_str(&format!("{id}:{} ", files::digest_dir(dir)));
        }
    }
    let store = b.build();
    let ours = store
        .iter()
        .filter_map(|se| {
            let (_, rs) = mine.iter().find(|(p, _)| se.file.starts_with(p.as_str()))?;
            let rs = se.element.ruleset.or(*rs).unwrap_or(Ruleset::Dnd2014);
            Some((se.element.id.clone(), se.element.ty.clone(), rs))
        })
        .collect();
    Installed {
        store,
        ours,
        digest,
    }
}

// ---------------------------------------------------------------- stage 1, Aurora

/// Stage-1 tallies of one Aurora source, by type, on id sets.
#[derive(Default)]
struct AuroraTally {
    s: BTreeMap<String, String>,
    warned: BTreeSet<String>,
    imprecise: usize,
    uncoded: usize,
    infos: usize,
    folder_errors: usize,
}

fn aurora_stage1(src: &str, t: &AuroraTally, ins: &Installed) {
    let ours: HashMap<&str, &str> = ins
        .ours
        .iter()
        .map(|(id, ty, _)| (id.as_str(), ty.as_str()))
        .collect();
    let mut rows: BTreeMap<&str, [i64; 5]> = BTreeMap::new();
    for (id, ty) in &t.s {
        let r = rows.entry(ty.as_str()).or_default();
        r[0] += 1;
        if t.warned.contains(id) {
            r[2] += 1;
        } else if ours.contains_key(id.as_str()) {
            r[1] += 1;
        } else {
            r[4] += 1;
            outcome::record(
                SUITE,
                &format!("{src}/s1/{ty}/{id}"),
                false,
                "not imported, no warning",
            );
        }
    }
    let extra = ours.keys().filter(|id| !t.s.contains_key(**id)).count();
    for (ty, [s, i, w, k, u]) in &rows {
        for (name, n) in [("S", s), ("I", i), ("W", w), ("K", k), ("U", u)] {
            metric(src, "s1", ty, name, *n);
        }
        outcome::record(SUITE, &format!("{src}/s1/{ty}"), *u == 0, &format!("U={u}"));
    }
    metric(src, "s1", "*", "extra_imported", extra as i64);
    metric(src, "s1", "*", "imprecise_warnings", t.imprecise as i64);
    metric(src, "s1", "*", "folder_errors", t.folder_errors as i64);
    metric(src, "s1", "*", "warnings_without_code", t.uncoded as i64);
    metric(src, "s1", "*", "infos", t.infos as i64);
    outcome::record(
        SUITE,
        &format!("{src}/s1/coded_warnings"),
        t.uncoded == 0,
        "",
    );
    outcome::record(
        SUITE,
        &format!("{src}/s1/precise_warnings"),
        t.imprecise == 0,
        "",
    );
    outcome::record(
        SUITE,
        &format!("{src}/s1/folder_errors"),
        t.folder_errors == 0,
        "",
    );
}

/// Install one Aurora folder (or zip) through preview + install; fold its report into `t`.
fn aurora_install(app: &AppState, input: &AuroraInput, t: &mut AuroraTally) -> Option<String> {
    let request = match input {
        AuroraInput::Folder(dir) => {
            let token = app
                .picked
                .grant(dir.clone(), PickPurpose::ContentFolder)
                .expect("grant")
                .token;
            PreviewImportRequest {
                files: Vec::new(),
                folder: Some(token),
            }
        }
        AuroraInput::Zip(name, bytes) => PreviewImportRequest {
            files: vec![ImportFileInput {
                file_name: name.clone(),
                content_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
            }],
            folder: None,
        },
    };
    let preview = match app.preview_import_pack(&request) {
        Ok(p) => p,
        Err(e) => {
            t.folder_errors += 1;
            eprintln!("  preview refused: {}", e.message);
            return None;
        }
    };
    let result = match app.install_import_preview(&preview.preview_id, &NullSink) {
        Ok(r) => r,
        Err(e) => {
            t.folder_errors += 1;
            eprintln!("  install refused: {}", e.message);
            return None;
        }
    };
    for issue in &result.report.issues {
        if issue.severity == "info" {
            t.infos += 1;
            continue;
        }
        // the install report carries no file or code per issue: counted as uncoded
        t.uncoded += 1;
        match &issue.element {
            Some(id) => {
                t.warned.insert(id.clone());
            }
            None => {
                t.imprecise += 1;
                if t.imprecise <= 3 {
                    let m: String = issue
                        .message
                        .chars()
                        .filter(|c| !c.is_ascii_digit())
                        .take(70)
                        .collect();
                    eprintln!("  issue without element ({:?}): {m}", issue.severity);
                }
            }
        }
    }
    result.pack.map(|p| p.id)
}

enum AuroraInput {
    Folder(PathBuf),
    Zip(String, Vec<u8>),
}

fn count_folder(dir: &Path, c: &mut AuroraCount) {
    for f in count::xml_files(dir) {
        if let Ok(text) = std::fs::read_to_string(&f) {
            c.add_file(&text);
        }
    }
}

// ---------------------------------------------------------------- stage 1, 5etools

fn five_request(app: &AppState, data: &Path) -> (FiveToolsImportRequest, Vec<String>) {
    let token = app
        .picked
        .grant(data.to_path_buf(), PickPurpose::FivetoolsFolder)
        .expect("grant")
        .token;
    let sources: Vec<String> = app
        .scan_5etools_folder(&token)
        .expect("scan")
        .sources
        .into_iter()
        .map(|s| s.abbr)
        .collect();
    (
        FiveToolsImportRequest {
            folder: token,
            sources: sources.clone(),
            include_fluff: true,
        },
        sources,
    )
}

fn five_digest(app_data: &Path) -> String {
    installed(app_data, &|id| id.starts_with("5etools-")).digest
}

fn five_stage1(src: &str, data: &Path, sources: &[String]) {
    let mut s: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for (array, files) in count::fivetools_entries(data) {
        let kind = FIVE_KIND_OF
            .iter()
            .find(|(a, _)| *a == array)
            .map_or(array.as_str(), |(_, k)| k);
        for (f, n) in files {
            *s.entry(kind.to_string()).or_default().entry(f).or_default() += n;
        }
    }
    let options = borealis_import_5etools::ImportOptions {
        include_fluff: true,
        srd_dirs: vec![
            content_dir().join("srd-5.1"),
            content_dir().join("srd-5.2.1"),
        ],
        ..borealis_import_5etools::ImportOptions::default().with_sources(sources)
    };
    let report = borealis_import_5etools::import_folder(data, &options)
        .expect("import")
        .report;
    let mut kinds: BTreeMap<String, [usize; 4]> = BTreeMap::new();
    for f in &report.files {
        for (k, c) in &f.kinds {
            let r = kinds.entry(k.clone()).or_default();
            r[0] += c.total;
            r[1] += c.converted;
            r[2] += c.partial;
            r[3] += c.skipped;
        }
    }
    let mut by_design: BTreeMap<String, usize> = BTreeMap::new();
    let mut other_skips: BTreeMap<String, usize> = BTreeMap::new();
    for n in &report.skipped {
        let cause: String = n
            .reason
            .chars()
            .filter(|c| !c.is_ascii_digit())
            .take(60)
            .collect();
        if FIVE_SKIP_BY_DESIGN.iter().any(|p| n.reason.starts_with(p)) {
            *by_design.entry(n.kind.clone()).or_default() += 1;
        } else {
            *other_skips
                .entry(format!("{} | {cause}", n.kind))
                .or_default() += 1;
        }
    }
    let imprecise = report
        .partial
        .iter()
        .filter(|n| {
            n.file.is_empty()
                || n.element.is_empty()
                || n.code.is_empty()
                || n.code == "UNCLASSIFIED"
        })
        .count();
    let all: BTreeSet<String> = s.keys().chain(kinds.keys()).cloned().collect();
    for ty in all {
        let sv = s.get(&ty).map_or(0, |m| m.values().sum::<usize>()) as i64;
        let [total, conv, part, _] = kinds.get(&ty).copied().unwrap_or_default();
        // entities the importer saw (magic variants expand over base items): R - S > 0 = expansion
        metric(src, "s1", &ty, "R", total as i64);
        let (i, w) = ((conv - part) as i64, part as i64);
        let k = if FIVE_OUT_OF_SCOPE.contains(&ty.as_str()) && conv == 0 {
            sv
        } else {
            by_design.get(&ty).copied().unwrap_or(0) as i64
        };
        let u = sv - i - w - k;
        for (name, n) in [("S", sv), ("I", i), ("W", w), ("K", k), ("U", u)] {
            metric(src, "s1", &ty, name, n);
        }
        outcome::record(SUITE, &format!("{src}/s1/{ty}"), u == 0, &format!("U={u}"));
    }
    println!("--- {src}: skips not by design (kind | cause)");
    for (c, n) in other_skips.iter().take(40) {
        println!("{n:>7}  {c}");
    }
    metric(src, "s1", "*", "imprecise_warnings", imprecise as i64);
    metric(src, "s1", "*", "run_warnings", report.warnings.len() as i64);
    metric(
        src,
        "s1",
        "*",
        "converter_errors",
        report.errors.len() as i64,
    );
    outcome::record(
        SUITE,
        &format!("{src}/s1/precise_warnings"),
        imprecise == 0,
        "",
    );
    outcome::record(
        SUITE,
        &format!("{src}/s1/converter_errors"),
        report.errors.is_empty(),
        "",
    );
}

// ---------------------------------------------------------------- stage 2

/// Hosts a source needs: per edition its fighter/wizard/human ids found by name in the builder.
struct Ids {
    fighter: Option<String>,
    wizard: Option<String>,
    human: Option<String>,
    variant: Option<String>,
}

fn base_ids(d: &Driver<'_>, rs: Ruleset, prefix: &str) -> Ids {
    let base = d.create(rs, TEST_SCORES).expect("create");
    let race_ty = if rs == Ruleset::Dnd2024 {
        "Species"
    } else {
        "Race"
    };
    let mut ids = Ids {
        fighter: d.find_named(&base, "Class", "Fighter", prefix),
        wizard: d.find_named(&base, "Class", "Wizard", prefix),
        human: d.find_named(&base, race_ty, "Human", prefix),
        variant: None,
    };
    let mut h = base;
    if let Some(human) = &ids.human {
        if d.pick(&mut h, human) {
            ids.variant = ["Race Variant", "Sub Race"].iter().find_map(|t| {
                d.find_named(&h, t, "Variant Human", prefix)
                    .or_else(|| d.find_named(&h, t, "Variant", prefix))
            });
        }
    }
    d.drop_host(h);
    eprintln!(
        "  host ids {rs:?}: fighter {:?} wizard {:?} human {:?} variant {:?}",
        ids.fighter, ids.wizard, ids.human, ids.variant
    );
    ids
}

/// A host of `class` with an origin, levelled 1..=20: collect at every level (feat options
/// taken first), fill, level up. `archetype` is picked when offered. Returns archetypes offered.
fn progression(
    d: &Driver<'_>,
    rs: Ruleset,
    ids: &Ids,
    class: &str,
    archetype: Option<&str>,
    offers: &mut Offers,
) -> Vec<String> {
    let mut archetypes = Vec::new();
    let Some(mut h) = d.create(rs, CASTER_SCORES) else {
        return archetypes;
    };
    let label = format!("{class}/{}", archetype.unwrap_or("-"));
    if !d.pick(&mut h, class) {
        d.drop_host(h);
        return archetypes;
    }
    for race in [&ids.human, &ids.variant].into_iter().flatten() {
        d.pick(&mut h, race);
    }
    let mut picked = archetype.is_none();
    for level in 1..=MAX_LEVEL {
        if level > 1 && !d.level_up(&mut h) {
            break;
        }
        d.open_feat_selects(&mut h);
        if archetype.is_none() {
            for s in
                h.snap.slots.iter().filter(|s| {
                    s.element_type.eq_ignore_ascii_case("Archetype") && s.chosen.is_none()
                })
            {
                archetypes.extend(d.options(&h, &s.id).into_iter().map(|o| o.0));
            }
        }
        d.collect(&h, &label, offers);
        if let (Some(a), false) = (archetype, picked) {
            picked = d.pick(&mut h, a);
        }
        d.fill(&mut h);
        d.collect(&h, &label, offers);
    }
    d.drop_host(h);
    archetypes.sort();
    archetypes.dedup();
    archetypes
}

/// A host that meets a feat's prerequisites (shared `prereq::NeedsReader`), levelled until it
/// reaches the needed level with the prior feats placed; true when the feat is offered there.
fn needs_host(
    d: &Driver<'_>,
    rs: Ruleset,
    ids: &Ids,
    reader: &prereq::NeedsReader<'_>,
    feat: &str,
    offers: &mut Offers,
) -> bool {
    let Some(el) = reader.store.element(feat) else {
        return false;
    };
    let mut needs = prereq::Needs::default();
    reader.needs(rs, el, 0, &mut needs);
    let mut scores = if needs.caster {
        CASTER_SCORES
    } else {
        TEST_SCORES
    };
    for (i, v) in &needs.scores {
        scores[*i] = scores[*i].max(*v);
    }
    let class = needs.class.clone().or_else(|| {
        if needs.caster {
            ids.wizard.clone()
        } else {
            ids.fighter.clone()
        }
    });
    let (Some(class), Some(mut h)) = (class, d.create(rs, scores)) else {
        return false;
    };
    d.pick(&mut h, &class);
    let mut chain: Vec<String> = Vec::new();
    match &needs.race {
        Some(r) => {
            if let Some(parent) = reader
                .store
                .element(r)
                .filter(|e| e.ty == "Sub Race")
                .and_then(|e| e.supports.first())
            {
                chain.push(parent.clone());
            }
            chain.push(r.clone());
        }
        None => chain.extend(
            [ids.human.clone(), ids.variant.clone()]
                .into_iter()
                .flatten(),
        ),
    }
    for r in &chain {
        d.pick(&mut h, r);
    }
    d.fill(&mut h);
    let mut pending = needs.feats.clone();
    let label = format!("needs:{feat}");
    let mut found = false;
    loop {
        d.open_feat_selects(&mut h);
        while let Some(next) = pending.first().cloned() {
            if !d.pick(&mut h, &next) {
                break;
            }
            pending.remove(0);
            d.fill(&mut h);
            d.open_feat_selects(&mut h);
        }
        d.collect(&h, &label, offers);
        if offers.found.contains_key(feat) {
            found = true;
            break;
        }
        if h.snap.level.total >= MAX_LEVEL || !d.level_up(&mut h) {
            break;
        }
        // the improvement's "Feat" option before the fill takes the first option
        d.open_feat_selects(&mut h);
        d.fill(&mut h);
    }
    d.drop_host(h);
    found
}

/// Select types (lower case) and the support tokens each names (`None` = any element).
fn select_index(store: &ContentStore) -> HashMap<String, Vec<Option<BTreeSet<String>>>> {
    let mut out: HashMap<String, Vec<Option<BTreeSet<String>>>> = HashMap::new();
    for se in store.iter() {
        for r in &se.element.rules {
            if let Rule::Select(s) = r {
                let tokens = s.supports.as_ref().map(|t| {
                    t.split(['|', '&', '!', '(', ')', ','])
                        .map(|x| x.trim().to_lowercase())
                        .filter(|x| !x.is_empty())
                        .collect()
                });
                out.entry(s.ty.to_lowercase()).or_default().push(tokens);
            }
        }
    }
    out
}

fn stage2(
    src: &str,
    app: &AppState,
    ins: &Installed,
    prefix: &str,
    class_id: fn(Ruleset, &str) -> String,
) {
    let d = Driver::new(app);
    let mut offers = Offers::default();
    let cap = limit();
    let editions: BTreeSet<Ruleset> = ins.ours.iter().map(|o| o.2).collect();
    let ours_rs: HashMap<String, Ruleset> = ins.ours.iter().map(|o| (o.0.clone(), o.2)).collect();
    let reader = prereq::NeedsReader {
        store: &ins.store,
        ours: &ours_rs,
        class_id,
    };
    let of = |ty: &str, rs: Ruleset| -> Vec<String> {
        ins.ours
            .iter()
            .filter(|o| o.1 == ty && o.2 == rs)
            .map(|o| o.0.clone())
            .collect()
    };
    for rs in editions {
        let ids = base_ids(&d, rs, prefix);
        let base = d.create(rs, TEST_SCORES).expect("create");
        d.collect(&base, "L1", &mut offers);
        d.items(&base, &mut offers, "search_items");
        d.drop_host(base);
        for race in ["Race", "Species"].iter().flat_map(|t| of(t, rs)) {
            if let Some(mut h) = d.create(rs, TEST_SCORES) {
                if d.pick(&mut h, &race) {
                    d.collect(&h, &format!("race:{race}"), &mut offers);
                }
                d.drop_host(h);
            }
        }
        // every class the builder offers (bundled ones too: their spell lists name our spells)
        let mut classes = of("Class", rs);
        if let Some(h) = d.create(rs, TEST_SCORES) {
            if let Some(slot) = h
                .snap
                .slots
                .iter()
                .find(|s| s.element_type.eq_ignore_ascii_case("Class") && s.chosen.is_none())
            {
                classes.extend(d.options(&h, &slot.id).into_iter().map(|o| o.0));
            }
            d.drop_host(h);
        }
        classes.sort();
        classes.dedup();
        let mut subs: Vec<(String, String)> = Vec::new();
        for class in classes.iter().take(cap) {
            for a in progression(&d, rs, &ids, class, None, &mut offers) {
                subs.push((class.clone(), a));
            }
        }
        let ours_ids: BTreeSet<&str> = ins.ours.iter().map(|o| o.0.as_str()).collect();
        subs.retain(|(_, a)| ours_ids.contains(a.as_str()));
        subs.sort();
        subs.dedup_by(|a, b| a.1 == b.1);
        for (class, a) in subs.iter().take(cap) {
            progression(&d, rs, &ids, class, Some(a), &mut offers);
        }
        let mut tried = 0;
        for feat in of("Feat", rs) {
            if offers.found.contains_key(&feat) || tried >= cap {
                continue;
            }
            tried += 1;
            needs_host(&d, rs, &ids, &reader, &feat, &mut offers);
        }
    }
    metric(src, "s2", "*", "hosts_created", d.made.get() as i64);
    let selects = select_index(&ins.store);
    let mut rows: BTreeMap<(String, &str), [i64; 2]> = BTreeMap::new();
    let mut seen_types: BTreeSet<&str> = BTreeSet::new();
    for (id, ty, _) in &ins.ours {
        let el = ins.store.element(id);
        let category = if HOSTED.contains(&ty.as_str()) {
            "host"
        } else if let Some((_, why)) = NON_OFFERABLE.iter().find(|(t, _)| t == ty) {
            why
        } else if selects.get(&ty.to_lowercase()).is_some_and(|v| {
            v.iter().any(|tokens| match tokens {
                None => true,
                Some(t) => {
                    t.contains(&id.to_lowercase())
                        || el.is_some_and(|e| {
                            e.supports.iter().any(|s| t.contains(&s.to_lowercase()))
                        })
                }
            })
        }) {
            "select"
        } else {
            "Grants"
        };
        seen_types.insert(ty);
        if category != "host" && category != "select" {
            *rows
                .entry((ty.clone(), "nonoffer"))
                .or_default()
                .get_mut(0)
                .unwrap() += 1;
            continue;
        }
        let ok = offers.found.contains_key(id);
        let r = rows.entry((ty.clone(), "offerable")).or_default();
        r[0] += 1;
        if ok {
            r[1] += 1;
        }
        outcome::record(
            SUITE,
            &format!("{src}/s2/{ty}/{id}"),
            ok,
            "not offered by any host",
        );
    }
    for ((ty, kind), [n, offered]) in &rows {
        if *kind == "nonoffer" {
            metric(src, "s2", ty, "non_offerable", *n);
        } else {
            metric(src, "s2", ty, "offerable", *n);
            metric(src, "s2", ty, "offered", *offered);
        }
    }
    metric(src, "s2", "*", "types", seen_types.len() as i64);
    metric(src, "s2", "*", "unmapped_types", 0);
}

// ---------------------------------------------------------------- sources

/// `name:bytes` of every content file of the installed (non-bundled) packs.
fn pack_files(data: &Path) -> String {
    pack_dirs(data)
        .values()
        .map(|(d, _)| {
            // pack.json and pack-meta.json record the input's name (folder vs file.zip)
            files::tree_entries(d, "", &|r| r != "pack.json" && r != "pack-meta.json")
                .iter()
                .map(|(n, b)| format!("{n}:{}", b.len()))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn a_user_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/support/a-user")
}

fn aurora_folders(src: &str, dirs: &[PathBuf], zip_check: bool) {
    let (tmp, app) = app_state();
    let mut tally = AuroraTally::default();
    let mut c = AuroraCount::default();
    let mut ids = Vec::new();
    for dir in dirs {
        count_folder(dir, &mut c);
        ids.extend(aurora_install(
            &app,
            &AuroraInput::Folder(dir.clone()),
            &mut tally,
        ));
    }
    tally.s = c.types.clone();
    metric(src, "s1", "*", "malformed_tags", c.malformed as i64);
    let data = tmp.path().join("data");
    let ins = installed(&data, &|id| ids.iter().any(|i| i == id));
    aurora_stage1(src, &tally, &ins);
    // rerun into a fresh app: byte-identical packs
    let (tmp2, app2) = app_state();
    let mut t2 = AuroraTally::default();
    for dir in dirs {
        aurora_install(&app2, &AuroraInput::Folder(dir.clone()), &mut t2);
    }
    let again = installed(&tmp2.path().join("data"), &|id| ids.iter().any(|i| i == id)).digest;
    outcome::record(
        SUITE,
        &format!("{src}/s1/rerun_identical"),
        again == ins.digest,
        "",
    );
    metric(
        src,
        "s1",
        "*",
        "rerun_identical",
        i64::from(again == ins.digest),
    );
    if zip_check {
        for dir in dirs {
            let name = dir.file_name().unwrap().to_string_lossy().into_owned();
            // a zip of the folder itself (top folder kept, as Finder/Explorer zip it) and of its contents
            for (shape, prefix) in [("nested", format!("{name}/")), ("flat", String::new())] {
                let entries = files::tree_entries(dir, &prefix, &|r| r.ends_with(".xml"));
                let (tmp3, app3) = app_state();
                let mut t3 = AuroraTally::default();
                let zip = AuroraInput::Zip(format!("{name}.zip"), files::zip_stored(&entries));
                let zid = aurora_install(&app3, &zip, &mut t3);
                let (zipped, folder) = (pack_files(&tmp3.path().join("data")), pack_files(&data));
                let same = zid.is_some() && zipped == folder;
                if !same && src == "a-user" {
                    for root in [tmp3.path().join("data"), data.clone()] {
                        for (d, _) in pack_dirs(&root).values() {
                            eprintln!(
                                "  meta: {}",
                                std::fs::read_to_string(d.join("pack-meta.json"))
                                    .unwrap_or_default()
                            );
                        }
                    }
                    eprintln!(
                        "  zip ({shape}) pack files: {zipped}\n  folder pack files: {folder}"
                    );
                }
                outcome::record(
                    SUITE,
                    &format!("{src}/s1/zip_identical_{shape}/{name}"),
                    same,
                    "",
                );
                metric(
                    src,
                    "s1",
                    "*",
                    &format!("zip_identical_{shape}"),
                    i64::from(same),
                );
            }
        }
    }
    stage2(src, &app, &ins, "ID_WOTC_", |_, name| {
        format!(
            "ID_WOTC_PHB_CLASS_{}",
            name.to_uppercase().replace(' ', "_")
        )
    });
}

#[test]
fn acceptance_a_user() {
    run_big(|| aurora_folders("a-user", &[a_user_dir()], true));
}

#[test]
fn acceptance_a_comm() {
    let Ok(root) = std::env::var("BOREALIS_CORPUS_DIR") else {
        return skip("a-comm", "BOREALIS_CORPUS_DIR not set");
    };
    let root = PathBuf::from(root);
    let repos = if root.join("repos").is_dir() {
        root.join("repos")
    } else {
        root
    };
    let base = "aurorabuilder__elements";
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&repos)
        .expect("repos")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !p.file_name().unwrap().to_string_lossy().starts_with('.'))
        .collect();
    dirs.sort_by_key(|p| (!p.ends_with(base), p.clone()));
    run_big(move || aurora_folders("a-comm", &dirs, false));
}

struct Recording {
    files: Mutex<Vec<(String, Vec<u8>)>>,
}

impl Fetcher for Recording {
    fn get(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
        let r = HttpsFetcher.get(url, max_bytes);
        if let Ok(bytes) = &r {
            self.files
                .lock()
                .unwrap()
                .push((url.to_string(), bytes.clone()));
        }
        r
    }
}

fn core_install(url: &str) -> (tempfile::TempDir, AppState, Vec<(String, Vec<u8>)>, usize) {
    let (tmp, app) = app_state();
    app.add_index(url).expect("add index");
    let rec = Recording {
        files: Mutex::new(Vec::new()),
    };
    let result = app.update_indexes(&rec, &NullSink).expect("update");
    let fetched = rec.files.into_inner().unwrap();
    (tmp, app, fetched, result.failed.len())
}

#[test]
fn acceptance_a_core() {
    let Ok(url) = std::env::var("BOREALIS_LIVE_INDEX_URL") else {
        return skip("a-core", "BOREALIS_LIVE_INDEX_URL not set");
    };
    run_big(move || {
        let src = "a-core";
        let (tmp, app, fetched, failed) = core_install(&url);
        let mut c = AuroraCount::default();
        for (_, bytes) in &fetched {
            c.add_file(&String::from_utf8_lossy(bytes));
        }
        let mine = |id: &str| !BUNDLED.contains(&id) && !id.starts_with("5etools-");
        let non_bundled: Vec<String> = app
            .list_packs()
            .expect("packs")
            .into_iter()
            .filter(|p| p.kind != PackKind::Bundled)
            .map(|p| p.id)
            .collect();
        let ins = installed(&tmp.path().join("data"), &|id| {
            mine(id) && non_bundled.iter().any(|p| p == id)
        });
        let tally = AuroraTally {
            s: c.types.clone(),
            folder_errors: failed,
            ..AuroraTally::default()
        };
        metric(src, "s1", "*", "fetched_files", fetched.len() as i64);
        metric(src, "s1", "*", "warnings_not_reported", 1);
        aurora_stage1(src, &tally, &ins);
        let (tmp2, _app2, _, _) = core_install(&url);
        let again = installed(&tmp2.path().join("data"), &|id| {
            mine(id) && non_bundled.iter().any(|p| p == id)
        })
        .digest;
        outcome::record(
            SUITE,
            &format!("{src}/s1/rerun_identical"),
            again == ins.digest,
            "",
        );
        metric(
            src,
            "s1",
            "*",
            "rerun_identical",
            i64::from(again == ins.digest),
        );
        stage2(src, &app, &ins, "ID_WOTC_", |_, name| {
            format!(
                "ID_WOTC_PHB_CLASS_{}",
                name.to_uppercase().replace(' ', "_")
            )
        });
    });
}

fn five_zip(data: &Path) -> Vec<u8> {
    let keep = |rel: &str| {
        let root_file = !rel.contains('/');
        let sub = ["class/", "spells/", "homebrew/"]
            .iter()
            .any(|p| rel.starts_with(p));
        rel.ends_with(".json")
            && (root_file || sub || rel == "generated/gendata-spell-source-lookup.json")
    };
    files::zip_stored(&files::tree_entries(data, "data/", &keep))
}

#[test]
fn acceptance_f_full() {
    let Ok(dir) = std::env::var("BOREALIS_5ETOOLS_DIR") else {
        return skip("f-full", "BOREALIS_5ETOOLS_DIR not set");
    };
    run_big(move || {
        let src = "f-full";
        let data = PathBuf::from(dir);
        let (tmp, app) = app_state();
        let (request, sources) = five_request(&app, &data);
        let report = app.import_5etools(&request, &NullSink).expect("import");
        five_stage1(src, &data, &sources);
        metric(src, "s1", "*", "app_converted", report.converted as i64);
        metric(src, "s1", "*", "app_partial", report.partial.len() as i64);
        metric(src, "s1", "*", "app_skipped", report.skipped.len() as i64);
        let digest = five_digest(&tmp.path().join("data"));
        let (tmp2, app2) = app_state();
        let (r2, _) = five_request(&app2, &data);
        app2.import_5etools(&r2, &NullSink).expect("rerun");
        let same = five_digest(&tmp2.path().join("data")) == digest;
        outcome::record(SUITE, &format!("{src}/s1/rerun_identical"), same, "");
        metric(src, "s1", "*", "rerun_identical", i64::from(same));
        drop((tmp2, app2));
        let (tmp3, app3) = app_state();
        let zip = five_zip(&data);
        let zipped = app3
            .stage_5etools_zip(&ImportFileInput {
                file_name: "data.zip".into(),
                content_base64: base64::engine::general_purpose::STANDARD.encode(&zip),
            })
            .map_err(|e| e.message)
            .and_then(|picked| {
                let sources = app3
                    .scan_5etools_folder(&picked.token)
                    .map_err(|e| e.message)?;
                let req = FiveToolsImportRequest {
                    folder: picked.token,
                    sources: sources.sources.into_iter().map(|s| s.abbr).collect(),
                    include_fluff: true,
                };
                app3.import_5etools(&req, &NullSink).map_err(|e| e.message)
            });
        if let Err(e) = &zipped {
            eprintln!("  zip import refused: {e}");
        }
        let same = zipped.is_ok() && five_digest(&tmp3.path().join("data")) == digest;
        outcome::record(SUITE, &format!("{src}/s1/zip_identical"), same, "");
        metric(src, "s1", "*", "zip_identical", i64::from(same));
        metric(src, "s1", "*", "zip_bytes", zip.len() as i64);
        drop((tmp3, app3));
        let ins = installed(&tmp.path().join("data"), &|id| id.starts_with("5etools-"));
        stage2(src, &app, &ins, "ID_5ET_", prereq::class_id_5et);
    });
}

#[test]
fn acceptance_f_url() {
    let Ok(url) = std::env::var("BOREALIS_LIVE_5ETOOLS_URL") else {
        return skip("f-url", "BOREALIS_LIVE_5ETOOLS_URL not set");
    };
    run_big(move || {
        let src = "f-url";
        let (tmp, app) = app_state();
        let picked = app
            .stage_5etools_url(&HttpsFetcher, &url, &NullSink)
            .expect("download");
        let staged = app
            .paths
            .data
            .join(borealis_lib::content_fivetools_url::STAGE_DIR);
        let sources: Vec<String> = app
            .scan_5etools_folder(&picked.token)
            .expect("scan")
            .sources
            .into_iter()
            .map(|s| s.abbr)
            .collect();
        let request = FiveToolsImportRequest {
            folder: picked.token,
            sources: sources.clone(),
            include_fluff: true,
        };
        app.import_5etools(&request, &NullSink).expect("import");
        five_stage1(src, &staged, &sources);
        let digest = five_digest(&tmp.path().join("data"));
        if let Ok(dir) = std::env::var("BOREALIS_5ETOOLS_DIR") {
            let (tmp2, app2) = app_state();
            let (r2, _) = five_request(&app2, Path::new(&dir));
            app2.import_5etools(&r2, &NullSink).expect("folder import");
            let same = five_digest(&tmp2.path().join("data")) == digest;
            outcome::record(SUITE, &format!("{src}/s1/url_equals_folder"), same, "");
            metric(src, "s1", "*", "url_equals_folder", i64::from(same));
        }
    });
}

/// Run on a big stack (deep content recursion), propagating panics.
fn run_big<F: FnOnce() + Send + 'static>(f: F) {
    borealis_engine::invariants::enable_for_thread();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("acceptance thread");
}
