#!/usr/bin/env python3
"""Tests for the release announcer: python3 -I tools/discord-issues/test_announce.py (stdlib only)."""
import contextlib
import io
import os
import sys
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import announce  # noqa: E402
import sync  # noqa: E402

REPO = "o/r"
URL = "https://github.com/o/r/releases/tag/v1.2.1"
NOTES = """**Borealis** 1.2.1 adds a spell tracker (#2).

| Platform | File |
|---|---|
| Windows | `setup.exe` |

![shot](https://example.com/a.png)
<!-- a note for maintainers -->
### New in 1.2.1
- **Spell slots** (#2), see [#3](https://github.com/o/r/issues/3) and page#4.

### Fixed in 1.2.1
- **Long lists** (#6).

**Community credits**: thanks!

- Spell Tracker (#2), suggested by Jack walljumper
---

Verify downloads with `SHA256SUMS.txt`:

```
42:A7
```
"""


def release(body=NOTES, **extra):
    return {"tag_name": "v1.2.1", "html_url": URL, "name": "Borealis 1.2.1", "body": body, "draft": False,
            "prerelease": False, "published_at": "2026-10-30T02:28:47Z", **extra}


def embed_text(message):
    return sum(len(e["title"]) + len(e["description"]) + len(e.get("footer", {}).get("text", ""))
               for e in message["embeds"])


class CleanNotesTest(unittest.TestCase):
    def test_drops_tables_images_comments_and_the_tail(self):
        notes = announce.clean_notes(NOTES)
        for gone in ("| Platform", "setup.exe", "![shot]", "maintainers", "Verify downloads", "42:A7"):
            self.assertNotIn(gone, notes)
        self.assertIn("suggested by Jack walljumper", notes)

    def test_table_rules_are_not_dividers(self):
        self.assertIn("after", announce.clean_notes("before\n|---|---|\nafter"))

    def test_issue_numbers_become_links_once(self):
        text = announce.link_issues("fix (#2), [#3](https://github.com/o/r/issues/3), page#4", REPO)
        linked = "fix ([#2](https://github.com/o/r/issues/2)), [#3](https://github.com/o/r/issues/3), page#4"
        self.assertEqual(text, linked)

    def test_sections_follow_headings(self):
        titles = [title for title, _ in announce.sections(announce.clean_notes(NOTES))]
        self.assertEqual(titles, [None, "New in 1.2.1", "Fixed in 1.2.1"])


class AnnouncementTest(unittest.TestCase):
    def test_message_carries_the_notes_without_pings(self):
        message = announce.announcement(release(), REPO)
        self.assertEqual([e["title"] for e in message["embeds"]], ["Borealis 1.2.1", "New in 1.2.1", "Fixed in 1.2.1"])
        self.assertEqual(message["embeds"][0]["url"], URL)
        self.assertEqual(message["allowed_mentions"], {"parse": []})
        self.assertIn("Jack walljumper", message["embeds"][-1]["description"])
        self.assertIn("[#6](https://github.com/o/r/issues/6)", message["embeds"][-1]["description"])

    def test_long_notes_fit_and_keep_the_credits(self):
        line = "- change number {} with a fairly long explanation of what it does"
        long_section = "\n".join(line.format(i) for i in range(200))
        body = f"Intro.\n### New\n{long_section}\n### Credits\n- Idea (#2), suggested by Xelial\n"
        message = announce.announcement(release(body), REPO)
        self.assertLessEqual(embed_text(message), announce.TOTAL_LIMIT)
        for embed in message["embeds"]:
            self.assertLessEqual(len(embed["description"]), announce.DESCRIPTION_LIMIT)
        self.assertIn("[Read the full release notes]", message["embeds"][1]["description"])
        self.assertIn("suggested by Xelial", message["embeds"][-1]["description"])

    def test_empty_notes_still_link_the_release(self):
        message = announce.announcement(release(""), REPO)
        self.assertEqual(len(message["embeds"]), 1)
        self.assertEqual(message["embeds"][0]["url"], URL)

    def test_already_announced_matches_the_release_page(self):
        self.assertTrue(announce.already_announced([{"embeds": [{"url": URL}]}], URL))
        self.assertFalse(announce.already_announced([{"embeds": [{"title": "New release published: v1.2.1"}]}], URL))

    def test_pick_release_skips_drafts_and_prereleases(self):
        releases = [release(), release(tag_name="v1.3.0", prerelease=True, published_at="2026-11-01T00:00:00Z"),
                    release(tag_name="v1.4.0", draft=True, published_at=None)]
        self.assertEqual(announce.pick_release(releases, None)["tag_name"], "v1.2.1")
        self.assertIsNone(announce.pick_release(releases, "v1.3.0"))


class MainTest(unittest.TestCase):
    def run_main(self, args, recent):
        calls = []

        def fake_http(method, url, headers, body=None, log_path=None):
            calls.append((method, url))
            if "/releases" in url:
                return [release()]
            if method == "GET":
                return recent
            return {}

        env = {"DISCORD_BOT_TOKEN": "d", "GITHUB_TOKEN": "g", "GITHUB_REPOSITORY": REPO}
        with mock.patch.dict(os.environ, env), mock.patch.object(sync, "http", fake_http), \
                contextlib.redirect_stdout(io.StringIO()) as out:
            code = announce.main(args)
        return code, [c for c in calls if c[0] == "POST"], out.getvalue()

    def test_posts_once(self):
        code, posts, _ = self.run_main([], [])
        self.assertEqual((code, len(posts)), (0, 1))
        self.assertIn(f"/channels/{announce.ANNOUNCEMENTS}/messages", posts[0][1])

    def test_skips_a_release_already_announced(self):
        code, posts, out = self.run_main([], [{"embeds": [{"url": URL}]}])
        self.assertEqual((code, posts), (0, []))
        self.assertIn("already announced", out)

    def test_dry_run_posts_nothing(self):
        code, posts, out = self.run_main(["--dry-run"], [])
        self.assertEqual((code, posts), (0, []))
        self.assertIn("Jack walljumper", out)


if __name__ == "__main__":
    unittest.main()
