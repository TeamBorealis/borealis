//! Development probe over the bundled SRD packs.
//!
//! * `cargo run --release -p borealis-search --example probe` prints corpus sizes, index build time and index size.
//! * `cargo run -p borealis-search --example probe -- "Fireball" "opportunity attack"` prints the top hits.

#[path = "../tests/common/mod.rs"]
mod common;

use borealis_search::{Corpus, SearchConfig, SearchEngine, SearchRequest, WriterBudget};
use std::time::Instant;

fn stats() {
    let started = Instant::now();
    let store = common::srd_store();
    println!(
        "content store: {} elements, {:?}",
        store.len(),
        started.elapsed()
    );
    let started = Instant::now();
    let (rules, lore) = common::srd_inputs(&store);
    println!("chunking: {:?}", started.elapsed());
    for (name, packs) in [("rules", &rules), ("lore", &lore)] {
        let chunks: usize = packs.iter().map(|p| p.chunks.len()).sum();
        let words: usize = packs
            .iter()
            .flat_map(|p| &p.chunks)
            .map(|c| c.body.split_whitespace().count())
            .sum();
        println!("{name}: {chunks} chunks, {words} body words");
        for p in packs.iter() {
            println!("   {:<26} {:>6} chunks", p.pack, p.chunks.len());
        }
    }
    let dir = tempfile::tempdir().expect("temp dir");
    for (label, budget) in [
        ("desktop 2t/64MB", WriterBudget::DESKTOP),
        ("mobile 1t/20MB", WriterBudget::MOBILE),
    ] {
        let path = dir.path().join(label.split(' ').next().unwrap());
        let (mut engine, _) = SearchEngine::open(&path, budget).expect("open");
        let started = Instant::now();
        engine.sync(Corpus::Rules, &rules).expect("sync rules");
        let t_rules = started.elapsed();
        let started = Instant::now();
        engine.sync(Corpus::Lore, &lore).expect("sync lore");
        let t_lore = started.elapsed();
        println!(
            "{label}: rules {t_rules:?} ({} B on disk, {} docs), lore {t_lore:?} ({} B, {} docs)",
            engine.corpus_index(Corpus::Rules).disk_bytes(),
            engine.corpus_index(Corpus::Rules).num_docs(),
            engine.corpus_index(Corpus::Lore).disk_bytes(),
            engine.corpus_index(Corpus::Lore).num_docs(),
        );
        let started = Instant::now();
        let mut n = 0;
        for q in [
            "Fireball",
            "firball",
            "opportunity attack",
            "how does grappling work",
            "cure w",
        ] {
            for c in [Corpus::Rules, Corpus::Lore] {
                let _ = engine.search(c, &SearchRequest::new(q), &SearchConfig::default());
                n += 1;
            }
        }
        println!("   {n} queries in {:?}", started.elapsed());
    }
}

fn main() {
    let queries: Vec<String> = std::env::args().skip(1).collect();
    if queries.is_empty() {
        stats();
        return;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = common::srd_engine(dir.path());
    for query in queries {
        for corpus in [Corpus::Rules, Corpus::Lore] {
            let hits = engine
                .search(
                    corpus,
                    &SearchRequest::new(&query),
                    &SearchConfig::default(),
                )
                .expect("search");
            println!("{query} [{}]", corpus.name());
            for h in hits.iter().take(6) {
                println!(
                    "   {:.2} {} | {} | {} | {}",
                    h.score, h.title, h.ty, h.ruleset, h.pack
                );
            }
        }
    }
}
