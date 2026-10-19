//! D-20 family (det-aurora-1): every throw of Aurora's custom-file load discards the WHOLE file,
//! not only the faulty element (`DataManager.InitializeElementDataAsync` has one `try` per custom
//! file). Borealis models those throws (`load_reject`) for custom content and reports the
//! culprit, every dropped element and a `file_skipped`. Synthetic content only.
//!
//! The corpus case: a repository's `system-proxies.xml` (proxy `Item`s without `<setters>`, which
//! Aurora's `ItemElementParser` dereferences) also holds an identity `Option` that, loaded,
//! replaced the bundled `ID_INTERNAL_OPTION_ALLOW_FEATS` and its setters.

use borealis_content::{ContentStoreBuilder, IssueCode, PackOrigin, Severity};
use borealis_migrate::{migrate_path, write_output, MigrateOptions, MigrateOutput};
use std::path::{Path, PathBuf};

const PROXIES: &str = r#"<elements>
  <element name="Feats" type="Option" source="Internal" id="ID_INTERNAL_OPTION_ALLOW_FEATS">
    <description><p>Allow feats.</p></description>
  </element>
  <element name="Proxy" type="Item" source="Synthetic" id="ID_SYNTH_PROXY_ITEM">
    <rules><grant type="Proficiency" id="ID_SYNTH_PROFICIENCY" /></rules>
  </element>
</elements>"#;

/// `<info>` without `<update>`: `ElementsFile.PopulateInformationSection` throws.
const NO_UPDATE: &str = r#"<elements>
  <info><name>Pack</name></info>
  <element name="Info Feat" type="Feat" source="Synthetic" id="ID_SYNTH_INFO_FEAT" />
</elements>"#;

/// A bad boolean on one select: Borealis used to drop only that element (D-10).
const BAD_BOOL: &str = r#"<elements>
  <element name="Good Feat" type="Feat" source="Synthetic" id="ID_SYNTH_GOOD_FEAT" />
  <element name="Bad Feat" type="Feat" source="Synthetic" id="ID_SYNTH_BAD_FEAT">
    <rules><select type="Feat" name="Pick" optional="yes" /></rules>
  </element>
</elements>"#;

const CLEAN: &str = r#"<elements>
  <info><name>Ok</name><update version="1.0.0"><file name="ok.xml" url="https://e.invalid/ok.xml" /></update></info>
  <element name="Clean Feat" type="Feat" source="Synthetic" id="ID_SYNTH_CLEAN_FEAT" />
</elements>"#;

const FILES: [(&str, &str); 4] = [
    ("content/system/system-proxies.xml", PROXIES),
    ("homebrew/no-update.xml", NO_UPDATE),
    ("homebrew/bad-bool.xml", BAD_BOOL),
    ("homebrew/ok.xml", CLEAN),
];

fn repo(tmp: &Path) -> PathBuf {
    let repo = tmp.join("repo");
    for (rel, text) in FILES {
        let path = repo.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    repo
}

fn migrate(dir: &Path, origin: PackOrigin) -> MigrateOutput {
    let opts = MigrateOptions {
        origin,
        ..MigrateOptions::default()
    };
    migrate_path(dir, &opts).expect("migrates")
}

fn kind(output: &MigrateOutput, rel: &str) -> String {
    let file = output.report.files.iter().find(|f| f.input == rel);
    file.map(|f| f.kind.clone()).unwrap_or_default()
}

#[test]
fn custom_files_aurora_cannot_load_are_discarded_whole_with_the_culprit_named() {
    let tmp = tempfile::tempdir().unwrap();
    let output = migrate(&repo(tmp.path()), PackOrigin::Custom);
    let issues = &output.report.issues;
    let culprit = |file: &str, code: IssueCode| {
        issues
            .iter()
            .find(|i| {
                i.file.as_deref() == Some(file)
                    && i.code == code
                    && i.message.contains("Aurora's loader throws")
            })
            .unwrap_or_else(|| panic!("{file}: no {code:?} culprit"))
    };

    let item = culprit(FILES[0].0, IssueCode::MissingRequiredSetter);
    assert_eq!(item.element.as_deref(), Some("ID_SYNTH_PROXY_ITEM"));
    assert_eq!(item.line, Some(5));
    assert!(
        item.message.contains("ItemElementParser.cs:115"),
        "{}",
        item.message
    );
    let info = culprit(FILES[1].0, IssueCode::MissingAttribute);
    assert_eq!((info.element.as_deref(), info.line), (None, Some(2)));
    let select = culprit(FILES[2].0, IssueCode::InvalidBoolean);
    assert_eq!(select.element.as_deref(), Some("ID_SYNTH_BAD_FEAT"));

    for (rel, _) in &FILES[..3] {
        assert_eq!(kind(&output, rel), "skipped", "{rel}");
        assert!(issues.iter().any(|i| i.file.as_deref() == Some(*rel)
            && i.code == IssueCode::FileSkipped
            && i.severity == Severity::Error));
    }
    assert_eq!(kind(&output, FILES[3].0), "content");
    let dropped = |id: &str| {
        issues
            .iter()
            .any(|i| i.code == IssueCode::ElementDropped && i.element.as_deref() == Some(id))
    };
    for id in [
        "ID_INTERNAL_OPTION_ALLOW_FEATS",
        "ID_SYNTH_INFO_FEAT",
        "ID_SYNTH_GOOD_FEAT",
    ] {
        assert!(dropped(id), "{id} is reported dropped");
    }

    let out_dir = tmp.path().join("out");
    write_output(&out_dir, &output).expect("writes");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/core");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root)
        .unwrap()
        .add_pack_dir(&out_dir)
        .unwrap()
        .build();
    let option = store.get("ID_INTERNAL_OPTION_ALLOW_FEATS").unwrap();
    assert!(
        option.file.starts_with("core/"),
        "bundled option kept: {}",
        option.file
    );
    assert!(store.get("ID_SYNTH_CLEAN_FEAT").is_some());
    for id in [
        "ID_SYNTH_PROXY_ITEM",
        "ID_SYNTH_INFO_FEAT",
        "ID_SYNTH_GOOD_FEAT",
    ] {
        assert!(store.get(id).is_none(), "{id}");
    }
}

#[test]
fn core_origin_content_keeps_aurora_per_element_handling() {
    let tmp = tempfile::tempdir().unwrap();
    let output = migrate(&repo(tmp.path()), PackOrigin::Core);
    for (rel, _) in FILES {
        assert_eq!(kind(&output, rel), "content", "{rel}");
    }
    assert!(!output
        .report
        .issues
        .iter()
        .any(|i| i.message.contains("Aurora's loader throws")));
}

#[test]
fn an_ignored_file_is_not_reported_as_throwing() {
    // DataManager.cs:265-269 skips an `ignore="true"` file before any element is parsed.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("repo");
    std::fs::create_dir_all(&dir).unwrap();
    let parked = BAD_BOOL.replacen("<elements>", r#"<elements ignore="true">"#, 1);
    std::fs::write(dir.join("parked.xml"), parked).unwrap();
    let output = migrate(&dir, PackOrigin::Custom);
    assert!(!output
        .report
        .issues
        .iter()
        .any(|i| i.message.contains("Aurora's loader throws")));
}
