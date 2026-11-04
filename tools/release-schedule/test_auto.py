"""Tick tests for auto.py on the scenario files (dry runs, no network, no app checkout):
python3 -I -m unittest discover -s tools/release-schedule"""
from __future__ import annotations

import json
import sys
import unittest
from datetime import date
from pathlib import Path
from typing import Optional

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import auto  # noqa: E402
import clients  # noqa: E402
import remind  # noqa: E402
import scenario  # noqa: E402
import test_bump  # noqa: E402

SCHEDULE = (("1.2.2", date(2026, 11, 5)), ("1.3.0", date(2026, 11, 12)), ("1.4.0", date(2026, 11, 19)))


class FakeGit:
    """origin/main = the test_bump texts at 1.2.2; nothing is ever pushed."""

    def fetch_main(self) -> str:
        return "9" * 40

    def show(self, rev: str, path: str) -> Optional[str]:
        return test_bump.TEXTS.get(path)

    def commit_to_main(self, message: str, change) -> str:
        auto.log(f"DRY-RUN would: push \"{message}\" to main")
        return "dry-run"


def tick(name: str, **overrides):
    facts = {**json.loads((HERE / "scenarios" / f"{name}.json").read_text()), **overrides}
    lines: list[str] = []
    log = lines.append
    discord = clients.Discord(None, True, log, frozenset(facts.get("posted", [])))
    t = auto.Tick(remind.parse_now(facts["now"]), True, facts.get("hold", ""),
                  clients.GitHub("", auto.REPO, True, log), None, discord,
                  scenario.FakeAsc(facts["asc"], log) if "asc" in facts else None,
                  scenario.FakeSite(facts["website_repo"], log) if "website_repo" in facts else None, facts)
    t.git = FakeGit()  # type: ignore[assignment]
    return t, discord, lines


def run(name: str, **overrides):
    t, discord, lines = tick(name, **overrides)
    auto.log = lines.append  # module-level log of the tick
    try:
        status = t.run(SCHEDULE)
    finally:
        auto.log = print
    new_keys = sorted(k.split("/", 1)[1] for k in discord.posted() - frozenset(t.facts.get("posted", [])))
    return status, new_keys, "\n".join(lines)


class NormalWeek(unittest.TestCase):
    def test_freeze_bumps_tags_and_starts_the_builds(self):
        status, keys, out = run("red-ci-freeze", **{"ci:ci.yml": "success"})
        self.assertEqual((status, keys), (0, ["frozen"]))
        self.assertIn('commit "chore: release 1.3.0"', out)
        self.assertIn("build number 1003000", out)
        for wf in ("ci.yml on v1.3.0", "acceptance.yml on v1.3.0", 'release.yml on v1.3.0 with {"tag": "v1.3.0"}'):
            self.assertIn(f"would: run {wf}", out)

    def test_monday_signed_builds(self):
        status, keys, out = run("normal-1-signed")
        self.assertEqual((status, keys), (0, ["signed"]))
        self.assertIn("set the v1.3.0 draft's title and notes", out)

    def test_tuesday_submits_ios_and_posts_the_notes(self):
        status, keys, out = run("normal-2-submit")
        self.assertEqual((status, keys), (0, ["ios-submitted", "notes"]))
        for step in ("create iOS version 1.3.0", "attach build 1003000", "set What's New", "submit for review",
                     "create release-1.3.0"):
            self.assertIn(step, out)
        self.assertIn("Silence = approval", out)

    def test_tuesday_waits_for_the_ios_build(self):
        status, keys, out = run("normal-2-submit", asc={"versions": [], "builds": []})
        self.assertEqual((status, keys), (0, ["ios-wait", "notes"]))
        self.assertNotIn("submit for review", out)

    def test_thursday_go(self):
        self.assertEqual(run("normal-3-gonogo")[:2], (0, ["go/2026-11-12"]))

    def test_thursday_publish(self):
        status, keys, out = run("normal-4-publish")
        self.assertEqual((status, keys), (0, ["published"]))
        for step in ("publish v1.3.0 as the latest release", "release iOS 1.3.0", "merge it into main",
                     "IndexNow", 'run discord-issues.yml on main with {"announce_tag": "v1.3.0"}'):
            self.assertIn(step, out)

    def test_published_release_is_left_alone(self):
        posted = json.loads((HERE / "scenarios" / "normal-4-publish.json").read_text())["posted"]
        self.assertEqual(run("normal-4-publish", posted=posted + ["release-auto 1.3.0/published"])[:2], (0, []))


class NoGo(unittest.TestCase):
    def test_apple_not_approved_slips_and_folds(self):
        status, keys, out = run("apple-not-approved")
        self.assertEqual((status, keys), (0, ["nogo/2026-11-12"]))
        self.assertIn("App Store: WAITING_FOR_REVIEW", out)
        self.assertIn("folds into 1.4.0", out)

    def test_hold_at_go_no_go(self):
        status, keys, out = run("hold-gonogo")
        self.assertEqual(keys, ["nogo/2026-11-12"])
        self.assertIn("on hold (RELEASE_HOLD)", out)

    def test_hold_at_freeze_does_nothing(self):
        status, keys, out = run("hold-freeze")
        self.assertEqual((status, keys), (0, ["hold/2026-11-12"]))
        self.assertNotIn("would: run", out)
        self.assertNotIn("commit", out)

    def test_red_ci_blocks_the_freeze(self):
        status, keys, out = run("red-ci-freeze")
        self.assertEqual(keys, ["freeze-blocked/badc0de00000"])
        self.assertNotIn("would: run", out)

    def test_partly_published_release_is_finished_not_slipped(self):
        published = {**json.loads((HERE / "scenarios" / "normal-4-publish.json").read_text())["release"],
                     "draft": False}
        status, keys, out = run("normal-4-publish", release=published, asc={
            "versions": [{"id": "v", "attributes": {"versionString": "1.3.0", "appStoreState": "WAITING_FOR_REVIEW"}}]})
        self.assertEqual(status, 1)
        self.assertEqual(keys, ["error/publish/2026-11-12"])
        self.assertNotIn("nogo", out)


class Handmade(unittest.TestCase):
    def test_a_release_tagged_by_hand_is_skipped(self):
        status, keys, out = run("normal-3-gonogo", tag={"sha": "cb131e7" + "0" * 33, "date": "2026-11-01T13:42:24+00:00"})
        self.assertEqual((status, keys), (0, []))
        self.assertIn("handled by hand; skipped", out)

    def test_the_plan_does_not_wake_up_for_a_release_tagged_by_hand(self):
        t, _, lines = tick("normal-3-gonogo", tag={"sha": "cb131e7" + "0" * 33, "date": "2026-11-01T13:42:24+00:00"})
        auto.log = lines.append
        try:
            self.assertFalse(t.plan(SCHEDULE).run)
        finally:
            auto.log = print
        self.assertIn("1.3.0: tagged by hand before the takeover; left alone", lines)


if __name__ == "__main__":
    unittest.main()
