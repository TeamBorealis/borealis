#!/usr/bin/env python3
"""Fail when a METRIC number in a log breaks its limit in tools/acceptance/thresholds.json.

thresholds.json holds numbers only: {"<platform>": {"<source>/<stage>": {"<metric>": max}}}.
Platforms: desktop-linux, desktop-windows (METRIC lines of tools/ci/run_filtered.py), android, ios
(e2e/tests/mobile/m4-import.e2e.ts reads the same file). A log line counts when it holds
`METRIC<TAB>source<TAB>stage<TAB>*<TAB>metric<TAB>n` anywhere (GitHub's timestamp and job prefixes are
ignored); only aggregate rows (type `*`) are checked. A value above its limit is a breach. The
measured `wall_s` and `peak_mem_mb` rows are printed with their limit (they are already in the log,
so nothing new is shown, IM6). A key without a limit is reported, not failed.

Usage: check_thresholds.py --platform NAME [--thresholds FILE] LOG [LOG...]
Exit: 0 all within limits, 1 a breach, 2 usage, file or thresholds error. Python 3.9+, stdlib only.
"""
import argparse
import json
import math
import os
import re
import sys
from pathlib import Path

THRESHOLDS = Path(__file__).resolve().parents[1] / "acceptance" / "thresholds.json"
METRIC = re.compile(r"METRIC\t([^\t]+)\t([^\t]+)\t([^\t]+)\t([^\t]+)\t(-?\d+)\s*$")
WATCHED = ("wall_s", "peak_mem_mb")


def load_thresholds(path):
    """Read and validate the thresholds file: every leaf is a positive number."""
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError("thresholds: top level must be an object")
    for platform, stages in data.items():
        if platform == "version":
            continue
        if not isinstance(stages, dict):
            raise ValueError(f"thresholds: {platform} must be an object")
        for key, limits in stages.items():
            if "/" not in key or not isinstance(limits, dict) or not limits:
                raise ValueError(f"thresholds: {platform}.{key} must be <source>/<stage> -> limits")
            for metric, limit in limits.items():
                ok = isinstance(limit, (int, float)) and not isinstance(limit, bool)
                if not ok or not math.isfinite(limit) or limit <= 0:
                    raise ValueError(f"thresholds: {platform}.{key}.{metric} must be a positive number")
    return data


def metric_rows(lines):
    """(source/stage, metric, value) of every aggregate METRIC row in the log lines."""
    rows = []
    for line in lines:
        m = METRIC.search(line.rstrip("\r\n"))
        if m and m.group(3) == "*":
            rows.append((f"{m.group(1)}/{m.group(2)}", m.group(4), int(m.group(5))))
    return rows


def check(rows, limits):
    """Returns (checked, breaches, unlimited): lists of (key, metric, value, limit)."""
    checked, breaches, unlimited = [], [], []
    for key, metric, value in rows:
        limit = limits.get(key, {}).get(metric)
        if limit is None:
            if metric in WATCHED:
                unlimited.append((key, metric, value, None))
            continue
        row = (key, metric, value, limit)
        checked.append(row)
        if value > limit:
            breaches.append(row)
    return checked, breaches, unlimited


def report(platform, checked, breaches, unlimited):
    out = [f"== thresholds {platform}: {len(checked)} checked, {len(breaches)} over the limit, "
           f"{len(unlimited)} without a limit"]
    for key, metric, value, limit in checked:
        mark = "OVER" if value > limit else "ok"
        out.append(f"   {mark:4} {key} {metric} {value} (limit {limit:g})")
    for key, metric, value, _ in unlimited:
        out.append(f"   new  {key} {metric} {value} (no limit yet)")
    return out


def write_step_summary(lines):
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if path:
        with open(path, "a", encoding="utf-8") as f:
            f.write("```\n" + "\n".join(lines) + "\n```\n")


def main(argv=None):
    ap = argparse.ArgumentParser(description="check METRIC numbers against thresholds.json")
    ap.add_argument("--platform", required=True)
    ap.add_argument("--thresholds", default=str(THRESHOLDS))
    ap.add_argument("logs", nargs="+")
    args = ap.parse_args(argv)
    try:
        data = load_thresholds(args.thresholds)
        lines = []
        for log in args.logs:
            lines += Path(log).read_text(encoding="utf-8", errors="replace").splitlines()
    except (OSError, ValueError) as e:  # json.JSONDecodeError is a ValueError
        print(f"::error::check_thresholds: {e}")
        return 2
    if args.platform not in data or args.platform == "version":
        print(f"::error::check_thresholds: no platform '{args.platform}' in the thresholds file")
        return 2
    checked, breaches, unlimited = check(metric_rows(lines), data[args.platform])
    lines = report(args.platform, checked, breaches, unlimited)
    print("\n".join(lines))
    write_step_summary(lines)
    if not checked:
        print(f"::warning::check_thresholds: no thresholded METRIC row in the log for {args.platform}")
    for key, metric, value, limit in breaches:
        print(f"::error::{args.platform} {key} {metric} {value} is over the limit {limit:g}")
    return 1 if breaches else 0


if __name__ == "__main__":
    sys.exit(main())
