# Nightly sweeps and the mutation gate (B-22)

## UI-vs-engine contract

`src-tauri/tests/snapshot_contract.rs` builds five bundled-SRD golden characters (fighter, wizard, 2024
cleric, two multiclass casters) and validates the real `CharacterSnapshot` JSON against
`ui/src/api/generated/contract.json` (generated from `ui/src/api/contract.ts`; vitest fails if it is stale).
A field the DTO adds or renames without the TS contract, or a required TS field the DTO drops, fails.
`ui/src/api/__tests__/snapshot-contract.test.ts` runs the same structural check on every MockApi snapshot.
Both run in the normal CI jobs.

## Nightly workflow (`.github/workflows/nightly.yml`)

Schedule plus manual run. It runs only the regression-gate suites that need no private data:
`golden` (26 SRD characters against hand-derived sheets) and `sheet`. There is no baseline in CI (baselines
hold private ids and live outside the repository), so the gate runs against an empty baseline and any
failing check is a regression. `diff.txt` and `results.json` are uploaded as the `regression-gate-diff`
artifact (14 days).

Not wired, because they need private corpora (run locally with `tools/regression-gate/gate.py`, see
`docs/regression-gate.md`): `5etools-import`, `5etools-engine`, `5etools-items`, `aurora-corpus`,
`aurora-sweep`, `import-cases`, `acceptance`, `dnd5e-truth`. The property tests in
`crates/borealis-character/tests/engine_properties.rs` already run in CI at their fixed case counts; a
larger-CASES env var is not added yet.

## Mutation gate (`tools/mutants/`)

Scope: grant and select gating in `crates/borealis-engine/src/custom_rules.rs` (`feat_slot`, `grant_feat`,
`grant_spell`, `is_empty_object`, `param_select`, `str_field`, `level_field`, `marked`), the area where the
engine test audit measured 16.7% survival. `baseline.txt` lists the known surviving mutants (missed or timed
out) without line numbers.

```
cargo install cargo-mutants                      # once, not part of the build
python3 -I tools/mutants/gate.py                 # weekly: fails on a NEW survivor
python3 -I tools/mutants/gate.py --accept        # after killing survivors or accepting new ones
```

Weekly cron entry (local or self-hosted): `0 4 * * 1 cd <repo> && python3 -I tools/mutants/gate.py`.
Runtime is about 25 minutes with `MUTANTS_JOBS=3` (73 mutants, the engine suite per mutant). The first run found 7 survivors in the `*_select` constructors (level, support and marker fields deleted); unit tests in `custom_rules.rs` killed them, so the baseline is empty: any survivor is new. The Aurora-XML fixture tests skip without
`BOREALIS_AURORA_FIXTURES`; the baseline was recorded without them (the CI-like state), so a local run with
them can only kill more mutants. Widening the scope (`spells.rs`, `derive.rs`, `eval.rs`, `options.rs`,
`levels.rs`) means editing `FILE`/`FUNCTIONS` in `gate.py` and re-accepting.
