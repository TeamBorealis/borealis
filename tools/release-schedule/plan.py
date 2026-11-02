#!/usr/bin/env python3
"""Borealis weekly release schedule (decisions RL3, RL4; specs/release-schedule.md).

Turns a version and a publish Thursday into dated milestones. --schedule adds the release to schedule.json
(committed; remind.py posts the milestone reminders to Discord #mod-chat from it). --write and --ics write a
per-release checklist and a calendar file into the private specs/releases folder next to app/.

    python3 -I tools/release-schedule/plan.py 1.3.0                 # print the plan for the next reachable Thursday
    python3 -I tools/release-schedule/plan.py 1.3.0 --publish 2026-11-12 --schedule --write --ics

Standard library only (Python 3.9+).
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from datetime import date, datetime, time, timedelta, timezone
from pathlib import Path
from typing import Optional, Sequence
from zoneinfo import ZoneInfo

LOCAL_TZ = ZoneInfo("Asia/Jerusalem")
THURSDAY = 3
VERSION_RE = re.compile(r"^(\d+)\.(\d+)\.(\d+)$")
HERE = Path(__file__).resolve().parent
SCHEDULE_FILE = HERE / "schedule.json"
RELEASES_DIR = HERE.parents[2] / "specs" / "releases"  # the private specs folder beside app/, when it exists
CHECKLIST = "app/docs/release-checklist.md"


@dataclass(frozen=True)
class Milestone:
    key: str
    title: str
    day_offset: int  # days relative to the publish Thursday
    at: time  # local (Israel) time
    remind_before: timedelta  # when remind.py posts the reminder, before `at`
    tasks: tuple[str, ...]


MILESTONES: tuple[Milestone, ...] = (
    Milestone("scope", "Scope locked", -7, time(20, 0), timedelta(0), (
        "Sprint scope recorded as a decision row and the backlog rows marked with the target release",
    )),
    Milestone("freeze", "Feature freeze", -4, time(23, 59), timedelta(hours=6), (
        "All sprint work merged on main; open items copied into the backlog",
        "`cd ui && npm run check` and `cargo test --workspace` green on main",
        "Anything not merged moves to the next release (backlog row updated)",
    )),
    Milestone("signed", "Signed builds ready", -3, time(20, 0), timedelta(hours=11), (
        f"Version bumped in Cargo.toml, src-tauri/tauri.conf.json, ui/package.json ({CHECKLIST} §2)",
        f"Gates on the release commit: full regression gate, fmt/clippy/test, CI + acceptance ({CHECKLIST} §1)",
        f"Privacy and attribution sweep ({CHECKLIST} §4); MCP spec version re-checked (§3)",
        "Tag pushed; release workflow green (signed APK, Windows, Linux bundles)",
        "macOS bundle built locally from the tag; desktop e2e passed with the data guard",
        "iOS and Play store builds made from the tag",
    )),
    Milestone("submitted", "Store submissions", -2, time(12, 0), timedelta(hours=3), (
        "App Store build submitted for review with manual release",
        "Play build submitted with managed publishing on (approved build waits)",
        "GitHub draft release complete: assets, SHA256SUMS, notes with Discord credits (tools/discord-issues/credits.py)",
    )),
    Milestone("buffer", "Last resubmission", -1, time(12, 0), timedelta(hours=3), (
        "Any store rejection fixed and resubmitted (Apple after this is a gamble)",
        "Website release branch ready: download links, guide screenshots, npm test, build, check:site",
    )),
    Milestone("gonogo", "Go / no-go", 0, time(18, 0), timedelta(hours=1), (
        "Both stores approved and waiting; draft release complete; website branch green",
        "Not ready: the release slips to the next Thursday (only critical fixes go out off-schedule)",
    )),
    Milestone("publish", "Publish", 0, time(20, 0), timedelta(minutes=30), (
        "GitHub draft published as latest; releases/latest checked unauthenticated",
        "Store builds released by hand",
        "Website: changelog fetched, deployed to production, indexnow",
        "Discord: announcer posted once; bridge replied in the fixed issues' threads",
    )),
    Milestone("follow", "Follow-ups", 1, time(12, 0), timedelta(0), (
        "winget manifest PR (needs the public release asset); AUR package bumped",
        "Backlog rows of the release moved to Done",
    )),
)


def parse_version(text: str) -> tuple[int, int, int]:
    m = VERSION_RE.match(text)
    if not m:
        raise ValueError(f"version must look like 1.3.0, got {text!r}")
    return int(m.group(1)), int(m.group(2)), int(m.group(3))


def release_kind(version: str) -> str:
    _, _, patch = parse_version(version)
    return "major (1.x.0: extensive new features)" if patch == 0 else "standard (1.x.y)"


def milestone_at(publish: date, m: Milestone) -> datetime:
    return datetime.combine(publish + timedelta(days=m.day_offset), m.at, tzinfo=LOCAL_TZ)


def next_publish_date(now: datetime) -> date:
    """The first Thursday whose feature freeze has not passed yet."""
    local_now = now.astimezone(LOCAL_TZ)
    freeze = next(m for m in MILESTONES if m.key == "freeze")
    day = local_now.date() + timedelta(days=(THURSDAY - local_now.weekday()) % 7)
    while milestone_at(day, freeze) <= local_now:
        day += timedelta(days=7)
    return day


def validate_publish(publish: date) -> None:
    if publish.weekday() != THURSDAY:
        raise ValueError(f"{publish.isoformat()} is a {publish.strftime('%A')}; releases publish on Thursdays (RL3)")


def fmt(dt: datetime) -> str:
    utc = dt.astimezone(timezone.utc)
    return f"{dt.strftime('%a %Y-%m-%d %H:%M')} Israel ({utc.strftime('%H:%M')} UTC)"


def render_plan(version: str, publish: date) -> str:
    lines = [f"Borealis {version}: {release_kind(version)}, publishes {fmt(milestone_at(publish, MILESTONES[-2]))}", ""]
    lines += [f"  {fmt(milestone_at(publish, m)):<44}  {m.title}" for m in MILESTONES]
    return "\n".join(lines)


def render_checklist(version: str, publish: date) -> str:
    out = [
        f"# Release {version} ({publish.isoformat()})",
        "",
        f"Kind: {release_kind(version)}. Schedule: specs/release-schedule.md (RL3, RL4). "
        f"Detailed steps: {CHECKLIST}. Generated by tools/release-schedule/plan.py; tick boxes as you go.",
        "",
    ]
    for m in MILESTONES:
        out += [f"## {m.title}: {fmt(milestone_at(publish, m))}", ""]
        out += [f"- [ ] {t}" for t in m.tasks]
        out.append("")
    return "\n".join(out)


def ics_escape(text: str) -> str:
    return text.replace("\\", "\\\\").replace(";", "\\;").replace(",", "\\,").replace("\n", "\\n")


def render_ics(version: str, publish: date, stamp: datetime) -> str:
    utc_fmt = "%Y%m%dT%H%M%SZ"
    out = ["BEGIN:VCALENDAR", "VERSION:2.0", "PRODID:-//Borealis//release-schedule//EN", "CALSCALE:GREGORIAN"]
    for m in MILESTONES:
        start = milestone_at(publish, m).astimezone(timezone.utc)
        out += [
            "BEGIN:VEVENT",
            f"UID:borealis-{version}-{m.key}@release-schedule",
            f"DTSTAMP:{stamp.astimezone(timezone.utc).strftime(utc_fmt)}",
            f"DTSTART:{start.strftime(utc_fmt)}",
            f"DTEND:{(start + timedelta(minutes=30)).strftime(utc_fmt)}",
            f"SUMMARY:{ics_escape(f'Borealis {version}: {m.title}')}",
            f"DESCRIPTION:{ics_escape(chr(10).join(m.tasks))}",
            "END:VEVENT",
        ]
    out.append("END:VCALENDAR")
    return "\r\n".join(out) + "\r\n"


def load_schedule(path: Path = SCHEDULE_FILE) -> tuple[tuple[str, date], ...]:
    """The planned releases: ((version, publish Thursday), ...), checked."""
    if not path.exists():
        return ()
    data = json.loads(path.read_text(encoding="utf-8"))
    out = []
    for entry in data.get("releases", []):
        parse_version(entry["version"])
        publish = date.fromisoformat(entry["publish"])
        validate_publish(publish)
        out.append((entry["version"], publish))
    return tuple(sorted(out, key=lambda r: r[1]))


def with_release(releases: tuple[tuple[str, date], ...], version: str, publish: date) -> tuple[tuple[str, date], ...]:
    """A new schedule with this version planned for `publish` (replacing an earlier date for the same version)."""
    others = tuple(r for r in releases if r[0] != version)
    return tuple(sorted(others + ((version, publish),), key=lambda r: r[1]))


def save_schedule(releases: tuple[tuple[str, date], ...], path: Path = SCHEDULE_FILE) -> None:
    data = {"releases": [{"version": v, "publish": d.isoformat()} for v, d in releases]}
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    print(f"scheduled in {path}")


def write_new(path: Path, text: str, force: bool) -> None:
    if path.exists() and not force:
        raise FileExistsError(f"{path} exists (it may have ticked boxes); pass --force to replace it")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    print(f"wrote {path}")


def main(argv: Optional[Sequence[str]] = None) -> int:
    p = argparse.ArgumentParser(description="Borealis weekly release schedule")
    p.add_argument("version", help="for example 1.3.0")
    p.add_argument("--publish", help="publish Thursday, YYYY-MM-DD (default: the next one whose freeze is ahead)")
    p.add_argument("--schedule", action="store_true", help="add the release to schedule.json (Discord reminders)")
    p.add_argument("--write", action="store_true", help="write specs/releases/<version>.md")
    p.add_argument("--ics", action="store_true", help="write <version>.ics for a calendar")
    p.add_argument("--force", action="store_true", help="replace existing files")
    args = p.parse_args(argv)
    try:
        parse_version(args.version)
        now = datetime.now(timezone.utc)
        publish = date.fromisoformat(args.publish) if args.publish else next_publish_date(now)
        validate_publish(publish)
        print(render_plan(args.version, publish))
        if args.schedule:
            save_schedule(with_release(load_schedule(), args.version, publish))
        if args.write:
            write_new(RELEASES_DIR / f"{args.version}.md", render_checklist(args.version, publish), args.force)
        if args.ics:
            write_new(RELEASES_DIR / f"{args.version}.ics", render_ics(args.version, publish, now), args.force)
    except (ValueError, FileExistsError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
