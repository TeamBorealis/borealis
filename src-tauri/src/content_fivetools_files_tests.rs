use super::*;
use crate::content_dto::FiveToolsImportRequest;
use crate::content_packs::NullSink;
use crate::zip_read::test_zip::build;

fn fixture() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/borealis-import-5etools/tests/fixtures/homebrew/quill-brew.json"),
    )
    .unwrap()
}

fn input(name: &str, bytes: &[u8]) -> ImportFileInput {
    ImportFileInput {
        file_name: name.into(),
        content_base64: STANDARD.encode(bytes),
    }
}

fn app() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let paths = crate::state::AppPaths {
        content_root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().join("data"),
    };
    (dir, AppState::new(paths).unwrap())
}

fn err(app: &AppState, files: &[ImportFileInput]) -> String {
    app.stage_5etools_files(files).unwrap_err().message
}

#[test]
fn a_single_homebrew_file_scans_and_imports_as_its_own_pack() {
    let (_dir, app) = app();
    let staged = app
        .stage_5etools_files(&[input("Quill Brew.json", &fixture())])
        .unwrap();
    assert_eq!(staged.name, "Quill Brew.json");
    let scan = app.scan_5etools_folder(&staged.token).unwrap();
    assert_eq!(scan.sources.len(), 1);
    let source = &scan.sources[0];
    assert_eq!(
        (source.abbr.as_str(), source.name.as_str()),
        ("QuillBrew", "The Quill Brew")
    );
    assert_eq!(source.category, "homebrew");
    assert_eq!(source.unsupported.get("deity"), Some(&1));
    assert_eq!(source.needs, vec!["InkwellExtras".to_string()]);
    let request = FiveToolsImportRequest {
        folder: staged.token.clone(),
        sources: vec!["QuillBrew".into()],
        include_fluff: false,
    };
    let report = app.import_5etools(&request, &NullSink).unwrap();
    assert_eq!(report.packs.len(), 1);
    assert_eq!(report.packs[0].id, "5etools-quillbrew");
    assert_eq!(report.packs[0].redistributable, Some(false));
    assert!(report.converted >= 8, "{}", report.converted);
    let skipped: Vec<(&str, &str)> = report
        .skipped
        .iter()
        .map(|n| (n.kind.as_str(), n.reason.as_str()))
        .collect();
    assert!(skipped.contains(&("deity", "not imported: Borealis has no deities")));
    assert!(skipped.contains(&("vehicle", "not imported: Borealis has no vehicles")));
}

#[test]
fn several_files_and_a_data_zip_are_staged_together() {
    let (_dir, app) = app();
    let extras = br#"{"_meta": {"sources": [{"json": "InkwellExtras", "full": "Inkwell Extras"}]},
        "feat": [{"name": "Blotter", "source": "InkwellExtras", "entries": ["Dry."]}]}"#;
    let zip = build(
        &[
            ("data/books.json", b"{\"book\":[]}"),
            (
                "data/feats.json",
                br#"{"feat":[{"name":"Plain","source":"ZZZ","entries":["x"]}]}"#,
            ),
        ],
        true,
    );
    let staged = app
        .stage_5etools_files(&[
            input("quill.json", &fixture()),
            input("QUILL.json", extras),
            input("data.zip", &zip),
        ])
        .unwrap();
    assert_eq!(staged.name, "quill.json and 2 more");
    let scan = app.scan_5etools_folder(&staged.token).unwrap();
    let abbrs: Vec<&str> = scan.sources.iter().map(|s| s.abbr.as_str()).collect();
    assert_eq!(abbrs, vec!["InkwellExtras", "QuillBrew", "ZZZ"]);
    // the dependency was chosen with it: nothing is missing
    assert!(scan.sources.iter().all(|s| s.needs.is_empty()));
}

#[test]
fn a_new_stage_replaces_the_previous_one() {
    let (_dir, app) = app();
    let first = app
        .stage_5etools_files(&[input("a.json", &fixture())])
        .unwrap();
    let second = app
        .stage_5etools_files(&[input("b.json", &fixture())])
        .unwrap();
    assert!(app.scan_5etools_folder(&first.token).is_err());
    assert!(app.scan_5etools_folder(&second.token).is_ok());
}

#[test]
fn files_that_are_not_5etools_homebrew_are_refused_with_a_reason() {
    let (_dir, app) = app();
    assert!(err(&app, &[]).contains("at least one"));
    assert!(err(&app, &[input("notes.txt", b"hi")]).contains("not a .json or .zip"));
    assert!(err(&app, &[input("bad.json", b"{ nope")]).contains("not valid JSON"));
    let borealis = br#"{"format": "borealis-content/1", "elements": []}"#;
    assert!(err(&app, &[input("mine.json", borealis)]).contains("Custom content"));
    assert!(err(&app, &[input("char.json", br#"{"name": "Hero"}"#)])
        .contains("not a 5etools-format homebrew file"));
    let only_deities = br#"{"_meta": {"sources": [{"json": "D", "full": "D"}]},
        "deity": [{"name": "A", "source": "D"}]}"#;
    assert_eq!(
        err(&app, &[input("gods.json", only_deities)]),
        "Nothing in gods.json can be imported: it only holds deity."
    );
    let zip = build(&[("data/books.json", b"{}")], false);
    assert!(err(&app, &[input("a.zip", &zip), input("b.zip", &zip)]).contains("one .zip"));
    let many: Vec<ImportFileInput> = (0..=MAX_FILES)
        .map(|i| input(&format!("h{i}.json"), &fixture()))
        .collect();
    assert!(err(&app, &many).contains("at most"));
    // nothing was staged by a refused set
    assert!(!app.paths.data.join(STAGE_DIR).exists());
}

#[test]
fn staged_names_are_safe_and_unique() {
    assert_eq!(safe_file_name("../../evil name!.json"), "evil name_.json");
    assert_eq!(safe_file_name(".hidden.json"), "hidden.json");
    assert_eq!(safe_file_name("..json"), "homebrew.json");
    let mut taken = BTreeSet::new();
    assert_eq!(unique_name("a.json".into(), &mut taken), "a.json");
    assert_eq!(unique_name("A.json".into(), &mut taken), "A-2.json");
    assert_eq!(unique_name("a.json".into(), &mut taken), "a-3.json");
}

#[test]
fn the_custom_content_preview_refuses_a_5etools_file_with_directions() {
    // the reported failure: "Not a Borealis content file: unknown field `_meta`"
    let (_dir, app) = app();
    let preview = |files: Vec<ImportFileInput>| {
        app.preview_import_pack(&crate::content_preview::PreviewImportRequest {
            files,
            folder: None,
        })
        .unwrap_err()
        .message
    };
    let alone = preview(vec![input("quill-brew.json", &fixture())]);
    assert_eq!(alone, route_to_5etools_message("quill-brew.json"));
    assert!(alone.contains(HOMEBREW_BUTTON), "{alone}");
    assert!(!alone.contains("unknown field"), "{alone}");
    let with_xml = preview(vec![
        input("a.xml", b"<elements/>"),
        input("quill-brew.json", &fixture()),
    ]);
    assert_eq!(with_xml, route_to_5etools_message("quill-brew.json"));
}

#[test]
fn the_custom_content_import_points_5etools_files_to_the_5etools_import() {
    assert!(refuse_5etools_json("brew.json", &fixture())
        .unwrap_err()
        .message
        .contains("5etools-format import"));
    assert!(refuse_5etools_json("pack.json", br#"{"format": "x", "elements": []}"#).is_ok());
    assert!(refuse_5etools_json("pack.xml", b"<elements/>").is_ok());
}
