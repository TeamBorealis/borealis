use crate::content_index::Fetcher;
use crate::content_packs::NullSink;
use crate::content_preview::{ImportFileInput, PreviewImportRequest};
use crate::picked_paths::PickPurpose;
use crate::state::{AppPaths, AppState};
use crate::zip_read::test_zip::build;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// An invented homebrew file: one element with a warning inside it, one dropped element,
/// one renamed id and one clean element (spells carry the setters Aurora requires).
const HB: &str = r#"<elements>
  <element name="Quirk" type="Feat" source="Hb" id="ID_HBT_FEAT_QUIRK">
    <description><p>q</p></description>
    <set name="x">1</set>
  </element>
  <element name="" type="Race" source="Hb" id="ID_HBT_RACE_NONAME"><description><p>n</p></description></element>
  <element name="Nest" type="Spell" source="Hb" id="ID_HBT_SPELL_O'NEST"><description><p>o</p></description><setters><set name="level">1</set><set name="school">Evocation</set><set name="time">1 action</set><set name="duration">Instantaneous</set><set name="range">Self</set></setters></element>
  <element name="Fine" type="Spell" source="Hb" id="ID_HBT_SPELL_FINE"><description><p>f</p></description><setters><set name="level">1</set><set name="school">Evocation</set><set name="time">1 action</set><set name="duration">Instantaneous</set><set name="range">Self</set></setters></element>
</elements>"#;

const B: &str = r#"<elements>
  <element name="Other" type="Feat" source="Hb" id="ID_HBT_FEAT_OTHER"><description><p>b</p></description></element>
</elements>"#;

fn app() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths {
        content_root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().join("data"),
    };
    (dir, AppState::new(paths).unwrap())
}

fn upload(name: &str, bytes: &[u8]) -> PreviewImportRequest {
    PreviewImportRequest {
        files: vec![ImportFileInput {
            file_name: name.into(),
            content_base64: STANDARD.encode(bytes),
        }],
        folder: None,
    }
}

fn install(app: &AppState, request: &PreviewImportRequest) -> crate::content_dto::ImportPackResult {
    let preview = app.preview_import_pack(request).unwrap();
    app.install_import_preview(&preview.preview_id, &NullSink)
        .unwrap()
}

fn assert_precise(issues: &[crate::content_dto::MigrationIssue], file: &str) {
    let warned: Vec<_> = issues.iter().filter(|i| i.severity != "info").collect();
    assert!(warned.len() >= 2, "{issues:#?}");
    for i in &warned {
        assert!(i.element.is_some(), "{i:?}");
        assert!(i.element_type.is_some(), "{i:?}");
        assert_eq!(i.file.as_deref(), Some(file), "{i:?}");
    }
    let dropped = issues
        .iter()
        .find(|i| i.element.as_deref() == Some("ID_HBT_RACE_NONAME"))
        .expect("dropped element reported");
    assert_eq!(
        (dropped.severity, dropped.element_type.as_deref()),
        ("error", Some("Race"))
    );
    let renamed = issues
        .iter()
        .find(|i| i.element.as_deref() == Some("ID_HBT_SPELL_O'NEST"))
        .expect("renamed element reported under its source id");
    assert_eq!(renamed.code, borealis_content::IssueCode::Normalized);
    assert_eq!(renamed.element_type.as_deref(), Some("Spell"));
}

#[test]
fn a_file_install_report_names_file_element_type_and_code() {
    let (_dir, app) = app();
    let result = install(&app, &upload("homebrew.xml", HB.as_bytes()));
    assert_eq!(result.report.converted, 3);
    assert_precise(&result.report.issues, "homebrew.xml");
    let wire = serde_json::to_value(&result.report.issues[0]).unwrap();
    for key in [
        "severity",
        "code",
        "message",
        "element",
        "elementType",
        "file",
        "line",
    ] {
        assert!(wire.get(key).is_some(), "{key} missing in {wire}");
    }
    assert!(wire["code"].is_string());
}

#[test]
fn a_folder_install_report_gives_the_path_inside_the_folder() {
    let (_dir, app) = app();
    let src = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(src.path().join("repo/sub")).unwrap();
    std::fs::write(src.path().join("repo/sub/hb.xml"), HB).unwrap();
    let token = app
        .picked
        .grant(src.path().join("repo"), PickPurpose::ContentFolder)
        .unwrap()
        .token;
    let result = install(
        &app,
        &PreviewImportRequest {
            files: Vec::new(),
            folder: Some(token),
        },
    );
    assert_precise(&result.report.issues, "sub/hb.xml");
}

/// Every file of every installed pack except `pack-meta.json` (which records the input's name).
fn installed_files(app_data: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else if entry.file_name() != "pack-meta.json" {
                let rel = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, std::fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(
        &app_data.join("library"),
        &app_data.join("library"),
        &mut out,
    );
    out
}

#[test]
fn folder_and_zips_with_or_without_the_top_folder_install_byte_identical_packs() {
    let src = tempfile::tempdir().unwrap();
    let repo: PathBuf = src.path().join("repo");
    std::fs::create_dir_all(repo.join("sub")).unwrap();
    std::fs::write(repo.join("sub/hb.xml"), HB).unwrap();
    std::fs::write(repo.join("b.xml"), B).unwrap();
    let (folder_dir, folder_app) = app();
    let token = folder_app
        .picked
        .grant(repo.clone(), PickPurpose::ContentFolder)
        .unwrap()
        .token;
    let folder_result = install(
        &folder_app,
        &PreviewImportRequest {
            files: Vec::new(),
            folder: Some(token),
        },
    );
    let folder = installed_files(&folder_dir.path().join("data"));
    assert!(
        folder.keys().any(|k| k.ends_with("repo/sub/hb.json")),
        "{:?}",
        folder.keys()
    );
    let nested = build(
        &[
            ("repo/b.xml", B.as_bytes()),
            ("repo/sub/hb.xml", HB.as_bytes()),
            ("__MACOSX/repo/sub/._hb.xml", &[0, 5, 22, 7]),
        ],
        true,
    );
    let flat = build(
        &[("b.xml", B.as_bytes()), ("sub/hb.xml", HB.as_bytes())],
        false,
    );
    for zip in [nested, flat] {
        let (zip_dir, zip_app) = app();
        let result = install(&zip_app, &upload("repo.zip", &zip));
        assert_eq!(
            installed_files(&zip_dir.path().join("data")),
            folder,
            "zip and folder packs differ"
        );
        assert_eq!(result.report.issues, folder_result.report.issues);
    }
}

struct Stub(HashMap<String, Vec<u8>>);

impl Fetcher for Stub {
    fn get(&self, url: &str, _max: usize) -> Result<Vec<u8>, String> {
        self.0
            .get(url)
            .cloned()
            .ok_or_else(|| "the server answered 404 Not Found".into())
    }
}

#[test]
fn update_indexes_returns_the_same_report_as_a_file_install() {
    const BASE: &str = "https://example.invalid/hb/master/";
    let index = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<index>\n  <info><name>Hb</name></info>\n  <files>\n    <file name=\"hb.xml\" url=\"{BASE}hb.xml\" />\n  </files>\n</index>"
    );
    let stub = Stub(HashMap::from([
        (format!("{BASE}hb.index"), index.into_bytes()),
        (format!("{BASE}hb.xml"), HB.as_bytes().to_vec()),
    ]));
    let (_dir, app) = app();
    app.add_index(&format!("{BASE}hb.index")).unwrap();
    let result = app.update_indexes(&stub, &NullSink).unwrap();
    assert_eq!(result.updated, 1);
    let [report] = result.reports.as_slice() else {
        panic!("one report per converted pack: {:?}", result.reports);
    };
    assert_eq!(report.converted, 3);
    assert_eq!(report.file_name, format!("{BASE}hb.index"));
    let file = report
        .issues
        .iter()
        .find_map(|i| i.file.clone())
        .expect("issues name their file");
    assert!(file.ends_with("hb.xml"), "{file}");
    assert_precise(&report.issues, &file);
    let again = app.update_indexes(&stub, &NullSink).unwrap();
    assert!(
        again.reports.is_empty(),
        "nothing converted when up to date"
    );
}

#[test]
fn the_ui_issue_code_union_names_only_real_codes() {
    let ts = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/src/api/types/content.ts"),
    )
    .unwrap();
    let start = ts.find("export type MigrationIssueCode").expect("union");
    let union = &ts[start..start + ts[start..].find(';').unwrap()];
    let codes: Vec<&str> = union.split('"').skip(1).step_by(2).collect();
    assert!(codes.len() >= 26, "{codes:?}");
    for code in codes {
        let parsed: Result<borealis_content::IssueCode, _> =
            serde_json::from_value(serde_json::Value::String(code.into()));
        assert!(parsed.is_ok(), "{code} is not an IssueCode");
    }
}

/// An invented file appending to an id only the bundled core pack defines, and to one nothing defines.
const APPENDS: &str = r#"<elements>
  <element name="Knack" type="Feat" source="Hb" id="ID_HBT_FEAT_KNACK"><description><p>k</p></description></element>
  <append id="ID_INTERNAL_GRANTS_CHARACTER_BASE">
    <rules><grant type="Feat" id="ID_HBT_FEAT_KNACK" /></rules>
  </append>
  <append id="ID_HBT_NOWHERE">
    <rules><grant type="Feat" id="ID_HBT_FEAT_KNACK" /></rules>
  </append>
</elements>"#;

#[test]
fn classify_drops_an_append_whose_target_is_installed_and_keeps_a_missing_one() {
    use borealis_content::{Issue, IssueCode};
    let missing = |id: &str| {
        Issue::warning(
            IssueCode::AppendTargetMissing,
            "append targets an element that does not exist",
        )
        .in_file("a.xml".to_string())
        .at_element(id.to_string())
    };
    let raw = vec![missing("ID_BUNDLED"), missing("ID_GONE")];
    let found = super::classify(&raw, &BTreeMap::new(), |t| t == "ID_BUNDLED");
    let kept: Vec<_> = found
        .diagnostics
        .iter()
        .map(|i| i.element.as_deref())
        .collect();
    assert_eq!(kept, vec![Some("ID_GONE")]);
    assert_eq!(found.summary.warnings, 1);
}

#[test]
fn an_append_to_a_bundled_core_id_is_not_reported_missing() {
    let (_dir, app) = app();
    let result = install(&app, &upload("appends.xml", APPENDS.as_bytes()));
    let missing: Vec<_> = result
        .report
        .issues
        .iter()
        .filter(|i| i.code == borealis_content::IssueCode::AppendTargetMissing)
        .map(|i| i.element.as_deref())
        .collect();
    assert_eq!(missing, vec![Some("ID_HBT_NOWHERE")]);
}

const LONE_INDEX: &str = r#"<index>
  <info><name>Hb</name><update version="1.0"><file name="hb.index" url="https://example.invalid/hb/master/hb.index"/></update></info>
  <files><file name="hb.xml" url="https://example.invalid/hb/master/hb.xml"/></files>
</index>"#;

#[test]
fn a_lone_index_preview_names_its_source_and_adding_it_downloads_the_listed_files() {
    const BASE: &str = "https://example.invalid/hb/master/";
    let (_dir, app) = app();
    let preview = app
        .preview_import_pack(&upload("hb.index", LONE_INDEX.as_bytes()))
        .unwrap();
    assert_eq!(preview.converted, 0);
    let url = preview.index_url.clone().expect("source offered");
    assert_eq!(url, format!("{BASE}hb.index"));
    // The offered outcome is the Indexes tab's: add the address, then update.
    app.cancel_import_preview(&preview.preview_id).unwrap();
    app.add_index(&url).unwrap();
    let stub = Stub(HashMap::from([
        (url.clone(), LONE_INDEX.as_bytes().to_vec()),
        (format!("{BASE}hb.xml"), HB.as_bytes().to_vec()),
    ]));
    let result = app.update_indexes(&stub, &NullSink).unwrap();
    assert_eq!(result.updated, 1);
    assert!(app.list_indexes().iter().any(|i| i.url == url));
}

#[test]
fn an_index_with_files_beside_it_or_no_https_source_offers_no_source() {
    let (_dir, app) = app();
    let beside = PreviewImportRequest {
        files: vec![
            upload("hb.index", LONE_INDEX.as_bytes()).files.remove(0),
            upload("hb.xml", HB.as_bytes()).files.remove(0),
        ],
        folder: None,
    };
    let preview = app.preview_import_pack(&beside).unwrap();
    assert!(preview.converted > 0 && preview.index_url.is_none());
    let plain = LONE_INDEX.replace("https://", "http://");
    let preview = app
        .preview_import_pack(&upload("hb.index", plain.as_bytes()))
        .unwrap();
    assert!(preview.index_url.is_none());
}

/// Issue 8: references a book of the set names (`TBK` = Test Book) but does not define, one to a
/// book nothing loaded names, and a marker that requirements only test the absence of.
const REFS: &str = r#"<elements>
  <element name="Test Book" type="Source" source="Core" id="ID_TBK_SOURCE">
    <setters><set name="abbreviation">TBK</set><set name="url">https://example.invalid</set></setters>
  </element>
  <element name="Seeker" type="Feat" source="Test Book" id="ID_TBK_FEAT_SEEKER">
    <description><p>s</p></description>
    <requirements>!ID_ZZQ_GRANTS_MARKER</requirements>
    <rules>
      <grant type="Feat" id="ID_TBK_FEAT_MISPELLED" />
      <grant type="Feat" id="ID_ZZQ_FEAT_ELSEWHERE" />
    </rules>
  </element>
</elements>"#;

#[test]
fn unresolved_references_name_their_files_and_known_books_and_conditions_are_apart() {
    let (_dir, app) = app();
    let preview = app
        .preview_import_pack(&upload("refs.xml", REFS.as_bytes()))
        .unwrap();
    let targets: Vec<(&str, &[String], Option<&str>)> = preview
        .unresolved
        .iter()
        .map(|u| {
            (
                u.target.as_str(),
                u.files.as_slice(),
                u.known_source.as_deref(),
            )
        })
        .collect();
    let file = vec!["refs.xml".to_string()];
    assert_eq!(
        targets,
        vec![
            ("ID_TBK_FEAT_MISPELLED", file.as_slice(), Some("Test Book")),
            ("ID_ZZQ_FEAT_ELSEWHERE", file.as_slice(), None),
        ]
    );
    assert_eq!(preview.unresolved_total, 2);
    let conditions: Vec<&str> = preview
        .conditions
        .iter()
        .map(|u| u.target.as_str())
        .collect();
    assert_eq!(conditions, vec!["ID_ZZQ_GRANTS_MARKER"]);
    assert_eq!(preview.conditions_total, 1);
}
