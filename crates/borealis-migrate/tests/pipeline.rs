//! Pipeline tests on small synthetic trees (no external data needed).

use borealis_content::{
    ContentStoreBuilder, IndexFile, IssueCode, PackFileKind, PackManifest, PackOrigin, Ruleset,
    Severity,
};
use borealis_migrate::{cli, migrate_path, write_output, MigrateOptions};
use std::fs;
use std::path::Path;

const GOOD: &str = r#"<?xml version="1.0" encoding="utf-8" ?>
<elements>
  <info><name>Mine</name><update version="1.0.0"><file name="mine.xml" url="https://example.invalid/mine.xml"/></update></info>
  <element name="Thing" type="Item" source="Homebrew" id="ID_HB_ITEM_THING">
    <description><p>A &amp; B</p></description>
    <setters />
    <rules><grant type="Item" id="ID_HB_ITEM_OTHER"/></rules>
  </element>
</elements>"#;

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn element_xml(id: &str, desc: &str) -> String {
    format!(
        r#"<elements><element name="{id}" type="Item" source="S" id="{id}"><description><p>{desc}</p></description><setters /></element></elements>"#
    )
}

#[test]
fn custom_folder_layout_is_ordered_and_recursive() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("custom");
    write(
        &input,
        "homebrew/b.xml",
        &element_xml("ID_SAME", "homebrew"),
    );
    write(&input, "core/a.xml", &element_xml("ID_SAME", "core"));
    write(&input, "user/sub/c.xml", &element_xml("ID_C", "c"));
    write(&input, "root.xml", &element_xml("ID_ROOT", "r"));
    write(
        &input,
        "custom/ignore/parked.xml",
        &element_xml("ID_PARKED", "p"),
    );
    write(&input, "notes.txt", "ignored");
    let out = migrate_path(&input, &MigrateOptions::default()).unwrap();

    let paths: Vec<_> = out.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "core/a.json",
            "homebrew/b.json",
            "pack.json",
            "root.json",
            "user/sub/c.json"
        ]
    );
    let manifest: PackManifest = PackManifest::from_json_str(
        &out.files
            .iter()
            .find(|f| f.path == "pack.json")
            .unwrap()
            .contents,
    )
    .unwrap();
    let order: Vec<_> = manifest.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        order,
        [
            "core/a.json",
            "homebrew/b.json",
            "user/sub/c.json",
            "root.json"
        ]
    );
    assert_eq!(manifest.rulesets, [Ruleset::Dnd2014]);
    assert!(out
        .report
        .issues
        .iter()
        .any(|i| i.code == IssueCode::FileSkipped
            && i.file.as_deref() == Some("custom/ignore/parked.xml")));

    // the custom-folder override rule holds when the pack is loaded back
    let tmp_out = tmp.path().join("out");
    write_output(&tmp_out, &out).unwrap();
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&tmp_out)
        .unwrap()
        .build();
    assert_eq!(
        store.element("ID_SAME").unwrap().description,
        "<p>homebrew</p>"
    );
    assert_eq!(store.get("ID_SAME").unwrap().origin, PackOrigin::Custom);
}

#[test]
fn output_is_deterministic_and_stamped_with_the_ruleset() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "in/a.xml", GOOD);
    let opts = MigrateOptions {
        ruleset: Ruleset::Dnd2024,
        ..MigrateOptions::default()
    };
    let a = migrate_path(&tmp.path().join("in"), &opts).unwrap();
    let b = migrate_path(&tmp.path().join("in"), &opts).unwrap();
    assert_eq!(a.files, b.files);
    assert_eq!(
        a.report.to_json_pretty().unwrap(),
        b.report.to_json_pretty().unwrap()
    );
    let json: serde_json::Value = serde_json::from_str(&a.files[0].contents).unwrap();
    assert_eq!(json["ruleset"], "dnd2024");
    assert_eq!(
        json["info"]["update"]["file"]["url"],
        "https://example.invalid/mine.xml"
    );
    assert_eq!(json["elements"][0]["description"], "<p>A &amp; B</p>");
}

#[test]
fn single_file_input_and_manifest_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "mine.xml", GOOD);
    let opts = MigrateOptions {
        pack_id: Some("mine".into()),
        pack_name: Some("Mine".into()),
        license: Some("CC-BY-4.0".into()),
        attribution: Some("me".into()),
        origin: PackOrigin::Core,
        ..MigrateOptions::default()
    };
    let out = migrate_path(&tmp.path().join("mine.xml"), &opts).unwrap();
    assert_eq!(out.files.len(), 2);
    let manifest = PackManifest::from_json_str(&out.files[1].contents).unwrap();
    assert_eq!(
        (manifest.id.as_str(), manifest.license.as_deref()),
        ("mine", Some("CC-BY-4.0"))
    );
    assert_eq!(manifest.origin, PackOrigin::Core);
    assert_eq!(manifest.files[0].path, "mine.json");
    // dangling grant reported with file/element context
    let dangling = out
        .report
        .issues
        .iter()
        .find(|i| i.code == IssueCode::DanglingReference)
        .unwrap();
    assert_eq!(dangling.file.as_deref(), Some("mine.xml"));
    assert_eq!(dangling.element.as_deref(), Some("ID_HB_ITEM_THING"));
    let no_validate = migrate_path(
        &tmp.path().join("mine.xml"),
        &MigrateOptions {
            validate: false,
            ..opts
        },
    )
    .unwrap();
    assert!(no_validate
        .report
        .issues
        .iter()
        .all(|i| i.code != IssueCode::DanglingReference));
}

const INDEX: &str = r#"<index app="1.0">
  <info><name>Pack</name><update version="1.0.0"><file name="pack.index" url="https://example.invalid/pack.index"/></update></info>
  <files>
    <file name="a.xml" url="https://example.invalid/a.xml"/>
    <file name="missing.xml" url="https://example.invalid/missing.xml"/>
    <file name="sub.index" url="https://example.invalid/sub.index"/>
    <file name="../escape.xml" url="https://example.invalid/e.xml"/>
    <obsolete name="old.xml" url="https://example.invalid/old.xml"/>
  </files>
</index>"#;

const SUB_INDEX: &str = r#"<index><info><update version="1.0"><file name="sub.index" url="https://example.invalid/sub.index"/></update></info>
  <files><file name="b.xml" url="https://example.invalid/b.xml"/></files></index>"#;

#[test]
fn index_input_follows_local_entries_and_stores_urls_without_fetching() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(root, "pack.index", INDEX);
    write(root, "pack/a.xml", &element_xml("ID_A", "a"));
    write(root, "pack/sub.index", SUB_INDEX);
    write(root, "pack/sub/b.xml", &element_xml("ID_B", "b"));
    write(root, "escape.xml", &element_xml("ID_E", "never read"));
    write(root, "unlisted.xml", &element_xml("ID_U", "never read"));

    let out = migrate_path(&root.join("pack.index"), &MigrateOptions::default()).unwrap();
    let paths: Vec<_> = out.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "pack.index.json",
            "pack.json",
            "pack/a.json",
            "pack/sub.index.json",
            "pack/sub/b.json"
        ]
    );

    let index = IndexFile::from_json_str(&out.files[0].contents).unwrap();
    assert_eq!(index.files.len(), 5);
    assert_eq!(index.files[0].url, "https://example.invalid/a.xml");
    assert!(index.files[4].is_obsolete());
    assert_eq!(
        index.info.remote_url(),
        Some("https://example.invalid/pack.index")
    );

    let missing: Vec<_> = out
        .report
        .issues
        .iter()
        .filter(|i| i.code == IssueCode::IndexEntryMissing)
        .collect();
    assert_eq!(
        missing.len(),
        2,
        "missing.xml (info) and the escaping entry (error): {missing:#?}"
    );
    assert!(missing
        .iter()
        .any(|i| i.severity == Severity::Error && i.message.contains("escapes")));
    assert!(
        missing
            .iter()
            .any(|i| i.severity == Severity::Info
                && i.message.contains("example.invalid/missing.xml"))
    );

    let manifest = PackManifest::from_json_str(&out.files[1].contents).unwrap();
    assert!(manifest.files.iter().any(|f| f.kind == PackFileKind::Index));
    assert_eq!(out.report.summary.index_files, 2);
    assert_eq!(out.report.totals.elements, 2);
}

#[test]
fn directory_with_index_checks_entries_but_walks_everything() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("tree");
    write(&root, "pack.index", INDEX);
    write(&root, "pack/a.xml", &element_xml("ID_A", "a"));
    let out = migrate_path(&root, &MigrateOptions::default()).unwrap();
    assert_eq!(out.report.summary.index_files, 1);
    assert_eq!(out.report.summary.content_files, 1);
    assert!(out
        .report
        .issues
        .iter()
        .any(|i| i.code == IssueCode::IndexEntryMissing));
}

#[test]
fn broken_files_are_reported_but_do_not_stop_the_run() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("in");
    write(&root, "good.xml", &element_xml("ID_GOOD", "g"));
    write(&root, "broken.xml", "<elements><element");
    fs::write(root.join("binary.xml"), [0xff_u8, 0xfe, 0x00]).unwrap();
    let out = migrate_path(&root, &MigrateOptions::default()).unwrap();
    assert_eq!(out.report.summary.content_files, 1);
    assert_eq!(out.report.summary.skipped_files, 2);
    assert_eq!(out.report.summary.errors, 2);
    assert!(!out.report.passes(false));
    assert!(out
        .report
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .all(|i| i.file.is_some()));
}

#[test]
fn colliding_output_names_are_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("in");
    write(&root, "a.xml", &element_xml("ID_A", "1"));
    write(&root, "a.XML", &element_xml("ID_B", "2"));
    let out = migrate_path(&root, &MigrateOptions::default()).unwrap();
    // On case-insensitive file systems only one file exists; otherwise one is rejected.
    let created = fs::read_dir(&root).unwrap().count();
    if created == 2 {
        assert_eq!(out.report.summary.errors, 1);
    }
}

#[cfg(unix)]
#[test]
fn symlinks_are_never_followed() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("in");
    write(&root, "a.xml", &element_xml("ID_A", "1"));
    write(
        tmp.path(),
        "outside/secret.xml",
        &element_xml("ID_SECRET", "x"),
    );
    std::os::unix::fs::symlink(tmp.path().join("outside"), root.join("link")).unwrap();
    std::os::unix::fs::symlink(
        tmp.path().join("outside/secret.xml"),
        root.join("secret.xml"),
    )
    .unwrap();
    let out = migrate_path(&root, &MigrateOptions::default()).unwrap();
    assert_eq!(out.report.totals.elements, 1);
}

#[test]
fn missing_input_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(migrate_path(&tmp.path().join("nope"), &MigrateOptions::default()).is_err());
}

fn run_cli(args: &[&str]) -> (i32, String, String) {
    let args: Vec<String> = args.iter().map(ToString::to_string).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = cli::run(&args, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

#[test]
fn cli_end_to_end_with_report_and_strict_mode() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("in");
    write(&input, "a.xml", GOOD); // has a dangling reference -> one warning
    let out_dir = tmp.path().join("out");
    let report = tmp.path().join("reports/report.json");

    let (code, stdout, stderr) = run_cli(&[
        input.to_str().unwrap(),
        "-o",
        out_dir.to_str().unwrap(),
        "--report",
        report.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("1 elements"));
    assert!(out_dir.join("a.json").is_file() && out_dir.join("pack.json").is_file());
    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&report).unwrap()).unwrap();
    assert_eq!(parsed["totals"]["elements"], 1);
    assert!(stderr.contains("DanglingReference"));

    let strict_out = tmp.path().join("strict");
    let (code, _, stderr) = run_cli(&[
        input.to_str().unwrap(),
        "-o",
        strict_out.to_str().unwrap(),
        "--strict",
    ]);
    assert_eq!(code, 1);
    assert!(stderr.contains("strict mode"));
    assert!(
        !strict_out.exists(),
        "strict mode writes nothing on failure"
    );

    let (code, _, stderr) = run_cli(&[
        tmp.path().join("missing").to_str().unwrap(),
        "-o",
        out_dir.to_str().unwrap(),
    ]);
    assert_eq!(code, 2);
    assert!(stderr.contains("error:"));
}
