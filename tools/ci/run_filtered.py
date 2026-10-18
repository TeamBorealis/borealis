#!/usr/bin/env python3
"""Run `cargo test --release <args>` and print counts only (IM6: public CI logs show no content).

The import suites read third-party content (5etools via a private URL, Aurora community XML) and
some of their output names elements or quotes text: skip causes, panic messages, triage lists.
This wrapper passes through only lines whose shape is known to carry no content:
  - libtest `running N tests`, `test <name> ... ok|FAILED|ignored`, `test result: ...`
  - `METRIC<TAB>source<TAB>stage<TAB>type<TAB>metric<TAB>n` lines (acceptance.rs, numbers only)
  - fixed `SKIPPED <source>: <reason>` lines
`OUTCOME` lines (tools/regression-gate/outcome.rs) are counted per `<suite>/<source>`; their ids
name elements, so only a fingerprint is printed: `OUTCOME_HASH <group> <PASS|FAIL> <hash>`, the
first 12 hex digits of sha256(OUTCOME_SALT + id). The salt is a fixed public constant (the ids are
public-content ids; the hash only keeps names out of the log), so tools/ci/match_hashes.py can
match CI rows against a local gate's OUTCOME lines. Everything else is withheld and only counted. With
GITHUB_STEP_SUMMARY set, a counts-only table is appended there. Nothing is written to disk.

Usage: run_filtered.py --label NAME [--fail-on-outcome-fail] -- <cargo test args>
Exit status: cargo's (1 also when --fail-on-outcome-fail and an OUTCOME failed), 2 usage error.
Python 3.9+, stdlib only.
"""
import argparse
import hashlib
import os
import re
import subprocess
import sys
import threading
from collections import Counter
from urllib.parse import urlsplit

SECRET_ENV = "BOREALIS_LIVE_5ETOOLS_URL"
SAFE = re.compile(r"^[A-Za-z0-9 _*.+():/-]{1,96}$")
TEST_LINE = re.compile(r"^test ([A-Za-z0-9_:]+) \.\.\. (ok|FAILED|ignored)$")
RESULT_LINE = re.compile(r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;")
RUNNING_LINE = re.compile(r"^running \d+ tests?$")
SKIP_LINE = re.compile(r"^SKIPPED(?: [a-z0-9-]+)?: [A-Za-z0-9_ ]{1,96}$")
OUTCOME_SALT = "borealis-outcome-v1:"
HASH_HEX = 12


def outcome_hash(check_id):
    """Privacy-safe fingerprint of an OUTCOME id (first HASH_HEX hex digits of the salted sha256)."""
    return hashlib.sha256((OUTCOME_SALT + check_id).encode("utf-8")).hexdigest()[:HASH_HEX]


class Tally:
    """Counts of one run; `take` returns the text to print for a line, or None to withhold it."""

    def __init__(self):
        self.tests = Counter()
        self.outcomes = Counter()  # (group, verdict) -> n
        self.metrics = []
        self.withheld = 0
        self.lock = threading.Lock()

    def take(self, line):
        line = line.rstrip("\r\n")
        with self.lock:
            return self._take(line)

    def _take(self, line):
        m = TEST_LINE.match(line)
        if m:
            self.tests[m.group(2)] += 1
            return line
        if RESULT_LINE.match(line) or RUNNING_LINE.match(line) or SKIP_LINE.match(line):
            return line
        parts = line.split("\t")
        if parts[0] == "OUTCOME" and len(parts) >= 3 and parts[1] in ("PASS", "FAIL"):
            group = outcome_group(parts[2])
            self.outcomes[(group, parts[1])] += 1
            return f"OUTCOME_HASH {group} {parts[1]} {outcome_hash(parts[2])}"
        if (parts[0] == "METRIC" and len(parts) == 6 and re.fullmatch(r"-?\d+", parts[5])
                and all(SAFE.match(p) for p in parts[1:5])):
            self.metrics.append(parts[1:])
            return line
        self.withheld += 1
        return None


def outcome_group(check_id):
    """`acceptance/a-core/s1/Class` -> `acceptance/a-core`; anything unexpected -> `other`."""
    group = "/".join(check_id.split("/")[:2])
    return group if SAFE.match(group) else "other"


def mask_secret():
    """Ask the runner to mask the 5etools URL and its host-less form (secrets are masked already;
    this also covers the URL as the downloader may rewrite its scheme)."""
    url = os.environ.get(SECRET_ENV, "").strip()
    if not url:
        return
    parts = urlsplit(url)
    for value in {url, parts.netloc + parts.path, parts.path.strip("/")}:
        if len(value) >= 8:
            print(f"::add-mask::{value}", flush=True)


def pump(stream, tally):
    for line in stream:
        text = tally.take(line)
        if text is not None:
            print(text, flush=True)


def run(cargo_args, tally):
    env = dict(os.environ, BOREALIS_OUTCOMES="1")
    proc = subprocess.Popen(["cargo", "test", "--release", *cargo_args], env=env,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            text=True, encoding="utf-8", errors="replace")
    threads = [threading.Thread(target=pump, args=(s, tally)) for s in (proc.stdout, proc.stderr)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    return proc.wait()


def summary_lines(label, code, tally):
    passed = sum(n for (_, v), n in tally.outcomes.items() if v == "PASS")
    failed = sum(n for (_, v), n in tally.outcomes.items() if v == "FAIL")
    head = (f"{label}: cargo exit {code}; tests ok {tally.tests['ok']}, failed "
            f"{tally.tests['FAILED']}, ignored {tally.tests['ignored']}; outcomes PASS {passed}, "
            f"FAIL {failed}; {len(tally.metrics)} metrics; {tally.withheld} lines withheld")
    groups = sorted({g for g, _ in tally.outcomes})
    rows = [(g, tally.outcomes[(g, "PASS")], tally.outcomes[(g, "FAIL")]) for g in groups]
    return head, rows


def write_step_summary(label, head, rows, tally):
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if not path:
        return
    out = [f"### {label}", "", head, ""]
    if rows:
        out += ["| outcome group | PASS | FAIL |", "|---|---:|---:|"]
        out += [f"| {g} | {p} | {f} |" for g, p, f in rows]
        out.append("")
    if tally.metrics:
        out += ["<details><summary>METRIC (source / stage / type / metric = n)</summary>", "",
                "| source | stage | type | metric | n |", "|---|---|---|---|---:|"]
        out += ["| " + " | ".join(m) + " |" for m in tally.metrics]
        out += ["", "</details>", ""]
    with open(path, "a", encoding="utf-8") as f:
        f.write("\n".join(out) + "\n")


def main():
    ap = argparse.ArgumentParser(description="cargo test --release with counts-only output")
    ap.add_argument("--label", required=True)
    ap.add_argument("--fail-on-outcome-fail", action="store_true")
    ap.add_argument("cargo_args", nargs=argparse.REMAINDER)
    args = ap.parse_args()
    cargo_args = args.cargo_args[1:] if args.cargo_args[:1] == ["--"] else args.cargo_args
    if not cargo_args:
        ap.error("missing cargo test arguments after --")
    mask_secret()
    tally = Tally()
    code = run(cargo_args, tally)
    head, rows = summary_lines(args.label, code, tally)
    print(f"== {head}")
    for g, p, f in rows:
        print(f"   outcomes {g}: PASS {p} FAIL {f}")
    write_step_summary(args.label, head, rows, tally)
    if code != 0 and not tally.tests:
        print(f"::error::{args.label}: cargo exited {code} before any test ran "
              "(the unfiltered build step shows compiler errors)")
    outcome_failed = any(v == "FAIL" for _, v in tally.outcomes)
    if code == 0 and args.fail_on_outcome_fail and outcome_failed:
        print(f"::error::{args.label}: OUTCOME checks failed (ids stay private; rerun the gate locally)")
        return 1
    return code


if __name__ == "__main__":
    sys.exit(main())
