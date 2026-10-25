//! Evaluation harness (spec section 6, WP S1.3).
//!
//! * `cargo run --release -p borealis-search --example eval -- --variant V2` prints the category x metric table.
//! * `--variant all` runs V0, V1, V2, V2b and the seven V2-t<k> ablations and prints a summary plus the
//!   adoption-rule decisions (V1 vs V0, V2 vs V1, V2b vs V2, each ablation vs V2).
//! * `--compare A B` prints the adoption decision of B over A.
//! * `--select` greedy forward selection of tweaks from V1 under the adoption rule.
//! * `--pool` writes `eval/pool.jsonl` (git-ignored; union of every variant's top 20, minus judged pairs, shuffled, blind).
//! * `--write-baseline T3,4,6,7` writes `eval/baseline.json` (phase C, after judging).
//! * V4 (S3.3, needs model files and `BOREALIS_V4_CACHE`; specs are `<model.gguf>:<max_tokens>:<dims>`):
//!   `--v4-cache <model.gguf>:<max_tokens>` embeds every chunk once into the cache; `--v4 <spec>...` evaluates each
//!   hybrid spec against the adopted keyword config (`T3,4,6,7`) under the hybrid adoption rule
//!   (`BOREALIS_LATENCY_BUDGET=phone` for the mid-phone budget) and prints the first spec against the others;
//!   `--v4-pool <out.jsonl> <spec>...` writes their unjudged top-20 pairs for the blind judge.
//!
//! Unjudged documents count as grade 0; `judged@10` reports how much of the top 10 is judged.

#[path = "../tests/common/mod.rs"]
mod common;
#[path = "../tests/harness/mod.rs"]
mod harness;

use borealis_ai::retrieval::{Filters, RetrievalIndex};
use borealis_content::Ruleset;
use harness::baseline::Baseline;
use harness::data::{canon, eval_dir, load_qrels, load_queries, EvalQuery};
use harness::metrics::{evaluate, VariantRun};
use harness::semantic::{fill_cache, EmbedSpec, RawCache, V4Ranker};
use harness::variants::{Fixture, VariantId};
use harness::{pool, report, stats, Ranker};
use std::collections::HashSet;

/// V0: the current in-memory BM25 of `borealis-ai` over the element store (no rules-text sections exist there).
struct V0 {
    index: RetrievalIndex,
}

impl Ranker for V0 {
    fn name(&self) -> String {
        "V0".into()
    }

    fn rank(&self, q: &EvalQuery, k: usize) -> Vec<String> {
        let filters = Filters {
            ruleset: match q.ruleset.as_deref() {
                Some("2014") => Some(Ruleset::Dnd2014),
                Some("2024") => Some(Ruleset::Dnd2024),
                _ => None,
            },
            ..Filters::default()
        };
        let mut seen = HashSet::new();
        self.index
            .retrieve(&q.text, k * 2, &filters)
            .into_iter()
            .map(|h| canon(&h.id).to_string())
            .filter(|key| seen.insert(key.clone()))
            .take(k)
            .collect()
    }
}

fn ranker(fixture: &Fixture, id: VariantId) -> Result<Box<dyn Ranker>, String> {
    if id == VariantId::V0 {
        return Ok(Box::new(V0 {
            index: RetrievalIndex::build(&fixture.store),
        }));
    }
    fixture.ranker(id)
}

fn usage() -> ! {
    eprintln!(
        "usage: eval --variant <V0|V1|V2|V2b|V2-t1..V2-t7|all> | --compare A B | --pool | --write-baseline V"
    );
    std::process::exit(2)
}

fn run_variant(
    fixture: &Fixture,
    id: VariantId,
    queries: &[EvalQuery],
    qrels: &harness::data::Qrels,
) -> VariantRun {
    match ranker(fixture, id) {
        Ok(r) => evaluate(r.as_ref(), queries, qrels),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1)
        }
    }
}

fn parse(s: &str) -> VariantId {
    s.parse().unwrap_or_else(|e: String| {
        eprintln!("{e}");
        usage()
    })
}

fn print_decision(a: &stats::Adoption) {
    println!(
        "{:<14} vs {:<14} delta nDCG@10 {:+.4}  p={:.4}  worst category {} {:+.4}  p95 {:.2} ms  -> {}",
        a.candidate,
        a.base,
        a.delta,
        a.p_value,
        a.worst_category,
        a.worst_category_delta,
        a.candidate_p95_ms,
        if a.adopted { "ADOPT" } else { "keep base" }
    );
}

/// Greedy forward selection from V1: each round adds the tweak whose gain over the current set passes the
/// adoption rule with the largest delta; stops when no tweak passes. Prints every candidate of every round.
fn select(fixture: &Fixture, queries: &[EvalQuery], qrels: &harness::data::Qrels) {
    let mut mask = 0u8;
    loop {
        let base = run_variant(fixture, VariantId::Custom(mask), queries, qrels);
        println!("round base {} nDCG@10 {:.4}", base.name, base.ndcg10(None));
        let mut best: Option<(u8, f64)> = None;
        for k in (1..=harness::variants::TWEAK_COUNT).filter(|k| mask & (1 << (k - 1)) == 0) {
            let cand_mask = mask | (1 << (k - 1));
            let cand = run_variant(fixture, VariantId::Custom(cand_mask), queries, qrels);
            let d = stats::decide(&base, &cand);
            print!("  +t{k}: ");
            print_decision(&d);
            if d.adopted && best.is_none_or(|(_, delta)| d.delta > delta) {
                best = Some((k, d.delta));
            }
        }
        match best {
            Some((k, _)) => {
                println!("  => adopt tweak {k}");
                mask |= 1 << (k - 1);
            }
            None => {
                println!("  => stop at {}", VariantId::Custom(mask).name());
                return;
            }
        }
    }
}

/// Sweep of the tweak 1 field weights (spec section 5: name 2-8, headings 1-3) on top of `base_mask`.
fn sweep(fixture: &Fixture, queries: &[EvalQuery], qrels: &harness::data::Qrels, base_mask: u8) {
    let base = run_variant(fixture, VariantId::Custom(base_mask), queries, qrels);
    println!("sweep base {} nDCG@10 {:.4}", base.name, base.ndcg10(None));
    let tweaks = harness::variants::from_mask(base_mask | 1);
    for name in [1.5f32, 2.0, 3.0, 4.0, 6.0, 8.0] {
        for headings in [1.0f32, 2.0, 3.0] {
            let weights = borealis_search::Weights {
                name,
                headings,
                ..borealis_search::Weights::default()
            };
            let label = format!("n{name}h{headings}");
            let r = fixture.ranker_custom(&label, tweaks, weights);
            let cand = evaluate(r.as_ref(), queries, qrels);
            print_decision(&stats::decide(&base, &cand));
        }
    }
}

fn all(fixture: &Fixture, queries: &[EvalQuery], qrels: &harness::data::Qrels) -> Vec<VariantRun> {
    VariantId::runnable()
        .into_iter()
        .map(|id| run_variant(fixture, id, queries, qrels))
        .collect()
}

fn print_all(runs: &[VariantRun]) {
    println!("{}", report::summary_table(runs));
    let by = |name: &str| runs.iter().find(|r| r.name == name).expect("variant ran");
    println!("Adoption decisions ");
    print_decision(&stats::decide(by("V0"), by("V1")));
    print_decision(&stats::decide(by("V1"), by("V2")));
    print_decision(&stats::decide(by("V2"), by("V2b")));
    for run in runs.iter().filter(|r| r.name.starts_with("V2-t")) {
        // An ablation "wins" when removing the tweak is at least as good: report removal vs the full V2.
        print_decision(&stats::decide(by("V2"), run));
    }
}

fn write_pool(fixture: &Fixture, runs: &[VariantRun], queries: &[EvalQuery]) {
    let qrels = load_qrels();
    let pairs = pool::pool_pairs(runs, &qrels);
    let (lines, skipped) = pool::pool_lines(&pairs, queries, &fixture.docs);
    let path = eval_dir().join("pool.jsonl");
    std::fs::write(&path, lines.join("\n") + "\n").expect("write pool");
    println!(
        "wrote {} ({} pairs, {skipped} skipped: unknown document)",
        path.display(),
        lines.len()
    );
}

/// The adopted keyword configuration V4 is compared with (EM8).
const ADOPTED: &str = "T3,4,6,7";

/// `<model.gguf>:<max_tokens>[:<dims>]`.
fn v4_spec(arg: &str) -> (EmbedSpec, Option<usize>) {
    let mut parts = arg.rsplitn(3, ':').collect::<Vec<_>>();
    parts.reverse();
    let num = |s: &str| -> usize {
        s.parse().unwrap_or_else(|_| {
            eprintln!("bad V4 spec {arg:?}: <model.gguf>:<max_tokens>[:<dims>]");
            usage()
        })
    };
    match parts.as_slice() {
        [model, tokens, dims] if dims.parse::<usize>().is_ok() => (
            EmbedSpec::new(std::path::Path::new(model), num(tokens)),
            Some(num(dims)),
        ),
        _ => {
            let (model, tokens) = arg.rsplit_once(':').unwrap_or_else(|| usage());
            (
                EmbedSpec::new(std::path::Path::new(model), num(tokens)),
                None,
            )
        }
    }
}

fn v4_runs(
    fixture: &Fixture,
    specs: &[String],
    queries: &[EvalQuery],
    qrels: &harness::data::Qrels,
) -> Vec<VariantRun> {
    specs
        .iter()
        .map(|arg| {
            let (spec, dims) = v4_spec(arg);
            let dims = dims.unwrap_or_else(|| usage());
            let cache = RawCache::open(&harness::semantic::cache_dir(), &spec);
            let ranker = V4Ranker::new(fixture, &spec, &cache, dims);
            let run = evaluate(&ranker, queries, qrels);
            let [kw, embed, fuse] = ranker.stage_ms();
            println!(
                "{} stages p50/p95 ms: keyword {:.1}/{:.1}, query embedding {:.1}/{:.1}, vector search + fusion {:.1}/{:.1}",
                run.name, kw.0, kw.1, embed.0, embed.1, fuse.0, fuse.1
            );
            run
        })
        .collect()
}

fn v4(fixture: &Fixture, specs: &[String], queries: &[EvalQuery], qrels: &harness::data::Qrels) {
    let phone = std::env::var("BOREALIS_LATENCY_BUDGET").is_ok_and(|b| b == "phone");
    let budget = if phone {
        stats::PHONE_P95_MS
    } else {
        stats::DESKTOP_P95_MS
    };
    let base = run_variant(fixture, parse(ADOPTED), queries, qrels);
    let runs = v4_runs(fixture, specs, queries, qrels);
    let mut all = vec![base.clone()];
    all.extend(runs.iter().cloned());
    println!("{}", report::summary_table(&all));
    println!("{}", report::detail_table(&base));
    for run in &runs {
        println!("{}", report::detail_table(run));
    }
    println!(
        "Hybrid adoption (keyword budget {budget} ms + {} ms, added p95 < {} ms)",
        stats::HYBRID_EXTRA_MS,
        stats::HYBRID_EXTRA_MS
    );
    for run in &runs {
        let a = stats::decide_hybrid(&base, run, budget);
        print_decision(&a);
        println!(
            "    p50 {:.2} ms, p95 {:.2} ms, added p95 {:+.2} ms",
            run.latency(None, 0.5),
            run.latency(None, 0.95),
            run.latency(None, 0.95) - base.latency(None, 0.95)
        );
    }
    if let Some((first, rest)) = runs.split_first() {
        println!("Against {} (quality only)", first.name);
        for run in rest {
            print_decision(&stats::decide(first, run));
        }
    }
}

fn v4_pool(fixture: &Fixture, out: &str, specs: &[String], queries: &[EvalQuery]) {
    let qrels = load_qrels();
    let runs = v4_runs(fixture, specs, queries, &qrels);
    let pairs = pool::pool_pairs(&runs, &qrels);
    let (lines, skipped) = pool::pool_lines(&pairs, queries, &fixture.docs);
    std::fs::write(out, lines.join("\n") + "\n").expect("write pool");
    println!(
        "wrote {out} ({} pairs, {skipped} skipped: unknown document)",
        lines.len()
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = args.first().map(String::as_str).unwrap_or_default();
    let queries = load_queries();
    let qrels = load_qrels();
    let fixture = Fixture::build();
    let missing = qrels
        .values()
        .flat_map(|m| m.keys())
        .filter(|k| !fixture.docs.contains_key(*k))
        .count();
    println!(
        "{} queries, {} judged queries, {} documents indexed, {missing} judged keys missing from the corpus",
        queries.len(),
        qrels.len(),
        fixture.docs.len()
    );
    match (flag, args.get(1), args.get(2)) {
        ("--variant", Some(v), _) if v == "all" => print_all(&all(&fixture, &queries, &qrels)),
        ("--variant", Some(v), _) => {
            let run = run_variant(&fixture, parse(v), &queries, &qrels);
            println!("{}", report::detail_table(&run));
        }
        ("--compare", Some(a), Some(b)) => {
            let base = run_variant(&fixture, parse(a), &queries, &qrels);
            let cand = run_variant(&fixture, parse(b), &queries, &qrels);
            print_decision(&stats::decide(&base, &cand));
        }
        ("--sweep", Some(m), _) => sweep(&fixture, &queries, &qrels, m.parse().expect("mask")),
        ("--select", _, _) => select(&fixture, &queries, &qrels),
        ("--v4-cache", Some(spec), _) => {
            let (spec, _) = v4_spec(spec);
            fill_cache(&fixture, &spec);
        }
        ("--v4", Some(_), _) => v4(&fixture, &args[1..], &queries, &qrels),
        ("--v4-pool", Some(out), Some(_)) => v4_pool(&fixture, out, &args[2..], &queries),
        ("--pool", _, _) => write_pool(&fixture, &all(&fixture, &queries, &qrels), &queries),
        ("--write-baseline", Some(v), _) => {
            let run = run_variant(&fixture, parse(v), &queries, &qrels);
            Baseline::from_run(&run).write().expect("write baseline");
            println!("wrote baseline.json for {}", run.name);
        }
        _ => usage(),
    }
}
