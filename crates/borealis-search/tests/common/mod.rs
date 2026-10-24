//! Shared helpers: synthetic chunks and the bundled SRD corpora. Only SRD content and invented fixtures (C4).
#![allow(dead_code)]

use borealis_content::{ContentStore, ContentStoreBuilder};
use borealis_search::{
    corpus, Chunk, Corpus, EditionTag, PackInput, SearchConfig, SearchEngine, SearchRequest,
    Tweaks, WriterBudget,
};
use std::path::{Path, PathBuf};

pub const SRD_ELEMENT_PACKS: &[&str] = &[
    "srd-5.1",
    "srd-5.1-monsters",
    "srd-5.2.1",
    "srd-5.2.1-monsters",
];
pub const RULES_TEXT_PACKS: &[&str] = &["srd-5.1-rules-text", "srd-5.2.1-rules-text"];

pub fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

pub fn chunk(key: &str, name: &str, headings: &str, body: &str, edition: EditionTag) -> Chunk {
    Chunk {
        doc_key: key.to_string(),
        name: name.to_string(),
        headings: headings.to_string(),
        body: body.to_string(),
        ty: "Test".to_string(),
        edition,
        pack: "fixture".to_string(),
        source: "Fixture".to_string(),
        campaign: None,
        is_primary: true,
    }
}

/// One synthetic pack (chunks are re-tagged with the pack id).
pub fn pack(id: &str, mut chunks: Vec<Chunk>) -> PackInput {
    for c in &mut chunks {
        c.pack = id.to_string();
    }
    PackInput::new(id, chunks)
}

/// An engine over one synthetic Lore pack.
pub fn lore_engine(dir: &Path, chunks: Vec<Chunk>) -> SearchEngine {
    let (mut engine, _) = SearchEngine::open(dir, WriterBudget::DESKTOP).unwrap();
    engine
        .sync(Corpus::Lore, &[pack("fixture", chunks)])
        .unwrap();
    engine
}

/// Titles of the top hits of the Lore corpus.
pub fn titles(engine: &SearchEngine, query: &str, tweaks: Tweaks) -> Vec<String> {
    titles_req(engine, &SearchRequest::new(query), tweaks)
}

pub fn titles_req(engine: &SearchEngine, req: &SearchRequest, tweaks: Tweaks) -> Vec<String> {
    let cfg = SearchConfig {
        tweaks,
        ..SearchConfig::default()
    };
    engine
        .search(Corpus::Lore, req, &cfg)
        .unwrap()
        .into_iter()
        .map(|h| h.title)
        .collect()
}

/// Only `f` applied on top of no tweaks.
pub fn only(f: impl FnOnce(&mut Tweaks)) -> Tweaks {
    let mut t = Tweaks::none();
    f(&mut t);
    t
}

pub fn srd_store() -> ContentStore {
    let mut builder = ContentStoreBuilder::new();
    for id in SRD_ELEMENT_PACKS {
        builder = builder
            .add_pack_dir(&content_dir().join(id))
            .unwrap_or_else(|e| panic!("{id}: {e}"));
    }
    builder.build()
}

pub fn rules_text_json() -> Vec<String> {
    RULES_TEXT_PACKS
        .iter()
        .map(|id| std::fs::read_to_string(content_dir().join(id).join("docs.json")).unwrap())
        .collect()
}

pub fn srd_inputs(store: &ContentStore) -> (Vec<PackInput>, Vec<PackInput>) {
    let docs = rules_text_json();
    let refs: Vec<&str> = docs.iter().map(String::as_str).collect();
    (
        corpus::rules_packs(&refs, store).unwrap(),
        corpus::lore_packs(store),
    )
}

pub fn srd_engine(dir: &Path) -> SearchEngine {
    let store = srd_store();
    let (rules, lore) = srd_inputs(&store);
    let (mut engine, _) = SearchEngine::open(dir, WriterBudget::DESKTOP).unwrap();
    engine.sync(Corpus::Rules, &rules).unwrap();
    engine.sync(Corpus::Lore, &lore).unwrap();
    engine
}
