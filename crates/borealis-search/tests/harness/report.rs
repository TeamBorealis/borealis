//! Plain-text tables.
use super::metrics::VariantRun;

/// Category x metric table of one variant.
pub fn detail_table(run: &VariantRun) -> String {
    let mut out = format!("== {} ==\n", run.name);
    out.push_str(&format!(
        "{:<22} {:>4} {:>8} {:>8} {:>9} {:>10} {:>8} {:>8}\n",
        "category", "n", "nDCG@10", "MRR@10", "Recall@50", "judged@10", "p50 ms", "p95 ms"
    ));
    let mut rows: Vec<Option<String>> = run.categories().into_iter().map(Some).collect();
    rows.push(None);
    for row in rows {
        let cat = row.as_deref();
        let n = run.select(cat).count();
        out.push_str(&format!(
            "{:<22} {:>4} {:>8.3} {:>8.3} {:>9.3} {:>10.3} {:>8.2} {:>8.2}\n",
            cat.unwrap_or("ALL"),
            n,
            run.ndcg10(cat),
            run.mean_of(cat, |q| q.rr10),
            run.mean_of(cat, |q| q.recall50),
            run.mean_of(cat, |q| q.judged10),
            run.latency(cat, 0.5),
            run.latency(cat, 0.95),
        ));
    }
    out
}

/// nDCG@10 per category for several variants side by side.
pub fn summary_table(runs: &[VariantRun]) -> String {
    let Some(first) = runs.first() else {
        return String::new();
    };
    let mut categories: Vec<Option<String>> = first.categories().into_iter().map(Some).collect();
    categories.push(None);
    let mut out = format!("{:<8}", "nDCG@10");
    for c in &categories {
        let label = c.as_deref().unwrap_or("ALL");
        out.push_str(&format!(
            " {:>10}",
            label.chars().take(10).collect::<String>()
        ));
    }
    out.push_str("  judged@10  p95 ms\n");
    for run in runs {
        out.push_str(&format!("{:<8}", run.name));
        for c in &categories {
            out.push_str(&format!(" {:>10.3}", run.ndcg10(c.as_deref())));
        }
        out.push_str(&format!(
            "  {:>9.3}  {:>6.2}\n",
            run.mean_of(None, |q| q.judged10),
            run.latency(None, 0.95)
        ));
    }
    out
}
