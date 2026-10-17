//! Local-only engine sweep over a real, full 5etools `data/` folder. Enabled by
//! `BOREALIS_5ETOOLS_DIR` (the `data/` directory of a 5etools checkout); skipped otherwise.
//! Prints counts only, never content text. Run it in release mode:
//!
//! ```text
//! BOREALIS_5ETOOLS_DIR=/path/to/5etools/data cargo test --release -p borealis \
//!     --test fivetools_corpus -- --nocapture
//! ```
//!
//! Every imported class (both editions) and every subclass takes a test character from level 1
//! to 20; every species, background and feat is picked on a level-1
//! character; weapons and armor are added and equipped. Failures are counted by cause; every swept
//! kind must have zero failures, and a full folder must hold at least 25 classes and 300
//! subclasses (thresholds set from a full 2026-10 5etools checkout).

#[path = "../../tools/regression-gate/outcome.rs"]
mod outcome;

use borealis_content::expr::{BoolExpr, RequirementLeaf, RequirementsExpr};
use borealis_content::{ContentStore, ContentStoreBuilder, Element, Rule, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{invariants, CharacterSnapshot, CharacterState, Choice, Engine, Severity};
use borealis_import_5etools::{import_folder, ImportOptions};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

type Counts = BTreeMap<String, usize>;

const TEST_SCORES: [i32; 6] = [15, 14, 14, 12, 12, 10];
/// Upper bound of slots filled automatically on one character.
const FILL_PASSES: usize = 60;
/// Slot types the automatic fill leaves to the sweep itself.
const NOT_FILLED: &[&str] = &[
    "Race",
    "Sub Race",
    "Species",
    "Background",
    "Class",
    "Archetype",
    "Feat",
    "Multiclass",
];
const MAX_LEVEL: u32 = 20;

/// Thresholds established on the 2026-10 corpus. Every swept kind must have zero failures
/// (see `failed` below); these are the lower bounds on what a full folder contains.
const MIN_CLASSES: usize = 25;
const MIN_SUBCLASSES: usize = 300;

fn bump(map: &mut Counts, key: impl Into<String>) {
    *map.entry(key.into()).or_default() += 1;
}

fn table(title: &str, map: &Counts) {
    println!("--- {title}");
    let mut rows: Vec<_> = map.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (k, v) in rows.into_iter().take(40) {
        println!("{v:>8}  {k}");
    }
}

fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../content")
}

/// Strip digits so failure messages group by cause.
fn cause(message: &str) -> String {
    let mut out: String = message
        .chars()
        .map(|c| if c.is_ascii_digit() { '#' } else { c })
        .collect();
    out.truncate(110);
    out
}

struct Sweeper<'a> {
    engine: &'a Engine,
    store: &'a ContentStore,
    ours: &'a HashMap<String, Ruleset>,
}

impl Sweeper<'_> {
    fn base(&self, ruleset: Ruleset) -> CharacterState {
        let state = commands::new_character(self.engine, ruleset);
        commands::set_base_scores(self.engine, &state, TEST_SCORES)
            .map(|o| o.state)
            .unwrap_or(state)
    }

    fn with_class(&self, ruleset: Ruleset, class: &str) -> Result<CharacterState, String> {
        let state = self.base(ruleset);
        let snap = self.engine.derive(&state);
        let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).ok_or("no class slot")?;
        commands::select(self.engine, &state, &key, Choice::Element(class.into()))
            .map(|o| o.state)
            .map_err(|e| format!("class refused: {e}"))
    }

    /// Problems of a snapshot that concern imported elements.
    fn problems(&self, state: &CharacterState, snap: &CharacterSnapshot, out: &mut Vec<String>) {
        // settle invariants (engine-test-audit-1 WP2); unknown rules of elements outside the
        // import belong to other tests
        for v in invariants::check(self.engine, state, snap) {
            let foreign = v.kind == invariants::kind::UNKNOWN_RULE
                && v.element
                    .as_deref()
                    .is_some_and(|e| !self.ours.contains_key(e));
            if !foreign {
                out.push(format!("{} {}", v.kind, v.message));
            }
        }
        for d in &snap.diagnostics {
            let ours = d
                .element
                .as_ref()
                .is_some_and(|e| self.ours.contains_key(e));
            if d.severity == Severity::Error && ours {
                out.push(format!("diagnostic {}: {}", d.code, d.message));
            }
        }
        for inv in &snap.invalid_selections {
            if self.ours.contains_key(&inv.slot.owner) {
                out.push("broken selection".into());
            }
        }
        for s in &snap.slots {
            if self.ours.contains_key(&s.owner.id)
                && s.ty != "Archetype"
                && s.options.is_empty()
                && s.list_items.is_empty()
            {
                out.push(format!("select of type {} offers no options", s.ty));
            }
        }
    }

    /// Fill open slots with their first option, as a user would, so that selects depending on
    /// earlier choices (expertise needs a proficiency) can be judged. Slots that refuse are left.
    fn fill(&self, mut state: CharacterState) -> CharacterState {
        let mut refused = std::collections::HashSet::new();
        for _ in 0..FILL_PASSES {
            let snap = self.engine.derive(&state);
            let open = snap.slots.iter().find(|s| {
                s.chosen.is_none()
                    && s.list_choice.is_none()
                    && !NOT_FILLED.contains(&s.ty.as_str())
                    && !refused.contains(&s.key)
                    && (!s.options.is_empty() || !s.list_items.is_empty())
            });
            let Some(slot) = open else { break };
            let choice = match (slot.options.first(), slot.list_items.first()) {
                (Some(o), _) => Choice::Element(o.id.clone()),
                (None, Some((item, _))) => Choice::ListItem(*item),
                (None, None) => break,
            };
            match commands::select(self.engine, &state, &slot.key, choice) {
                Ok(o) => state = o.state,
                Err(_) => {
                    refused.insert(slot.key.clone());
                }
            }
        }
        state
    }

    /// Choose `target` in the first open slot that offers it.
    fn pick(&self, state: &CharacterState, target: &str) -> Option<Result<CharacterState, String>> {
        let snap = self.engine.derive(state);
        let slot = snap
            .slots
            .iter()
            .find(|s| s.chosen.is_none() && s.options.iter().any(|o| o.id == target))?;
        Some(
            commands::select(
                self.engine,
                state,
                &slot.key,
                Choice::Element(target.into()),
            )
            .map(|o| o.state)
            .map_err(|e| format!("select refused: {e}")),
        )
    }

    /// Levels 1-20 for a class, optionally picking a subclass. Returns failure causes.
    fn sweep(&self, class: &Element, sub: Option<&str>, ruleset: Ruleset) -> Vec<String> {
        let mut out = Vec::new();
        let mut state = match self.with_class(ruleset, &class.id) {
            Ok(s) => s,
            Err(e) => return vec![e],
        };
        let expected: Vec<(&str, u32)> = class
            .rules
            .iter()
            .filter_map(|r| match r {
                Rule::Grant(g) if g.ty.as_deref() == Some("Class Feature") => {
                    Some((g.id.as_str(), g.level.unwrap_or(1)))
                }
                _ => None,
            })
            .filter(|(id, _)| self.ours.contains_key(*id))
            .collect();
        let mut picked = sub.is_none();
        let mut slots_seen = 0;
        for level in 1..=MAX_LEVEL {
            if level > 1 {
                match commands::level_up_main(self.engine, &state, None) {
                    Ok(o) => state = o.state,
                    Err(e) => {
                        out.push(format!("level-up fails: {e}"));
                        break;
                    }
                }
            }
            if let (Some(s), false) = (sub, picked) {
                if let Some(r) = self.pick(&state, s) {
                    match r {
                        Ok(next) => {
                            state = next;
                            picked = true;
                        }
                        Err(e) => out.push(e),
                    }
                }
            }
            state = self.fill(state);
            let snap = self.engine.derive(&state);
            self.problems(&state, &snap, &mut out);
            for (fid, flevel) in &expected {
                if *flevel == level && !snap.features.iter().any(|f| f.id == *fid) {
                    out.push("class feature missing at its level".into());
                }
            }
            let slots: i32 = snap
                .spellcasting
                .iter()
                .map(|s| s.slots.iter().sum::<i32>() + s.pact.map_or(0, |p| p.0 * p.1))
                .sum();
            if slots < slots_seen {
                out.push("spell slots shrink".into());
            }
            slots_seen = slots;
            if snap.level != level {
                out.push("character level does not match".into());
            }
        }
        if !picked {
            out.push("subclass never offered".into());
        }
        out.sort();
        out.dedup();
        out
    }
}

#[test]
fn five_etools_engine_sweep() {
    let Ok(dir) = std::env::var("BOREALIS_5ETOOLS_DIR") else {
        eprintln!("five_etools_engine_sweep: BOREALIS_5ETOOLS_DIR not set, skipping");
        return;
    };
    let handle = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || run(Path::new(&dir)))
        .expect("spawn");
    handle.join().expect("sweep thread");
}

fn run(data: &Path) {
    let options = ImportOptions {
        srd_dirs: vec![
            content_dir().join("srd-5.1"),
            content_dir().join("srd-5.2.1"),
        ],
        ..ImportOptions::default()
    };
    let result = import_folder(data, &options).expect("import");
    let tmp = tempfile::tempdir().unwrap();
    let mut builder = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        builder = builder.add_pack_dir(&content_dir().join(pack)).unwrap();
    }
    let mut ours: HashMap<String, Ruleset> = HashMap::new();
    let mut elements: Vec<(Element, Ruleset)> = Vec::new();
    for pack in &result.packs {
        builder = builder
            .add_pack_dir(&pack.write_to(tmp.path()).unwrap())
            .unwrap();
        for e in pack.elements() {
            let rs = e.ruleset.or(pack.ruleset).unwrap_or(Ruleset::Dnd2014);
            ours.insert(e.id.clone(), rs);
            elements.push((e.clone(), rs));
        }
    }
    let store = builder.build();
    let engine = Engine::new(&store);
    let sw = Sweeper {
        engine: &engine,
        store: &store,
        ours: &ours,
    };
    let by_id: HashMap<&str, &(Element, Ruleset)> =
        elements.iter().map(|x| (x.0.id.as_str(), x)).collect();
    let of_type = |ty: &str| -> Vec<&(Element, Ruleset)> {
        elements.iter().filter(|(e, _)| e.ty == ty).collect()
    };

    // classes and subclasses
    let mut causes: BTreeMap<&str, Counts> = BTreeMap::new();
    let mut failed: BTreeMap<&str, usize> = BTreeMap::new();
    let mut tried: BTreeMap<&str, usize> = BTreeMap::new();
    let mut record = |kind: &'static str, id: &str, problems: Vec<String>| {
        outcome::record(
            "fivetools_corpus",
            &format!("{kind}/{id}"),
            problems.is_empty(),
            &problems.join("; "),
        );
        *tried.entry(kind).or_default() += 1;
        if !problems.is_empty() {
            *failed.entry(kind).or_default() += 1;
        }
        for p in problems {
            bump(causes.entry(kind).or_default(), cause(&p));
        }
    };
    let classes = of_type("Class");
    for (class, rs) in &classes {
        record("class", &class.id, sw.sweep(class, None, *rs));
    }
    for (sub, _) in of_type("Archetype") {
        // a subclass is swept on its parent class's edition (5etools attaches 2014 subclasses
        // to 2024 classes too)
        let Some((class, rs)) = sub
            .supports
            .first()
            .and_then(|c| by_id.get(c.as_str()).copied())
        else {
            record(
                "subclass",
                &sub.id,
                vec!["parent class not imported".into()],
            );
            continue;
        };
        record("subclass", &sub.id, sw.sweep(class, Some(&sub.id), *rs));
    }

    // origins and feats on a level-1 character of a fixed class
    let fighter = |rs: Ruleset| match rs {
        Ruleset::Dnd2024 => "ID_5ET_XPHB_CLASS_FIGHTER",
        _ => "ID_5ET_PHB_CLASS_FIGHTER",
    };
    for (ty, kind) in [
        ("Race", "species"),
        ("Species", "species"),
        ("Sub Race", "subrace"),
        ("Background", "background"),
    ] {
        for (el, rs) in of_type(ty) {
            let mut problems = Vec::new();
            match sw.with_class(*rs, fighter(*rs)) {
                Err(e) => problems.push(e),
                Ok(mut state) => {
                    if ty == "Sub Race" {
                        if let Some(parent) = el.supports.first() {
                            if let Some(Ok(s)) = sw.pick(&state, parent) {
                                state = s;
                            }
                        }
                    }
                    match sw.pick(&state, &el.id) {
                        None => problems.push(format!("{ty} is not offered")),
                        Some(Err(e)) => problems.push(e),
                        Some(Ok(s)) => {
                            let s = sw.fill(s);
                            sw.problems(&s, &sw.engine.derive(&s), &mut problems);
                        }
                    }
                }
            }
            problems.sort();
            problems.dedup();
            record(kind, &el.id, problems);
        }
    }

    // feats: offered somewhere on a fighter (variant human / human origin feat at 1, the
    // ability score improvement at 4, the epic boon at 19), then picked without problems
    let hosts: HashMap<Ruleset, Vec<CharacterState>> = [Ruleset::Dnd2014, Ruleset::Dnd2024]
        .into_iter()
        .map(|rs| (rs, feat_hosts(&sw, rs, fighter(rs))))
        .collect();
    let mut unoffered = Counts::new();
    for (el, rs) in of_type("Feat") {
        let mut problems = Vec::new();
        // the shared hosts first; a feat with prerequisites they lack gets a host that meets
        // them (its race, a caster class, the ability score, the prior feat, the level)
        let mut why = String::new();
        let picked = hosts[rs]
            .iter()
            .find_map(|h| sw.pick(h, &el.id))
            .or_else(|| eligible_host(&sw, *rs, el, &mut why));
        outcome::record(
            "fivetools_corpus",
            &format!("feat_offered/{}", el.id),
            picked.is_some(),
            &format!("not offered: {why}; requirements {:?}", el.requirements),
        );
        match picked {
            None => {
                // `data.prerequisite` is the structured feature check (Spellcasting, ...)
                let why = if !el.requirements.is_empty() || el.data.contains_key("prerequisite") {
                    "has requirements no test host meets"
                } else {
                    "no requirements"
                };
                let category = el.supports.first().map_or("none", |c| {
                    if c.starts_with("ID_") {
                        "a parent feat"
                    } else {
                        c.as_str()
                    }
                });
                bump(&mut unoffered, format!("{rs:?} {category}: {why}"));
            }
            Some(Err(e)) => problems.push(e),
            Some(Ok(s)) => {
                let s = sw.fill(s);
                sw.problems(&s, &sw.engine.derive(&s), &mut problems);
            }
        }
        problems.sort();
        problems.dedup();
        record("feat", &el.id, problems);
    }

    // gear: add and equip every weapon and armor
    for ty in ["Weapon", "Armor"] {
        for (el, rs) in of_type(ty) {
            let mut problems = Vec::new();
            let state = sw
                .with_class(*rs, fighter(*rs))
                .unwrap_or_else(|_| sw.base(*rs));
            match commands::add_item(sw.engine, &state, &el.id, 1) {
                Err(e) => problems.push(format!("add refused: {e}")),
                Ok(o) => {
                    let uid = o.state.inventory.items.last().unwrap().uid.clone();
                    match commands::equip(sw.engine, &o.state, &uid, None) {
                        Err(e) => problems.push(cause(&format!("equip refused: {e}"))),
                        Ok(eq) => {
                            let snap = sw.engine.derive(&eq.state);
                            sw.problems(&eq.state, &snap, &mut problems);
                            if ty == "Weapon" && snap.attacks.is_empty() {
                                problems.push("equipped weapon gives no attack".into());
                            }
                        }
                    }
                }
            }
            record("equip", &el.id, problems);
        }
    }

    // spells: every imported spell is on at least one class list
    let spells = of_type("Spell");
    let without_list = spells.iter().filter(|(s, _)| s.supports.is_empty()).count();

    println!("=== 5etools engine sweep");
    for (kind, n) in &tried {
        println!(
            "{kind:>12}: {n} tried, {} with problems",
            failed.get(kind).unwrap_or(&0)
        );
    }
    println!(
        "{:>12}: {} imported, {without_list} on no class list",
        "spells",
        spells.len()
    );
    for (kind, c) in &causes {
        table(&format!("{kind} problems by cause"), c);
    }
    table("feats not offered to any test host", &unoffered);
    let unoffered_plain: usize = unoffered
        .iter()
        .filter(|(k, _)| k.ends_with("no requirements") && !k.contains("parent feat"))
        .map(|(_, v)| v)
        .sum();
    assert!(classes.len() >= MIN_CLASSES, "classes: {}", classes.len());
    assert!(tried.get("subclass").copied().unwrap_or(0) >= MIN_SUBCLASSES);
    assert!(
        failed.is_empty(),
        "every class, subclass, species, background, feat and item must sweep cleanly: {failed:?}"
    );
    assert_eq!(
        unoffered_plain, 0,
        "feats without requirements must be offered somewhere"
    );
}

/// Level 1 (with a species that grants a feat), 4 (feat instead of the improvement) and 19
/// characters of a class, on which feats are looked for.
fn feat_hosts(sw: &Sweeper<'_>, rs: Ruleset, class: &str) -> Vec<CharacterState> {
    let Ok(state) = sw.with_class(rs, class) else {
        return Vec::new();
    };
    // the class's own choices (skills, fighting style) first, as a user would
    let mut state = sw.fill(state);
    let species = match rs {
        Ruleset::Dnd2024 => "ID_5ET_XPHB_SPECIES_HUMAN",
        _ => "ID_5ET_PHB_RACE_HUMAN",
    };
    if let Some(Ok(s)) = sw.pick(&state, species) {
        state = s;
    }
    if let Some(Ok(s)) = sw.pick(&state, "ID_5ET_PHB_SUBRACE_HUMAN_VARIANT") {
        state = s;
    }
    let mut state = sw.fill(state);
    let mut out = vec![state.clone()];
    for target in [4, 19] {
        while sw.engine.derive(&state).level < target {
            match commands::level_up_main(sw.engine, &state, None) {
                Ok(o) => state = o.state,
                Err(_) => return out,
            }
        }
        // 2014: take the "Feat" improvement option so the feat select opens
        let snap = sw.engine.derive(&state);
        let feat_option = snap.slots.iter().find_map(|s| {
            let o = s.options.iter().find(|o| o.name == "Feat")?;
            (s.chosen.is_none() && s.ty == "Class Feature").then(|| (s.key.clone(), o.id.clone()))
        });
        let mut host = state.clone();
        if let Some((key, id)) = feat_option {
            if let Ok(o) = commands::select(sw.engine, &host, &key, Choice::Element(id)) {
                host = o.state;
            }
        }
        out.push(host);
    }
    out
}

/// Ability keys of requirement tests, in score order.
const ABILITY_KEYS: [&str; 6] = ["str", "dex", "con", "int", "wis", "cha"];
/// How deep prior-feat prerequisites are followed (a feat needing a feat needing a feat).
const MAX_PRIOR_FEATS: usize = 3;

/// What a feat's requirements ask of a character: the first buildable alternative of every
/// OR, negations ignored (a fresh character has none of the excluded elements).
#[derive(Debug, Default)]
struct Needs {
    level: u32,
    caster: bool,
    class: Option<String>,
    race: Option<String>,
    feats: Vec<String>,
    scores: Vec<(usize, i32)>,
}

impl Sweeper<'_> {
    fn class_id(rs: Ruleset, name: &str) -> String {
        let book = if rs == Ruleset::Dnd2024 {
            "XPHB"
        } else {
            "PHB"
        };
        format!(
            "ID_5ET_{book}_CLASS_{}",
            name.to_uppercase().replace(' ', "_")
        )
    }

    /// An id a host can use: it exists and, for origins, is of the feat's edition.
    fn usable(&self, rs: Ruleset, id: &str) -> bool {
        self.store.element(id).is_some_and(|e| {
            !matches!(e.ty.as_str(), "Race" | "Species" | "Sub Race")
                || e.ruleset.or_else(|| self.ours.get(id).copied()) == Some(rs)
        })
    }

    fn buildable(&self, rs: Ruleset, expr: &BoolExpr<RequirementLeaf>) -> bool {
        match expr {
            BoolExpr::Leaf(RequirementLeaf::Id(id)) => self.usable(rs, id),
            BoolExpr::Leaf(RequirementLeaf::Test(_)) | BoolExpr::Not(_) => true,
            BoolExpr::And(items) => items.iter().all(|e| self.buildable(rs, e)),
            BoolExpr::Or(items) => items.iter().any(|e| self.buildable(rs, e)),
        }
    }

    fn collect(&self, rs: Ruleset, expr: &BoolExpr<RequirementLeaf>, needs: &mut Needs) {
        match expr {
            BoolExpr::Not(_) => {}
            BoolExpr::And(items) => items.iter().for_each(|e| self.collect(rs, e, needs)),
            BoolExpr::Or(items) => {
                if let Some(first) = items
                    .iter()
                    .find(|e| self.buildable(rs, e))
                    .or(items.first())
                {
                    self.collect(rs, first, needs);
                }
            }
            BoolExpr::Leaf(RequirementLeaf::Id(id)) => {
                match self.store.element(id).map(|e| e.ty.as_str()) {
                    Some("Race" | "Species" | "Sub Race") => {
                        needs.race.get_or_insert_with(|| id.clone());
                    }
                    Some("Feat") => needs.feats.push(id.clone()),
                    Some("Class") => {
                        needs.class.get_or_insert_with(|| id.clone());
                    }
                    _ => {}
                }
            }
            BoolExpr::Leaf(RequirementLeaf::Test(t)) => {
                let key = t.normalized_key();
                let n = match &t.value {
                    borealis_content::expr::TestValue::Number(n) => u32::try_from(*n).unwrap_or(0),
                    borealis_content::expr::TestValue::Text(_) => 0,
                };
                let feature = key.strip_prefix("feature:").map(str::to_string);
                match (key.as_str(), feature.as_deref()) {
                    ("character" | "level", _) => needs.level = needs.level.max(n),
                    ("spellcasting", _) => needs.caster = true,
                    (_, Some("spellcasting" | "pact magic")) => needs.caster = true,
                    (k, _) if ABILITY_KEYS.contains(&k) => {
                        let i = ABILITY_KEYS.iter().position(|a| *a == k).unwrap_or(0);
                        needs.scores.push((i, i32::try_from(n).unwrap_or(0)));
                    }
                    (k, None) if self.store.element(&Self::class_id(rs, k)).is_some() => {
                        needs.class.get_or_insert_with(|| Self::class_id(rs, k));
                        needs.level = needs.level.max(n);
                    }
                    _ => {}
                }
            }
        }
    }

    /// The needs of an element's requirements, prior feats' own needs included.
    fn needs(&self, rs: Ruleset, el: &Element, depth: usize, needs: &mut Needs) {
        let Ok(Some(expr)) = RequirementsExpr::parse(&el.requirements) else {
            return;
        };
        let before = needs.feats.len();
        self.collect(rs, expr.root(), needs);
        if depth >= MAX_PRIOR_FEATS {
            return;
        }
        // a prior feat's own prerequisites come first
        let prior: Vec<String> = needs.feats[before..].to_vec();
        for id in prior {
            if let Some(feat) = self.store.element(&id) {
                let mut inner = Needs::default();
                self.needs(rs, feat, depth + 1, &mut inner);
                needs.level = needs.level.max(inner.level);
                needs.caster |= inner.caster;
                needs.class = needs.class.take().or(inner.class);
                needs.race = needs.race.take().or(inner.race);
                needs.scores.extend(inner.scores);
                let at = needs.feats.iter().position(|f| *f == id).unwrap_or(0);
                for f in inner.feats.into_iter().rev() {
                    if !needs.feats.contains(&f) {
                        needs.feats.insert(at, f);
                    }
                }
            }
        }
    }

    /// A level-1 character of the class, species and scores the needs ask for.
    fn start(&self, rs: Ruleset, needs: &Needs, fighter: &str) -> Option<CharacterState> {
        let wizard = Self::class_id(rs, "wizard");
        let class = needs.class.clone().unwrap_or_else(|| {
            if needs.caster {
                wizard
            } else {
                fighter.to_string()
            }
        });
        let mut scores = TEST_SCORES;
        for (i, v) in &needs.scores {
            scores[*i] = scores[*i].max(*v);
        }
        let state = commands::new_character(self.engine, rs);
        let state = commands::set_base_scores(self.engine, &state, scores)
            .ok()?
            .state;
        let snap = self.engine.derive(&state);
        let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1)?;
        let state = commands::select(self.engine, &state, &key, Choice::Element(class))
            .ok()?
            .state;
        let mut state = self.fill(state);
        let human = match rs {
            Ruleset::Dnd2024 => "ID_5ET_XPHB_SPECIES_HUMAN",
            _ => "ID_5ET_PHB_RACE_HUMAN",
        };
        let race = needs.race.clone().unwrap_or_else(|| human.to_string());
        let mut chain = Vec::new();
        if let Some(parent) = self
            .store
            .element(&race)
            .filter(|e| e.ty == "Sub Race")
            .and_then(|e| e.supports.first())
        {
            chain.push(parent.clone());
        }
        chain.push(race);
        if needs.race.is_none() {
            chain.push("ID_5ET_PHB_SUBRACE_HUMAN_VARIANT".into());
        }
        for id in chain {
            if let Some(Ok(s)) = self.pick(&state, &id) {
                state = s;
            }
        }
        Some(self.fill(state))
    }

    /// Take the 2014 "Feat" option of open improvement selects so the feat select opens.
    fn open_feat_selects(&self, mut state: CharacterState) -> CharacterState {
        loop {
            let snap = self.engine.derive(&state);
            let feat_option = snap.slots.iter().find_map(|s| {
                let o = s.options.iter().find(|o| o.name == "Feat")?;
                (s.chosen.is_none() && s.ty == "Class Feature")
                    .then(|| (s.key.clone(), o.id.clone()))
            });
            let Some((key, id)) = feat_option else {
                return state;
            };
            match commands::select(self.engine, &state, &key, Choice::Element(id)) {
                Ok(o) => state = o.state,
                Err(_) => return state,
            }
        }
    }
}

/// A character that meets a feat's requirements, levelled until the feat is offered; the
/// result of picking it (as [`Sweeper::pick`]), or `None` when no such host offers it.
fn eligible_host(
    sw: &Sweeper<'_>,
    rs: Ruleset,
    el: &Element,
    why: &mut String,
) -> Option<Result<CharacterState, String>> {
    let mut needs = Needs::default();
    sw.needs(rs, el, 0, &mut needs);
    *why = format!("host {needs:?} could not be built");
    let fighter = match rs {
        Ruleset::Dnd2024 => "ID_5ET_XPHB_CLASS_FIGHTER",
        _ => "ID_5ET_PHB_CLASS_FIGHTER",
    };
    let mut state = sw.start(rs, &needs, fighter)?;
    let mut pending = needs.feats.clone();
    loop {
        state = sw.open_feat_selects(state);
        // prior feats, in order, wherever a feat slot offers them
        while let Some(next) = pending.first() {
            match sw.pick(&state, next) {
                Some(Ok(s)) => {
                    state = sw.fill(s);
                    pending.remove(0);
                }
                _ => break,
            }
        }
        let snap = sw.engine.derive(&state);
        if pending.is_empty() && snap.level >= needs.level {
            if let Some(r) = sw.pick(&state, &el.id) {
                return Some(r);
            }
        }
        let open: Vec<&str> = snap
            .slots
            .iter()
            .filter(|s| s.chosen.is_none() && s.list_choice.is_none())
            .map(|s| s.ty.as_str())
            .collect();
        *why = format!(
            "host {needs:?} reached level {} (prior feats not placed {pending:?}, open slots {open:?})",
            snap.level
        );
        if snap.level >= MAX_LEVEL {
            return None;
        }
        match commands::level_up_main(sw.engine, &state, None) {
            Ok(o) => state = o.state,
            Err(e) => {
                why.push_str(&format!("; level-up refused: {e}"));
                return None;
            }
        }
    }
}
