#!/usr/bin/env python3
"""Local regression gate with a ratchet (see docs/regression-gate.md).

Runs the local-only checks public CI cannot run (corpus tests, offline re-finalize + judge of
private import cases), records one outcome per check (libtest results, the `OUTCOME` lines of
tools/regression-gate/outcome.rs, judge rows) and compares them with a baseline:
every check that passed in the baseline must still pass. `--accept` makes this run the new
baseline for the suites it ran. Results and baselines hold private ids, so they live outside
the repository (default `../specs/regression-baseline`, override with BOREALIS_GATE_DIR).

Exit status: 0 no regression, 1 regression, 2 usage or setup error. Python 3.9+, stdlib only.
"""
import argparse
import datetime
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

APP = Path(__file__).resolve().parents[2]
ROOT = APP.parent
PRIVATE = Path(os.environ.get("BOREALIS_GATE_DIR", ROOT / "specs" / "regression-baseline"))
FIVE = Path(os.environ.get("BOREALIS_5ETOOLS_DIR", ROOT / "corpus" / "5etools" / "data"))
CORPUS = Path(os.environ.get("BOREALIS_CORPUS_DIR", ROOT / "corpus"))
CASES = Path(os.environ.get("BOREALIS_HOMEBREW_CASES", ROOT / "specs" / "homebrew-cases"))
SHOW = 40  # ids listed per category on the terminal (all of them go to diff.txt)
TEST_LINE = re.compile(r"^test (\S+) \.\.\. (ok|FAILED|ignored)")
ROW = re.compile(r"^(PASS|FAIL)\s+(must|should)\s+(\S+)\s+(\S+)(?:\s+--\s+(.*))?$")
CASE_HEAD = re.compile(r"== case (\S+) ==$")
SUMMARY = re.compile(r"^summary: (\d+) rows, must (\d+)/(\d+) pass, should (\d+)/(\d+) pass")
PINNED = PRIVATE / "pinned-drafts"  # the drafts the gate re-finalizes (live ones move during live runs)
STALE_RUN = re.compile(r"-(prev|rerun\d*)$")
REPLAY_TEST = "import::anything::replay_tests::refinalize_a_stored_draft"
OPT_IN = ("acceptance", "dnd5e-truth")  # suites run only when named in --only


def corpus_repos():
    return CORPUS / "repos" if (CORPUS / "repos").is_dir() else CORPUS


# name -> (description, required dir, [(package, test target, env)])
def suites():
    five = {"BOREALIS_5ETOOLS_DIR": str(FIVE)}
    root = {"BOREALIS_CORPUS_DIR": str(CORPUS)}
    table = {
        "5etools-import": ("5etools converter over the corpus", FIVE, [
            ("borealis-import-5etools", t, five) for t in ("corpus", "corpus_spells")]),
        "5etools-engine": ("5etools engine sweep, casters, gaps, prerequisites, spells", FIVE, [
            ("borealis", t, five) for t in ("fivetools_corpus", "fivetools_casters",
                                            "fivetools_gaps", "fivetools_prereqs",
                                            "fivetools_spells")]),
        "5etools-items": ("5etools item audit, per item and check", FIVE, [
            ("borealis", "fivetools_items", five)]),
        "aurora-corpus": ("Aurora characters, requirements, index path, migration", CORPUS, [
            ("borealis-character", "corpus", root),
            ("borealis-character", "aurora_corpus_reqs", root),
            ("borealis", "aurora_index_path", root),
            ("borealis-migrate", "corpus", {"BOREALIS_CORPUS_DIR": str(corpus_repos())})]),
        "aurora-sweep": ("Aurora community engine sweep, per element (slow)", CORPUS, [
            ("borealis-character", "aurora_corpus_sweep", root)]),
        "import-cases": ("stored AI-import drafts re-finalized offline + judged", CASES, None),
    }
    # IM4 acceptance (specs/acceptance-imports-1.md): the app-level harness (stages 1-2: source vs
    # imported accounting, offered in the builder) for every source whose input is available, then
    # the deeper engine checks. Opt-in (--only acceptance): it re-runs the suites above.
    acc_env = {**five, **root}
    for var in ("BOREALIS_LIVE_INDEX_URL", "BOREALIS_LIVE_5ETOOLS_URL", "BOREALIS_ACCEPT_LIMIT",
                "BOREALIS_ACCEPT_WORKERS", "BOREALIS_ACCEPT_THREADS"):
        if os.environ.get(var):
            acc_env[var] = os.environ[var]
    deeper = [t for n in ("5etools-import", "5etools-engine", "5etools-items", "aurora-corpus",
                          "aurora-sweep") for t in table[n][2]]
    table["acceptance"] = ("IM4 end-to-end acceptance, per source x stage x type (slow, opt-in)",
                           CORPUS, [("borealis", "acceptance", acc_env)] + deeper)
    # A-comm stage 2 per community repo on A-core (about 2700 builder hosts): too slow for every
    # acceptance run, so its own opt-in suite (--only acceptance-comm).
    comm_env = {**acc_env, "BOREALIS_ACCEPT_ONLY": "a-comm", "BOREALIS_ACCEPT_COMM_S2": "1"}
    table["acceptance-comm"] = ("IM4 acceptance stage 2 per community repo on A-core (very slow, opt-in)",
                                CORPUS, [("borealis", "acceptance", comm_env)])
    # IM4 stage 3c: values Aurora wrote into .dnd5e files vs Borealis (opt-in: --only dnd5e-truth).
    truth_env = dict(root)
    for var in ("BOREALIS_DND5E_DIR", "BOREALIS_DND5E_PRIVATE"):
        if os.environ.get(var):
            truth_env[var] = os.environ[var]
    table["dnd5e-truth"] = ("IM4 stage 3c: Aurora .dnd5e ground truth vs Borealis values (opt-in)",
                            CORPUS, [("borealis-character", "dnd5e_truth", truth_env)])
    return table


def cargo(args, env_extra, out_base):
    """Run cargo test --release; stdout/stderr go to <out_base>.out/.err. Returns (code, out, err)."""
    env = dict(os.environ, BOREALIS_OUTCOMES="1", **env_extra)
    cmd = ["cargo", "test", "--release"] + args
    p = subprocess.run(cmd, cwd=APP, env=env, capture_output=True, text=True, errors="replace")
    out_base.with_suffix(".out").write_text(p.stdout)
    out_base.with_suffix(".err").write_text(p.stderr)
    return p.returncode, p.stdout, p.stderr


class Outcomes:
    def __init__(self):
        self.status, self.detail, self.metrics = {}, {}, {}

    def add(self, cid, ok, detail=""):
        if cid in self.status and self.status[cid] == "FAIL":
            return  # a check recorded twice fails when either record fails
        self.status[cid] = "PASS" if ok else "FAIL"
        if not ok and detail:
            self.detail[cid] = detail[:300]

    def parse_libtest(self, prefix, stdout):
        for line in stdout.splitlines():
            m = TEST_LINE.match(line)
            if m and m.group(2) != "ignored":
                self.add(f"test/{prefix}::{m.group(1)}", m.group(2) == "ok")

    def parse_outcome_lines(self, stderr):
        for line in stderr.splitlines():
            parts = line.split("\t")
            if len(parts) >= 3 and parts[0] == "OUTCOME":
                self.add(parts[2], parts[1] == "PASS", parts[3] if len(parts) > 3 else "")
            elif len(parts) == 6 and parts[0] == "METRIC" and parts[5].lstrip("-").isdigit():
                # acceptance numbers: source / stage / type / metric -> n
                self.metrics["/".join(parts[1:5])] = int(parts[5])

    def as_json(self):
        return {"outcomes": self.status, "details": self.detail, "metrics": self.metrics}


def run_tests(targets, logs, oc):
    for pkg, target, env in targets:
        code, out, err = cargo(["-p", pkg, "--test", target], env, logs / f"{pkg}__{target}")
        oc.parse_libtest(f"{pkg}:{target}", out)
        oc.parse_outcome_lines(err)
        if code != 0 and "test result:" not in out:
            oc.add(f"build/{pkg}:{target}", False, err.strip().splitlines()[-1:][0] if err.strip() else "")
            print(f"    {pkg}:{target}: did not run (build error?), see {logs}")


def live_drafts():
    """(name, case dir, draft.json) of every live draft: <case>/draft.json and
    <case>/<sub>/draft.json (stale `-prev` / `-rerunN` copies skipped)."""
    found = []
    for case in sorted(p for p in CASES.iterdir() if (p / "expected.json").is_file()):
        if (case / "draft.json").is_file():
            found.append((case.name, case, case / "draft.json"))
        for sub in sorted(p for p in case.iterdir() if p.is_dir()):
            if (sub / "draft.json").is_file() and not STALE_RUN.search(sub.name):
                found.append((f"{case.name}@{sub.name}", case, sub / "draft.json"))
    return found


def pinned_drafts():
    """(name, case dir, draft.json) of every pinned draft: <PINNED>/<name>/draft.json, judged against
    the case named in <name>/case (default: <name>)."""
    found = []
    for d in sorted(p for p in PINNED.iterdir() if (p / "draft.json").is_file()):
        marker = d / "case"
        case = CASES / (marker.read_text().strip() if marker.is_file() else d.name)
        if not (case / "expected.json").is_file():
            raise SystemExit(f"pinned draft {d}: no expected.json in {case} (write the case name to {marker})")
        found.append((d.name, case, d / "draft.json"))
    return found


def pin_drafts():
    """Replace the pinned drafts with copies of the live ones (after a live run is accepted)."""
    live = live_drafts()
    if not live:
        print(f"no live drafts under {CASES}", file=sys.stderr)
        return 2
    shutil.rmtree(PINNED, ignore_errors=True)
    for name, case, draft in live:
        (PINNED / name).mkdir(parents=True)
        shutil.copy(draft, PINNED / name / "draft.json")
        (PINNED / name / "case").write_text(case.name + "\n")
        print(f"  pinned {draft} -> {PINNED / name}")
    print("pinned drafts replaced; run the gate and --accept the import-cases suite")
    return 0


def run_import_cases(logs, work, oc):
    judge_root = work / "cases"
    shutil.rmtree(judge_root, ignore_errors=True)
    if not PINNED.is_dir():
        oc.add("import-cases/(no pinned drafts)", False, f"{PINNED} missing: run --pin-drafts")
        return
    for name, case, draft in pinned_drafts():
        dest = judge_root / name
        (dest / "imported").mkdir(parents=True)
        shutil.copy(case / "expected.json", dest / "expected.json")
        env = {"BOREALIS_FINALIZE_REPLAY": str(draft), "BOREALIS_FINALIZE_OUT": str(dest / "imported")}
        code, _, err = cargo(["-p", "borealis-assist", "--lib", "--", "--ignored", "--exact",
                              REPLAY_TEST], env, logs / f"refinalize__{name}")
        tail = err.strip().splitlines()[-1:] or [""]
        oc.add(f"import-cases/{name}/refinalize", code == 0, tail[0])
    code, out, err = cargo(["-p", "borealis-assist", "--test", "homebrew_cases"],
                           {"BOREALIS_HOMEBREW_CASES": str(judge_root)}, logs / "judge")
    oc.parse_libtest("borealis-assist:homebrew_cases", out)
    case = None
    for line in err.splitlines():
        if CASE_HEAD.search(line):
            case = CASE_HEAD.search(line).group(1)
        elif case and line.startswith("FAIL  (case)"):
            oc.add(f"import-cases/{case}/(case)", False, line[12:].strip())
        elif case and ROW.match(line):
            m = ROW.match(line)
            oc.add(f"import-cases/{case}/{m.group(4)}", m.group(1) == "PASS",
                   f"{m.group(2)} {m.group(5) or ''}".strip())
        elif case and SUMMARY.match(line):
            oc.metrics[case] = line.strip()


def git_state():
    def git(*a):
        p = subprocess.run(["git", *a], cwd=APP, capture_output=True, text=True)
        return p.stdout.strip()
    return {"head": git("rev-parse", "--short", "HEAD"),
            "dirty_files": len(git("status", "--porcelain").splitlines())}


def run(selected, run_dir):
    results = {"started": datetime.datetime.now().isoformat(timespec="seconds"),
               "git": git_state(), "suites": {}}
    table = suites()
    for name in selected:
        desc, needs, targets = table[name]
        if not Path(needs).is_dir():
            results["suites"][name] = {"skipped": f"{needs} not found"}
            print(f"  {name}: SKIPPED ({needs} not found)")
            continue
        print(f"  {name}: {desc} ...", flush=True)
        t0, oc = time.time(), Outcomes()
        logs = run_dir / "logs" / name
        logs.mkdir(parents=True, exist_ok=True)
        if targets is None:
            run_import_cases(logs, run_dir / name, oc)
        else:
            run_tests(targets, logs, oc)
        entry = oc.as_json()
        entry["seconds"] = round(time.time() - t0)
        passed = sum(1 for v in oc.status.values() if v == "PASS")
        print(f"    {passed}/{len(oc.status)} checks pass ({entry['seconds']} s)")
        results["suites"][name] = entry
        if name in ("acceptance", "acceptance-comm"):
            write_acceptance_report(run_dir / f"{name}-report.json", oc.metrics)
    results["finished"] = datetime.datetime.now().isoformat(timespec="seconds")
    return results


def write_acceptance_report(path, metrics):
    """source -> stage -> type -> {metric: n} (numbers only, no ids)."""
    table = {}
    for key, n in sorted(metrics.items()):
        src, stage, ty, name = key.split("/", 3)
        table.setdefault(src, {}).setdefault(stage, {}).setdefault(ty, {})[name] = n
    path.write_text(json.dumps(table, indent=1, sort_keys=True) + "\n")
    print(f"    acceptance report: {path}")


def compare(base, cur, allow_new):
    """Returns (report lines, full lines, regression count)."""
    short, full, bad = [], [], 0
    for name, s in cur["suites"].items():
        if "skipped" in s:
            continue
        b = base.get("suites", {}).get(name)
        if not b or "outcomes" not in b:
            short.append(f"{name}: no baseline yet ({len(s['outcomes'])} checks; --accept records one)")
            continue
        bo, co, det = b["outcomes"], s["outcomes"], s.get("details", {})
        reg, van, new, up, gone = (
            "REGRESSED (passed, now fails)", "VANISHED (passed, now not run)",
            "NEW FAILURE (not in baseline)", "newly passing", "fixed or gone (failed, now not run)")
        cats = {
            reg: [i for i, v in bo.items() if v == "PASS" and co.get(i) == "FAIL"],
            van: [i for i, v in bo.items() if v == "PASS" and i not in co],
            new: [i for i, v in co.items() if v == "FAIL" and i not in bo],
            up: [i for i, v in co.items() if v == "PASS" and bo.get(i) != "PASS"],
            gone: [i for i, v in bo.items() if v == "FAIL" and i not in co],
        }
        n_bad = len(cats[reg]) + len(cats[van]) + (0 if allow_new else len(cats[new]))
        bad += n_bad
        still = sum(1 for i, v in co.items() if v == "FAIL" and bo.get(i) == "FAIL")
        same = sum(1 for i, v in co.items() if v == "PASS" and bo.get(i) == "PASS")
        short.append(f"{name}: {'REGRESSION' if n_bad else 'ok'} - regressed {len(cats[reg])}, "
                     f"vanished {len(cats[van])}, new failures {len(cats[new])}, "
                     f"newly passing {len(cats[up])}, unchanged pass {same}, still failing {still}")
        for cat, ids in cats.items():
            if not ids:
                continue
            lines = [f"  {cat} [{name}]: {len(ids)}"] + [
                f"    {i}" + (f"  -- {det[i]}" if i in det else "") for i in sorted(ids)]
            full.extend(lines)
            short.extend(lines[:SHOW + 1] + ([f"    ... {len(ids) - SHOW} more in diff.txt"]
                                             if len(ids) > SHOW else []))
        for k, v in s.get("metrics", {}).items():
            old = b.get("metrics", {}).get(k)
            if old != v:
                line = f"  metric [{name}] {k}: {old} -> {v}"
                short.append(line)
                full.append(line)
    for name, b in base.get("suites", {}).items():
        if name not in cur["suites"] or "skipped" in cur["suites"][name]:
            short.append(f"{name}: not run this time (baseline kept)")
    return short, full, bad


def accept(base, cur, base_file):
    """Replace the baseline of every suite `cur` ran."""
    ran = [n for n, s in cur["suites"].items() if "skipped" not in s]
    for name in ran:
        base["suites"][name] = dict(cur["suites"][name], accepted=cur["finished"], git=cur["git"])
    base_file.write_text(json.dumps(base, indent=1, sort_keys=True))
    print(f"baseline updated for: {', '.join(ran) or 'nothing'}")


def main():
    table = suites()
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--accept", action="store_true", help="make this run the baseline of the suites it ran")
    ap.add_argument("--accept-last", action="store_true",
                    help="make the last run the baseline of the suites it ran, without running")
    ap.add_argument("--pin-drafts", action="store_true",
                    help="copy the live import drafts to the pinned set (after accepting a live run) and exit")
    ap.add_argument("--only", default="", help="comma-separated suites to run")
    ap.add_argument("--skip", default="", help="comma-separated suites to leave out")
    ap.add_argument("--quick", action="store_true", help="leave out the slow aurora-sweep suite")
    ap.add_argument("--allow-new-failures", action="store_true",
                    help="do not fail on failing checks that are not in the baseline")
    ap.add_argument("--list", action="store_true", help="list the suites and exit")
    a = ap.parse_args()
    if a.list:
        for n, (d, needs, _) in table.items():
            print(f"{n:16} {d}  [{needs}]")
        return 0
    only = [s for s in a.only.split(",") if s]
    skip = [s for s in a.skip.split(",") if s] + (["aurora-sweep"] if a.quick else [])
    unknown = [s for s in only + skip if s not in table]
    if unknown:
        print(f"unknown suite(s): {unknown}; see --list", file=sys.stderr)
        return 2
    # acceptance re-runs the other corpus suites, dnd5e-truth is new: only when asked for by name
    selected = [n for n in table if (not only or n in only) and n not in skip
                and (not n.startswith(OPT_IN) or n in only)]
    PRIVATE.mkdir(parents=True, exist_ok=True)
    run_dir, prev = PRIVATE / "last-run", PRIVATE / "prev-run"
    base_file = PRIVATE / "baseline.json"
    base = json.loads(base_file.read_text()) if base_file.is_file() else {"suites": {}}
    if a.pin_drafts:
        return pin_drafts()
    if a.accept_last:
        last = run_dir / "results.json"
        if not last.is_file():
            print(f"no last run at {last}", file=sys.stderr)
            return 2
        accept(base, json.loads(last.read_text()), base_file)
        return 0
    if run_dir.exists():
        shutil.rmtree(prev, ignore_errors=True)
        run_dir.rename(prev)
    run_dir.mkdir()
    print(f"regression gate: {', '.join(selected)} -> {run_dir}")
    cur = run(selected, run_dir)
    (run_dir / "results.json").write_text(json.dumps(cur, indent=1, sort_keys=True))
    short, full, bad = compare(base, cur, a.allow_new_failures)
    (run_dir / "diff.txt").write_text("\n".join(short + ["", "== full lists =="] + full) + "\n")
    print("\n".join(short))
    if a.accept:
        accept(base, cur, base_file)
        return 0
    print(f"{'FAIL' if bad else 'PASS'}: {bad} regression(s); details in {run_dir / 'diff.txt'}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
