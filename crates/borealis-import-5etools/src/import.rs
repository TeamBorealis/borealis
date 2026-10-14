//! The importer pipeline: read a 5etools `data/` folder, resolve `_copy` / `_versions`,
//! convert every entity and assemble one pack per source.

use crate::convert::feature::key_of_feature;
use crate::convert::{self, Conv, Ent, Shared, State};
use crate::copy;
use crate::core_ids;
use crate::error::ImportError;
use crate::folder::{self, Loaded};
use crate::ids::IdKind;
use crate::model::{self, Feature};
use crate::options::{ImportOptions, Phase, ProgressEvent, ProgressFn};
use crate::pack::{build_pack, ImportedPack};
use crate::report::{ImportReport, PackReport, ReportNote};
use crate::source::{is_srd, ruleset_for, ruleset_of_entry};
use crate::spell_lookup::SpellClassLookup;
use crate::srd::SrdIndex;
use crate::text::RefKind;
use crate::util::{key, name_source_key};
use borealis_content::{Element, Ruleset};
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Everything an import run produces.
#[derive(Debug)]
pub struct ImportResult {
    /// One pack per source that produced at least one element, sorted by source.
    pub packs: Vec<ImportedPack>,
    /// What was converted, lost and skipped.
    pub report: ImportReport,
}

/// Entities selected for conversion, by kind.
#[derive(Default)]
struct Ents {
    races: Vec<Ent>,
    subraces: Vec<Ent>,
    classes: Vec<Ent>,
    subclasses: Vec<Ent>,
    class_features: Vec<Ent>,
    subclass_features: Vec<Ent>,
    backgrounds: Vec<Ent>,
    feats: Vec<Ent>,
    optfeatures: Vec<Ent>,
    spells: Vec<Ent>,
    items: Vec<Ent>,
    groups: Vec<Ent>,
    masteries: Vec<Ent>,
    languages: Vec<Ent>,
    conditions: Vec<(&'static str, Ent)>,
    catalog: Vec<(&'static str, Ent)>,
}

impl Ents {
    fn total(&self) -> usize {
        self.races.len()
            + self.subraces.len()
            + self.classes.len()
            + self.subclasses.len()
            + self.class_features.len()
            + self.subclass_features.len()
            + self.backgrounds.len()
            + self.feats.len()
            + self.optfeatures.len()
            + self.spells.len()
            + self.items.len()
            + self.groups.len()
            + self.masteries.len()
            + self.languages.len()
            + self.conditions.len()
            + self.catalog.len()
    }
}

/// Reports progress to the callback.
struct Progress<'a, 'b> {
    sink: &'a mut ProgressFn<'b>,
    current: usize,
    total: usize,
}

impl Progress<'_, '_> {
    fn send(&mut self, phase: Phase, current: usize, total: usize, message: &str) {
        (self.sink)(&ProgressEvent {
            phase,
            current,
            total,
            message: message.to_string(),
        });
    }

    fn group_done(&mut self, label: &str, count: usize) {
        self.current += count;
        let (current, total) = (self.current, self.total);
        self.send(Phase::Converting, current, total, label);
    }
}

/// Import a 5etools `data/` folder.
///
/// # Errors
/// Folder-level failures only ([`ImportError::NotADirectory`], [`ImportError::NotFiveTools`]).
/// Problems with single files or entries are recorded in the report instead.
pub fn import_folder(path: &Path, options: &ImportOptions) -> Result<ImportResult, ImportError> {
    import_folder_with_progress(path, options, &mut |_| {})
}

/// [`import_folder`] with a progress callback.
///
/// # Errors
/// See [`import_folder`].
pub fn import_folder_with_progress(
    path: &Path,
    options: &ImportOptions,
    progress: &mut ProgressFn<'_>,
) -> Result<ImportResult, ImportError> {
    let mut prog = Progress {
        sink: progress,
        current: 0,
        total: 0,
    };
    prog.send(Phase::Scanning, 0, 0, &path.display().to_string());
    let mut loaded = folder::load(path, |i, n, file| prog.send(Phase::Reading, i, n, file))?;
    let mut st = State::default();
    for (file, reason) in &loaded.failed {
        st.report.warn(format!("{file}: not imported: {reason}"));
    }
    prog.send(Phase::Resolving, 0, 0, "_copy and _versions");
    let mut case = BTreeMap::new();
    let ents = select_entities(&loaded, options, &mut st, &mut case);
    prog.total = ents.total();
    let sh = build_shared(&mut loaded, &ents, options, &mut st);
    convert_all(&sh, &ents, &mut st, &mut prog);
    let result = finish(&sh, st, &case);
    prog.send(Phase::Done, prog.total, prog.total, "done");
    Ok(result)
}

// ---------------------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------------------

/// Resolve one property array and return `(entry, file)` pairs.
fn resolved(
    loaded: &Loaded,
    prop: &str,
    opts: &ImportOptions,
    st: &mut State,
) -> Vec<(Value, String)> {
    let Some(entries) = loaded.props.get(prop) else {
        return Vec::new();
    };
    let raw: Vec<Value> = entries.iter().map(|e| e.value.clone()).collect();
    let r = copy::resolve_array(prop, &raw);
    for n in &r.notes {
        st.report.warn(format!("{prop}: {n}"));
    }
    for ((name, reason), origin) in r.failed.iter().zip(&r.failed_origins) {
        let source = raw[*origin].get("source").and_then(Value::as_str);
        if source.map_or(true, |s| opts.allows_source(s)) {
            let msg = format!("cannot resolve _copy: {reason}");
            st.report.skipped(&entries[*origin].file, prop, name, &msg);
        }
    }
    r.entries
        .into_iter()
        .zip(r.origins)
        .map(|(v, o)| (v, entries[o].file.clone()))
        .collect()
}

/// Apply the source / ruleset / SRD filters and wrap the survivors.
fn to_ents(
    items: Vec<(Value, String)>,
    kind: &str,
    opts: &ImportOptions,
    st: &mut State,
    case: &mut BTreeMap<String, String>,
) -> Vec<Ent> {
    let mut out = Vec::new();
    for (raw, file) in items {
        let name = raw
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let Some(source) = raw
            .get("source")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            st.report
                .skipped(&file, kind, &format!("{name}|?"), "entry has no source");
            continue;
        };
        let ruleset = ruleset_of_entry(&raw);
        let srd = is_srd(&raw);
        let wanted = opts.allows_source(&source)
            && opts.ruleset.map_or(true, |r| r == ruleset)
            && (!opts.only_srd || srd);
        if !wanted {
            continue;
        }
        case.entry(source.to_uppercase())
            .or_insert_with(|| source.clone());
        out.push(Ent {
            raw,
            file,
            name,
            source,
            ruleset,
            is_srd: srd,
        });
    }
    out
}

fn tagged(list: Vec<Ent>, tag: &'static str) -> Vec<(&'static str, Ent)> {
    list.into_iter().map(|e| (tag, e)).collect()
}

fn select_entities(
    loaded: &Loaded,
    opts: &ImportOptions,
    st: &mut State,
    case: &mut BTreeMap<String, String>,
) -> Ents {
    let mut get = |prop: &str, kind: &str, st: &mut State| {
        let items = resolved(loaded, prop, opts, st);
        to_ents(items, kind, opts, st, case)
    };
    let mut e = Ents {
        races: get("race", "race", st),
        subraces: get("subrace", "subrace", st),
        classes: get("class", "class", st),
        subclasses: get("subclass", "subclass", st),
        class_features: get("classFeature", "classFeature", st),
        subclass_features: get("subclassFeature", "subclassFeature", st),
        backgrounds: get("background", "background", st),
        feats: get("feat", "feat", st),
        optfeatures: get("optionalfeature", "optionalfeature", st),
        spells: get("spell", "spell", st),
        groups: get("itemGroup", "itemGroup", st),
        masteries: get("itemMastery", "itemMastery", st),
        languages: get("language", "language", st),
        ..Ents::default()
    };
    e.conditions = tagged(get("condition", "condition", st), "condition");
    e.conditions
        .extend(tagged(get("disease", "disease", st), "disease"));
    e.conditions
        .extend(tagged(get("status", "status", st), "status"));
    for (prop, tag) in [
        ("itemProperty", "itemProperty"),
        ("itemType", "itemType"),
        ("itemEntry", "itemEntry"),
        ("skill", "skill"),
        ("sense", "sense"),
    ] {
        e.catalog.extend(tagged(get(prop, tag, st), tag));
    }
    e.items = get("item", "item", st);
    e.items.extend(get("baseitem", "item", st));
    let bases: Vec<Value> = resolved(loaded, "baseitem", opts, st)
        .into_iter()
        .map(|(v, _)| v)
        .collect();
    let variants: Vec<Value> = resolved(loaded, "magicvariant", opts, st)
        .into_iter()
        .map(|(v, _)| v)
        .collect();
    let generated = convert::item::generate_variant_items(&variants, &bases)
        .into_iter()
        .map(|v| (v, "magicvariants.json".to_string()))
        .collect();
    e.items.extend(to_ents(generated, "item", opts, st, case));
    let generic = convert::item::generic_variant_items(&variants)
        .into_iter()
        .map(|v| (v, "magicvariants.json".to_string()))
        .collect();
    e.items.extend(to_ents(generic, "item", opts, st, case));
    e
}

// ---------------------------------------------------------------------------------------
// Shared lookup tables
// ---------------------------------------------------------------------------------------

fn race_key(name: &str, source: &str) -> String {
    format!("{}|{}", name.to_lowercase(), source.to_uppercase())
}

fn add_link(sh: &mut Shared, st: &mut State, link: RefKind, id: IdKind, ent: &Ent) -> String {
    let element_id = st.ids.named(id, &ent.source, &ent.name);
    sh.links.insert(
        (link, key(&ent.name), ent.source.to_uppercase()),
        element_id.clone(),
    );
    element_id
}

fn register_links(sh: &mut Shared, st: &mut State, e: &Ents) {
    for ent in &e.races {
        let kind = if ent.ruleset == Ruleset::Dnd2024 {
            IdKind::Species
        } else {
            IdKind::Race
        };
        let id = add_link(sh, st, RefKind::Race, kind, ent);
        sh.race_ids.insert(race_key(&ent.name, &ent.source), id);
    }
    for ent in &e.subraces {
        let parent = ent.raw.get("raceName").and_then(Value::as_str);
        let parent_source = ent.raw.get("raceSource").and_then(Value::as_str);
        let id = convert::race::subrace_id(st, ent);
        if let Some(name) = parent {
            let parent_key = race_key(name, parent_source.unwrap_or(&ent.source));
            for prop in convert::race::replaced_props(&ent.raw) {
                sh.race_replaced
                    .entry(parent_key.clone())
                    .or_default()
                    .entry(prop)
                    .or_default()
                    .push(id.clone());
            }
            sh.races_with_subraces.insert(parent_key);
        }
    }
    for ent in &e.classes {
        let id = add_link(sh, st, RefKind::Class, IdKind::Class, ent);
        let class_key = race_key(&ent.name, &ent.source);
        if let Some(title) = ent.raw.get("subclassTitle").and_then(Value::as_str) {
            sh.subclass_titles
                .insert(class_key.clone(), title.to_string());
        }
        if ent.raw.get("casterProgression").is_some()
            || ent.raw.get("spellcastingAbility").is_some()
        {
            sh.caster_classes.insert(ent.name.to_lowercase());
        }
        sh.class_ids.insert(class_key, id);
    }
    register_subclass_twins(sh, &e.subclasses);
    let simple = [
        (&e.backgrounds, RefKind::Background, IdKind::Background),
        (&e.feats, RefKind::Feat, IdKind::Feat),
        (
            &e.optfeatures,
            RefKind::OptionalFeature,
            IdKind::OptionalFeature,
        ),
        (&e.spells, RefKind::Spell, IdKind::Spell),
        (&e.items, RefKind::Item, IdKind::Item),
        (&e.groups, RefKind::Item, IdKind::Item),
    ];
    for (list, link, id) in simple {
        for ent in list {
            add_link(sh, st, link, id, ent);
        }
    }
    for ent in &e.languages {
        if core_ids::core_language_id(&ent.name).is_none() {
            let id = st.ids.id(IdKind::Language, "", &[&ent.name], &ent.name);
            sh.languages.entry(key(&ent.name)).or_insert(id);
        }
    }
}

fn register_subclass_twins(sh: &mut Shared, subclasses: &[Ent]) {
    let field = |ent: &Ent, k: &str| {
        ent.raw
            .get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let mut first: BTreeMap<String, (String, bool)> = BTreeMap::new();
    for ent in subclasses {
        let key = convert::subclass_twin_key(
            &ent.source,
            &field(ent, "className"),
            &field(ent, "shortName"),
        );
        let class_source = field(ent, "classSource").to_uppercase();
        match first.get_mut(&key) {
            None => {
                first.insert(key, (class_source, false));
            }
            Some((src, twin)) => *twin |= *src != class_source,
        }
    }
    sh.subclass_twins = first
        .into_iter()
        .filter(|(_, (_, twin))| *twin)
        .map(|(k, (src, _))| (k, src))
        .collect();
}

fn index_features(list: &[Ent], sub: bool) -> (Vec<Ent>, BTreeMap<String, usize>) {
    let mut kept: Vec<Ent> = Vec::new();
    let mut index = BTreeMap::new();
    for ent in list {
        let Ok(f) = model::parse::<Feature>(&ent.raw) else {
            // keep it so the converter reports the invalid shape
            kept.push(ent.clone());
            continue;
        };
        let k = key_of_feature(&f, sub);
        if index.contains_key(&k) {
            continue;
        }
        index.insert(k, kept.len());
        kept.push(ent.clone());
    }
    (kept, index)
}

fn fluff_table(
    loaded: &Loaded,
    opts: &ImportOptions,
    st: &mut State,
) -> BTreeMap<(String, String), Vec<Value>> {
    let mut table = BTreeMap::new();
    if !opts.include_fluff {
        return table;
    }
    for (target, entries) in &loaded.fluff {
        let raw: Vec<Value> = entries.iter().map(|e| e.value.clone()).collect();
        let r = copy::resolve_array(&format!("{target}Fluff"), &raw);
        for n in &r.notes {
            st.report.warn(format!("{target}Fluff: {n}"));
        }
        for v in r.entries {
            let name = v.get("name").and_then(Value::as_str).unwrap_or("");
            let source = v.get("source").and_then(Value::as_str).unwrap_or("");
            let Some(text) = v.get("entries").and_then(Value::as_array) else {
                continue;
            };
            table
                .entry((target.clone(), name_source_key(name, source)))
                .or_insert_with(|| text.clone());
        }
    }
    table
}

fn build_shared(loaded: &mut Loaded, e: &Ents, opts: &ImportOptions, st: &mut State) -> Shared {
    let mut sh = Shared {
        opts: opts.clone(),
        spell_classes: SpellClassLookup::new(loaded.spell_lookup.take()),
        srd: SrdIndex::load(&opts.srd_dirs),
        source_names: loaded.books.clone(),
        fluff: fluff_table(loaded, opts, st),
        ..Shared::default()
    };
    register_links(&mut sh, st, e);
    sh.spell_rows = convert::prereq_index::spell_rows(&sh, &e.spells);
    let (cf, cf_keys) = index_features(&e.class_features, false);
    sh.class_features = cf;
    sh.class_feature_keys = cf_keys;
    let (sf, sf_keys) = index_features(&e.subclass_features, true);
    sh.subclass_features = sf;
    sh.subclass_feature_keys = sf_keys;
    sh
}

// ---------------------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------------------

/// Convert one group of entities. Converters report their own failures, so an empty result
/// is not reported a second time.
fn run_group(
    label: &str,
    list: &[Ent],
    st: &mut State,
    prog: &mut Progress<'_, '_>,
    mut f: impl FnMut(&mut State, &Ent) -> Conv,
) {
    for ent in list {
        let conv = f(st, ent);
        if !conv.elements.is_empty() {
            st.emit(ent, label, conv);
        }
    }
    prog.group_done(label, list.len());
}

fn feature_done(st: &State, ent: &Ent, sub: bool) -> bool {
    let Ok(f) = model::parse::<Feature>(&ent.raw) else {
        return false;
    };
    let prefix = if sub { "s" } else { "c" };
    st.done_features
        .contains(&format!("{prefix}:{}", key_of_feature(&f, sub)))
}

fn convert_all(sh: &Shared, e: &Ents, st: &mut State, prog: &mut Progress<'_, '_>) {
    use convert::{background, class, feat, feature, item, misc, optfeat, race, spell};
    run_group("race", &e.races, st, prog, |st, ent| {
        race::convert(sh, st, ent, false)
    });
    run_group("subrace", &e.subraces, st, prog, |st, ent| {
        race::convert(sh, st, ent, true)
    });
    run_group("class", &e.classes, st, prog, |st, ent| {
        class::convert(sh, st, ent)
    });
    run_group("subclass", &e.subclasses, st, prog, |st, ent| {
        feature::convert_subclass(sh, st, ent)
    });
    let cf = sh.class_features.clone();
    run_group("classFeature", &cf, st, prog, |st, ent| {
        if feature_done(st, ent, false) {
            return Conv::default();
        }
        feature::convert_feature(sh, st, ent, false)
    });
    let sf = sh.subclass_features.clone();
    run_group("subclassFeature", &sf, st, prog, |st, ent| {
        if feature_done(st, ent, true) {
            return Conv::default();
        }
        feature::convert_feature(sh, st, ent, true)
    });
    run_group("background", &e.backgrounds, st, prog, |st, ent| {
        background::convert(sh, st, ent)
    });
    st.prereq = convert::prereq_index::PrereqIndex::build(
        sh,
        st,
        convert::prereq_index::Inputs {
            races: &e.races,
            subraces: &e.subraces,
            subclasses: &e.subclasses,
            feats: &e.feats,
            optfeatures: &e.optfeatures,
            spells: &e.spells,
        },
    );
    run_group("feat", &e.feats, st, prog, |st, ent| {
        feat::convert(sh, st, ent)
    });
    run_group("optionalfeature", &e.optfeatures, st, prog, |st, ent| {
        optfeat::convert(sh, st, ent)
    });
    run_group("spell", &e.spells, st, prog, |st, ent| {
        spell::convert(sh, st, ent)
    });
    run_group("item", &e.items, st, prog, |st, ent| {
        item::convert(sh, st, ent)
    });
    run_group("itemGroup", &e.groups, st, prog, |st, ent| {
        item::convert_group(sh, st, ent)
    });
    run_group("itemMastery", &e.masteries, st, prog, |st, ent| {
        item::convert_mastery(sh, st, ent)
    });
    run_group("language", &e.languages, st, prog, |st, ent| {
        misc::convert_language(sh, st, ent)
    });
    for (tag, ent) in &e.conditions {
        let conv = misc::convert_condition(sh, st, ent, tag);
        if !conv.elements.is_empty() {
            st.emit(ent, tag, conv);
        }
    }
    for (tag, ent) in &e.catalog {
        misc::skip_catalog(st, ent, tag);
    }
    prog.group_done(
        "conditions and catalogs",
        e.conditions.len() + e.catalog.len(),
    );
}

// ---------------------------------------------------------------------------------------
// Packs and report
// ---------------------------------------------------------------------------------------

fn ids_in(value: &Value, re: &Regex, out: &mut BTreeSet<String>) {
    match value {
        Value::String(s) => {
            out.extend(re.find_iter(s).map(|m| m.as_str().to_string()));
        }
        Value::Array(a) => a.iter().for_each(|v| ids_in(v, re, out)),
        // `renamed_from` names a former id of the element itself, not a reference
        Value::Object(m) => m
            .iter()
            .filter(|(k, _)| k.as_str() != "renamed_from")
            .for_each(|(_, v)| ids_in(v, re, out)),
        _ => {}
    }
}

/// `ID_5ET_*` ids referenced by the elements but defined by no imported element.
fn external_references(packs: &[ImportedPack], emitted: &BTreeSet<String>) -> Vec<String> {
    let Ok(re) = Regex::new(r"ID_5ET_[A-Z0-9_]*[A-Z0-9]") else {
        return Vec::new();
    };
    let mut referenced = BTreeSet::new();
    for pack in packs {
        for el in pack.elements() {
            if let Ok(v) = serde_json::to_value(el) {
                ids_in(&v, &re, &mut referenced);
            }
        }
    }
    referenced.difference(emitted).cloned().collect()
}

/// Emitted custom rules the engine would ignore (`rule_kinds::KNOWN_KINDS`), one note each.
fn ignored_rules(packs: &[ImportedPack]) -> Vec<ReportNote> {
    let mut out = Vec::new();
    for pack in packs {
        for el in pack.elements() {
            for problem in borealis_content::rule_kinds::check_rules(&el.rules) {
                out.push(ReportNote {
                    element: el.id.clone(),
                    kind: el.ty.clone(),
                    file: pack.manifest.id.clone(),
                    reason: format!("{}: {problem}", problem.code()),
                });
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn source_ruleset(abbr: &str, elements: &[Element]) -> Ruleset {
    let set: BTreeSet<Ruleset> = elements.iter().filter_map(|e| e.ruleset).collect();
    if set.len() == 1 {
        if let Some(r) = set.into_iter().next() {
            return r;
        }
    }
    ruleset_for(abbr, None)
}

fn finish(sh: &Shared, mut st: State, case: &BTreeMap<String, String>) -> ImportResult {
    let mut packs = Vec::new();
    for (upper, mut elements) in std::mem::take(&mut st.out) {
        let abbr = case.get(&upper).cloned().unwrap_or(upper);
        let ruleset = source_ruleset(&abbr, &elements);
        elements.push(convert::misc::source_element(sh, &mut st, &abbr, ruleset));
        packs.push(build_pack(
            &abbr,
            &sh.source_display(&abbr),
            &sh.opts.pack_version,
            elements,
        ));
    }
    let mut emitted = std::mem::take(&mut st.emitted);
    for el in packs.iter().flat_map(ImportedPack::elements) {
        emitted.insert(el.id.clone());
        // The synthetic multiclass id lives inside the class element.
        if let Some(mc) = &el.multiclass {
            emitted.insert(mc.id.clone());
        }
    }
    let external = external_references(&packs, &emitted);
    let mut report = std::mem::take(&mut st.report).build();
    report.external_references = external;
    report.errors = ignored_rules(&packs);
    report.packs = packs
        .iter()
        .map(|p| {
            let mut elements: BTreeMap<String, usize> = BTreeMap::new();
            for e in p.elements() {
                *elements.entry(e.ty.clone()).or_default() += 1;
            }
            PackReport {
                source: p.source.clone(),
                pack_id: p.manifest.id.clone(),
                ruleset: p.ruleset.map_or("mixed", Ruleset::as_str).to_string(),
                elements,
            }
        })
        .collect();
    ImportResult { packs, report }
}
