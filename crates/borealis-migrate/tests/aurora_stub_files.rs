//! Regression (det-src-2): a repository that ships its own copy of the core rules plus a file of
//! identity-only stubs (`<element type="Level" id="ID_LEVEL_1" />`, empty `Grants`) left the
//! builder with no Race or Class select: the stub file loads after the real levels and, later
//! file wins, replaced `ID_LEVEL_1` with an element without rules. Aurora never loads such a
//! file: its Level parser throws for the missing `level`/`experience` setters and a parse error
//! discards the whole custom file (content-format 9.1). Synthetic content only.
//!
//! `BOREALIS_BUILDER_PROBE=<dir>[:<dir>...]` additionally prints the L1 Class/Race option counts
//! with those folders (Aurora sources, or already migrated pack folders) stacked on the bundled
//! packs, in order. Local corpus only; nothing is asserted or committed from it.

use borealis_content::{ContentFile, ContentStore, ContentStoreBuilder, IssueCode, PackOrigin};
use borealis_content::{Ruleset, Severity};
use borealis_engine::{commands, Engine};
use borealis_migrate::{migrate_path, write_output, MigrateOptions, MigrateOutput};
use std::path::{Path, PathBuf};

const BUNDLED: [&str; 3] = ["core", "srd-5.1", "srd-5.2.1"];

/// The repository's own copy of the level-1 element, valid Aurora (same body as the core one).
const LEVELS: &str = r#"<elements>
  <element name="1" type="Level" source="Internal" id="ID_LEVEL_1">
    <setters><set name="Level">1</set><set name="Experience">0</set></setters>
    <rules>
      <stat name="proficiency" value="2" alt="Level 1" />
      <grant type="Grants" id="ID_INTERNAL_GRANTS_CHARACTER_BASE" />
      <grant type="Grants" id="ID_INTERNAL_GRANTS_SPELLCASTING_BASE" />
      <grant type="Grants" id="ID_INTERNAL_GRANTS_ARMOR_CLASS_BASE" />
      <grant type="Grants" id="ID_INTERNAL_GRANTS_ARMOR_CLASS_DEXTERITY_MODIFIER" />
      <grant type="Grants" id="ID_INTERNAL_GRANTS_HP_CONSTITUTION_MODIFIER" />
      <select name="Race" type="Race" />
      <select name="Class" type="Class" />
      <select name="Background" type="Background" />
      <select name="Alignment" type="Alignment" />
    </rules>
  </element>
</elements>"#;

/// Identity stubs for another engine; `system/` sorts after `core/`, so it loads later.
const STUBS: &str = r#"<elements>
  <element name="1" type="Level" source="Internal" id="ID_LEVEL_1" />
  <element name="Base Armor Class" type="Grants" source="Internal" id="ID_INTERNAL_GRANTS_ARMOR_CLASS_BASE" />
  <element name="Stub Feat" type="Feat" source="Internal" id="ID_SYNTH_STUB_FEAT" />
  <append id="ID_LEVEL_2"><supports>Synthetic</supports></append>
</elements>"#;

/// A homebrew feat beside them, in a file Aurora loads.
const FEATS: &str = r#"<elements>
  <element name="Synthetic Feat" type="Feat" source="Synthetic" id="ID_SYNTH_FEAT">
    <description><p>A feat.</p></description>
  </element>
</elements>"#;

fn bundled() -> ContentStoreBuilder {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    BUNDLED.iter().fold(ContentStoreBuilder::new(), |b, p| {
        b.add_pack_dir(&root.join(p)).expect("bundled pack loads")
    })
}

fn migrate(dir: &Path, out: &Path) -> MigrateOutput {
    let output = migrate_path(dir, &MigrateOptions::default()).expect("migrates");
    write_output(out, &output).expect("writes");
    output
}

/// `(ruleset, class options, race options)` of a new character's level-1 selects.
fn builder_counts(store: &ContentStore) -> Vec<(Ruleset, usize, usize)> {
    let engine = Engine::new(store);
    [Ruleset::Dnd2014, Ruleset::Dnd2024]
        .into_iter()
        .map(|rs| {
            let snap = engine.derive_with(&commands::new_character(&engine, rs), true);
            let count = |ty: &str| {
                snap.slots
                    .iter()
                    .filter(|s| s.owner.id == "ID_LEVEL_1" && s.ty == ty)
                    .map(|s| s.options.len())
                    .sum::<usize>()
            };
            (rs, count("Class"), count("Race"))
        })
        .collect()
}

fn repo(tmp: &Path) -> PathBuf {
    let repo = tmp.join("repo");
    for (rel, text) in [
        ("content/core/levels.xml", LEVELS),
        ("content/system/system-elements.xml", STUBS),
        ("content/homebrew/feats.xml", FEATS),
    ] {
        let path = repo.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    repo
}

#[test]
fn a_stub_level_file_is_discarded_like_aurora_and_the_builder_keeps_its_selects() {
    let tmp = tempfile::tempdir().unwrap();
    let out_dir = tmp.path().join("out");
    let output = migrate(&repo(tmp.path()), &out_dir);

    let stub_file = "content/system/system-elements.xml";
    let issues = &output.report.issues;
    let culprit = issues
        .iter()
        .find(|i| i.code == IssueCode::MissingRequiredSetter)
        .expect("the stub level is named");
    assert_eq!(culprit.element.as_deref(), Some("ID_LEVEL_1"));
    assert_eq!(culprit.file.as_deref(), Some(stub_file));
    assert_eq!(culprit.line, Some(2));
    let dropped: Vec<_> = issues
        .iter()
        .filter(|i| i.code == IssueCode::ElementDropped && i.severity == Severity::Error)
        .map(|i| (i.element.as_deref().unwrap(), i.file.as_deref().unwrap()))
        .collect();
    assert_eq!(
        dropped,
        [
            ("ID_LEVEL_1", stub_file),
            ("ID_INTERNAL_GRANTS_ARMOR_CLASS_BASE", stub_file),
            ("ID_SYNTH_STUB_FEAT", stub_file),
        ]
    );
    assert!(issues
        .iter()
        .any(|i| i.code == IssueCode::FileSkipped && i.message.contains("1 append(s)")));
    let report = output
        .report
        .files
        .iter()
        .find(|f| f.input == stub_file)
        .unwrap();
    assert_eq!(report.kind, "skipped");
    assert_eq!(
        (report.stats.elements, report.stats.dropped_elements),
        (0, 3)
    );

    let store = bundled().add_pack_dir(&out_dir).unwrap().build();
    let level = store.get("ID_LEVEL_1").unwrap();
    assert!(
        level.file.ends_with("content/core/levels.json"),
        "the repository's full level (later file) replaces core: {}",
        level.file
    );
    assert!(store.get("ID_SYNTH_FEAT").is_some(), "the rest is imported");
    assert!(store.get("ID_SYNTH_STUB_FEAT").is_none());
    let armor = store.get("ID_INTERNAL_GRANTS_ARMOR_CLASS_BASE").unwrap();
    assert!(armor.file.starts_with("core/"), "core grant kept");
    for (rs, classes, races) in builder_counts(&store) {
        assert!(
            classes > 0 && races > 0,
            "{rs:?}: {classes} classes, {races} races"
        );
    }
}

#[test]
fn the_mechanism_a_loaded_stub_level_empties_the_builder() {
    // What the import did before: the stub, loaded after core, replaces ID_LEVEL_1.
    let stub = ContentFile::from_json_str(
        r#"{"format":"borealis-content/1","elements":[
            {"id":"ID_LEVEL_1","name":"1","type":"Level","source":"Internal"}]}"#,
    )
    .unwrap();
    let healthy = builder_counts(&bundled().build());
    assert!(healthy.iter().all(|(_, c, r)| *c > 0 && *r > 0));
    let broken = bundled()
        .add_file("stubs.json", PackOrigin::Custom, stub)
        .build();
    assert!(builder_counts(&broken)
        .iter()
        .all(|(_, c, r)| *c == 0 && *r == 0));
}

#[test]
fn a_valid_aurora_override_of_a_level_still_wins() {
    // Later file wins for valid elements: a Level with its setters replaces the core one.
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("levels.xml"),
        r#"<elements><element name="1" type="Level" source="Internal" id="ID_LEVEL_1">
        <setters><set name="Level">1</set><set name="Experience">0</set></setters>
        <rules><select name="Class" type="Class" /></rules></element></elements>"#,
    )
    .unwrap();
    let out_dir = tmp.path().join("out");
    let output = migrate(&src, &out_dir);
    assert!(output
        .report
        .issues
        .iter()
        .all(|i| i.code != IssueCode::ElementDropped));
    let store = bundled().add_pack_dir(&out_dir).unwrap().build();
    assert_eq!(&*store.get("ID_LEVEL_1").unwrap().file, "src/levels.json");
    let counts = builder_counts(&store);
    assert!(
        counts.iter().all(|(_, c, r)| *c > 0 && *r == 0),
        "{counts:?}"
    );
}

#[test]
fn builder_probe_on_local_folders() {
    let Ok(list) = std::env::var("BOREALIS_BUILDER_PROBE") else {
        return eprintln!("SKIPPED: BOREALIS_BUILDER_PROBE not set");
    };
    let tmp = tempfile::tempdir().unwrap();
    let mut builder = bundled();
    for (i, dir) in list.split(':').filter(|d| !d.is_empty()).enumerate() {
        let dir = PathBuf::from(dir);
        let pack = if dir.join("pack.json").is_file() {
            dir
        } else {
            let out = tmp.path().join(format!("pack{i}"));
            migrate(&dir, &out);
            out
        };
        builder = builder.add_pack_dir(&pack).unwrap();
    }
    for (rs, classes, races) in builder_counts(&builder.build()) {
        eprintln!("PROBE {rs:?} class_options={classes} race_options={races}");
    }
}
