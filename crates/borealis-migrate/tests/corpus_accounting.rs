//! Local-only IM4 stage-1 accounting for the converter, enabled by `BOREALIS_CORPUS_DIR` (a folder of
//! content repositories; never committed). Per repository, every source `<element id>` (counted by an
//! independent scan, later id wins, comments skipped) must be converted, converted under a new id with
//! the source id kept as an alias (`renamed`), or named by a warning/error. Prints counts and the ids
//! of anything lost; content text is never printed.

use borealis_content::Severity;
use borealis_migrate::{migrate_path, MigrateOptions};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn xml_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() || entry.file_name() == ".git" {
            continue;
        }
        if kind.is_dir() {
            xml_files(&path, out);
        } else if path.to_string_lossy().to_lowercase().ends_with(".xml") {
            out.push(path);
        }
    }
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        rest = rest[start..]
            .find("-->")
            .map_or("", |end| &rest[start + end + 3..]);
    }
    out.push_str(rest);
    out
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let at = tag.find(&format!(" {name}=")).or_else(|| {
        tag.find(&format!("\t{name}="))
            .or_else(|| tag.find(&format!("\n{name}=")))
    })?;
    let rest = &tag[at + name.len() + 2..];
    let quote = rest.chars().next()?;
    let body = &rest[1..];
    Some(
        body[..body.find(quote)?]
            .trim()
            .replace("&apos;", "'")
            .replace("&quot;", "\"")
            .replace("&amp;", "&"),
    )
}

/// Distinct source ids (with type) of one repository.
fn source_ids(repo: &Path) -> BTreeMap<String, String> {
    let mut files = Vec::new();
    xml_files(repo, &mut files);
    let mut ids = BTreeMap::new();
    for f in files {
        let Ok(bytes) = std::fs::read(&f) else {
            continue;
        };
        let text = strip_comments(&String::from_utf8_lossy(&bytes));
        for piece in text.split("<element").skip(1) {
            if !piece.starts_with(char::is_whitespace) {
                continue;
            }
            let tag = &piece[..piece.find('>').unwrap_or(piece.len())];
            if let (Some(id), Some(ty)) = (attr(tag, "id"), attr(tag, "type")) {
                ids.insert(id, ty);
            }
        }
    }
    ids
}

#[test]
fn every_source_element_converts_or_is_reported() {
    let Ok(root) = std::env::var("BOREALIS_CORPUS_DIR") else {
        eprintln!("BOREALIS_CORPUS_DIR not set; skipping");
        return;
    };
    let root = PathBuf::from(root);
    let root = if root.join("repos").is_dir() {
        root.join("repos")
    } else {
        root
    };
    let mut repos: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    repos.sort();
    let (mut s, mut imported, mut renamed, mut warned, mut lost) = (0, 0, 0, 0, Vec::new());
    let mut imprecise = 0;
    let mut file_level: BTreeMap<String, usize> = BTreeMap::new();
    for repo in &repos {
        let ids = source_ids(repo);
        if ids.is_empty() {
            continue;
        }
        let out = migrate_path(repo, &MigrateOptions::default()).expect("migrate");
        let converted: BTreeSet<String> = out
            .files
            .iter()
            .filter(|f| f.path != "pack.json" && !f.path.ends_with(".index.json"))
            .filter_map(|f| serde_json::from_str::<serde_json::Value>(&f.contents).ok())
            .flat_map(|v| {
                v["elements"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| e["id"].as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .collect();
        let mut named = BTreeSet::new();
        for i in &out.report.issues {
            if i.severity == Severity::Info {
                continue;
            }
            match &i.element {
                Some(e) => {
                    named.insert(e.clone());
                }
                None if i.file.is_none() => imprecise += 1,
                None => {
                    *file_level
                        .entry(format!("{:?} {:?} {}", i.severity, i.code, i.message.chars().take(70).collect::<String>()))
                        .or_insert(0) += 1
                }
            }
        }
        for (id, ty) in &ids {
            s += 1;
            if named.contains(id) {
                warned += 1;
            } else if converted.contains(id) {
                imported += 1;
            } else if out
                .report
                .renamed
                .get(id)
                .is_some_and(|new| converted.contains(new))
            {
                renamed += 1;
            } else {
                lost.push(format!("{ty} {id}"));
            }
        }
    }
    eprintln!(
        "corpus accounting: S={s} I={imported} renamed={renamed} W={warned} U={} (issues without file or element: {imprecise})",
        lost.len()
    );
    eprintln!("file-level warnings/errors (no element in the file's scope): {file_level:?}");
    for l in lost.iter().take(40) {
        eprintln!("  lost: {l}");
    }
    assert!(
        lost.is_empty(),
        "{} source elements lost silently",
        lost.len()
    );
}
