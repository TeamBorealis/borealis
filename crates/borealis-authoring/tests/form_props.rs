//! Spec 18.2 "Random" (WB-27a): proptest-generated forms per kind -> elements -> schema validation -> form again is a
//! fixed point.
//!
//! A generated form starts from the form of a bundled element of the kind, made new (`support::recreated`: no stored
//! text, nothing kept), then a random list of edits is applied, the way a user edits: a text field typed, a check box
//! flipped, a number set inside its range, a combo choice picked, chips typed, a field given another element's value of
//! the same kind (every control's real shapes, from class tables to creature actions), rule blocks removed, moved or
//! taken over from any other bundled element. Then:
//!
//! * the form writes elements, or answers with problems that each have a path and a plain sentence (a form check, not
//!   a crash); writing is the common case (asserted over the run);
//! * every written element passes the content schema (`validate_content`);
//! * read again as the creator opens it and written again, the elements are the same (the written form is a fixed
//!   point), and read a third time the form has the same values.
//!
//! The rich-text documents and expressions have their own random suites (`borealis-content/tests/rich_text_props.rs`:
//! random documents and HTML soup; `expr_props.rs`: random builder models against the engine's own readers).
//!
//! Cases: 512 in a normal run (a few seconds in debug), seeded so CI is deterministic. A longer local soak:
//! `PROPTEST_CASES=20000 PROPTEST_RNG_SEED=<n> cargo test --release -p borealis-authoring --test form_props`.

mod support;

use borealis_authoring::form::{
    element_to_form, element_tree_to_form, form_to_elements, kind, Block, CatalogBlockCodec,
    Control, FormContext, FormState, KindDef,
};
use borealis_content::schema::validate_content;
use borealis_content::{ContentStore, ContentStoreBuilder, Element};
use proptest::prelude::*;
use proptest::sample::Index;
use proptest::test_runner::{Config, RngSeed, TestRunner};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use support::{bundled_packs, recreated, values};

/// Cases of a normal run (`PROPTEST_CASES` overrides).
const CASES: u32 = 512;
/// The seed of a normal run (`PROPTEST_RNG_SEED` overrides).
const SEED: u64 = 0x5EED_2718;
/// Edits per generated form.
const MAX_EDITS: usize = 6;
/// At least this share of the generated forms must write (the rest answer with form problems), in percent.
const MIN_WRITTEN_PERCENT: usize = 60;

const WORDS: [&str; 12] = [
    "Ember",
    "gloom",
    "Hollow",
    "of the",
    "veil",
    "Æther",
    "naïve",
    "it's",
    "storm-born",
    "7",
    "x",
    "Knight",
];

/// One edit a user makes.
#[derive(Debug, Clone)]
enum Edit {
    /// A field takes the value another element of the kind has (or loses it when that one has none).
    Borrow { field: Index, donor: Index },
    /// A text field typed.
    Type { field: Index, words: Vec<Index> },
    /// A check box flipped.
    Flip { field: Index },
    /// A number set in its range.
    Number { field: Index, at: Index },
    /// A combo choice picked.
    Pick { field: Index, choice: Index },
    /// Chips typed.
    Chips { field: Index, words: Vec<Index> },
    /// A block removed.
    DropBlock { at: Index },
    /// A block moved.
    MoveBlock { from: Index, to: Index },
    /// A block of any other bundled element added.
    AddBlock { donor: Index, block: Index },
}

fn edit() -> impl Strategy<Value = Edit> {
    let words = || prop::collection::vec(any::<Index>(), 1..4);
    prop_oneof![
        3 => (any::<Index>(), any::<Index>()).prop_map(|(field, donor)| Edit::Borrow { field, donor }),
        2 => (any::<Index>(), words()).prop_map(|(field, words)| Edit::Type { field, words }),
        1 => any::<Index>().prop_map(|field| Edit::Flip { field }),
        1 => (any::<Index>(), any::<Index>()).prop_map(|(field, at)| Edit::Number { field, at }),
        1 => (any::<Index>(), any::<Index>()).prop_map(|(field, choice)| Edit::Pick { field, choice }),
        1 => (any::<Index>(), words()).prop_map(|(field, words)| Edit::Chips { field, words }),
        1 => any::<Index>().prop_map(|at| Edit::DropBlock { at }),
        1 => (any::<Index>(), any::<Index>()).prop_map(|(from, to)| Edit::MoveBlock { from, to }),
        1 => (any::<Index>(), any::<Index>()).prop_map(|(donor, block)| Edit::AddBlock { donor, block }),
    ]
}

/// The bundled content and, per kind, the forms of its elements (made new).
struct World {
    store: ContentStore,
    /// Per kind id: the elements of that kind's forms.
    donors: BTreeMap<&'static str, Vec<FormState>>,
    /// Every bundled block.
    blocks: Vec<Block>,
}

fn world() -> World {
    let packs = bundled_packs();
    let mut b = ContentStoreBuilder::new();
    for p in &packs {
        b = b.add_pack_dir(&p.dir).expect("pack");
    }
    let store = b.build();
    let codec = CatalogBlockCodec;
    let cx = FormContext::from_store(&store, &codec);
    let mut donors: BTreeMap<&'static str, Vec<FormState>> = BTreeMap::new();
    let mut blocks = Vec::new();
    for p in &packs {
        for e in p.elements() {
            let form = recreated(&element_to_form(&e, &cx));
            let Some(k) = kind(&form.kind) else { continue };
            blocks.extend(form.blocks.iter().cloned());
            donors.entry(k.id).or_default().push(form);
        }
    }
    World {
        store,
        donors,
        blocks,
    }
}

fn fields_of(k: &KindDef, controls: &[Control]) -> Vec<&'static str> {
    k.fields
        .iter()
        .filter(|f| !f.read_only && controls.contains(&f.control))
        .map(|f| f.key)
        .collect()
}

fn words(ws: &[Index]) -> String {
    ws.iter()
        .map(|i| *i.get(&WORDS))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `edits` applied to `base`, a form of kind `k`.
fn apply(w: &World, k: &KindDef, base: &FormState, edits: &[Edit]) -> FormState {
    let mut f = base.clone();
    let donors = &w.donors[k.id];
    for e in edits {
        match e {
            Edit::Borrow { field, donor } => {
                let editable: Vec<&str> = k
                    .fields
                    .iter()
                    .filter(|d| !d.read_only)
                    .map(|d| d.key)
                    .collect();
                if editable.is_empty() {
                    continue;
                }
                let key = *field.get(&editable);
                match donor.get(donors).fields.get(key) {
                    Some(v) => f.fields.insert(key.to_string(), v.clone()),
                    None => f.fields.remove(key),
                };
            }
            Edit::Type { field, words: ws } => {
                let keys = fields_of(k, &[Control::Text, Control::Multiline]);
                if !keys.is_empty() {
                    f.fields
                        .insert((*field.get(&keys)).to_string(), json!(words(ws)));
                }
            }
            Edit::Flip { field } => {
                let keys = fields_of(k, &[Control::Check]);
                if !keys.is_empty() {
                    let key = *field.get(&keys);
                    let on = f.fields.get(key).and_then(Value::as_bool).unwrap_or(false);
                    f.fields.insert(key.to_string(), json!(!on));
                }
            }
            Edit::Number { field, at } => {
                let defs: Vec<_> = k
                    .fields
                    .iter()
                    .filter(|d| !d.read_only && d.control == Control::Number)
                    .collect();
                if !defs.is_empty() {
                    let d = field.get(&defs);
                    let (lo, hi) = (d.min.unwrap_or(0), d.max.unwrap_or(30));
                    let span = usize::try_from(hi - lo + 1).unwrap_or(1).max(1);
                    let n = lo + i32::try_from(at.index(span)).unwrap_or(0);
                    f.fields.insert(d.key.to_string(), json!(n));
                }
            }
            Edit::Pick { field, choice } => {
                let defs: Vec<_> = k
                    .fields
                    .iter()
                    .filter(|d| {
                        !d.read_only && d.control == Control::Combo && !d.choices.is_empty()
                    })
                    .collect();
                if !defs.is_empty() {
                    let d = field.get(&defs);
                    f.fields
                        .insert(d.key.to_string(), json!(choice.get(d.choices).0));
                }
            }
            Edit::Chips { field, words: ws } => {
                let keys = fields_of(k, &[Control::Chips]);
                if !keys.is_empty() {
                    let chips: Vec<&str> = ws.iter().map(|i| *i.get(&WORDS)).collect();
                    f.fields
                        .insert((*field.get(&keys)).to_string(), json!(chips));
                }
            }
            Edit::DropBlock { at } => {
                if !f.blocks.is_empty() {
                    f.blocks.remove(at.index(f.blocks.len()));
                }
            }
            Edit::MoveBlock { from, to } => {
                if !f.blocks.is_empty() {
                    let b = f.blocks.remove(from.index(f.blocks.len()));
                    let at = to.index(f.blocks.len() + 1);
                    f.blocks.insert(at, b);
                }
            }
            Edit::AddBlock { donor, block } => {
                let _ = donor;
                let mut b = block.get(&w.blocks).clone();
                b.key = format!("r{}", f.blocks.len() + 100);
                f.blocks.push(b);
            }
        }
    }
    f
}

/// The forms of written elements as the creator opens them: the first with the others as its children.
fn reread(els: &[Element], cx: &FormContext<'_>) -> FormState {
    element_tree_to_form(&els[0], els, cx)
}

#[test]
fn random_forms_write_valid_elements_and_read_back_as_a_fixed_point() {
    let w = world();
    let codec = CatalogBlockCodec;
    let cx = FormContext::from_store(&w.store, &codec);
    let kinds: Vec<&'static str> = w.donors.keys().copied().collect();
    let written = AtomicUsize::new(0);
    let edited = AtomicUsize::new(0);
    let cases = AtomicUsize::new(0);
    let mut config = Config {
        cases: CASES,
        failure_persistence: None,
        rng_seed: RngSeed::Fixed(SEED),
        ..Config::default()
    };
    if let Some(n) = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        config.cases = n;
    }
    if let Some(n) = std::env::var("PROPTEST_RNG_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        config.rng_seed = RngSeed::Fixed(n);
    }
    let mut runner = TestRunner::new(config);
    let strategy = (
        any::<Index>(),
        any::<Index>(),
        prop::collection::vec(edit(), 0..=MAX_EDITS),
    );
    let result = runner.run(&strategy, |(k, donor, edits)| {
        cases.fetch_add(1, Ordering::Relaxed);
        let k = kind(k.get(&kinds)).expect("kind");
        let base = donor.get(&w.donors[k.id]);
        let form = apply(&w, k, base, &edits);
        if form != *base {
            edited.fetch_add(1, Ordering::Relaxed);
        }
        let els = match form_to_elements(&form, &cx) {
            Ok(els) => els,
            Err(problems) => {
                for p in &problems {
                    prop_assert!(
                        !p.message.trim().is_empty(),
                        "a problem without words: {p:?}"
                    );
                }
                return Ok(());
            }
        };
        written.fetch_add(1, Ordering::Relaxed);
        prop_assert!(!els.is_empty(), "a form writes its element");
        let doc = json!({"format": "borealis-content/1", "elements": els});
        let violations = validate_content(&doc).expect("schema");
        prop_assert!(
            violations.is_empty(),
            "schema: {violations:?}\n  {}",
            serde_json::to_string(&els).unwrap_or_default()
        );
        let again = reread(&els, &cx);
        let els2 = form_to_elements(&again, &cx);
        prop_assert!(
            els2.is_ok(),
            "written elements read back with problems: {els2:?}"
        );
        let els2 = els2.unwrap_or_default();
        prop_assert_eq!(
            serde_json::to_value(&els2).ok(),
            serde_json::to_value(&els).ok(),
            "not a fixed point"
        );
        let third = reread(&els2, &cx);
        prop_assert_eq!(values(&third), values(&again), "the form reads differently");
        Ok(())
    });
    let (n, ok) = (
        cases.load(Ordering::Relaxed),
        written.load(Ordering::Relaxed),
    );
    let changed = edited.load(Ordering::Relaxed);
    println!(
        "random forms: {n} cases over {} kinds, {changed} edited, {ok} written",
        kinds.len()
    );
    if let Err(e) = result {
        panic!("{e}");
    }
    assert!(
        ok * 100 >= n * MIN_WRITTEN_PERCENT,
        "only {ok} of {n} generated forms write"
    );
}
