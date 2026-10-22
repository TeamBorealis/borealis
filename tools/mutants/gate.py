#!/usr/bin/env python3
"""Mutation gate (B-22): run cargo-mutants on a small scope and fail on NEW surviving mutants.

Scope: the grant/select gating functions of crates/borealis-engine/src/custom_rules.rs (the area where
the engine test audit measured 16.7% survival). A survivor is a mutant no test caught. The checked-in
baseline (tools/mutants/baseline.txt) lists the known survivors, one per line, without line numbers
(they drift): `<file>: <mutation> in <function>`. Output prints labels and counts only.

  python3 -I tools/mutants/gate.py            run and compare; exit 1 on a new survivor
  python3 -I tools/mutants/gate.py --accept   rewrite the baseline from this run
  python3 -I tools/mutants/gate.py --compare <missed.txt>   compare an existing cargo-mutants missed.txt

Env: MUTANTS_JOBS (default 2), MUTANTS_TIMEOUT seconds per mutant (default 180), MUTANTS_OUT (output dir,
default target/mutants-gate). Needs `cargo mutants` (cargo install cargo-mutants). Python 3.9+, stdlib only.
"""
import argparse
import os
import re
import subprocess
import sys
from pathlib import Path

APP = Path(__file__).resolve().parents[2]
BASELINE = APP / "tools" / "mutants" / "baseline.txt"
PACKAGE = "borealis-engine"
FILE = "crates/borealis-engine/src/custom_rules.rs"
FUNCTIONS = "feat_slot|grant_feat|grant_spell|is_empty_object|param_select|str_field|level_field|marked"
LOC = re.compile(r":\d+:\d+: ")


def normalise(line):
    """Drop the line and column so a baseline survives unrelated edits above the mutant."""
    return LOC.sub(": ", line.strip())


def read_set(path):
    if not path.is_file():
        return set()
    return {normalise(l) for l in path.read_text().splitlines() if l.strip() and not l.startswith("#")}


def run_mutants(out):
    env = dict(os.environ)
    cmd = ["cargo", "mutants", "-p", PACKAGE, "-f", FILE, "-F", FUNCTIONS,
           "-j", env.get("MUTANTS_JOBS", "2"), "--timeout", env.get("MUTANTS_TIMEOUT", "180"),
           "--output", str(out)]
    print(f"cargo mutants scope: {FILE} ({FUNCTIONS})")
    code = subprocess.run(cmd, cwd=APP, env=env).returncode
    # cargo-mutants exits 2 when mutants survive, 3 on timeouts: both are results, not failures to run.
    if code not in (0, 2, 3):
        print(f"cargo mutants failed to run (exit {code})", file=sys.stderr)
        sys.exit(2)
    return out / "mutants.out"


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--accept", action="store_true", help="write this run's survivors as the baseline")
    ap.add_argument("--compare", metavar="MISSED", help="compare an existing missed.txt, do not run")
    a = ap.parse_args()
    if a.compare:
        run_dir = Path(a.compare).parent
    else:
        out = Path(os.environ.get("MUTANTS_OUT", APP / "target" / "mutants-gate"))
        out.mkdir(parents=True, exist_ok=True)
        run_dir = run_mutants(out)
    missed = read_set(run_dir / "missed.txt")
    timeouts = read_set(run_dir / "timeout.txt")
    survivors = missed | timeouts
    if a.accept:
        BASELINE.write_text("# cargo-mutants survivors (missed + timeout), one per line, no line numbers.\n"
                            "# Regenerate: python3 -I tools/mutants/gate.py --accept\n"
                            + "".join(s + "\n" for s in sorted(survivors)))
        print(f"baseline written: {len(survivors)} survivors")
        return 0
    base = read_set(BASELINE)
    new = sorted(survivors - base)
    fixed = sorted(base - survivors)
    print(f"survivors {len(survivors)}, baseline {len(base)}, new {len(new)}, no longer surviving {len(fixed)}")
    for s in new:
        print(f"  NEW   {s}")
    for s in fixed:
        print(f"  FIXED {s}  (remove it from the baseline with --accept)")
    print(f"{'FAIL' if new else 'PASS'}: {len(new)} new survivor(s)")
    return 1 if new else 0


if __name__ == "__main__":
    sys.exit(main())
