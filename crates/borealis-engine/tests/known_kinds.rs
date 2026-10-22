//! Loud content (engine-test-audit-1 WP1): every custom rule kind and field in the bundled packs
//! has a reader in `custom_rules::KNOWN_KINDS`, and anything else is a diagnostic.
use borealis_content::file::{ContentFile, PackOrigin};
use borealis_content::{ContentStoreBuilder, Element, Ruleset};
use borealis_engine::custom_rules::{self, spec, KNOWN_KINDS};
use borealis_engine::diag::code;
use borealis_engine::universe::Universe;
use borealis_engine::{Engine, Severity};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const PACKS: [&str; 5] = [
    "core",
    "srd-5.1",
    "srd-5.2.1",
    "srd-5.1-monsters",
    "srd-5.2.1-monsters",
];

/// The typed rule kinds of the content model (not custom).
const CORE_KINDS: [&str; 4] = ["grant", "select", "stat", "unknown"];

fn content_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

fn json_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("content dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            json_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "json") {
            out.push(path);
        }
    }
}

/// Custom kind -> field -> number of uses, over every `rules` array of the raw JSON.
fn content_kinds() -> BTreeMap<String, BTreeMap<String, usize>> {
    fn walk(v: &Value, out: &mut BTreeMap<String, BTreeMap<String, usize>>) {
        match v {
            Value::Object(m) => {
                if let Some(Value::Array(rules)) = m.get("rules") {
                    for r in rules {
                        let Some(kind) = r.get("kind").and_then(Value::as_str) else {
                            continue;
                        };
                        if CORE_KINDS.contains(&kind) {
                            continue;
                        }
                        let fields = out.entry(kind.to_string()).or_default();
                        for f in r.as_object().into_iter().flat_map(|o| o.keys()) {
                            if f != "kind" {
                                *fields.entry(f.clone()).or_default() += 1;
                            }
                        }
                    }
                }
                m.values().for_each(|x| walk(x, out));
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut files = Vec::new();
    json_files(&content_root(), &mut files);
    let mut out = BTreeMap::new();
    for f in files {
        let text = std::fs::read_to_string(&f).expect("read");
        let v: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{f:?}: {e}"));
        walk(&v, &mut out);
    }
    out
}

#[test]
fn registry_matches_the_kinds_in_bundled_content() {
    let found = content_kinds();
    assert!(!found.is_empty(), "no custom rules found under content/");
    let mut problems = Vec::new();
    for (kind, fields) in &found {
        let Some(s) = spec(kind) else {
            problems.push(format!("kind {kind:?} has no reader in KNOWN_KINDS"));
            continue;
        };
        for f in fields.keys() {
            if !s.knows(f) {
                problems.push(format!(
                    "{kind}.{f} is not read (add a reader, then the field)"
                ));
            }
        }
    }
    for s in KNOWN_KINDS {
        let Some(fields) = found.get(s.kind) else {
            problems.push(format!(
                "registry kind {:?} is used by no bundled content",
                s.kind
            ));
            continue;
        };
        for (f, _) in s.unmodelled {
            if !fields.contains_key(*f) {
                problems.push(format!("stale unmodelled field {}.{f}", s.kind));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn reader_kind_lists_are_registered() {
    for k in borealis_engine::notes_d::KINDS {
        assert!(
            spec(k).is_some(),
            "notes_d kind {k} missing from KNOWN_KINDS"
        );
    }
    assert_eq!(
        custom_rules::RuleProblem::UnknownKind(String::new()).code(),
        code::CONTENT_UNKNOWN_RULE_KIND
    );
    assert_eq!(
        custom_rules::RuleProblem::UnknownField {
            kind: String::new(),
            field: String::new()
        }
        .code(),
        code::CONTENT_UNKNOWN_RULE_FIELD
    );
}

#[test]
fn bundled_packs_have_no_unknown_rule_diagnostics() {
    let root = content_root();
    let mut b = ContentStoreBuilder::new();
    for p in PACKS {
        b = b.add_pack_dir(&root.join(p)).expect("pack loads");
    }
    let store = b.build();
    for ruleset in [Ruleset::Dnd2014, Ruleset::Dnd2024] {
        let uni = Universe::new(&store, ruleset);
        let loud: Vec<_> = uni
            .diagnostics
            .iter()
            .filter(|d| d.code.starts_with("content.unknown_rule"))
            .map(|d| format!("{:?} {}", d.element, d.message))
            .collect();
        assert!(loud.is_empty(), "{}", loud.join("\n"));
    }
}

fn element(rules: Value) -> Element {
    serde_json::from_value(json!({
        "id": "ID_TEST_BACKGROUND",
        "type": "Background",
        "name": "Test Background",
        "source": "Test",
        "rules": rules,
    }))
    .expect("element parses")
}

fn derive_with(origin: PackOrigin, el: Element) -> Vec<borealis_engine::Diagnostic> {
    let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/core");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&core)
        .expect("core loads")
        .add_file("test.json", origin, ContentFile::new(vec![el]))
        .build();
    let engine = Engine::new(&store);
    let state = borealis_engine::commands::new_character(&engine, Ruleset::Dnd2024);
    engine.derive(&state).diagnostics
}

#[test]
fn an_unknown_kind_is_a_warning_in_third_party_and_an_error_in_bundled_content() {
    let el = element(json!([{
        "kind": "select.ability_distribution",
        "pool": ["strength", "dexterity", "constitution"]
    }]));
    for (origin, severity) in [
        (PackOrigin::Custom, Severity::Warning),
        (PackOrigin::Core, Severity::Error),
    ] {
        let diags = derive_with(origin, el.clone());
        let d = diags
            .iter()
            .find(|d| d.code == code::CONTENT_UNKNOWN_RULE_KIND)
            .unwrap_or_else(|| panic!("no diagnostic in {diags:?}"));
        assert_eq!(d.severity, severity);
        assert_eq!(d.element.as_deref(), Some("ID_TEST_BACKGROUND"));
        assert!(d.message.contains("select.ability_distribution"));
    }
}

#[test]
fn an_unknown_field_of_a_known_kind_is_a_diagnostic() {
    let el = element(json!([{
        "kind": "ability_distribution",
        "pool": ["strength", "dexterity", "constitution"],
        "options": [[2, 1], [1, 1, 1]],
        "weighted": true
    }]));
    let diags = custom_rules::content_diagnostics(&el, PackOrigin::Custom);
    let codes: BTreeSet<_> = diags.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, BTreeSet::from([code::CONTENT_UNKNOWN_RULE_FIELD]));
    assert!(diags[0].message.contains("weighted"));
    let known = element(json!([{"kind": "evasion", "level": 7}]));
    assert!(custom_rules::content_diagnostics(&known, PackOrigin::Core).is_empty());
}

fn all_packs_engine() -> &'static Engine {
    static ENGINE: std::sync::OnceLock<Engine> = std::sync::OnceLock::new();
    ENGINE.get_or_init(|| {
        let root = content_root();
        let mut b = ContentStoreBuilder::new();
        for p in PACKS {
            b = b.add_pack_dir(&root.join(p)).expect("pack loads");
        }
        Engine::new(&b.build())
    })
}

fn hero_2024(species: &str, class: &str, level: u32) -> borealis_engine::CharacterSnapshot {
    use borealis_engine::commands::{self, slot_key};
    let e = all_packs_engine();
    let mut s = commands::new_character(e, Ruleset::Dnd2024);
    for (name, id) in [("Race", species), ("Class", class)] {
        let key = slot_key(&e.derive(&s), "ID_LEVEL_1", name, 1).expect("slot");
        s = commands::select(e, &s, &key, borealis_engine::Choice::Element(id.into()))
            .expect("select")
            .state;
    }
    while s.level() < level {
        s = commands::level_up_main(e, &s, None)
            .expect("level up")
            .state;
    }
    e.derive(&s)
}

fn resource_max(snap: &borealis_engine::CharacterSnapshot, id: &str) -> Option<i32> {
    snap.resources.iter().find(|r| r.id == id).map(|r| r.max)
}

/// `resource.level` was read by nobody: the L17 "second use" rule replaced the base Action Surge
/// from Fighter 2, and level-5 species resources showed at level 1.
#[test]
fn gated_resources_follow_their_level() {
    const FIGHTER: &str = "ID_2024_CLASS_FIGHTER";
    const DWARF: &str = "ID_2024_SPECIES_DWARF";
    let f2 = hero_2024(DWARF, FIGHTER, 2);
    assert_eq!(resource_max(&f2, "action surge"), Some(1));
    let f13 = hero_2024(DWARF, FIGHTER, 13);
    assert_eq!(resource_max(&f13, "action surge"), Some(1));
    assert_eq!(resource_max(&f13, "indomitable"), Some(2));
    let f17 = hero_2024(DWARF, FIGHTER, 17);
    assert_eq!(resource_max(&f17, "action surge"), Some(2));
    assert_eq!(resource_max(&f17, "indomitable"), Some(3));
    let d1 = hero_2024("ID_2024_SPECIES_DRAGONBORN", FIGHTER, 1);
    assert_eq!(resource_max(&d1, "draconic flight"), None);
    let d5 = hero_2024("ID_2024_SPECIES_DRAGONBORN", FIGHTER, 5);
    assert_eq!(resource_max(&d5, "draconic flight"), Some(1));
}

/// The fields bundled content carries that nothing evaluates yet (`rule_kinds::GAP`). The list
/// only shrinks: a new unread field fails `bundled_content_uses_only_registered_kinds_and_fields`
/// until it has a reader or a documented reason, and cannot slip in as a new `GAP` without
/// editing this list (review).
const PINNED_GAPS: [(&str, &str); 12] = [
    ("grant_spell", "ability"),
    ("martial_arts", "dexterous_attacks"),
    ("martial_arts", "monk_weapons"),
    ("movement_mode", "requires_resource"),
    ("resource", "counter"),
    ("resource", "pool"),
    ("resource", "start"),
    ("resource", "step"),
    ("ritual_casting", "requires_prepared"),
    ("select_param", "grants_resistance"),
    ("slot_recovery", "improved_at"),
    ("spell_list_extend", "for_prepared"),
];

/// A reason shorter than this says nothing.
const MIN_REASON_LEN: usize = 20;

#[test]
fn unmodelled_fields_are_pinned() {
    use borealis_content::rule_kinds::GAP;
    let mut gaps = BTreeSet::new();
    for s in KNOWN_KINDS {
        for (field, reason) in s.unmodelled {
            assert!(
                reason.len() > MIN_REASON_LEN,
                "{}.{field} has an undocumented reason {reason:?}",
                s.kind
            );
            assert!(
                !s.fields.contains(field),
                "{}.{field} is read and unmodelled",
                s.kind
            );
            if *reason == GAP {
                gaps.insert((s.kind, *field));
            }
        }
    }
    let pinned: BTreeSet<_> = PINNED_GAPS.iter().copied().collect();
    assert_eq!(gaps, pinned, "the unread-field allow-list changed");
}
