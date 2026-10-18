use super::*;
use crate::content_index::HttpsFetcher;
use std::collections::BTreeSet;

fn gh(owner: &str, repo: &str, git_ref: Option<&str>, path: Option<&str>) -> FiveToolsLocation {
    FiveToolsLocation::GitHub {
        owner: owner.into(),
        repo: repo.into(),
        git_ref: git_ref.map(str::to_string),
        path: path.map(str::to_string),
    }
}

#[test]
fn parses_github_and_folder_addresses() {
    let ok = |u: &str| parse_location(u).unwrap();
    assert_eq!(ok("https://github.com/o/r"), gh("o", "r", None, None));
    assert_eq!(
        ok(" https://github.com/o/r.git/ "),
        gh("o", "r", None, None)
    );
    assert_eq!(
        ok("https://www.github.com/o/r?tab=readme"),
        gh("o", "r", None, None)
    );
    assert_eq!(
        ok("https://github.com/o/r/tree/main"),
        gh("o", "r", Some("main"), None)
    );
    assert_eq!(
        ok("https://github.com/o/r/tree/v2.1/sub/data"),
        gh("o", "r", Some("v2.1"), Some("sub/data"))
    );
    assert_eq!(
        ok("https://raw.githubusercontent.com/o/r/main/data/"),
        gh("o", "r", Some("main"), Some("data"))
    );
    assert_eq!(
        ok("https://example.org/5e/data"),
        FiveToolsLocation::Folder {
            base: "https://example.org/5e/data/".into()
        }
    );
    assert_eq!(
        ok("https://example.org/5e/data/books.json"),
        FiveToolsLocation::Folder {
            base: "https://example.org/5e/data/".into()
        }
    );
}

#[test]
fn refuses_unsafe_or_non_https_addresses() {
    for bad in [
        "http://github.com/o/r",
        "ftp://example.org/data",
        "github.com/o/r",
        "https://",
        "https://github.com/o",
        "https://github.com/o/r/blob/main/x.json",
        "https://github.com/o/r/tree/../x",
        "https://github.com/o/r/tree/main/data/../../x",
        "https://raw.githubusercontent.com/o/r",
    ] {
        assert!(parse_location(bad).is_err(), "{bad}");
    }
    assert!(parse_location("http://example.org/data")
        .unwrap_err()
        .contains("https://"));
}

#[test]
fn reads_the_git_ref_advertisement() {
    let line = |payload: &str| format!("{:04x}{payload}", payload.len() + 4);
    let sha = |c: char| c.to_string().repeat(40);
    let body = [
        line("# service=git-upload-pack\n"),
        "0000".into(),
        line(&format!(
            "{} HEAD\0multi_ack symref=HEAD:refs/heads/main\n",
            sha('a')
        )),
        line(&format!("{} refs/heads/main\n", sha('a'))),
        line(&format!("{} refs/tags/v1\n", sha('b'))),
        line(&format!("{} refs/tags/v1^{{}}\n", sha('c'))),
        "0000".into(),
    ]
    .concat();
    let refs = parse_ref_advertisement(body.as_bytes());
    assert_eq!(refs["HEAD"], sha('a'));
    assert_eq!(refs["refs/heads/main"], sha('a'));
    assert_eq!(refs["refs/tags/v1^{}"], sha('c'));
    assert!(parse_ref_advertisement(b"zzzz").is_empty());
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1f, 0x8b, 8, 8, 0, 0, 0, 0, 0, 3];
    out.extend_from_slice(b"name.json\0");
    out.extend(miniz_oxide::deflate::compress_to_vec(data, 6));
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
    out
}

#[test]
fn gunzip_decodes_and_caps_output() {
    let data = b"{\"race\":[]}".repeat(1000);
    assert_eq!(gunzip(&gzip(&data), data.len()).unwrap(), data);
    // A compression bomb stops at the cap.
    let bomb = gzip(&vec![b' '; 10 * 1024 * 1024]);
    assert!(bomb.len() < 64 * 1024);
    assert!(gunzip(&bomb, 1024 * 1024).is_err());
    assert!(gunzip(b"not gzip at all, not at all", 100).is_err());
    let mut wrong_size = gzip(&data);
    let n = wrong_size.len();
    wrong_size[n - 4] ^= 1;
    assert!(gunzip(&wrong_size, data.len() + 10).is_err());
}

/// The importer's own root-file list, read from its source: the one list the download follows.
fn importer_root_files() -> BTreeSet<String> {
    const SRC: &str = include_str!("../../crates/borealis-import-5etools/src/folder.rs");
    const DECL: &str = "const ROOT_FILES: &[&str] = &[";
    let body = &SRC[SRC.find(DECL).expect("importer declares ROOT_FILES") + DECL.len()..];
    let body = &body[..body.find("];").expect("ROOT_FILES ends")];
    body.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

#[test]
fn root_files_are_exactly_what_the_importer_reads() {
    let importer = importer_root_files();
    assert!(
        importer.contains("charcreationoptions.json"),
        "{importer:?}"
    );
    let ours: BTreeSet<String> = ROOT_FILES
        .iter()
        .filter(|f| !f.starts_with("generated/"))
        .map(|s| (*s).to_string())
        .collect();
    assert_eq!(ours, importer, "the importer's root file list changed");
    // Every listed file (the spell lookup included) is one the importer actually picks up.
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("generated")).unwrap();
    for f in ROOT_FILES {
        std::fs::write(dir.path().join(f), "{}").unwrap();
    }
    std::fs::write(dir.path().join("bestiary.json"), "{}").unwrap();
    let read: BTreeSet<String> = borealis_import_5etools::folder::data_files(dir.path())
        .unwrap()
        .into_iter()
        .collect();
    let all: BTreeSet<String> = ROOT_FILES.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(read, all);
}

/// Serves a local folder as `https://mirror.test/data/<rel>` (404 for anything missing).
struct Mirror(std::path::PathBuf);

impl RemoteFetcher for Mirror {
    fn fetch(&self, request: &FetchRequest<'_>) -> Result<FetchResponse, String> {
        let rel = request
            .url
            .strip_prefix("https://mirror.test/data/")
            .ok_or("unexpected address")?;
        Ok(match std::fs::read(self.0.join(rel)) {
            Ok(body) => FetchResponse {
                status: 200,
                body,
                ..FetchResponse::default()
            },
            Err(_) => FetchResponse {
                status: 404,
                ..FetchResponse::default()
            },
        })
    }
}

#[test]
fn a_url_download_holds_every_file_the_importer_reads_from_the_folder() {
    let src = tempfile::tempdir().unwrap();
    for rel in importer_root_files().iter().map(String::as_str).chain([
        "generated/gendata-spell-source-lookup.json",
        "class/class-x.json",
        "spells/spells-x.json",
    ]) {
        let path = src.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "{}").unwrap();
    }
    std::fs::write(
        src.path().join("class/index.json"),
        r#"{"x":"class-x.json"}"#,
    )
    .unwrap();
    std::fs::write(
        src.path().join("spells/index.json"),
        r#"{"x":"spells-x.json"}"#,
    )
    .unwrap();
    let dest = tempfile::tempdir().unwrap();
    download_folder(
        &Mirror(src.path().to_path_buf()),
        "https://mirror.test/data/",
        dest.path(),
        None,
        &crate::content_packs::NullSink,
    )
    .unwrap();
    let read = |d: &Path| borealis_import_5etools::folder::data_files(d).unwrap();
    let got = read(dest.path());
    assert!(got.contains(&"charcreationoptions.json".to_string()));
    assert_eq!(got, read(src.path()));
}

#[test]
fn manifest_names_must_be_plain_file_names() {
    let ok = manifest_files(
        "class",
        br#"{"wizard":"class-wizard.json","x":"class-wizard.json"}"#,
    );
    assert_eq!(ok.unwrap(), vec!["class/class-wizard.json"]);
    for evil in [
        r#"{"a":"../../evil.json"}"#,
        r#"{"a":"sub/x.json"}"#,
        r#"{"a":"/etc/x.json"}"#,
        r#"{"a":"x\\..\\y.json"}"#,
        r#"{"a":"readme.md"}"#,
    ] {
        assert!(manifest_files("class", evil.as_bytes()).is_err(), "{evil}");
    }
    assert!(manifest_files("class", b"[1,2]").is_err());
}

#[test]
fn the_real_client_refuses_plain_http_without_connecting() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/data/books.json", listener.local_addr().unwrap());
    let err = HttpsFetcher
        .fetch(&FetchRequest::get(&url, 1024))
        .unwrap_err();
    assert!(err.contains("https://"), "{err}");
    assert!(listener.accept().is_err(), "no connection may be made");
}
