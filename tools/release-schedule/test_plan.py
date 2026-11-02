"""Tests for plan.py: python3 -I -m unittest discover -s tools/release-schedule"""
from __future__ import annotations

import sys
import unittest
from datetime import date, datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import plan  # noqa: E402


def by_key(key: str) -> plan.Milestone:
    return next(m for m in plan.MILESTONES if m.key == key)


class PublishTime(unittest.TestCase):
    def test_publish_is_20_local_18_utc_in_winter(self):
        dt = plan.milestone_at(date(2026, 11, 12), by_key("publish"))
        self.assertEqual(dt.astimezone(timezone.utc).hour, 18)

    def test_publish_is_17_utc_in_summer(self):
        dt = plan.milestone_at(date(2027, 7, 1), by_key("publish"))
        self.assertEqual(dt.astimezone(timezone.utc).hour, 17)

    def test_signed_build_is_monday_evening(self):
        dt = plan.milestone_at(date(2026, 11, 12), by_key("signed"))
        self.assertEqual((dt.date(), dt.hour), (date(2026, 11, 9), 20))

    def test_milestones_are_in_time_order(self):
        times = [plan.milestone_at(date(2026, 11, 12), m) for m in plan.MILESTONES]
        self.assertEqual(times, sorted(times))


class NextPublishDate(unittest.TestCase):
    def test_monday_after_freeze_skips_to_next_week(self):
        now = datetime(2026, 11, 2, 9, 0, tzinfo=timezone.utc)  # Monday; Sunday's freeze has passed
        self.assertEqual(plan.next_publish_date(now), date(2026, 11, 12))

    def test_saturday_before_freeze_takes_this_week(self):
        now = datetime(2026, 11, 7, 12, 0, tzinfo=timezone.utc)
        self.assertEqual(plan.next_publish_date(now), date(2026, 11, 12))

    def test_after_publish_on_thursday_moves_on(self):
        now = datetime(2026, 11, 12, 19, 0, tzinfo=timezone.utc)
        self.assertEqual(plan.next_publish_date(now), date(2026, 11, 19))


class Validation(unittest.TestCase):
    def test_rejects_non_thursday(self):
        with self.assertRaises(ValueError):
            plan.validate_publish(date(2026, 11, 11))

    def test_rejects_bad_version(self):
        for bad in ("1.3", "v1.3.0", "1.3.0-beta", ""):
            with self.assertRaises(ValueError):
                plan.parse_version(bad)

    def test_release_kind(self):
        self.assertTrue(plan.release_kind("1.3.0").startswith("major"))
        self.assertTrue(plan.release_kind("1.2.10").startswith("standard"))


class Rendering(unittest.TestCase):
    def test_checklist_has_every_milestone_and_task(self):
        text = plan.render_checklist("1.3.0", date(2026, 11, 12))
        for m in plan.MILESTONES:
            self.assertIn(f"## {m.title}:", text)
        self.assertEqual(text.count("- [ ] "), sum(len(m.tasks) for m in plan.MILESTONES))

    def test_ics_has_one_event_per_milestone_with_utc_times(self):
        ics = plan.render_ics("1.3.0", date(2026, 11, 12), datetime(2026, 11, 2, tzinfo=timezone.utc))
        self.assertEqual(ics.count("BEGIN:VEVENT"), len(plan.MILESTONES))
        self.assertIn("DTSTART:20261112T180000Z", ics)
        self.assertTrue(all(line.endswith("\r") or line == "" for line in ics.split("\n")))

    def test_main_refuses_to_overwrite(self):
        tmp = Path(self.id().replace(".", "_"))
        old = plan.RELEASES_DIR
        try:
            plan.RELEASES_DIR = Path(__import__("tempfile").mkdtemp()) / tmp
            self.assertEqual(plan.main(["1.3.0", "--publish", "2026-11-12", "--write"]), 0)
            self.assertEqual(plan.main(["1.3.0", "--publish", "2026-11-12", "--write"]), 2)
            self.assertEqual(plan.main(["1.3.0", "--publish", "2026-11-12", "--write", "--force"]), 0)
        finally:
            plan.RELEASES_DIR = old


if __name__ == "__main__":
    unittest.main()
