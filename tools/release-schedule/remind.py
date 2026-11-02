#!/usr/bin/env python3
"""Post release milestone reminders to the Discord #mod-chat channel (decision RL4).

Reads the planned releases from schedule.json and the milestones from plan.py. A milestone's reminder is due
from `deadline - remind_before` for CATCH_UP hours, so a dropped hourly run is caught by the next one. A
reminder the bot already posted (an embed whose footer is its key) is skipped, so re-runs are safe. Nobody is
pinged. Times are shown as Discord timestamps, which every reader sees in their own time zone.

Env: DISCORD_BOT_TOKEN. Usage: remind.py [--dry-run] [--now ISO-8601] [--test].
Exit 0 when done (posted, nothing due or already posted), 1 on an API error, 2 on bad input. Stdlib only.
"""
from __future__ import annotations

import argparse
import os
import sys
from datetime import date, datetime, timedelta, timezone
from pathlib import Path
from typing import Optional, Sequence

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent / "discord-issues"))
import plan  # noqa: E402
import sync  # noqa: E402

MOD_CHAT = "1564901794671493120"
CATCH_UP = timedelta(hours=3)
RECENT_MESSAGES = 50
COLOUR = 0x2BA68A
KEY_PREFIX = "release-reminder"


def reminder_key(version: str, milestone: plan.Milestone) -> str:
    return f"{KEY_PREFIX} {version}/{milestone.key}"


def due(now: datetime, releases: Sequence[tuple[str, date]]) -> list[tuple[str, date, plan.Milestone]]:
    """Every (version, publish, milestone) whose reminder window contains `now`."""
    out = []
    for version, publish in releases:
        for m in plan.MILESTONES:
            start = plan.milestone_at(publish, m) - m.remind_before
            if start <= now < start + CATCH_UP:
                out.append((version, publish, m))
    return out


def stamp(dt: datetime, style: str) -> str:
    return f"<t:{int(dt.timestamp())}:{style}>"


def timeline(publish: date) -> str:
    return "\n".join(f"{stamp(plan.milestone_at(publish, m), 'f')}  {m.title}" for m in plan.MILESTONES)


def reminder(version: str, publish: date, m: plan.Milestone) -> dict:
    at = plan.milestone_at(publish, m)
    tasks = "\n".join(f"- {t}" for t in m.tasks)
    description = f"{tasks}\n\n**Timeline**\n{timeline(publish)}" if m.key == "scope" else tasks
    return {
        "content": f"⏰ **Borealis {version}: {m.title}** {stamp(at, 'F')} ({stamp(at, 'R')})",
        "embeds": [{
            "title": f"Borealis {version}: {m.title}",
            "description": description,
            "color": COLOUR,
            "footer": {"text": reminder_key(version, m)},
        }],
        "allowed_mentions": {"parse": []},
    }


def test_message(now: datetime, releases: Sequence[tuple[str, date]]) -> dict:
    planned = "\n".join(f"- Borealis {v}: publish {stamp(plan.milestone_at(d, plan.MILESTONES[-2]), 'F')}"
                        for v, d in releases) or "- No release is scheduled."
    return {
        "content": "⏰ Release reminders are set up. Each milestone of a scheduled release is posted here.",
        "embeds": [{"title": "Scheduled releases", "description": planned, "color": COLOUR,
                    "footer": {"text": f"{KEY_PREFIX} test {now.isoformat(timespec='minutes')}"}}],
        "allowed_mentions": {"parse": []},
    }


def posted_keys(messages: Sequence[dict], bot_id: str) -> frozenset[str]:
    return frozenset(e.get("footer", {}).get("text", "") for msg in messages
                     if msg.get("author", {}).get("id") == bot_id for e in msg.get("embeds", []))


def parse_now(text: Optional[str]) -> datetime:
    if not text:
        return datetime.now(timezone.utc)
    dt = datetime.fromisoformat(text)
    if dt.tzinfo is None:
        raise ValueError("--now needs a time zone, for example 2026-11-08T18:00+02:00")
    return dt


def run(bridge: Optional[sync.Bridge], now: datetime, test: bool) -> int:
    releases = plan.load_schedule()
    messages = [test_message(now, releases)] if test else [reminder(*r) for r in due(now, releases)]
    if not messages:
        print("no reminder due")
        return 0
    if bridge is None:
        for msg in messages:
            print(f"DRY-RUN would post to #mod-chat:\n{msg['content']}\n{msg['embeds'][0]['description']}\n")
        return 0
    bot_id = bridge.discord("GET", "/users/@me")["id"]
    seen = posted_keys(bridge.discord("GET", f"/channels/{MOD_CHAT}/messages?limit={RECENT_MESSAGES}"), bot_id)
    for msg in messages:
        key = msg["embeds"][0]["footer"]["text"]
        if key in seen:
            print(f"already posted: {key}")
            continue
        bridge.discord("POST", f"/channels/{MOD_CHAT}/messages", msg)
        print(f"posted: {key}")
    return 0


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--dry-run", action="store_true", help="print the messages instead of posting them")
    parser.add_argument("--now", help="pretend it is this time (ISO 8601 with a zone), for checking")
    parser.add_argument("--test", action="store_true", help="post a one-off test message listing the schedule")
    args = parser.parse_args(argv)
    try:
        now = parse_now(args.now)
        if args.dry_run:
            return run(None, now, args.test)
        token = os.environ.get("DISCORD_BOT_TOKEN", "").strip()
        if not token:
            print("::error::DISCORD_BOT_TOKEN must be set")
            return 2
        return run(sync.Bridge(False, token, "", "TeamBorealis/borealis"), now, args.test)
    except (ValueError, KeyError) as err:
        print(f"::error::bad schedule or input: {err}")
        return 2
    except sync.ApiError as err:
        print(f"::error::{err}")
        return 1


if __name__ == "__main__":
    sys.exit(main())
