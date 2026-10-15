# Local regression gate

Public CI (`cargo test --workspace`) runs only what ships in this repository. The importers and the engine are
also checked by tests that need local, never-committed inputs: a 5etools `data/` folder, a corpus of community
Aurora repositories, and private import cases (a source document, its `expected.json` and the stored AI draft).
Those tests skip when their env var is unset, so CI cannot see them regress. The regression gate runs them all
locally and ratchets: **every check that passed in the baseline must still pass.**

```sh
python3 -I tools/regression-gate/gate.py            # run everything, compare, exit 1 on a regression
python3 -I tools/regression-gate/gate.py --quick    # leave out the slow Aurora engine sweep
python3 -I tools/regression-gate/gate.py --only 5etools-items,import-cases
python3 -I tools/regression-gate/gate.py --list     # suites and the inputs they need
python3 -I tools/regression-gate/gate.py --accept   # make this run the baseline of the suites it ran
python3 -I tools/regression-gate/gate.py --accept-last   # same for the last run, without running again
python3 -I tools/regression-gate/gate.py --pin-drafts    # pin the live import drafts (after accepting a live run)
```

Run it from `app/` (any directory works; paths are resolved from the script). Python 3.9+, standard library
only. It builds and runs the tests with `cargo test --release`, one test target at a time.

## Inputs

| Env var | Default (relative to the folder holding `app/`) | Used by |
|---|---|---|
| `BOREALIS_5ETOOLS_DIR` | `corpus/5etools/data` | `5etools-import`, `5etools-engine`, `5etools-items` |
| `BOREALIS_CORPUS_DIR` | `corpus` (holding `repos/`) | `aurora-corpus`, `aurora-sweep` |
| `BOREALIS_HOMEBREW_CASES` | `specs/homebrew-cases` | `import-cases` |
| `BOREALIS_GATE_DIR` | `specs/regression-baseline` | baseline and results |

A suite whose input folder is missing is skipped and keeps its baseline.

## What is checked

* Every libtest result of the gated test targets (`test/<package>:<target>::<test>`).
* One outcome per checked item where the tests report it: the 5etools item audit (category / item id, so
  "this item's +1 bonus is correct" is its own check), the 5etools engine sweep (class, subclass, origin,
  feat offered, feat, equip), the 5etools caster tables (caster / level), the Aurora engine sweep (kind /
  element id, feat offered). The tests emit them through `tools/regression-gate/outcome.rs` when
  `BOREALIS_OUTCOMES` is set; to cover a new per-item test, call `outcome::record(suite, id, pass, detail)` once
  per checked item (ids and short reasons only, never content text).
* `import-cases`: the AI importer without the model. Each pinned draft
  (`$BOREALIS_GATE_DIR/pinned-drafts/<name>/draft.json`, judged against the case named in `<name>/case`) is
  re-finalized with the current code
  (`replay_tests.rs`, `BOREALIS_FINALIZE_REPLAY`) and judged by the homebrew harness (`tests/homebrew_cases`):
  every `must` and `should` row, a fatal case, and the case summary line. Everything after the model calls is
  deterministic, so it is ratcheted like the rest. Live model runs vary and stay outside the gate.
  The gate never reads the live drafts, which a live run moves aside and rewrites. When a live run is accepted,
  `--pin-drafts` replaces the pinned set with copies of `<case>/draft.json` and `<case>/<sub>/draft.json`
  (`-prev` / `-rerunN` folders skipped; a nested draft is named `<case>@<sub>`); then run the gate and
  `--accept` so the baseline matches the new drafts.

## Reading the result

One line per suite, then the ids by category (40 per category on the terminal, all in `diff.txt`):

| Category | Gates? |
|---|---|
| regressed: passed in the baseline, fails now | yes |
| vanished: passed in the baseline, not reported now (a crash, a build error or a renamed id) | yes |
| new failure: fails now, not in the baseline | yes, unless `--allow-new-failures` |
| newly passing, fixed or gone, still failing | no |

Changed summary lines (`metric`) are shown for information. Exit status: 0 clean, 1 regression, 2 usage error.

Results go to `$BOREALIS_GATE_DIR/last-run/` (`results.json`, `diff.txt`, `logs/<suite>/`, the re-finalized
packs), the previous run to `prev-run/`, the baseline to `baseline.json`. They name private content, so the
default location is outside this repository; never commit them.

## When to run, and accepting

Run the gate before pushing changes to an importer (`borealis-import-5etools`, `borealis-migrate`,
`borealis-assist` import), the content model or the engine. A regression is fixed, not accepted. `--accept`
is for deliberate changes: a check that is now correctly failing or renamed, or a new baseline after new
passes (new passes never raise the baseline on their own). The whole run takes as long as its slowest suites
(the Aurora engine sweep alone is about half an hour; `--quick` leaves it out).
