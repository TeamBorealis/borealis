//! Helpers shared by the integration tests.
#![allow(dead_code)]

use borealis_content::{ContentStore, ContentStoreBuilder, Element, Issue};
use borealis_import_5etools::{import_folder, ImportOptions, ImportResult};
use std::path::{Path, PathBuf};

/// Directory with the SRD-only fixture data folder.
pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data")
}

/// The core pack of the repository.
pub fn core_pack() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/core")
}

/// Import the fixtures with default options.
pub fn import_fixtures() -> ImportResult {
    import_folder(&fixtures(), &ImportOptions::default()).expect("fixtures import")
}

/// Write all packs of a result below `dir` and return the pack directories.
pub fn write_packs(result: &ImportResult, dir: &Path) -> Vec<PathBuf> {
    result
        .packs
        .iter()
        .map(|p| p.write_to(dir).expect("write pack"))
        .collect()
}

/// Core pack plus the given pack directories, loaded into a store.
pub fn store_with(dirs: &[PathBuf]) -> ContentStore {
    let mut builder = ContentStoreBuilder::new()
        .add_pack_dir(&core_pack())
        .expect("core pack");
    for d in dirs {
        builder = builder.add_pack_dir(d).expect("imported pack");
    }
    builder.build()
}

/// Find an element of a result by id.
pub fn find<'a>(result: &'a ImportResult, id: &str) -> Option<&'a Element> {
    result
        .packs
        .iter()
        .flat_map(|p| p.elements())
        .find(|e| e.id == id)
}

/// Compact text of issues for assertion messages.
pub fn show(issues: &[&Issue]) -> String {
    issues
        .iter()
        .map(|i| {
            format!(
                "{:?}/{:?} {} [{}]",
                i.severity,
                i.code,
                i.message,
                i.element.as_deref().unwrap_or("-")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
