"""Tick tests for auto.py on the scenario files (dry runs, no network, no app checkout):
python3 -I -m unittest discover -s tools/release-schedule"""
from __future__ import annotations

import base64
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
import play  # noqa: E402
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
    return tick_from({**json.loads((HERE / "scenarios" / f"{name}.json").read_text()), **overrides})


def tick_from(facts: dict):
    lines: list[str] = []
    log = lines.append
    discord = clients.Discord(None, True, log, frozenset(facts.get("posted", [])))
    t = auto.Tick(remind.parse_now(facts["now"]), True, facts.get("hold", ""),
                  clients.GitHub("", auto.REPO, True, log), None, discord,
                  scenario.FakeAsc(facts["asc"], log) if "asc" in facts else None,
                  scenario.FakeSite(facts["website_repo"], log) if "website_repo" in facts else None, facts,
                  play=scenario.FakePlay(facts["play"], log) if facts.get("play") is not None else None)
    t.git = FakeGit()  # type: ignore[assignment]
    return t, discord, lines


def run(name: str, **overrides):
    return run_from(tick(name, **overrides), SCHEDULE)


def run_from(ticked, schedule):
    t, discord, lines = ticked
    auto.log = lines.append  # module-level log of the tick
    try:
        status = t.run(schedule)
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

    def test_monday_reruns_a_starved_release_workflow_instead_of_calling_it_late(self):
        starved = {"id": 41, "status": "completed", "conclusion": "cancelled", "run_attempt": 1,
                   "created_at": "2026-11-08T22:00:00Z", "html_url": "https://example.test/runs/41"}
        status, keys, out = run("normal-1-signed", release={"draft": True, "assets": []},
                                **{"ci:release.yml": "cancelled", "release_runs": [starved]})
        self.assertEqual((status, keys), (0, ["signed-rerun/2"]))
        self.assertIn("would: re-run the failed jobs of release.yml run 41", out)

    def test_monday_waits_for_a_running_release_workflow(self):
        running = {"id": 41, "status": "in_progress", "conclusion": None, "run_attempt": 2, "created_at": "1"}
        status, keys, out = run("normal-1-signed", release={"draft": True, "assets": []},
                                **{"ci:release.yml": "pending", "release_runs": [running]})
        self.assertEqual((status, keys), (0, []))
        self.assertIn("still running", out)

    def test_monday_calls_the_builds_late_once_the_reruns_are_spent(self):
        spent = {"id": 41, "status": "completed", "conclusion": "failure", "run_attempt": 3, "created_at": "1"}
        status, keys, out = run("normal-1-signed", release={"draft": True, "assets": []},
                                **{"ci:release.yml": "failure", "release_runs": [spent]})
        self.assertEqual((status, keys), (0, ["signed-late"]))
        self.assertNotIn("re-run the failed jobs of", out)

    def test_tuesday_submits_ios_and_posts_the_notes(self):
        status, keys, out = run("normal-2-submit")
        self.assertEqual((status, keys), (0, ["ios-submitted", "notes", "play-uploaded"]))
        for step in ("create iOS version 1.3.0", "attach build 1003000", "set What's New", "submit for review",
                     "create release-1.3.0"):
            self.assertIn(step, out)
        self.assertIn("Silence = approval", out)

    def test_tuesday_waits_for_the_ios_build(self):
        status, keys, out = run("normal-2-submit", asc={"versions": [], "builds": []})
        self.assertEqual((status, keys), (0, ["ios-wait", "notes", "play-uploaded"]))
        self.assertNotIn("submit for review", out)

    def test_tuesday_rolls_the_play_bundle_out_on_alpha_only(self):
        status, keys, out = run("normal-2-submit")
        self.assertIn("would: upload Borealis_1.3.0_play.aab to Play", out)
        self.assertIn("would: put 1.3.0 (versionCode 1003000) on alpha, rolled out to its testers", out)
        self.assertIn("would: commit the Play edit for 1.3.0", out)
        self.assertIn("**Play (closed testing):** would roll out versionCode 1003000 on alpha", out)
        self.assertNotIn("production", out)

    def test_play_notes_with_a_cs1_finding_are_replaced(self):
        def notes_sent(text: str) -> str:
            t, _, _ = tick("normal-2-submit")
            sent: list[str] = []
            t.play.release_to_testers = lambda aab, v, code, notes: sent.append(notes) or "ok"  # type: ignore
            t.submit_play("1.3.0", date(2026, 11, 12), t.tag("1.3.0"), text)
            return sent[0]
        self.assertEqual(notes_sent("Import from 5etools now.\n"), play.release_notes("Borealis 1.3.0."))
        self.assertEqual(notes_sent("Dice rolls.\n"), play.release_notes("Dice rolls."))

    def test_tuesday_play_waits_for_the_bundle_artifact(self):
        status, keys, out = run("normal-2-submit", play_artifact=None)
        self.assertEqual((status, keys), (0, ["ios-submitted", "notes", "play-wait"]))
        self.assertIn("release.yml artifact `play-bundle-1.3.0-5ca1ab1e0000`", out)

    def test_tuesday_play_leaves_a_live_release_alone(self):
        live = {"track": {"releases": [{"versionCodes": ["1003000"], "status": "completed"}]}, "bundles": []}
        status, keys, out = run("normal-2-submit", play=live)
        self.assertIn("play-uploaded", keys)
        self.assertIn("versionCode 1003000 is already on alpha (completed)", out)
        self.assertNotIn("would: upload", out)

    def test_tuesday_without_the_play_key_asks_for_a_hand_once(self):
        status, keys, out = run("normal-2-submit", play=None)
        self.assertEqual((status, keys), (0, ["ios-submitted", "notes", "play-manual"]))
        self.assertIn("PLAY_SERVICE_ACCOUNT_JSON", out)

    def test_a_held_release_does_not_reach_play(self):
        status, keys, out = run("normal-2-submit", hold="1.3.0")
        self.assertEqual(keys, ["hold/2026-11-12"])
        self.assertNotIn("Play", out.replace("Play tab", ""))

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


UPDATER_MISSING = "in-app updates: the draft lacks latest.json, Borealis_1.3.0_aarch64.app.tar.gz"


class UpdaterFiles(unittest.TestCase):
    def test_monday_counts_the_updater_files_when_signing_is_on(self):
        status, keys, out = run("normal-1-signed")
        self.assertEqual((status, keys), (0, ["signed"]))
        self.assertIn("every installer (7); with the in-app update files, 12 files.", out)
        self.assertNotIn("Warning", out)

    def test_monday_warns_about_missing_updater_files_but_is_ready(self):
        rel = json.loads((HERE / "scenarios" / "updater-missing-gonogo.json").read_text())["release"]
        status, keys, out = run("normal-1-signed", release=rel)
        self.assertEqual((status, keys), (0, ["signed"]))
        self.assertIn(UPDATER_MISSING, out)
        self.assertIn("::warning::1.3.0", out)

    def test_go_no_go_is_go_with_the_warning_in_the_note(self):
        status, keys, out = run("updater-missing-gonogo")
        self.assertEqual((status, keys), (0, ["go/2026-11-12"]))
        self.assertIn(f"**Warning:** {UPDATER_MISSING}", out)

    def test_no_updater_files_are_expected_while_signing_is_off(self):
        status, keys, out = run("updater-missing-gonogo", updater_signing=False)
        self.assertEqual((status, keys), (0, ["go/2026-11-12"]))
        self.assertNotIn("in-app updates", out)

    def test_a_no_go_note_also_reports_the_warning(self):
        status, keys, out = run("updater-missing-gonogo", ios_state="WAITING_FOR_REVIEW")
        self.assertEqual(keys, ["nogo/2026-11-12"])
        self.assertIn(f"**Warning (not blocking):** {UPDATER_MISSING}", out)

    def test_signing_is_read_from_the_release_commit_without_a_scenario_fact(self):
        facts = json.loads((HERE / "scenarios" / "updater-missing-gonogo.json").read_text())
        del facts["updater_signing"]
        t, _, _ = tick("updater-missing-gonogo")
        t.facts = facts
        self.assertFalse(t.updater_signing("1.3.0"))  # FakeGit's tauri.conf.json has no updater key
        t, _, _ = tick("updater-missing-gonogo")
        t.facts, t.git = facts, KeyedGit()  # type: ignore[assignment]
        self.assertTrue(t.updater_signing("1.3.0"))
        self.assertEqual(t.git.asked, ["5ca1ab1e00000000000000000000000000000000:src-tauri/tauri.conf.json"])


SCHEDULE_150 = (("1.5.0", date(2026, 11, 12)),)


def as_150(name: str) -> dict:
    """A 1.3.0 scenario retold as 1.5.0 (same week, same files), for the version-aware updater check (B-215)."""
    text = (HERE / "scenarios" / f"{name}.json").read_text()
    return json.loads(text.replace("1.3.0", "1.5.0").replace("1003000", "1005000"))


def run_150(name: str, **overrides):
    return run_from(tick_from({**as_150(name), **overrides}), SCHEDULE_150)


class UpdaterFilesFrom150(unittest.TestCase):
    """B-215 (decisions FU1, RL11): from 1.4.1 a missing latest.json or Windows/AppImage .sig fails the release check."""

    def test_the_scenario_retold_as_1_5_0_is_a_normal_week(self):
        self.assertEqual(run_150("normal-3-gonogo")[:2], (0, ["go/2026-11-12"]))
        self.assertEqual(run_150("normal-1-signed")[:2], (0, ["signed"]))

    def test_go_no_go_is_no_go_without_the_feed_and_still_warns_about_the_mac_archive(self):
        status, keys, out = run_150("updater-missing-gonogo")
        self.assertEqual((status, keys), (0, ["nogo/2026-11-12"]))
        self.assertIn("- in-app updates: the draft lacks latest.json (required from 1.4.1)", out)
        self.assertIn("**Warning (not blocking):** in-app updates: the draft lacks Borealis_1.5.0_aarch64.app.tar.gz, "
                      "Borealis_1.5.0_aarch64.app.tar.gz.sig.", out)

    def test_a_missing_installer_signature_blocks_too(self):
        rel = as_150("normal-3-gonogo")["release"]
        rel = {**rel, "assets": [a for a in rel["assets"] if a["name"] != "Borealis_1.5.0_amd64.AppImage.sig"]}
        status, keys, out = run_150("normal-3-gonogo", release=rel)
        self.assertEqual(keys, ["nogo/2026-11-12"])
        self.assertIn("the draft lacks Borealis_1.5.0_amd64.AppImage.sig (required from 1.4.1)", out)

    def test_monday_calls_a_draft_without_the_feed_not_ready(self):
        done = {"id": 41, "status": "completed", "conclusion": "success", "run_attempt": 1, "created_at": "1"}
        rel = as_150("updater-missing-gonogo")["release"]
        status, keys, out = run_150("normal-1-signed", release=rel, release_runs=[done])
        self.assertEqual((status, keys), (0, ["signed-late"]))
        self.assertIn("signed builds not ready: the draft lacks latest.json", out)

    def test_a_missing_mac_archive_alone_is_go_with_the_warning(self):
        rel = as_150("normal-3-gonogo")["release"]
        rel = {**rel, "assets": [a for a in rel["assets"] if "app.tar.gz" not in a["name"]]}
        status, keys, out = run_150("normal-3-gonogo", release=rel)
        self.assertEqual((status, keys), (0, ["go/2026-11-12"]))
        self.assertIn("**Warning:** in-app updates: the draft lacks Borealis_1.5.0_aarch64.app.tar.gz", out)


class KeyedGit(FakeGit):
    """A release commit whose tauri.conf.json has a real updater public key."""

    def __init__(self):
        self.asked: list[str] = []

    def show(self, rev: str, path: str) -> Optional[str]:
        self.asked.append(f"{rev}:{path}")
        key = base64.b64encode(b"untrusted comment: minisign public key: 1\nRWQ\n").decode()
        return json.dumps({"plugins": {"updater": {"pubkey": key}}})


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


class UntaggedDraft(unittest.TestCase):
    """A draft can lose its tag (GitHub shows `untagged-...`): it is still found and its tag is put back."""

    DRAFT = {"id": 7, "draft": True, "name": "Borealis 1.3.0", "tag_name": "untagged-3486686083108756938a"}

    def test_an_untagged_draft_is_found_by_its_title(self):
        other = {"id": 8, "draft": True, "name": "Borealis 1.2.2", "tag_name": "untagged-ffff"}
        self.assertIs(clients.find_release([other, self.DRAFT], "v1.3.0"), self.DRAFT)

    def test_the_tagged_release_wins_and_a_published_one_is_never_guessed(self):
        tagged = {"id": 9, "draft": True, "name": "Borealis 1.3.0", "tag_name": "v1.3.0"}
        self.assertIs(clients.find_release([self.DRAFT, tagged], "v1.3.0"), tagged)
        published = {**self.DRAFT, "draft": False}
        self.assertIsNone(clients.find_release([published], "v1.3.0"))

    def test_every_draft_edit_sends_the_tag(self):
        sent: list[dict] = []
        for name in ("normal-1-signed", "normal-4-publish"):
            t, _, lines = tick(name)
            t.gh.write = lambda method, path, body, what, sent=sent: sent.append(body)  # type: ignore[method-assign]
            run_from((t, _, lines), SCHEDULE)
        edits = [b for b in sent if "body" in b and "name" in b]
        self.assertGreaterEqual(len(edits), 2)
        self.assertTrue(all(b.get("tag_name") == "v1.3.0" for b in edits), edits)

    def test_a_draft_that_lost_its_tag_gets_it_back(self):
        t, _, lines = tick("normal-1-signed")
        sent: list[dict] = []
        t.gh.write = lambda method, path, body, what: sent.append(body)  # type: ignore[method-assign]
        rel = {**t.release("1.3.0"), "tag_name": "untagged-1"}
        t.sync_draft("1.3.0", rel)
        self.assertEqual([b.get("tag_name") for b in sent], ["v1.3.0"])


if __name__ == "__main__":
    unittest.main()
