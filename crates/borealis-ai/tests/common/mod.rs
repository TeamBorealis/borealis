#![allow(dead_code)]
//! Shared helpers for integration tests (fake CLI wiring).

use std::path::PathBuf;

use borealis_ai::provider::DetectionSource;
use borealis_ai::{AiSession, Backend, ProviderInfo, SessionOptions};

pub fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

pub fn fake_provider(backend: Backend) -> ProviderInfo {
    ProviderInfo {
        backend,
        path: fixture("fake-cli.sh"),
        version: None,
        source: DetectionSource::Override,
    }
}

/// A session whose env selects the fake CLI behaviour.
pub fn fake_session(
    backend: Backend,
    mode: &str,
    stream: Option<&str>,
    extra: &[(&str, String)],
) -> AiSession {
    let mut options = SessionOptions::default();
    options.env.push(("FAKE_MODE".into(), mode.into()));
    if let Some(s) = stream {
        options.env.push((
            "FAKE_STREAM".into(),
            fixture(s).to_string_lossy().into_owned(),
        ));
    }
    for (k, v) in extra {
        options.env.push(((*k).into(), v.clone()));
    }
    AiSession::new(fake_provider(backend), options)
}
