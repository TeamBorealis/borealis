"""Tests for decide.py: python3 -I -m unittest discover -s tools/release-schedule"""
from __future__ import annotations

import base64
import json
import sys
import unittest
from datetime import date, datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import decide  # noqa: E402

PUBLISH = date(2026, 11, 12)
SCHEDULE = (("1.2.2", date(2026, 11, 5)), ("1.3.0", PUBLISH), ("1.4.0", date(2026, 11, 19)))


def at(text: str) -> datetime:
    return datetime.fromisoformat(text)


def go(**kw) -> decide.GoFacts:
    base = dict(version="1.3.0", tag=True, release="draft", assets=decide.expected_assets("1.3.0"),
                tag_ci="success", tag_acceptance="success", ios_state="PENDING_DEVELOPER_RELEASE", website="ready")
    return decide.GoFacts(**{**base, **kw})


class Phases(unittest.TestCase):
    def test_prep_starts_at_the_freeze(self):
        self.assertIsNone(decide.phase(at("2026-11-08T23:58+02:00"), PUBLISH))
        self.assertEqual(decide.phase(at("2026-11-08T23:59+02:00"), PUBLISH), "prep")
        self.assertEqual(decide.phase(at("2026-11-12T17:59+02:00"), PUBLISH), "prep")

    def test_decide_then_publish_then_over(self):
        self.assertEqual(decide.phase(at("2026-11-12T18:00+02:00"), PUBLISH), "decide")
        self.assertEqual(decide.phase(at("2026-11-12T20:00+02:00"), PUBLISH), "publish")
        self.assertEqual(decide.phase(at("2026-11-13T19:59+02:00"), PUBLISH), "publish")
        self.assertIsNone(decide.phase(at("2026-11-13T20:00+02:00"), PUBLISH))

    def test_publish_time_follows_israel_daylight_saving(self):
        summer = date(2027, 6, 10)
        self.assertEqual(decide.phase(at("2027-06-10T17:00+00:00"), summer), "publish")
        self.assertEqual(decide.phase(at("2027-06-10T16:59+00:00"), summer), "decide")

    def test_active_picks_only_the_release_in_its_week(self):
        found = decide.active(at("2026-11-10T12:00+02:00"), SCHEDULE)
        self.assertEqual(found, [("1.3.0", PUBLISH, "prep")])


class Hold(unittest.TestCase):
    def test_hold_names_versions_or_all(self):
        self.assertTrue(decide.held("1.3.0", "1.3.0"))
        self.assertTrue(decide.held("v1.3.0, 1.4.0", "1.4.0"))
        self.assertTrue(decide.held("all", "1.3.0"))
        self.assertTrue(decide.held("*", "1.3.0"))
        self.assertFalse(decide.held("", "1.3.0"))
        self.assertFalse(decide.held("1.3.1", "1.3.0"))


class Handmade(unittest.TestCase):
    def test_tags_before_the_takeover_are_handmade(self):
        self.assertTrue(decide.handmade(at("2026-11-01T13:42:24+00:00")))  # v1.2.2
        self.assertFalse(decide.handmade(at("2026-11-08T23:59:40+02:00")))
        self.assertFalse(decide.handmade(None))


class Freeze(unittest.TestCase):
    def test_green_main_is_bumped_and_tagged(self):
        self.assertEqual(decide.freeze_decision("1.2.2", "1.3.0", "success", False).action, "bump")

    def test_red_or_missing_ci_blocks(self):
        self.assertEqual(decide.freeze_decision("1.2.2", "1.3.0", "failure", False).action, "red")
        self.assertEqual(decide.freeze_decision("1.2.2", "1.3.0", "none", False).action, "red")

    def test_running_ci_waits(self):
        self.assertEqual(decide.freeze_decision("1.2.2", "1.3.0", "pending", False).action, "wait")

    def test_an_existing_tag_is_done(self):
        self.assertEqual(decide.freeze_decision("1.3.0", "1.3.0", "failure", True).action, "done")

    def test_main_already_bumped_is_only_tagged(self):
        self.assertEqual(decide.freeze_decision("1.3.0", "1.3.0", "success", False).action, "tag")

    def test_main_past_the_version_is_an_error(self):
        self.assertEqual(decide.freeze_decision("1.4.0", "1.3.0", "success", False).action, "error")


class Ci(unittest.TestCase):
    def test_any_success_counts(self):
        runs = [{"status": "completed", "conclusion": "failure", "created_at": "2"},
                {"status": "completed", "conclusion": "success", "created_at": "1"}]
        self.assertEqual(decide.ci_status(runs), "success")

    def test_pending_none_and_newest_failure(self):
        self.assertEqual(decide.ci_status([{"status": "in_progress", "conclusion": None}]), "pending")
        self.assertEqual(decide.ci_status([]), "none")
        runs = [{"status": "completed", "conclusion": "cancelled", "created_at": "1"},
                {"status": "completed", "conclusion": "failure", "created_at": "2"}]
        self.assertEqual(decide.ci_status(runs), "failure")


class SignedRerun(unittest.TestCase):
    def run_(self, conclusion, attempt=1, status="completed"):
        return {"id": 7, "status": status, "conclusion": conclusion, "run_attempt": attempt, "created_at": "1"}

    def test_failed_or_starved_runs_are_rerun_until_the_attempts_run_out(self):
        for conclusion in ("failure", "cancelled", "timed_out", "startup_failure"):
            self.assertEqual(decide.signed_rerun([self.run_(conclusion)]), "rerun", conclusion)
        self.assertEqual(decide.signed_rerun([self.run_("cancelled", decide.RELEASE_ATTEMPTS - 1)]), "rerun")
        self.assertEqual(decide.signed_rerun([self.run_("cancelled", decide.RELEASE_ATTEMPTS)]), "late")

    def test_a_running_run_waits_and_no_run_or_a_pass_is_late(self):
        self.assertEqual(decide.signed_rerun([self.run_(None, status="queued")]), "wait")
        self.assertEqual(decide.signed_rerun([]), "late")
        self.assertEqual(decide.signed_rerun([self.run_("success")]), "late")  # passed, yet the draft lacks files

    def test_only_the_newest_run_counts(self):
        runs = [{**self.run_("cancelled", 3), "created_at": "1"}, {**self.run_("failure", 1), "id": 8, "created_at": "2"}]
        self.assertEqual(decide.signed_rerun(runs), "rerun")
        self.assertEqual(decide.newest(runs)["id"], 8)


class GoNoGo(unittest.TestCase):
    def test_everything_ready_is_go(self):
        self.assertEqual(decide.go_problems(go()), [])

    def test_already_released_on_the_app_store_counts_as_approved(self):
        self.assertEqual(decide.go_problems(go(ios_state="READY_FOR_SALE")), [])

    def test_apple_not_approved_is_no_go(self):
        problems = decide.go_problems(go(ios_state="WAITING_FOR_REVIEW"))
        self.assertEqual(len(problems), 1)
        self.assertIn("App Store: WAITING_FOR_REVIEW", problems[0])

    def test_hold_missing_dmg_red_ci_and_cs1_are_each_no_go(self):
        assets = tuple(a for a in decide.expected_assets("1.3.0") if not a.endswith(".dmg"))
        problems = decide.go_problems(go(held=True, assets=assets, tag_ci="failure", notes_findings=("x",),
                                         website="no release-1.3.0 branch"))
        self.assertEqual(len(problems), 5)
        self.assertIn("Borealis_1.3.0_aarch64.dmg", problems[1])

    def test_nothing_frozen(self):
        problems = decide.go_problems(decide.GoFacts(version="1.3.0"))
        self.assertIn("not frozen: no tag v1.3.0", problems)
        self.assertIn("no draft release", problems)


def conf(pubkey: object) -> str:
    return json.dumps({"version": "1.4.0", "plugins": {"updater": {"pubkey": pubkey, "endpoints": ["x"]}}})


REAL_PUBKEY = base64.b64encode(b"untrusted comment: minisign public key: D0529EC964159C23\n"
                               b"RWQjnBVkyZ5S0KNR4EGA7/f4UAIPD8tVjdhlELmBN+KLaywDUfnI5S2x\n").decode()
PLACEHOLDER = "PLACEHOLDER-NOT-A-KEY: replace with the updater public key"
APP_CONF = Path(__file__).resolve().parents[2] / decide.TAURI_CONF


class UpdaterSigning(unittest.TestCase):
    def test_a_minisign_public_key_turns_signing_on(self):
        self.assertTrue(decide.updater_signing(conf(REAL_PUBKEY)))

    def test_the_placeholder_or_no_key_is_off(self):
        self.assertFalse(decide.updater_signing(conf(PLACEHOLDER)))
        self.assertFalse(decide.updater_signing(conf("")))
        self.assertFalse(decide.updater_signing(json.dumps({"version": "1.4.0"})))
        self.assertFalse(decide.updater_signing(json.dumps({"plugins": {"updater": None}})))
        self.assertFalse(decide.updater_signing(None))

    def test_base64_that_is_not_a_minisign_key_or_bad_json_is_off(self):
        self.assertFalse(decide.updater_signing(conf(base64.b64encode(b"untrusted comment: other\nRW").decode())))
        self.assertFalse(decide.updater_signing(conf(base64.b64encode(b"\xff\xfe").decode())))
        self.assertFalse(decide.updater_signing(conf(42)))
        self.assertFalse(decide.updater_signing("{not json"))
        self.assertFalse(decide.updater_signing("[]"))

    @unittest.skipUnless(APP_CONF.is_file(), "the app's tauri.conf.json is not in this (sparse) checkout")
    def test_the_committed_app_config_agrees_with_updater_sh(self):
        text = APP_CONF.read_text(encoding="utf-8")
        pubkey = json.loads(text)["plugins"]["updater"]["pubkey"]
        self.assertEqual(decide.updater_signing(text), not pubkey.startswith("PLACEHOLDER"))


class UpdaterAssets(unittest.TestCase):
    def test_the_feed_signatures_and_mac_archive(self):
        self.assertEqual(decide.updater_assets("1.4.0"), (
            "latest.json", "Borealis_1.4.0_x64-setup.exe.sig", "Borealis_1.4.0_amd64.AppImage.sig",
            "Borealis_1.4.0_aarch64.app.tar.gz", "Borealis_1.4.0_aarch64.app.tar.gz.sig"))

    def test_none_expected_while_signing_is_off(self):
        self.assertEqual(decide.missing_updater_assets("1.4.0", [], signing=False), [])
        self.assertIsNone(decide.updater_warning([]))

    def test_missing_files_are_listed_in_order(self):
        have = [*decide.expected_assets("1.4.0"), "Borealis_1.4.0_x64-setup.exe.sig",
                "Borealis_1.4.0_amd64.AppImage.sig"]
        self.assertEqual(decide.missing_updater_assets("1.4.0", have, signing=True),
                         ["latest.json", "Borealis_1.4.0_aarch64.app.tar.gz",
                          "Borealis_1.4.0_aarch64.app.tar.gz.sig"])
        everything = [*have, *decide.updater_assets("1.4.0")]
        self.assertEqual(decide.missing_updater_assets("1.4.0", everything, signing=True), [])

    def test_the_updater_files_are_not_installers(self):
        self.assertFalse(set(decide.updater_assets("1.4.0")) & set(decide.expected_assets("1.4.0")))


class GoWarnings(unittest.TestCase):
    def test_missing_updater_files_warn_but_never_block(self):
        facts = go(updater=True)
        self.assertEqual(decide.go_problems(facts), [])
        warnings = decide.go_warnings(facts)
        self.assertEqual(len(warnings), 1)
        self.assertIn("latest.json", warnings[0])
        self.assertIn("Not blocking", warnings[0])

    def test_a_complete_updater_draft_or_signing_off_has_no_warning(self):
        assets = (*decide.expected_assets("1.3.0"), *decide.updater_assets("1.3.0"))
        self.assertEqual(decide.go_warnings(go(updater=True, assets=assets)), [])
        self.assertEqual(decide.go_warnings(go(updater=False)), [])

    def test_no_draft_is_a_problem_not_an_updater_warning(self):
        facts = go(updater=True, release=None, assets=())
        self.assertEqual(decide.go_warnings(facts), [])
        self.assertIn("no draft release", decide.go_problems(facts))


class Slip(unittest.TestCase):
    def test_slips_a_week(self):
        schedule = (("1.3.0", PUBLISH),)
        new, folded = decide.slip(schedule, "1.3.0", PUBLISH)
        self.assertEqual(new, (("1.3.0", date(2026, 11, 19)),))
        self.assertIsNone(folded)

    def test_folds_into_the_release_planned_that_thursday(self):
        new, folded = decide.slip(SCHEDULE, "1.3.0", PUBLISH)
        self.assertEqual(folded, "1.4.0")
        self.assertEqual([v for v, _ in new], ["1.2.2", "1.4.0"])


class Supersede(unittest.TestCase):
    def test_only_a_folded_automated_version_in_review(self):
        after = at("2026-11-08T23:59+02:00")
        self.assertTrue(decide.auto_supersede("1.3.0", {"1.4.0"}, after, "WAITING_FOR_REVIEW"))
        self.assertFalse(decide.auto_supersede("1.3.0", {"1.3.0", "1.4.0"}, after, "WAITING_FOR_REVIEW"))
        self.assertFalse(decide.auto_supersede("1.2.2", {"1.3.0"}, at("2026-11-01T13:42+00:00"), "IN_REVIEW"))
        self.assertFalse(decide.auto_supersede("1.3.0", {"1.4.0"}, after, "PENDING_DEVELOPER_RELEASE"))


class Needs(unittest.TestCase):
    def test_nothing_due_outside_a_release_week(self):
        n = decide.needs(at("2026-11-07T12:00+02:00"), SCHEDULE[1:2], frozenset())
        self.assertFalse(n.run)

    def test_freeze_needs_no_environment(self):
        n = decide.needs(at("2026-11-08T23:59+02:00"), SCHEDULE, frozenset())
        self.assertEqual((n.run, n.env), (True, False))

    def test_tuesday_needs_the_environment_until_submitted(self):
        k = lambda n: decide.key("1.3.0", n)  # noqa: E731
        now = at("2026-11-10T12:15+02:00")
        self.assertEqual(decide.needs(now, SCHEDULE, frozenset({k("frozen"), k("signed")})).env, True)
        stores = frozenset({k("frozen"), k("signed"), k("notes"), k("ios-submitted")})
        self.assertTrue(decide.needs(now, SCHEDULE, stores).env)  # Play not done yet
        for play in decide.PLAY_DONE:
            self.assertFalse(decide.needs(now, SCHEDULE, stores | {k(play)}).run)

    def test_go_no_go_and_publish_run_until_their_note(self):
        k = lambda n: decide.key("1.3.0", n)  # noqa: E731
        self.assertTrue(decide.needs(at("2026-11-12T18:00+02:00"), SCHEDULE, frozenset()).env)
        self.assertFalse(decide.needs(at("2026-11-12T18:15+02:00"), SCHEDULE,
                                      frozenset({k("go/2026-11-12")})).run)
        self.assertTrue(decide.needs(at("2026-11-12T20:00+02:00"), SCHEDULE, frozenset({k("go/2026-11-12")})).run)
        self.assertFalse(decide.needs(at("2026-11-12T20:30+02:00"), SCHEDULE, frozenset({k("published")})).run)


class Notes(unittest.TestCase):
    def test_note_has_key_and_no_pings(self):
        msg = decide.Note("1.3.0", "go/2026-11-12", "go", ["a"]).message()
        self.assertEqual(msg["embeds"][0]["footer"]["text"], "release-auto 1.3.0/go/2026-11-12")
        self.assertEqual(msg["allowed_mentions"], {"parse": []})


if __name__ == "__main__":
    unittest.main()
