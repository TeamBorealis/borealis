//! Network-mocked download tests against the loopback [`TestServer`] (never the real model, never the internet).

use super::http::HttpTransport;
use super::test_server::{Route, TestServer};
use super::*;
use crate::models::{Q6_K_TE_Q4_K, Q8_0};
use std::collections::HashMap;

/// 1 MiB + a bit of deterministic bytes (several copy chunks).
fn blob() -> Vec<u8> {
    (0..(1_048_576 + 12_345u32))
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect()
}

fn sha_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

fn spec_for(data: &[u8], sources: Vec<String>) -> DownloadSpec {
    DownloadSpec {
        file_name: "embeddinggemma-2-TEST.gguf".into(),
        sha256: sha_hex(data),
        bytes: data.len() as u64,
        sources,
    }
}

fn server(routes: &[(&str, Route)]) -> TestServer {
    TestServer::start(
        routes
            .iter()
            .map(|(p, r)| ((*p).to_string(), r.clone()))
            .collect::<HashMap<_, _>>(),
    )
}

fn good(data: &[u8]) -> Route {
    Route {
        data: data.to_vec(),
        ..Route::default()
    }
}

fn corrupt(data: &[u8]) -> Route {
    let mut bad = data.to_vec();
    bad[1000] ^= 0xff;
    good(&bad)
}

fn run(dir: &ModelDir, spec: &DownloadSpec) -> Result<Outcome, DownloadError> {
    download(
        dir,
        spec,
        &HttpTransport::loopback_for_tests(),
        &Control::new(),
        &mut |_| {},
    )
}

#[test]
fn downloads_verifies_and_renames_with_no_identifying_headers() {
    let data = blob();
    let srv = server(&[("/m.gguf", good(&data))]);
    let tmp = tempfile::tempdir().unwrap();
    let dir = ModelDir::new(tmp.path().join("models"));
    let spec = spec_for(&data, vec![srv.url("/m.gguf")]);
    let mut last = None;
    let out = download(
        &dir,
        &spec,
        &HttpTransport::loopback_for_tests(),
        &Control::new(),
        &mut |p| last = Some(p),
    )
    .unwrap();
    assert_eq!(out, Outcome::Installed(dir.final_path(&spec)));
    assert_eq!(fs::read(dir.final_path(&spec)).unwrap(), data);
    assert!(!dir.partial_path(&spec).exists(), "partial renamed away");
    assert!(dir.is_installed(&spec));
    assert_eq!(
        last.map(|p| (p.phase, p.done)),
        Some((Phase::Verifying, spec.bytes))
    );
    // LICENSE + NOTICE next to the file (EM6).
    assert!(fs::read_to_string(dir.root().join(LICENSE_FILE_NAME))
        .unwrap()
        .contains("Apache License"));
    assert!(dir.root().join(NOTICE_FILE_NAME).exists());
    let reqs = srv.requests();
    assert_eq!(reqs.len(), 1);
    let names: Vec<&str> = reqs[0].headers.iter().map(|(n, _)| n.as_str()).collect();
    for name in &names {
        assert!(
            ["host", "accept"].contains(name),
            "unexpected header {name}: {names:?}"
        );
    }
}

#[test]
fn nothing_is_requested_without_a_source() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = ModelDir::new(tmp.path().join("models"));
    let no_source = DownloadSpec {
        sources: Vec::new(),
        ..DownloadSpec::for_file(&Q6_K_TE_Q4_K)
    };
    assert_eq!(run(&dir, &no_source), Err(DownloadError::NotAvailable));
    assert_eq!(
        DownloadError::NotAvailable.to_string(),
        "The model for this device is not available yet."
    );
    assert!(
        !dir.root().exists(),
        "no folder is created for an unavailable file"
    );
}

#[test]
fn compiled_files_use_hugging_face_then_the_published_mirror() {
    let q8 = DownloadSpec::for_file(&Q8_0);
    assert_eq!(q8.sources.len(), 2, "Hugging Face, then the mirror");
    assert!(q8.sources[0].starts_with("https://huggingface.co/"));
    assert!(q8.sources[1].starts_with(crate::models::MIRROR_RELEASE_BASE));
    let t1 = DownloadSpec::for_file(&Q6_K_TE_Q4_K);
    assert_eq!(t1.sources, vec![Q6_K_TE_Q4_K.mirror_url.to_owned()]);
}

#[test]
fn resumes_with_range_after_an_interruption() {
    let data = blob();
    let cut = 300_000;
    let srv = server(&[(
        "/m.gguf",
        Route {
            cut_first_after: Some(cut),
            ..good(&data)
        },
    )]);
    let tmp = tempfile::tempdir().unwrap();
    let dir = ModelDir::new(tmp.path().join("models"));
    let spec = spec_for(&data, vec![srv.url("/m.gguf")]);
    let first = run(&dir, &spec);
    assert_eq!(
        first,
        Err(DownloadError::Failed(
            "the connection was interrupted".into()
        ))
    );
    let have = dir.partial_bytes(&spec);
    assert!(have > 0 && have <= cut as u64, "partial kept: {have}");
    assert!(!dir.final_path(&spec).exists());

    assert_eq!(
        run(&dir, &spec).unwrap(),
        Outcome::Installed(dir.final_path(&spec))
    );
    assert_eq!(fs::read(dir.final_path(&spec)).unwrap(), data);
    let reqs = srv.requests();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[0].header("range"), None);
    assert_eq!(
        reqs[1].header("range"),
        Some(format!("bytes={have}-").as_str())
    );
}

#[test]
fn a_server_that_ignores_range_restarts_from_zero() {
    let data = blob();
    let srv = server(&[(
        "/m.gguf",
        Route {
            ignore_range: true,
            ..good(&data)
        },
    )]);
    let tmp = tempfile::tempdir().unwrap();
    let dir = ModelDir::new(tmp.path().join("models"));
    let spec = spec_for(&data, vec![srv.url("/m.gguf")]);
    fs::create_dir_all(dir.root().join(PARTIAL_DIR)).unwrap();
    fs::write(dir.partial_path(&spec), &data[..5000]).unwrap();
    run(&dir, &spec).unwrap();
    assert_eq!(fs::read(dir.final_path(&spec)).unwrap(), data);
    assert_eq!(srv.requests()[0].header("range"), Some("bytes=5000-"));
}

#[test]
fn checksum_mismatch_deletes_and_tries_the_other_url_once() {
    let data = blob();
    let srv = server(&[("/bad", corrupt(&data)), ("/good", good(&data))]);
    let tmp = tempfile::tempdir().unwrap();
    let dir = ModelDir::new(tmp.path().join("models"));
    let spec = spec_for(&data, vec![srv.url("/bad"), srv.url("/good")]);
    assert_eq!(
        run(&dir, &spec).unwrap(),
        Outcome::Installed(dir.final_path(&spec))
    );
    assert_eq!(fs::read(dir.final_path(&spec)).unwrap(), data);
    let paths: Vec<String> = srv.requests().into_iter().map(|r| r.path).collect();
    assert_eq!(paths, ["/bad", "/good"]);
    // The second URL started from zero: the bad partial was deleted.
    assert_eq!(srv.requests()[1].header("range"), None);
}

#[test]
fn two_mismatches_fail_with_a_plain_error_and_leave_nothing() {
    let data = blob();
    let srv = server(&[("/a", corrupt(&data)), ("/b", corrupt(&data))]);
    let tmp = tempfile::tempdir().unwrap();
    let dir = ModelDir::new(tmp.path().join("models"));
    let spec = spec_for(&data, vec![srv.url("/a"), srv.url("/b")]);
    let err = run(&dir, &spec).unwrap_err();
    assert_eq!(
        err.to_string(),
        "The model could not be downloaded: the downloaded file did not match its checksum."
    );
    assert!(!dir.final_path(&spec).exists());
    assert!(!dir.partial_path(&spec).exists());
    let paths: Vec<String> = srv.requests().into_iter().map(|r| r.path).collect();
    assert_eq!(paths, ["/a", "/b"], "each URL once");
}

#[test]
fn an_http_error_falls_back_to_the_other_url() {
    let data = blob();
    let srv = server(&[
        (
            "/gone",
            Route {
                status: Some(404),
                ..Route::default()
            },
        ),
        ("/good", good(&data)),
    ]);
    let tmp = tempfile::tempdir().unwrap();
    let dir = ModelDir::new(tmp.path().join("models"));
    let spec = spec_for(&data, vec![srv.url("/gone"), srv.url("/good")]);
    run(&dir, &spec).unwrap();
    assert!(dir.is_installed(&spec));
    let only_gone = spec_for(&data, vec![srv.url("/gone")]);
    let tmp2 = tempfile::tempdir().unwrap();
    assert_eq!(
        run(&ModelDir::new(tmp2.path()), &only_gone),
        Err(DownloadError::Failed("the server answered HTTP 404".into()))
    );
}

#[test]
fn the_size_cap_is_the_compiled_size() {
    let data = blob();
    // The server claims more bytes than compiled: refused before anything is written.
    let srv = server(&[
        (
            "/long",
            Route {
                claimed_length: Some(data.len() as u64 + 1),
                ..good(&data)
            },
        ),
        ("/bigger", good(&[data.as_slice(), b"extra"].concat())),
    ]);
    let tmp = tempfile::tempdir().unwrap();
    let dir = ModelDir::new(tmp.path().join("models"));
    let spec = spec_for(&data, vec![srv.url("/long")]);
    assert_eq!(
        run(&dir, &spec),
        Err(DownloadError::Failed(
            "the server sent a file of the wrong size".into()
        ))
    );
    assert_eq!(dir.partial_bytes(&spec), 0);
    let spec = spec_for(&data, vec![srv.url("/bigger")]);
    assert!(run(&dir, &spec).is_err());
    assert!(!dir.final_path(&spec).exists());
    // A partial larger than the compiled size is discarded, never trusted.
    let fine = server(&[("/m", good(&data))]);
    let spec = spec_for(&data, vec![fine.url("/m")]);
    fs::write(dir.partial_path(&spec), vec![0u8; data.len() + 10]).unwrap();
    run(&dir, &spec).unwrap();
    assert_eq!(fs::read(dir.final_path(&spec)).unwrap(), data);
    assert_eq!(fine.requests()[0].header("range"), None);
}

#[test]
fn pause_keeps_the_partial_and_resume_completes_it() {
    let data = blob();
    let srv = server(&[("/m", good(&data))]);
    let tmp = tempfile::tempdir().unwrap();
    let dir = ModelDir::new(tmp.path().join("models"));
    let spec = spec_for(&data, vec![srv.url("/m")]);
    let control = Control::new();
    let pauser = control.clone();
    let out = download(
        &dir,
        &spec,
        &HttpTransport::loopback_for_tests(),
        &control,
        &mut |_| pauser.pause(),
    )
    .unwrap();
    let Outcome::Paused { have } = out else {
        panic!("{out:?}")
    };
    assert_eq!(have, dir.partial_bytes(&spec));
    assert!(have > 0 && have < spec.bytes);
    run(&dir, &spec).unwrap();
    assert_eq!(fs::read(dir.final_path(&spec)).unwrap(), data);
    assert_eq!(
        srv.requests()[1].header("range"),
        Some(format!("bytes={have}-").as_str())
    );
}

#[test]
fn cancel_deletes_the_partial() {
    let data = blob();
    let srv = server(&[("/m", good(&data))]);
    let tmp = tempfile::tempdir().unwrap();
    let dir = ModelDir::new(tmp.path().join("models"));
    let spec = spec_for(&data, vec![srv.url("/m")]);
    let control = Control::new();
    let canceller = control.clone();
    let out = download(
        &dir,
        &spec,
        &HttpTransport::loopback_for_tests(),
        &control,
        &mut |_| canceller.cancel(),
    )
    .unwrap();
    assert_eq!(out, Outcome::Cancelled);
    assert!(!dir.partial_path(&spec).exists());
    assert!(!dir.final_path(&spec).exists());
    // A cancelled control stops before any request.
    let before = srv.requests().len();
    assert_eq!(
        download(
            &dir,
            &spec,
            &HttpTransport::loopback_for_tests(),
            &control,
            &mut |_| {}
        ),
        Ok(Outcome::Cancelled)
    );
    assert_eq!(srv.requests().len(), before);
}

#[test]
fn an_installed_file_is_verified_without_a_request() {
    let data = blob();
    let srv = server(&[("/m", good(&data))]);
    let tmp = tempfile::tempdir().unwrap();
    let dir = ModelDir::new(tmp.path().join("models"));
    let spec = spec_for(&data, vec![srv.url("/m")]);
    run(&dir, &spec).unwrap();
    run(&dir, &spec).unwrap();
    assert_eq!(srv.requests().len(), 1);
}

#[test]
fn a_failed_connection_reads_as_plain_words_without_the_address() {
    let data = blob();
    let tmp = tempfile::tempdir().unwrap();
    let dir = ModelDir::new(tmp.path().join("models"));
    // Port 1 on this machine: refused at once.
    let spec = spec_for(&data, vec!["http://127.0.0.1:1/m".into()]);
    let real = HttpTransport::loopback_for_tests();
    let err = download(&dir, &spec, &real, &Control::new(), &mut |_| {}).unwrap_err();
    assert_eq!(
        err.to_string(),
        "The model could not be downloaded: could not connect; check your connection."
    );
}

#[test]
fn the_real_transport_refuses_plain_http_without_connecting() {
    let data = blob();
    let srv = server(&[("/m", good(&data))]);
    let tmp = tempfile::tempdir().unwrap();
    let dir = ModelDir::new(tmp.path().join("models"));
    let spec = spec_for(&data, vec![srv.url("/m")]);
    let real = HttpTransport::new(reqwest::blocking::Client::builder()).unwrap();
    let err = download(&dir, &spec, &real, &Control::new(), &mut |_| {}).unwrap_err();
    assert_eq!(
        err,
        DownloadError::Failed("only HTTPS addresses are allowed".into())
    );
    assert!(srv.requests().is_empty());
}

#[test]
fn unsafe_names_and_hashes_are_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = ModelDir::new(tmp.path());
    let data = blob();
    for name in ["../x.gguf", "a/b.gguf", ".hidden", "", "a\\b", "x y"] {
        let spec = DownloadSpec {
            file_name: name.into(),
            ..spec_for(&data, vec!["https://example.invalid/x".into()])
        };
        assert_eq!(run(&dir, &spec), Err(DownloadError::Invalid), "{name:?}");
    }
    let spec = DownloadSpec {
        sha256: "../../etc".into(),
        ..spec_for(&data, vec!["https://example.invalid/x".into()])
    };
    assert_eq!(run(&dir, &spec), Err(DownloadError::Invalid));
}

#[cfg(unix)]
#[test]
fn a_symlinked_partial_is_replaced_not_followed() {
    let data = blob();
    let srv = server(&[("/m", good(&data))]);
    let tmp = tempfile::tempdir().unwrap();
    let dir = ModelDir::new(tmp.path().join("models"));
    let spec = spec_for(&data, vec![srv.url("/m")]);
    let outside = tmp.path().join("outside.txt");
    fs::write(&outside, b"keep me").unwrap();
    fs::create_dir_all(dir.root().join(PARTIAL_DIR)).unwrap();
    std::os::unix::fs::symlink(&outside, dir.partial_path(&spec)).unwrap();
    run(&dir, &spec).unwrap();
    assert_eq!(fs::read(&outside).unwrap(), b"keep me");
    assert_eq!(fs::read(dir.final_path(&spec)).unwrap(), data);
}

#[test]
fn delete_removes_only_the_model_files() {
    let data = blob();
    let srv = server(&[("/m", good(&data))]);
    let tmp = tempfile::tempdir().unwrap();
    let dir = ModelDir::new(tmp.path().join("models"));
    let spec = spec_for(&data, vec![srv.url("/m")]);
    run(&dir, &spec).unwrap();
    let other = DownloadSpec {
        file_name: "embeddinggemma-2-OTHER.gguf".into(),
        sha256: "0".repeat(64),
        ..spec.clone()
    };
    fs::write(dir.partial_path(&other), b"partial").unwrap();
    fs::write(dir.root().join("user-note.txt"), b"not ours").unwrap();
    let removed = dir.delete(&[spec.clone(), other.clone()]).unwrap();
    assert_eq!(removed, 4, "model, other partial, LICENSE, NOTICE");
    assert!(!dir.final_path(&spec).exists());
    assert!(!dir.root().join(PARTIAL_DIR).exists());
    assert!(dir.root().join("user-note.txt").exists());
    assert_eq!(dir.delete(&[spec]).unwrap(), 0, "idempotent");
}
