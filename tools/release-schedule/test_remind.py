"""Tests for remind.py: python3 -I -m unittest discover -s tools/release-schedule"""
from __future__ import annotations

import sys
import unittest
from datetime import date, datetime, timedelta, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import plan  # noqa: E402
import remind  # noqa: E402

RELEASE = (("1.3.0", date(2026, 11, 12)),)


def at(text: str) -> datetime:
    return datetime.fromisoformat(text)


def keys(now: datetime) -> list[str]:
    return [m.key for _, _, m in remind.due(now, RELEASE)]


class Due(unittest.TestCase):
    def test_freeze_reminder_six_hours_before(self):
        self.assertEqual(keys(at("2026-11-08T17:59+02:00")), ["freeze"])
        self.assertEqual(keys(at("2026-11-08T17:58+02:00")), [])

    def test_signed_reminder_monday_morning(self):
        self.assertEqual(keys(at("2026-11-09T09:30+02:00")), ["signed"])

    def test_window_closes_after_catch_up(self):
        self.assertEqual(keys(at("2026-11-08T20:58+02:00")), ["freeze"])
        self.assertEqual(keys(at("2026-11-08T20:59+02:00")), [])

    def test_go_no_go_and_publish_on_thursday(self):
        self.assertEqual(keys(at("2026-11-12T17:10+02:00")), ["gonogo"])
        self.assertEqual(keys(at("2026-11-12T19:45+02:00")), ["gonogo", "publish"])

    def test_every_milestone_is_reminded_once_by_hourly_runs(self):
        start = at("2026-11-05T00:07+00:00")
        seen = []
        for hour in range(9 * 24):
            seen += keys(start + timedelta(hours=hour))
        self.assertEqual(sorted(set(seen)), sorted(m.key for m in plan.MILESTONES))

    def test_nothing_without_a_scheduled_release(self):
        self.assertEqual(remind.due(at("2026-11-08T18:00+02:00"), ()), [])


class Messages(unittest.TestCase):
    def test_reminder_has_key_timestamps_and_no_pings(self):
        m = next(m for m in plan.MILESTONES if m.key == "freeze")
        msg = remind.reminder("1.3.0", date(2026, 11, 12), m)
        self.assertEqual(msg["embeds"][0]["footer"]["text"], "release-reminder 1.3.0/freeze")
        self.assertEqual(msg["allowed_mentions"], {"parse": []})
        ts = int(plan.milestone_at(date(2026, 11, 12), m).timestamp())
        self.assertIn(f"<t:{ts}:F>", msg["content"])

    def test_scope_reminder_carries_the_timeline(self):
        m = plan.MILESTONES[0]
        desc = remind.reminder("1.3.0", date(2026, 11, 12), m)["embeds"][0]["description"]
        self.assertEqual(desc.count("<t:"), len(plan.MILESTONES))

    def test_posted_keys_counts_only_the_bot(self):
        msgs = [
            {"author": {"id": "bot"}, "embeds": [{"footer": {"text": "release-reminder 1.3.0/freeze"}}]},
            {"author": {"id": "someone"}, "embeds": [{"footer": {"text": "release-reminder 1.3.0/signed"}}]},
        ]
        self.assertEqual(remind.posted_keys(msgs, "bot"), frozenset({"release-reminder 1.3.0/freeze"}))

    def test_now_needs_a_zone(self):
        with self.assertRaises(ValueError):
            remind.parse_now("2026-11-08T18:00")


class FakeBridge:
    def __init__(self, history):
        self.history = history
        self.posts = []

    def discord(self, method, path, body=None):
        if path == "/users/@me":
            return {"id": "bot"}
        if method == "GET":
            return self.history
        self.posts.append(body)
        return {}


class Run(unittest.TestCase):
    def setUp(self):
        self._load = plan.load_schedule
        plan.load_schedule = lambda: RELEASE

    def tearDown(self):
        plan.load_schedule = self._load

    def test_posts_a_due_reminder_once(self):
        bridge = FakeBridge([])
        remind.run(bridge, at("2026-11-08T18:07+02:00"), test=False)
        self.assertEqual(len(bridge.posts), 1)
        again = FakeBridge([{"author": {"id": "bot"}, "embeds": bridge.posts[0]["embeds"]}])
        remind.run(again, at("2026-11-08T19:07+02:00"), test=False)
        self.assertEqual(again.posts, [])


class Schedule(unittest.TestCase):
    def test_with_release_replaces_the_same_version_and_sorts(self):
        releases = (("1.3.1", date(2026, 11, 19)), ("1.3.0", date(2026, 11, 5)))
        out = plan.with_release(releases, "1.3.0", date(2026, 11, 12))
        self.assertEqual(out, (("1.3.0", date(2026, 11, 12)), ("1.3.1", date(2026, 11, 19))))
        self.assertEqual(len(releases), 2)  # input untouched

    def test_load_schedule_checks_thursdays(self):
        import json
        import tempfile
        path = Path(tempfile.mkdtemp()) / "schedule.json"
        path.write_text(json.dumps({"releases": [{"version": "1.3.0", "publish": "2026-11-11"}]}))
        with self.assertRaises(ValueError):
            plan.load_schedule(path)


if __name__ == "__main__":
    unittest.main()
