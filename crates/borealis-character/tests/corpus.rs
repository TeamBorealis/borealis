//! Coverage sweep over real Aurora `.dnd5e` files. Gated on `BOREALIS_CORPUS_DIR` (a local,
//! never-committed corpus with an `index.json`); it skips silently when the variable is unset.
//! `BOREALIS_COMMUNITY_DIR` may point at an extra folder of legacy element XML files to load.
//! Only counts and element ids are printed - never character names or file contents.

mod common;

use borealis_character::{import_dnd5e, recheck, Character};
use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin};
use borealis_engine::Engine;
use borealis_migrate::{migrate_path, MigrateOptions};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn add_tree(
    mut b: ContentStoreBuilder,
    dir: &Path,
    origin: PackOrigin,
    tag: &str,
) -> ContentStoreBuilder {
    let opts = MigrateOptions {
        origin,
        validate: false,
        ..MigrateOptions::default()
    };
    let Ok(out) = migrate_path(dir, &opts) else {
        return b;
    };
    let mut files: Vec<_> = out
        .files
        .iter()
        .filter(|f| f.path.ends_with(".json") && !f.path.ends_with("pack.json"))
        .collect();
    files.sort_by_key(|f| f.path.clone());
    for f in files {
        if let Ok(cf) = ContentFile::from_json_str(&f.contents) {
            b = b.add_file(format!("{tag}/{}", f.path), origin, cf);
        }
    }
    b
}

fn characters(corpus: &Path) -> Vec<PathBuf> {
    let text = std::fs::read_to_string(corpus.join("index.json")).expect("index.json");
    let rows: Vec<serde_json::Value> = serde_json::from_str(&text).expect("index json");
    let mut out: Vec<PathBuf> = rows
        .iter()
        .filter(|r| r["kind"] == "character")
        .filter_map(|r| r["path"].as_str())
        .map(|p| corpus.join(p))
        .collect();
    out.sort();
    out
}

#[test]
fn imports_every_corpus_character_and_rechecks_cleanly() {
    let Ok(dir) = std::env::var("BOREALIS_CORPUS_DIR") else {
        eprintln!("SKIPPED: BOREALIS_CORPUS_DIR is not set");
        return;
    };
    let corpus = PathBuf::from(dir);
    let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/core");
    let mut b = ContentStoreBuilder::new()
        .add_pack_dir(&core)
        .expect("core");
    if let Some(fx) = common::fixtures_dir() {
        b = add_tree(b, &fx, PackOrigin::Core, "fixtures");
    }
    if let Ok(community) = std::env::var("BOREALIS_COMMUNITY_DIR") {
        b = add_tree(b, Path::new(&community), PackOrigin::Custom, "community");
    }
    let engine = Engine::new(&b.build());

    let files = characters(&corpus);
    assert!(!files.is_empty(), "the corpus has no character files");
    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    let (mut clean, mut with_gaps) = (0usize, 0usize);
    for path in &files {
        let xml = String::from_utf8_lossy(&std::fs::read(path).expect("read")).into_owned();
        let xml = xml.trim_start_matches('\u{feff}');
        let out = import_dnd5e(&engine, xml).unwrap_or_else(|e| panic!("import failed: {e}"));
        let mut ch = out.character;
        let r = &out.report;
        for i in &r.invalid_selections {
            *by_kind.entry(format!("invalid {i}")).or_default() += 1;
        }
        if std::env::var("BOREALIS_CORPUS_VERBOSE").is_ok() {
            for m in &r.missing_content {
                eprintln!("MISSING {} {} {}", m.ty, m.context, m.id);
            }
            for u in &r.unmapped_slots {
                eprintln!("UNMAPPED {} {} {} <- {}", u.owner, u.ty, u.name, u.chosen);
            }
        }
        for m in &r.missing_content {
            *by_kind
                .entry(format!("missing {}|{}", m.ty, m.context))
                .or_default() += 1;
        }
        for u in &r.unmapped_slots {
            *by_kind
                .entry(format!("unmapped {}|{}", u.ty, u.reason))
                .or_default() += 1;
        }
        if r.missing_content.is_empty() && r.unmapped_slots.is_empty() {
            clean += 1;
        } else {
            with_gaps += 1;
        }
        // Save/load round trip and an idempotent re-check (nothing new is installed).
        let json = ch.to_json_string().expect("serialise");
        let back = Character::from_json_str(&json).expect("reload");
        assert_eq!(back, ch, "round trip changed the character");
        let first = recheck(&engine, &mut ch);
        assert_eq!(
            first.resolved, 0,
            "a re-check without new content changed something"
        );
        assert_eq!(first.still_unmapped.len(), r.unmapped_slots.len());
    }
    eprintln!("corpus characters: {} clean, {with_gaps} with gaps", clean);
    for (k, n) in &by_kind {
        eprintln!("  {n:4}  {k}");
    }
}
