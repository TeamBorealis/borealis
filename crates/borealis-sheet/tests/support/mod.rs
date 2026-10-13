#![allow(dead_code)]
//! Golden-file helpers shared by the snapshot tests: compare, unified-ish diff, update switch.

use lopdf::{Dictionary, Document, Object};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const UPDATE_VAR: &str = "BOREALIS_UPDATE_GOLDEN";

pub fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

pub fn updating() -> bool {
    std::env::var(UPDATE_VAR).is_ok_and(|v| !v.is_empty() && v != "0")
}

/// Line diff (LCS) in unified style, context-free: `-` expected, `+` actual.
pub fn diff(expected: &str, actual: &str) -> String {
    let a: Vec<&str> = expected.lines().collect();
    let b: Vec<&str> = actual.lines().collect();
    let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, String::new());
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            i += 1;
            j += 1;
        } else if j >= b.len() || (i < a.len() && lcs[i + 1][j] >= lcs[i][j + 1]) {
            out.push_str(&format!("-{:>4} {}\n", i + 1, a[i]));
            i += 1;
        } else {
            out.push_str(&format!("+{:>4} {}\n", j + 1, b[j]));
            j += 1;
        }
    }
    out
}

/// Compare `actual` to `tests/golden/<name>`; with the update variable set, rewrite it instead.
/// Returns an error message (with diff) on mismatch.
pub fn check_text(name: &str, actual: &str) -> Result<(), String> {
    let path = golden_dir().join(name);
    if updating() {
        std::fs::create_dir_all(golden_dir()).expect("golden dir");
        std::fs::write(&path, actual).expect("write golden");
        return Ok(());
    }
    let Ok(expected) = std::fs::read_to_string(&path) else {
        return Err(format!(
            "{name}: golden missing; run with {UPDATE_VAR}=1 to create it"
        ));
    };
    if expected == actual {
        return Ok(());
    }
    Err(format!(
        "{name}: differs from the committed golden (--- golden, +++ actual)\n{}\nIf the change is intended, run: {UPDATE_VAR}=1 cargo test -p borealis-sheet --test golden_structure --test golden_raster",
        diff(&expected, actual)
    ))
}

pub fn widget_name(doc: &Document, d: &Dictionary) -> String {
    let partial = |d: &Dictionary| match d.get(b"T") {
        Ok(Object::String(t, _)) => String::from_utf8_lossy(t).into_owned(),
        _ => String::new(),
    };
    let mut parts = vec![partial(d)];
    let mut cur = d.clone();
    while let Ok(Object::Reference(r)) = cur.get(b"Parent") {
        let Ok(Object::Dictionary(pd)) = doc.get_object(*r) else {
            break;
        };
        parts.push(partial(pd));
        cur = pd.clone();
    }
    parts.reverse();
    parts.join(".")
}

pub fn page_widgets(doc: &Document, page: lopdf::ObjectId) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Ok(Object::Dictionary(p)) = doc.get_object(page) else {
        return out;
    };
    let Ok(annots) = p.get(b"Annots") else {
        return out;
    };
    let annots = match annots {
        Object::Reference(r) => doc.get_object(*r).cloned().unwrap_or(Object::Null),
        o => o.clone(),
    };
    let Object::Array(items) = annots else {
        return out;
    };
    for it in items {
        let Object::Reference(r) = it else { continue };
        if let Ok(Object::Dictionary(d)) = doc.get_object(r) {
            if d.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Widget") {
                out.insert(widget_name(doc, d));
            }
        }
    }
    out
}
