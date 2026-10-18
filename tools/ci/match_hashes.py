#!/usr/bin/env python3
"""Match a CI log's OUTCOME_HASH rows against a local gate's OUTCOME lines (IM6-safe).

CI prints `OUTCOME_HASH <group> <PASS|FAIL> <hash>` (tools/ci/run_filtered.py) instead of the ids.
This helper hashes the local ids the same way and lists, per group, the rows present on only one
side and the rows whose verdict differs. It runs locally only: the ids it prints never leave the
machine, and CI-only rows can only be shown by their hash.

Usage: match_hashes.py LOCAL CI
  LOCAL  text with the local gate's `OUTCOME<TAB>PASS|FAIL<TAB><id>...` lines (others are ignored)
  CI     a CI log with OUTCOME_HASH lines (a GitHub Actions timestamp prefix is fine)
Exit status: 0 the same rows, 1 differences, 2 usage error. Python 3.9+, stdlib only.
"""
import argparse
import re
import sys
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import run_filtered as rf  # noqa: E402

HASH_LINE = re.compile(r"OUTCOME_HASH (.+) (PASS|FAIL) ([0-9a-f]{%d})\s*$" % rf.HASH_HEX)
VERDICTS = ("PASS", "FAIL")


def local_rows(lines):
    """`{group: {hash: (id, verdict)}}` from raw OUTCOME lines."""
    rows = defaultdict(dict)
    for line in lines:
        at = line.find("OUTCOME\t")
        if at < 0:
            continue
        parts = line[at:].rstrip("\r\n").split("\t")
        if len(parts) >= 3 and parts[1] in VERDICTS:
            rows[rf.outcome_group(parts[2])][rf.outcome_hash(parts[2])] = (parts[2], parts[1])
    return rows


def ci_rows(lines):
    """`{group: {hash: verdict}}` from OUTCOME_HASH lines."""
    rows = defaultdict(dict)
    for line in lines:
        m = HASH_LINE.search(line.rstrip("\r\n"))
        if m:
            rows[m.group(1)][m.group(3)] = m.group(2)
    return rows


def compare(local, ci):
    """Per group: rows only local `(id, verdict)`, only in CI `(hash, verdict)`, and verdicts that
    differ `(id, local, ci)`; groups that match exactly are left out."""
    out = {}
    for group in sorted(set(local) | set(ci)):
        mine, theirs = local.get(group, {}), ci.get(group, {})
        diff = {
            "local": len(mine),
            "ci": len(theirs),
            "only_local": sorted(v for h, v in mine.items() if h not in theirs),
            "only_ci": sorted((h, v) for h, v in theirs.items() if h not in mine),
            "differs": sorted((i, lv, theirs[h]) for h, (i, lv) in mine.items()
                              if h in theirs and theirs[h] != lv),
        }
        if diff["only_local"] or diff["only_ci"] or diff["differs"]:
            out[group] = diff
    return out


def report(diffs):
    lines = []
    for group, d in diffs.items():
        lines.append(f"{group}: local {d['local']}, CI {d['ci']}; only local "
                     f"{len(d['only_local'])}, only CI {len(d['only_ci'])}, verdict differs "
                     f"{len(d['differs'])}")
        lines += [f"  only local  {v}  {i}" for i, v in d["only_local"]]
        lines += [f"  only CI     {v}  {h}" for h, v in d["only_ci"]]
        lines += [f"  differs     local {lv}, CI {cv}  {i}" for i, lv, cv in d["differs"]]
    return lines


def read_lines(path):
    with open(path, encoding="utf-8", errors="replace") as f:
        return f.readlines()


def main(argv=None):
    ap = argparse.ArgumentParser(description="match CI OUTCOME_HASH rows against local OUTCOME lines")
    ap.add_argument("local")
    ap.add_argument("ci")
    args = ap.parse_args(argv)
    try:
        local, ci = local_rows(read_lines(args.local)), ci_rows(read_lines(args.ci))
    except OSError as e:
        print(f"match_hashes: {e}", file=sys.stderr)
        return 2
    if not local or not ci:
        side = "local OUTCOME" if not local else "CI OUTCOME_HASH"
        print(f"match_hashes: no {side} lines found", file=sys.stderr)
        return 2
    diffs = compare(local, ci)
    for line in report(diffs):
        print(line)
    groups = len(set(local) | set(ci))
    print(f"== {groups} group(s), {len(diffs)} with differences")
    return 1 if diffs else 0


if __name__ == "__main__":
    sys.exit(main())
