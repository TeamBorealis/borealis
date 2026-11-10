//! Rich text round trip over every description in the bundled packs (homebrew creator spec 7.3, 18.2).
//!
//! For each element description (the multiclass, extract, equipment and creature `data.description` texts too, and
//! the HTML sections of the rules-text packs):
//! * `render(parse(html)) == html`, byte for byte;
//! * every editable block of an element text is canonical: no palette block had to be kept for being written
//!   differently, so kept blocks are exactly the parts outside the palette (`h5`, `h6`, `pre`, `class="..."`);
//! * the tree is a fixed point of `parse . render`, of normalization and of its JSON form;
//! * no text looks like an escape artefact.
//!
//! The numbers are printed (`--nocapture`) and written to `target/rich_text_corpus.txt`.

use borealis_content::rich::{lint, parse, render, Block, Doc, KeptReason};
use borealis_content::{ContentFile, PackFileKind, PackManifest};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

const RULES_TEXT: &str = "rules-text html";

fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

/// (pack, element id, field, html)
fn descriptions() -> Vec<(String, String, &'static str, String)> {
    let mut out = Vec::new();
    let mut dirs: Vec<PathBuf> = fs::read_dir(content_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    dirs.sort();
    for docs in dirs
        .iter()
        .map(|d| d.join("docs.json"))
        .filter(|p| p.exists())
    {
        let name = docs
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&docs).unwrap()).unwrap();
        for section in value["sections"].as_array().unwrap() {
            let id = section["id"].as_str().unwrap_or_default().to_owned();
            out.push((
                name.clone(),
                id,
                RULES_TEXT,
                section["html"].as_str().unwrap().to_owned(),
            ));
        }
    }
    for pack in dirs.iter().filter(|p| p.join("pack.json").exists()) {
        let name = pack.file_name().unwrap().to_string_lossy().into_owned();
        let manifest =
            PackManifest::from_json_str(&fs::read_to_string(pack.join("pack.json")).unwrap())
                .unwrap();
        for file in manifest
            .files
            .iter()
            .filter(|f| f.kind == PackFileKind::Content)
        {
            let text = fs::read_to_string(pack.join(&file.path)).unwrap();
            let content =
                ContentFile::from_json_str(&text).unwrap_or_else(|e| panic!("{}: {e}", file.path));
            for e in &content.elements {
                let mut add = |field: &'static str, html: &str| {
                    out.push((name.clone(), e.id.clone(), field, html.to_owned()));
                };
                add("description", &e.description);
                if let Some(m) = &e.multiclass {
                    add("multiclass.description", &m.description);
                }
                if let Some(d) = e.extract.as_ref().and_then(|x| x.description.as_deref()) {
                    add("extract.description", d);
                }
                if let Some(d) = e.equipment.as_ref().and_then(|x| x.description.as_deref()) {
                    add("equipment.description", d);
                }
                if let Some(d) = e.data.get("description").and_then(|v| v.as_str()) {
                    add("data.description", d);
                }
            }
        }
    }
    out
}

#[derive(Default)]
struct Tally {
    texts: usize,
    non_empty: usize,
    fully_editable: usize,
    with_kept: usize,
    kept_blocks: usize,
    blocks: usize,
}

#[test]
fn every_bundled_description_round_trips() {
    let all = descriptions();
    assert!(
        all.len() > 3000,
        "expected the bundled packs, found {} texts",
        all.len()
    );
    let mut per_pack: BTreeMap<String, Tally> = BTreeMap::new();
    let mut kept_tags: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures = Vec::new();
    for (pack, id, field, html) in &all {
        let doc = parse(html);
        let here = format!("{pack} {id} {field}");
        if render(&doc) != *html {
            failures.push(format!("{here}: render(parse) differs"));
            continue;
        }
        if parse(&render(&doc)) != doc {
            failures.push(format!("{here}: parse(render(doc)) differs"));
        }
        if doc.normalized() != doc {
            failures.push(format!("{here}: normalization changes the parsed tree"));
        }
        let json = serde_json::to_value(&doc).unwrap();
        if serde_json::from_value::<Doc>(json).unwrap() != doc {
            failures.push(format!("{here}: JSON round trip differs"));
        }
        for f in lint(&doc) {
            failures.push(format!("{here}: escape lint {:?} {:?}", f.kind, f.found));
        }
        let t = per_pack.entry(pack.clone()).or_default();
        t.texts += 1;
        t.non_empty += usize::from(!html.is_empty());
        t.blocks += doc.content.len();
        let kept: Vec<_> = doc
            .content
            .iter()
            .filter_map(|b| match b {
                Block::Kept { attrs } => Some(attrs),
                _ => None,
            })
            .collect();
        if kept.is_empty() {
            t.fully_editable += usize::from(!html.is_empty());
        } else {
            t.with_kept += 1;
            t.kept_blocks += kept.len();
        }
        for k in kept {
            // Rules-text sections are not element descriptions (the creator never edits them) and nest italic
            // outside bold (`<i><b>`), which the writer does not produce: they keep those paragraphs as written.
            if k.reason == KeptReason::NotCanonical {
                if *field == RULES_TEXT {
                    *kept_tags
                        .entry("rules-text: palette, not canonical".into())
                        .or_default() += 1;
                    continue;
                }
                failures.push(format!(
                    "{here}: palette block kept as not canonical: {:.120}",
                    k.html
                ));
            }
            let tag: String = k
                .html
                .trim_start_matches('<')
                .chars()
                .take_while(char::is_ascii_alphanumeric)
                .collect();
            let attrs = k.html.split('>').next().is_some_and(|t| t.contains(' '));
            *kept_tags
                .entry(format!("{tag}{}", if attrs { " (attributes)" } else { "" }))
                .or_default() += 1;
        }
    }
    let mut report = String::new();
    writeln!(
        report,
        "pack | texts | non-empty | fully editable | with kept blocks | kept blocks | blocks"
    )
    .unwrap();
    let mut total = Tally::default();
    for (pack, t) in &per_pack {
        writeln!(
            report,
            "{pack} | {} | {} | {} | {} | {} | {}",
            t.texts, t.non_empty, t.fully_editable, t.with_kept, t.kept_blocks, t.blocks
        )
        .unwrap();
        total.texts += t.texts;
        total.non_empty += t.non_empty;
        total.fully_editable += t.fully_editable;
        total.with_kept += t.with_kept;
        total.kept_blocks += t.kept_blocks;
        total.blocks += t.blocks;
    }
    writeln!(
        report,
        "total | {} | {} | {} | {} | {} | {}",
        total.texts,
        total.non_empty,
        total.fully_editable,
        total.with_kept,
        total.kept_blocks,
        total.blocks
    )
    .unwrap();
    writeln!(report, "kept blocks by first tag: {kept_tags:?}").unwrap();
    eprintln!("{report}");
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/rich_text_corpus.txt");
    let _ = fs::write(out, &report);
    assert!(
        failures.is_empty(),
        "{} problems:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(total.texts, all.len());
}
