//! Per-check outcome lines for the local regression gate (`tools/regression-gate/gate.py`).
//!
//! Local-only corpus tests call [`record`] once per checked item. With `BOREALIS_OUTCOMES` set
//! it writes `OUTCOME<TAB>PASS|FAIL<TAB><suite>/<id><TAB><detail>` straight to the process
//! stderr, so libtest does not capture it and passing tests report too; unset, it does nothing.
//! The gate ratchets on these lines: a check that passed in its baseline must still pass.
//! Callers pass ids and short reasons only, never content text.

use std::io::Write as _;

/// Env var that turns the outcome lines on.
pub const ENV: &str = "BOREALIS_OUTCOMES";
/// Longest detail written (characters).
const DETAIL_MAX: usize = 240;

/// One outcome of `suite`'s check `id` (no-op unless [`ENV`] is set).
pub fn record(suite: &str, id: &str, pass: bool, detail: &str) {
    if std::env::var_os(ENV).is_none() {
        return;
    }
    let clean = |s: &str| s.replace(['\t', '\n', '\r'], " ");
    let detail: String = clean(detail).chars().take(DETAIL_MAX).collect();
    let verdict = if pass { "PASS" } else { "FAIL" };
    let line = format!(
        "OUTCOME\t{verdict}\t{}/{}\t{detail}\n",
        clean(suite),
        clean(id)
    );
    let _ = std::io::stderr().lock().write_all(line.as_bytes());
}
