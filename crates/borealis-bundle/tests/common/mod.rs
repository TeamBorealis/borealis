//! Shared fixtures: synthetic characters and packs (no real content).
#![allow(dead_code)]

use borealis_bundle::{BundleMeta, FileEntry, Item, ItemKind, Manifest, FORMAT};
use std::io::{Cursor, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

pub fn meta() -> BundleMeta {
    BundleMeta {
        app_version: "1.2.3".into(),
        source_device: "Test PC".into(),
        created_ms: 1_700_000_000_000,
    }
}

pub fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

pub fn character_json(name: &str, element: &str) -> Vec<u8> {
    format!(
        r#"{{"format_version":2,"meta":{{"id":"c_test","name":"{name}"}},"build":{{"options":["{element}"]}}}}"#
    )
    .into_bytes()
}

pub fn pack_json(element: &str) -> Vec<u8> {
    format!(r#"{{"format":"x","elements":[{{"id":"{element}","name":"Thing"}}]}}"#).into_bytes()
}

/// A raw zip from `(name, bytes)` pairs, written exactly as given.
pub fn raw_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut z = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        z.start_file(*name, SimpleFileOptions::default()).unwrap();
        z.write_all(bytes).unwrap();
    }
    z.finish().unwrap().into_inner()
}

pub fn file_entry(path: &str, bytes: &[u8]) -> FileEntry {
    FileEntry {
        path: path.into(),
        sha256: borealis_bundle::sha256_hex(bytes),
        size: bytes.len() as u64,
    }
}

pub fn character_item(key: &str, bytes: &[u8], edited: u64) -> Item {
    let f = file_entry(&format!("characters/{key}.json"), bytes);
    Item {
        kind: ItemKind::Character,
        key: key.into(),
        sha256: f.sha256.clone(),
        size: f.size,
        edited_at_ms: edited,
        files: vec![f],
        required_packs: Vec::new(),
    }
}

pub fn manifest_of(items: Vec<Item>) -> Manifest {
    Manifest {
        format: FORMAT.into(),
        app_version: "1".into(),
        created_ms: 1,
        source_device: "d".into(),
        items,
        settings: None,
        campaigns: Vec::new(),
    }
}

pub fn manifest_bytes(m: &Manifest) -> Vec<u8> {
    serde_json::to_vec(m).unwrap()
}
