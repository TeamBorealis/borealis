//! A3 (specs/handoffs/st3-diff.md): Aurora's core index ships the 2014 and the 2024 books side by
//! side, and the 2024 spells carry the same class tags (`Wizard`) as the 2014 ones. Migrated as one
//! 2014 pack, a 2014 Wizard was offered both editions' spells. The migrator now tags the content of
//! 2024 sources `dnd2024` (`borealis_migrate::edition`).
//!
//! Always on: a synthetic two-source repository, through `migrate_path` and the engine.
//! Local only: `BOREALIS_ACORE_RAW=<dir>`, the files of the A-core index laid out by URL path as the
//! app's index install stores them (migrated as a folder, like the app does),
//! prints element counts per edition and the L1 Wizard's spell options before (every file 2014, the
//! old output) and after; it asserts that no 2024 spell reaches the 2014 Wizard. Counts only.

use borealis_content::{ContentFile, ContentStore, ContentStoreBuilder, Ruleset};
use borealis_engine::{commands, Choice, Engine};
use borealis_migrate::{migrate_path, write_output, MigrateOptions, MigrateOutput};
use std::collections::BTreeMap;
use std::path::Path;

const BUNDLED: [&str; 3] = ["core", "srd-5.1", "srd-5.2.1"];
const SRC_2024: &str = "Synthetic Handbook (2024)";
/// The other setters Aurora requires on a spell (a file missing them is discarded, D-20).
const COMMON: &str = r#"<set name="time">1 action</set><set name="duration">Instantaneous</set><set name="range">60 feet</set>"#;

fn source_xml(name: &str, id: &str, extra: &str) -> String {
    format!(
        r#"<elements><element name="{name}" type="Source" source="Core" id="{id}"><setters>
<set name="abbreviation">S</set><set name="url">https://example.invalid</set>
<set name="official">true</set>{extra}</setters></element></elements>"#
    )
}

fn spells_xml(source: &str, prefix: &str) -> String {
    format!(
        r#"<elements>
<element name="Synthetic Spark" type="Spell" source="{source}" id="ID_{prefix}_SPELL_SPARK">
<supports>Wizard</supports><setters><set name="level">0</set><set name="school">Evocation</set>{COMMON}</setters></element>
<element name="Synthetic Ward" type="Spell" source="{source}" id="ID_{prefix}_SPELL_WARD">
<supports>Wizard</supports><setters><set name="level">1</set><set name="school">Abjuration</set>{COMMON}</setters></element>
<element name="Synthetic Helper" type="Grants" source="Internal" id="ID_{prefix}_GRANTS_HELPER" />
</elements>"#
    )
}

/// `old/` is a 2014 book, `new/` a 2024 one (the `D&D 5.5e` marker), both naming their source.
fn repo(dir: &Path) {
    let files = [
        (
            "old/source.xml",
            source_xml(
                "Synthetic Handbook",
                "ID_SYN_SOURCE_OLD",
                "<set name=\"release\">20140819</set>",
            ),
        ),
        (
            "old/spells.xml",
            spells_xml("Synthetic Handbook", "SYN_OLD"),
        ),
        (
            "new/source.xml",
            source_xml(
                SRC_2024,
                "ID_SYN_SOURCE_NEW",
                "<set name=\"information\">D&amp;D 5.5e</set>",
            ),
        ),
        ("new/spells.xml", spells_xml(SRC_2024, "SYN_NEW")),
    ];
    for (rel, text) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

fn store_with(out: &Path) -> ContentStore {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    BUNDLED
        .iter()
        .fold(ContentStoreBuilder::new(), |b, p| {
            b.add_pack_dir(&root.join(p)).expect("bundled pack loads")
        })
        .add_pack_dir(out)
        .expect("migrated pack loads")
        .build()
}

/// The old output: every file 2014, no element tags, a 2014-only manifest.
fn as_2014_only(output: &MigrateOutput) -> MigrateOutput {
    let mut old = output.clone();
    for f in &mut old.files {
        if f.path == "pack.json" {
            let mut v: serde_json::Value = serde_json::from_str(&f.contents).unwrap();
            v["rulesets"] = serde_json::json!(["dnd2014"]);
            f.contents = serde_json::to_string_pretty(&v).unwrap();
        } else if let Ok(mut file) = ContentFile::from_json_str(&f.contents) {
            file.ruleset = Some(Ruleset::Dnd2014);
            for e in &mut file.elements {
                e.ruleset = None;
            }
            f.contents = file.to_json_pretty().unwrap();
        }
    }
    old
}

/// Effective edition of every converted element: `(2014, 2024)`.
fn per_edition(output: &MigrateOutput) -> (usize, usize) {
    let (mut old, mut new) = (0, 0);
    for f in output.files.iter().filter(|f| f.path != "pack.json") {
        let Ok(file) = ContentFile::from_json_str(&f.contents) else {
            continue;
        };
        for e in &file.elements {
            match e.ruleset.or(file.ruleset) {
                Some(Ruleset::Dnd2024) => new += 1,
                _ => old += 1,
            }
        }
    }
    (old, new)
}

/// Spell options of a 2014 level-1 Wizard (`class_id`): option source -> distinct option count.
fn wizard_spells(store: &ContentStore, class_id: &str) -> BTreeMap<String, usize> {
    let engine = Engine::new(store);
    let state = commands::new_character(&engine, Ruleset::Dnd2014);
    let snap = engine.derive_with(&state, true);
    let class = snap
        .slots
        .iter()
        .find(|s| s.owner.id == "ID_LEVEL_1" && s.ty == "Class")
        .expect("class slot");
    let state = commands::select(
        &engine,
        &state,
        &class.key,
        Choice::Element(class_id.into()),
    )
    .expect("the 2014 wizard is offered")
    .state;
    let snap = engine.derive_with(&state, true);
    let mut seen = std::collections::BTreeSet::new();
    let mut out = BTreeMap::new();
    for o in snap
        .slots
        .iter()
        .filter(|s| s.ty == "Spell")
        .flat_map(|s| &s.options)
    {
        if seen.insert(o.id.clone()) {
            *out.entry(o.source.clone()).or_default() += 1;
        }
    }
    out
}

fn migrate_to(input: &Path, out: &Path) -> MigrateOutput {
    let output = migrate_path(input, &MigrateOptions::default()).expect("migrates");
    write_output(out, &output).expect("writes");
    output
}

#[test]
fn a_2024_source_is_tagged_and_kept_out_of_a_2014_wizard() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("repo");
    repo(&input);
    let output = migrate_to(&input, &tmp.path().join("after"));
    // the 2024 source.xml + spells.xml (2 spells, the Internal helper follows its file) are 2024
    assert_eq!(per_edition(&output), (4, 4));
    let pack = output.files.iter().find(|f| f.path == "pack.json").unwrap();
    assert!(
        pack.contents.contains("dnd2024"),
        "manifest lists both rulesets"
    );

    let before_dir = tmp.path().join("before");
    write_output(&before_dir, &as_2014_only(&output)).unwrap();
    let wizard = "ID_WOTC_PHB_CLASS_WIZARD";
    let before = wizard_spells(&store_with(&before_dir), wizard);
    let after = wizard_spells(&store_with(&tmp.path().join("after")), wizard);
    assert_eq!(before.get(SRC_2024), Some(&2), "{before:?}");
    assert_eq!(after.get(SRC_2024), None, "{after:?}");
    assert_eq!(after.get("Synthetic Handbook"), Some(&2), "{after:?}");
}

#[test]
fn acore_editions_local() {
    let Some(raw) = std::env::var_os("BOREALIS_ACORE_RAW") else {
        eprintln!("SKIP acore_editions_local: BOREALIS_ACORE_RAW not set");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let output = migrate_to(Path::new(&raw), &tmp.path().join("after"));
    let before_out = as_2014_only(&output);
    let before_dir = tmp.path().join("before");
    write_output(&before_dir, &before_out).unwrap();
    eprintln!(
        "ACORE elements per edition (2014, 2024): before {:?} after {:?}",
        per_edition(&before_out),
        per_edition(&output)
    );
    let wizard = "ID_WOTC_PHB_CLASS_WIZARD";
    let before = wizard_spells(&store_with(&before_dir), wizard);
    let after = wizard_spells(&store_with(&tmp.path().join("after")), wizard);
    let total = |m: &BTreeMap<String, usize>| m.values().sum::<usize>();
    eprintln!(
        "ACORE 2014 Wizard 1 spell options: before {} {before:?}",
        total(&before)
    );
    eprintln!(
        "ACORE 2014 Wizard 1 spell options: after {} {after:?}",
        total(&after)
    );
    assert!(!after.keys().any(|s| s.contains("2024")), "{after:?}");
}
