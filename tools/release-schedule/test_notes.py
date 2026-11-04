"""Tests for notes.py, asc.py and website.py helpers: python3 -I -m unittest discover -s tools/release-schedule"""
from __future__ import annotations

import sys
import unittest
from datetime import date
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import asc  # noqa: E402
import notes  # noqa: E402
import website  # noqa: E402

SAMPLE = """<!-- drafting hint -->
**Borealis** 1.3.0 adds dice rolls.

### New in 1.3.0
- **Dice rolls** on the [Play tab](https://borealisbuilder.com/guides) (#1).
- Several companions (#13, #14).

### Fixed in 1.3.0
- `Spell filters` work again (#21).

**Community credits**: thanks!

---

Licensed under MIT OR Apache-2.0.
"""


class Cs1(unittest.TestCase):
    def test_clean_notes(self):
        self.assertEqual(notes.cs1_findings(SAMPLE), [])

    def test_bare_5etools_foreign_links_and_source_names(self):
        bad = "Import 5etools data from https://example.org/data, like Xanathar's. 5etools-format is fine."
        found = notes.cs1_findings(bad)
        self.assertEqual(len(found), 3)

    def test_scrub_turns_5etools_into_the_format_name(self):
        self.assertEqual(notes.scrub_cs1("5etools import fails"), "5etools-format import fails")
        self.assertEqual(notes.scrub_cs1("5etools-format import"), "5etools-format import")

    def test_comments_are_not_checked_or_published(self):
        self.assertEqual(notes.cs1_findings("<!-- 5etools -->\nok"), [])
        self.assertNotIn("drafting hint", notes.strip_comments(SAMPLE))


class Commits(unittest.TestCase):
    def test_user_facing_feat_and_fix(self):
        self.assertEqual(notes.classify_commit("feat: dice module for Play rolls"), ("new", "Dice module for Play rolls"))
        self.assertEqual(notes.classify_commit("fix(ui): spell filters (IM14, review H1)\n\nbody"),
                         ("fixed", "Spell filters"))
        self.assertEqual(notes.classify_commit("feat: iOS privacy manifest"), ("new", "iOS privacy manifest"))

    def test_tooling_and_chores_are_left_out(self):
        for msg in ("chore: release 1.3.0", "fix(ci): cache", "docs: guide", "feat(tools): x", "test: y"):
            self.assertIsNone(notes.classify_commit(msg), msg)


class Render(unittest.TestCase):
    def test_shape_of_the_1_2_2_notes(self):
        text = notes.render("1.3.0", "v1.2.1", [(1, "dice rolling.", "new"), (9, "help buttons", "fixed")],
                           [("fixed", "On phones a sheet never focuses a field")], "**Community credits**: x",
                           date(2026, 11, 4))
        body = notes.strip_comments(text)
        self.assertTrue(body.startswith("**Borealis** 1.3.0"))
        self.assertIn("`Borealis_1.3.0_aarch64.dmg`", body)
        self.assertIn("### New in 1.3.0\n- **Dice rolling** (#1).", body)
        self.assertIn("### Fixed in 1.3.0\n- **Help buttons** (#9).\n- On phones", body)
        self.assertLess(body.index("**Community credits**"), body.index("\n---\n"))
        self.assertIn("apksigner verify --print-certs Borealis_1.3.0_arm64.apk", body)
        self.assertEqual(notes.cs1_findings(text), [])


class WhatsNew(unittest.TestCase):
    def test_plain_text_without_markdown_or_issue_numbers(self):
        text = notes.whats_new(SAMPLE)
        self.assertTrue(text.startswith("Borealis 1.3.0 adds dice rolls."))
        self.assertIn("New in 1.3.0\n• Dice rolls on the Play tab.\n• Several companions.", text)
        self.assertIn("• Spell filters work again.", text)
        for gone in ("**", "#1", "](", "`", "Licensed", "credits"):
            self.assertNotIn(gone, text)

    def test_cut_to_the_app_store_limit(self):
        long = "**Borealis** x.\n\n### New in 1.3.0\n" + "".join(f"- item {i} " + "x" * 80 + "\n" for i in range(80))
        self.assertLessEqual(len(notes.whats_new(long)), notes.WHATS_NEW_LIMIT)

    def test_summary_and_sections(self):
        self.assertEqual(notes.summary(SAMPLE), "**Borealis** 1.3.0 adds dice rolls.")
        self.assertEqual(list(notes.sections(SAMPLE)), ["New in 1.3.0", "Fixed in 1.3.0"])


def version(v: str, state: str) -> dict:
    return {"id": f"id-{v}", "attributes": {"platform": "IOS", "versionString": v, "appStoreState": state}}


class AscChoice(unittest.TestCase):
    def test_uses_renames_creates_or_is_blocked(self):
        self.assertEqual(asc.choose_version([version("1.3.0", "PREPARE_FOR_SUBMISSION")], "1.3.0").action, "use")
        self.assertEqual(asc.choose_version([version("1.2.2", "DEVELOPER_REJECTED")], "1.3.0").action, "rename")
        self.assertEqual(asc.choose_version([version("1.2.0", "READY_FOR_SALE")], "1.3.0").action, "create")
        self.assertEqual(asc.choose_version([], "1.3.0").action, "create")
        blocked = asc.choose_version([version("1.2.2", "WAITING_FOR_REVIEW")], "1.3.0")
        self.assertEqual(blocked.action, "blocked")
        self.assertIn("1.2.2 is WAITING_FOR_REVIEW", blocked.reason)

    def test_pick_build_takes_the_newest_valid(self):
        builds = [{"id": "a", "attributes": {"processingState": "PROCESSING", "uploadedDate": "3"}},
                  {"id": "b", "attributes": {"processingState": "VALID", "uploadedDate": "1"}},
                  {"id": "c", "attributes": {"processingState": "VALID", "uploadedDate": "2", "expired": False}}]
        self.assertEqual(asc.pick_build(builds)["id"], "c")
        self.assertIsNone(asc.pick_build(builds[:1]))

    def test_der_signature_to_raw(self):
        r, s = b"\x00" + b"\x81" * 32, b"\x05" * 31
        der = bytes([0x30, 4 + len(r) + len(s), 0x02, len(r)]) + r + bytes([0x02, len(s)]) + s
        raw = asc.der_to_raw(der)
        self.assertEqual(raw, b"\x81" * 32 + b"\x00" + b"\x05" * 31)


class Website(unittest.TestCase):
    CONFIG = 'const x = 1;\nconst version = "1.2.2";\nconst v = version;\n'

    def test_site_version_and_bump(self):
        self.assertEqual(website.site_version(self.CONFIG), "1.2.2")
        self.assertEqual(website.site_version(website.bump_config(self.CONFIG, "1.3.0")), "1.3.0")

    def test_new_branch_starts_from_an_unmerged_older_release(self):
        branches = ["main", "release-1.2.2", "release-1.2.1", "cs1-format-only"]
        self.assertEqual(website.base_branch(branches, ["release-1.2.2"], "1.3.0"), "release-1.2.2")
        self.assertEqual(website.base_branch(branches, [], "1.3.0"), "main")
        self.assertEqual(website.base_branch(branches, ["release-1.2.2"], "1.2.2"), "main")

    def test_live_check(self):
        self.assertTrue(website.live_has_version("", 'a="1.3.0"', "1.3.0"))
        self.assertTrue(website.live_has_version('href="/x/Borealis_1.3.0_x64-setup.exe"', "", "1.3.0"))
        self.assertFalse(website.live_has_version("Borealis_1.2.2_", '"1.2.2"', "1.3.0"))


if __name__ == "__main__":
    unittest.main()
