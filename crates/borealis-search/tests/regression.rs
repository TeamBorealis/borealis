//! CI regression gate: the adopted configuration must not drop more than 0.01 nDCG@10 below `eval/baseline.json`.
//! `baseline.json` holds the adopted configuration (phase C); the test skips only if the file is absent.
mod common;
mod harness;

use harness::baseline::Baseline;
use harness::data::{load_qrels, load_queries};
use harness::metrics::evaluate;
use harness::variants::{Fixture, VariantId};

#[test]
fn adopted_config_does_not_regress_below_baseline() {
    let Some(baseline) = Baseline::load() else {
        eprintln!("eval/baseline.json is absent: regression gate skipped");
        return;
    };
    let id: VariantId = baseline.variant.parse().expect("baseline variant");
    let fixture = Fixture::build();
    let ranker = fixture
        .ranker(id)
        .unwrap_or_else(|e| panic!("baseline variant {}: {e}", baseline.variant));
    let run = evaluate(ranker.as_ref(), &load_queries(), &load_qrels());
    let regressions = baseline.regressions(&run);
    assert!(regressions.is_empty(), "{}", regressions.join("\n"));
}
