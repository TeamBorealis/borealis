//! Rules-based content import: preview before install, multi-file/folder/index/zip inputs.
//! All fixtures are synthetic.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_lib::content_dto::ImportPackRequest;
use borealis_lib::content_packs::NullSink;
use borealis_lib::content_preview::{ImportFileInput, PreviewImportRequest};
use borealis_lib::picked_paths::PickPurpose;
use borealis_lib::state::{AppPaths, AppState};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn element(id: &str, name: &str, grants: &[&str]) -> String {
    let rules: String = grants
        .iter()
        .map(|g| format!(r#"<grant type="Feat" id="{g}"/>"#))
        .collect();
    format!(
        r#"<element name="{name}" type="Feat" source="Test Book" id="{id}">
  <description><p>A synthetic feat.</p></description>
  <rules>{rules}</rules>
</element>"#
    )
}

fn xml(elements: &[String]) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<elements>\n  <info><name>Test Book</name><update version=\"1.0.0\"><file name=\"test-book.xml\" url=\"https://example.invalid/test-book.xml\"/></update></info>\n{}\n</elements>",
        elements.join("\n")
    )
}

fn file_a() -> String {
    xml(&[element("ID_TEST_FEAT_A", "Alpha", &[])])
}

/// Grants Alpha (defined in the other file) and an id nothing defines.
fn file_b() -> String {
    xml(&[element(
        "ID_TEST_FEAT_B",
        "Beta",
        &["ID_TEST_FEAT_A", "ID_TEST_FEAT_NOWHERE"],
    )])
}

fn app() -> (tempfile::TempDir, AppState) {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    let state = AppState::new(paths).expect("state");
    (dir, state)
}

fn input(name: &str, body: &[u8]) -> ImportFileInput {
    ImportFileInput {
        file_name: name.into(),
        content_base64: STANDARD.encode(body),
    }
}

fn files(list: &[(&str, &[u8])]) -> PreviewImportRequest {
    PreviewImportRequest {
        files: list.iter().map(|(n, b)| input(n, b)).collect(),
        folder: None,
    }
}

fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                let rel = p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
                out.insert(rel, std::fs::read(&p).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

fn unresolved_targets(p: &borealis_lib::content_preview::ImportPreview) -> Vec<&str> {
    p.unresolved.iter().map(|u| u.target.as_str()).collect()
}

#[test]
fn preview_then_install_equals_import_pack() {
    let body = file_a();
    let (_d1, direct) = app();
    direct
        .import_pack(
            &ImportPackRequest {
                file_name: "alpha.xml".into(),
                content_base64: STANDARD.encode(&body),
                format: None,
            },
            &NullSink,
        )
        .expect("import_pack");

    let (_d2, staged) = app();
    let preview = staged
        .preview_import_pack(&files(&[("alpha.xml", body.as_bytes())]))
        .expect("preview");
    assert_eq!(preview.converted, 1);
    assert_eq!(preview.counts[0].ty, "Feat");
    assert!(
        staged.list_packs().unwrap().iter().all(|p| p.id != "alpha"),
        "a preview must not install anything"
    );
    let result = staged
        .install_import_preview(&preview.preview_id, &NullSink)
        .expect("install");
    assert_eq!(result.pack.as_ref().map(|p| p.id.as_str()), Some("alpha"));

    assert_eq!(
        tree(&direct.paths.library().join("alpha")),
        tree(&staged.paths.library().join("alpha")),
        "installed bytes must be identical"
    );
    // A preview can only be installed once.
    assert!(staged
        .install_import_preview(&preview.preview_id, &NullSink)
        .is_err());
}

#[test]
fn cancelled_preview_cannot_be_installed() {
    let (_d, app) = app();
    let p = app
        .preview_import_pack(&files(&[("alpha.xml", file_a().as_bytes())]))
        .unwrap();
    app.cancel_import_preview(&p.preview_id).unwrap();
    assert!(app
        .install_import_preview(&p.preview_id, &NullSink)
        .is_err());
}

#[test]
fn a_single_file_reports_references_it_cannot_resolve() {
    let (_d, app) = app();
    let p = app
        .preview_import_pack(&files(&[("beta.xml", file_b().as_bytes())]))
        .unwrap();
    let targets = unresolved_targets(&p);
    assert!(targets.contains(&"ID_TEST_FEAT_A"), "{targets:?}");
    assert!(targets.contains(&"ID_TEST_FEAT_NOWHERE"), "{targets:?}");
    assert_eq!(p.unresolved[0].referenced_by, vec!["ID_TEST_FEAT_B"]);
}

#[test]
fn several_files_are_converted_as_one_set_so_cross_references_resolve() {
    let (_d, app) = app();
    let p = app
        .preview_import_pack(&files(&[
            ("alpha.xml", file_a().as_bytes()),
            ("beta.xml", file_b().as_bytes()),
        ]))
        .unwrap();
    assert_eq!(p.converted, 2);
    assert_eq!(p.file_count, 2);
    assert_eq!(unresolved_targets(&p), vec!["ID_TEST_FEAT_NOWHERE"]);
    let installed = app
        .install_import_preview(&p.preview_id, &NullSink)
        .unwrap();
    assert_eq!(installed.pack.unwrap().element_count, 2);
}

#[test]
fn a_whole_folder_resolves_cross_file_references_recursively() {
    let (_d, app) = app();
    let folder = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(folder.path().join("nested/deeper")).unwrap();
    std::fs::write(folder.path().join("nested/deeper/a.xml"), file_a()).unwrap();
    std::fs::write(folder.path().join("b.xml"), file_b()).unwrap();
    std::fs::write(folder.path().join("notes.txt"), "ignored").unwrap();
    let token = app
        .picked
        .grant(folder.path().to_path_buf(), PickPurpose::ContentFolder)
        .unwrap()
        .token;
    let p = app
        .preview_import_pack(&PreviewImportRequest {
            files: Vec::new(),
            folder: Some(token),
        })
        .unwrap();
    assert_eq!(p.converted, 2);
    assert_eq!(p.file_count, 2);
    assert_eq!(unresolved_targets(&p), vec!["ID_TEST_FEAT_NOWHERE"]);
    assert!(p
        .diagnostics
        .iter()
        .all(|d| d.file.as_deref().is_none_or(|f| !f.contains('/'))));
}

#[test]
fn a_folder_token_of_the_wrong_purpose_is_refused() {
    let (_d, app) = app();
    let folder = tempfile::tempdir().unwrap();
    let token = app
        .picked
        .grant(folder.path().to_path_buf(), PickPurpose::FivetoolsFolder)
        .unwrap()
        .token;
    assert!(app
        .preview_import_pack(&PreviewImportRequest {
            files: Vec::new(),
            folder: Some(token)
        })
        .is_err());
}

const INDEX: &str = r#"<index app="1.0">
  <info><name>Pack</name></info>
  <files>
    <file name="alpha.xml" url="https://example.invalid/alpha.xml"/>
    <file name="remote-only.xml" url="https://example.invalid/remote-only.xml"/>
  </files>
</index>"#;

#[test]
fn an_index_converts_its_listed_files_that_are_present_and_notes_the_rest() {
    let (_d, app) = app();
    let p = app
        .preview_import_pack(&files(&[
            ("pack.index", INDEX.as_bytes()),
            ("alpha.xml", file_a().as_bytes()),
        ]))
        .unwrap();
    assert_eq!(p.converted, 1);
    assert_eq!(p.pack_id, "pack");
    let missing: Vec<_> = p
        .diagnostics
        .iter()
        .filter(|d| d.message.contains("remote-only.xml"))
        .collect();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].severity, "info");
    assert!(
        !p.diagnostics
            .iter()
            .any(|d| d.message.contains("alpha.xml")),
        "the present entry must be found"
    );
}

#[test]
fn an_index_alone_is_accepted_and_converts_nothing_without_failing() {
    let (_d, app) = app();
    let p = app
        .preview_import_pack(&files(&[("pack.index", INDEX.as_bytes())]))
        .unwrap();
    assert_eq!(p.converted, 0);
    assert!(app
        .install_import_preview(&p.preview_id, &NullSink)
        .is_err());
}

/// A stored (uncompressed) zip built by hand.
fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, body) in entries {
        let at = out.len() as u32;
        let (n, l) = (name.len() as u16, body.len() as u32);
        out.extend(0x0403_4b50u32.to_le_bytes());
        out.extend([20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        out.extend(l.to_le_bytes());
        out.extend(l.to_le_bytes());
        out.extend(n.to_le_bytes());
        out.extend([0, 0]);
        out.extend(name.as_bytes());
        out.extend(*body);
        central.extend(0x0201_4b50u32.to_le_bytes());
        central.extend([20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        central.extend(l.to_le_bytes());
        central.extend(l.to_le_bytes());
        central.extend(n.to_le_bytes());
        central.extend([0; 12]);
        central.extend(at.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let cen_at = out.len() as u32;
    let cen_len = central.len() as u32;
    out.extend(central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0, 0, 0, 0]);
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend(cen_len.to_le_bytes());
    out.extend(cen_at.to_le_bytes());
    out.extend([0, 0]);
    out
}

#[test]
fn a_zip_is_unpacked_and_converted_as_one_set() {
    let (_d, app) = app();
    let zip = zip_of(&[
        ("book/alpha.xml", file_a().as_bytes()),
        ("book/beta.xml", file_b().as_bytes()),
        ("book/readme.txt", b"skip"),
        ("../escape.xml", file_a().as_bytes()),
    ]);
    let p = app
        .preview_import_pack(&files(&[("my-book.zip", &zip)]))
        .unwrap();
    assert_eq!(p.converted, 2);
    assert_eq!(p.pack_id, "my-book");
    assert_eq!(unresolved_targets(&p), vec!["ID_TEST_FEAT_NOWHERE"]);
}

#[test]
fn import_pack_accepts_a_zip_too() {
    let (_d, app) = app();
    let zip = zip_of(&[("alpha.xml", file_a().as_bytes())]);
    let r = app
        .import_pack(
            &ImportPackRequest {
                file_name: "book.zip".into(),
                content_base64: STANDARD.encode(zip),
                format: None,
            },
            &NullSink,
        )
        .unwrap();
    assert_eq!(r.pack.unwrap().element_count, 1);
}

#[test]
fn bad_input_is_refused_with_a_message() {
    let (_d, app) = app();
    assert!(app
        .preview_import_pack(&PreviewImportRequest::default())
        .is_err());
    assert!(app
        .preview_import_pack(&files(&[("x.zip", b"not a zip")]))
        .is_err());
    assert!(app
        .preview_import_pack(&PreviewImportRequest {
            files: vec![ImportFileInput {
                file_name: "a.xml".into(),
                content_base64: "!!!".into()
            }],
            folder: None
        })
        .is_err());
}

/// Optional sanity run over a local corpus (never committed): every repo folder previews without error.
#[test]
fn corpus_folders_preview_when_available() {
    let Ok(dir) = std::env::var("BOREALIS_CORPUS_DIR") else {
        eprintln!("BOREALIS_CORPUS_DIR not set; skipping");
        return;
    };
    let (_d, app) = app();
    let (mut ok, mut total) = (0usize, 0usize);
    for repo in std::fs::read_dir(dir).unwrap().flatten().take(5) {
        if !repo.path().is_dir() {
            continue;
        }
        total += 1;
        let token = app
            .picked
            .grant(repo.path(), PickPurpose::ContentFolder)
            .unwrap()
            .token;
        if let Ok(p) = app.preview_import_pack(&PreviewImportRequest {
            files: Vec::new(),
            folder: Some(token),
        }) {
            ok += 1;
            eprintln!("folder: {} files, {} elements", p.file_count, p.converted);
        }
    }
    assert_eq!(ok, total);
}
