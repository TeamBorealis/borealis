#!/usr/bin/env python3
"""Tests for the Discord -> GitHub issues bridge: python3 -I tools/discord-issues/test_sync.py (stdlib only)."""
import contextlib
import io
import sys
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import bridge  # noqa: E402
import credits  # noqa: E402
import sync  # noqa: E402

BUG_TAGS = [
    {"id": "w", "name": "Windows"}, {"id": "m", "name": "macOS"}, {"id": "l", "name": "Linux"},
    {"id": "a", "name": "Android"}, {"id": "c", "name": "Crash"}, {"id": "b", "name": "Builder"},
    {"id": "s", "name": "Character sheet"}, {"id": "i", "name": "Imports"}, {"id": "cf", "name": "Confirmed"},
    {"id": "fx", "name": "Fixed"}, {"id": "ni", "name": "Needs info"},
]
IDEA_TAGS = [
    {"id": "B", "name": "Builder"}, {"id": "P", "name": "Phone"}, {"id": "D", "name": "Desktop"},
    {"id": "PL", "name": "Planned"}, {"id": "DN", "name": "Done"}, {"id": "NP", "name": "Not planned"},
]


class MarkerTest(unittest.TestCase):
    def test_marker_round_trips(self):
        self.assertEqual(bridge.thread_of(bridge.marker("123") + "\nbody"), "123")

    def test_only_leading_marker_counts(self):
        self.assertIsNone(bridge.thread_of("text\n<!-- discord-thread:999 -->"))
        self.assertIsNone(bridge.thread_of(None))
        self.assertIsNone(bridge.thread_of(""))

    def test_post_text_cannot_forge_a_marker(self):
        body = bridge.issue_body("1", [], "<!-- discord-thread:999 -->", [], [], bridge.credit_line("bug", "Ann"))
        self.assertEqual(bridge.thread_of(body), "1")
        self.assertNotIn("<!-- discord-thread:999", body)


class NeutraliseTest(unittest.TestCase):
    def test_mentions_do_not_ping(self):
        out = bridge.neutralise("cc @octocat and @TeamBorealis/maintainers")
        self.assertNotIn("@octocat", out)
        self.assertIn("@​octocat", out)

    def test_issue_references_do_not_link(self):
        self.assertEqual(bridge.neutralise("see #12"), "see #​12")
        self.assertEqual(bridge.neutralise("C# code"), "C# code")

    def test_html_is_inert(self):
        self.assertNotIn("<img", bridge.neutralise('<img src=x onerror=alert(1)>'))

    def test_discord_ids_are_removed(self):
        out = bridge.scrub_discord("hi <@123> <@!456> <@&7> in <#8> :) <:pog:999> </roll:42> <t:0:R>")
        for leaked in ("123", "456", "999", "<#8>", "<@&7>"):
            self.assertNotIn(leaked, out)
        self.assertIn("@user", out)
        self.assertIn(":pog:", out)
        self.assertIn("/roll", out)
        self.assertIn("1970-01-01 00:00 UTC", out)


class BodyTest(unittest.TestCase):
    def test_body_quotes_text_and_links_the_post(self):
        body = bridge.issue_body("55", ["Windows", "Builder"], "line one\n\nline @two", [("a b.png", "https://x/1.png")],
                                 ["log.txt"], bridge.credit_line("bug", "Ann"))
        self.assertTrue(body.startswith("<!-- discord-thread:55 -->\n**Reported by** Ann on Discord\n"))
        self.assertEqual(bridge.credit_of(body), ("reported", "Ann"))
        self.assertIn("https://discord.com/channels/1466376402986336550/55", body)
        self.assertIn("**Discord tags:** Windows, Builder", body)
        self.assertIn("> line one\n>\n> line @​two", body)
        self.assertIn("![a_b.png](https://x/1.png)", body)
        self.assertIn("`log.txt` is in the Discord post", body)

    def test_attachment_only_post(self):
        self.assertIn("no text, attachments only", bridge.issue_body("1", [], "  ", [], ["x.zip"], bridge.credit_line("bug", "Ann")))

    def test_long_body_is_truncated(self):
        body = bridge.issue_body("1", [], "x" * 70000, [], [], bridge.credit_line("bug", "Ann"))
        self.assertLess(len(body), 65536)
        self.assertIn("truncated", body)

    def test_filenames_are_sanitised(self):
        self.assertEqual(bridge.safe_filename("../../etc/pass wd`.png"), "_.._etc_pass_wd_.png")
        self.assertEqual(bridge.safe_filename(""), "file")
        self.assertEqual(bridge.media_path("9", "7", "my shot.PNG", "png"), "9/7-my_shot.png")

    def test_title(self):
        self.assertEqual(bridge.issue_title("  Spell   Tracker ", "1"), "Spell Tracker")
        self.assertEqual(bridge.issue_title("", "1"), "Discord post 1")

    def test_image_sniffing(self):
        self.assertEqual(bridge.sniff_image(b"\x89PNG\r\n\x1a\n...."), "png")
        self.assertEqual(bridge.sniff_image(b"\xff\xd8\xff\xe0"), "jpg")
        self.assertEqual(bridge.sniff_image(b"GIF89a.."), "gif")
        self.assertEqual(bridge.sniff_image(b"RIFF\0\0\0\0WEBPVP8 "), "webp")
        self.assertIsNone(bridge.sniff_image(b"<svg xmlns='http://www.w3.org/2000/svg'/>"))
        self.assertIsNone(bridge.sniff_image(b"MZ\x90\x00"))


class LabelTest(unittest.TestCase):
    def test_bug_tags_map_to_labels(self):
        names = bridge.tag_names(["c", "w", "ni", "s"], BUG_TAGS)
        self.assertEqual(names, ["Windows", "Crash", "Character sheet", "Needs info"])
        self.assertEqual(bridge.labels_for(bridge.BUG_FORUM, names),
                         ["bug", "character-sheet", "crash", "discord", "windows"])

    def test_idea_status_tags_are_not_labels(self):
        names = ["Builder", "Phone", "Planned", "Done", "Not planned"]
        self.assertEqual(bridge.labels_for(bridge.IDEA_FORUM, names), ["builder", "discord", "enhancement", "phone"])

    def test_every_mapped_label_has_a_colour(self):
        for label in list(bridge.TAG_LABELS.values()) + ["discord"]:
            self.assertRegex(bridge.LABEL_COLOURS[label][0], r"^[0-9a-f]{6}$")


class TagLimitTest(unittest.TestCase):
    def test_adds_tag(self):
        self.assertEqual(bridge.with_status_tag(["w"], BUG_TAGS, "fixed"), ["w", "fx"])

    def test_existing_tag_is_kept_once(self):
        self.assertEqual(bridge.with_status_tag(["w", "fx"], BUG_TAGS, "fixed"), ["w", "fx"])

    def test_drops_needs_info_before_platforms(self):
        out = bridge.with_status_tag(["w", "m", "l", "a", "ni"], BUG_TAGS, "fixed")
        self.assertEqual(out, ["w", "m", "l", "a", "fx"])

    def test_drops_needs_info_then_confirmed(self):
        out = bridge.with_status_tag(["w", "cf", "ni", "c", "b"], BUG_TAGS, "fixed")
        self.assertEqual(out, ["w", "cf", "c", "b", "fx"])

    def test_drops_area_before_platform(self):
        out = bridge.with_status_tag(["w", "m", "l", "a", "c"], BUG_TAGS, "fixed")
        self.assertEqual(out, ["w", "m", "l", "a", "fx"])

    def test_never_more_than_five(self):
        out = bridge.with_status_tag(["w", "m", "l", "a", "c", "b"], BUG_TAGS, "fixed")
        self.assertEqual(len(out), 5)
        self.assertIn("fx", out)

    def test_done_and_not_planned_exclude_each_other(self):
        self.assertEqual(bridge.with_status_tag(["B", "DN"], IDEA_TAGS, "not planned", ("done", "planned")),
                         ["B", "NP"])
        self.assertEqual(bridge.with_status_tag(["PL", "NP"], IDEA_TAGS, "done", ("not planned",)), ["PL", "DN"])

    def test_unknown_tag_name_changes_nothing(self):
        self.assertEqual(bridge.with_status_tag(["w"], BUG_TAGS, "shipped"), ["w"])


class ClosePlanTest(unittest.TestCase):
    def test_plans(self):
        url = "https://github.com/o/r/issues/3"
        self.assertEqual(bridge.close_plan("bug", "completed", url)[0], "fixed")
        self.assertIn("Fixed on main", bridge.close_plan("bug", "completed", url)[2])
        self.assertEqual(bridge.close_plan("idea", "completed", url)[:2], ("done", ("not planned",)))
        self.assertEqual(bridge.close_plan("idea", "not_planned", url)[0], "not planned")
        bug_np = bridge.close_plan("bug", "not_planned", url)
        self.assertIsNone(bug_np[0])
        self.assertIn(url, bug_np[2])
        self.assertIsNone(bridge.close_plan("bug", "duplicate", url)[0])
        self.assertIsNone(bridge.close_plan("bug", "reopened", url))
        self.assertIsNone(bridge.close_plan("bug", None, url))


class ReleaseWindowTest(unittest.TestCase):
    RELEASES = [
        {"tag_name": "v1.2.1", "draft": False, "prerelease": False, "published_at": "2026-11-02T10:00:00Z"},
        {"tag_name": "v1.3.0", "draft": True, "prerelease": False, "published_at": None},
        {"tag_name": "models-x", "draft": False, "prerelease": True, "published_at": "2026-11-01T00:00:00Z"},
        {"tag_name": "v1.2.0", "draft": False, "prerelease": False, "published_at": "2026-10-28T19:53:06Z"},
    ]

    def test_window_starts_at_previous_stable_release(self):
        self.assertEqual(bridge.release_window(self.RELEASES, "v1.2.1"),
                         ("2026-10-28T19:53:06Z", "2026-11-02T10:00:00Z"))

    def test_first_release_has_open_start(self):
        self.assertEqual(bridge.release_window(self.RELEASES, "v1.2.0"), ("", "2026-10-28T19:53:06Z"))

    def test_drafts_and_prereleases_have_no_window(self):
        self.assertIsNone(bridge.release_window(self.RELEASES, "v1.3.0"))
        self.assertIsNone(bridge.release_window(self.RELEASES, "models-x"))

    def test_released_issues(self):
        issues = [
            {"number": 1, "state_reason": "completed", "closed_at": "2026-10-30T00:00:00Z"},
            {"number": 2, "state_reason": "not_planned", "closed_at": "2026-10-30T00:00:00Z"},
            {"number": 3, "state_reason": "completed", "closed_at": "2026-10-27T00:00:00Z"},
            {"number": 4, "state_reason": None, "closed_at": None},
        ]
        got = bridge.released_issues(issues, "2026-10-28T19:53:06Z", "2026-11-02T10:00:00Z")
        self.assertEqual([i["number"] for i in got], [1])


class FakeBridge(sync.Bridge):
    """Bridge with canned API answers; records every non-GET call."""

    def __init__(self, dry_run, routes):
        super().__init__(dry_run, "discord-token", "github-token", "o/r")
        self.routes = routes
        self.calls = []
        self.bot_id = "bot"
        self.forum_tags = {bridge.BUG_FORUM: BUG_TAGS, bridge.IDEA_FORUM: IDEA_TAGS}

    def _answer(self, api, method, path, body):
        if method != "GET":
            self.calls.append((api, method, path, body))
        for (m, prefix), answer in self.routes.items():
            if m == method and path.startswith(prefix):
                if isinstance(answer, Exception):
                    raise answer
                return answer(path, body) if callable(answer) else answer
        raise AssertionError(f"unexpected {method} {path}")

    def discord(self, method, path, body=None):
        return self._answer("discord", method, path, body)

    def github(self, method, path, body=None):
        return self._answer("github", method, path, body)


def closed_issue(reason, closed_at="2026-10-30T00:00:00Z", thread="77"):
    return {"number": 9, "state": "closed", "state_reason": reason, "closed_at": closed_at,
            "html_url": "https://github.com/o/r/issues/9", "body": bridge.marker(thread)}


def quiet(fn, *args):
    with contextlib.redirect_stdout(io.StringIO()) as out:
        result = fn(*args)
    return result, out.getvalue()


class ReportBackTest(unittest.TestCase):
    def routes(self, messages):
        return {
            ("GET", "/channels/77/messages"): messages,
            ("GET", "/channels/77"): {"id": "77", "parent_id": bridge.BUG_FORUM, "applied_tags": ["w", "ni"],
                                      "thread_metadata": {"archived": True}},
            ("PATCH", "/channels/77"): {},
            ("POST", "/channels/77/messages"): {},
        }

    def test_fixed_bug_gets_tag_and_one_reply(self):
        fake = FakeBridge(False, self.routes([]))
        quiet(fake.report_closed, {"77": closed_issue("completed")}, "2026-10-01T00:00:00Z")
        self.assertEqual(fake.calls[0][1:3], ("PATCH", "/channels/77"))
        self.assertEqual(fake.calls[0][3], {"applied_tags": ["w", "ni", "fx"], "archived": False})
        post = fake.calls[1][3]
        self.assertIn("Fixed on main", post["content"])
        self.assertEqual(post["allowed_mentions"], {"parse": []})
        quiet(fake.report_closed, {"77": closed_issue("completed")}, "2026-10-01T00:00:00Z")
        self.assertEqual(len(fake.calls), 2)

    def test_reply_log_hides_the_posters_id(self):
        fake = FakeBridge(False, self.routes([]))
        _, out = quiet(fake.reply_once, "77", "Thanks <@123456789012345678> for the idea!", None, "123456789012345678")
        self.assertEqual(fake.calls[-1][3]["content"], "Thanks <@123456789012345678> for the idea!")
        self.assertNotIn("123456789012345678", out)
        self.assertIn("Thanks @poster for the idea!", out)

    def test_existing_reply_is_not_repeated(self):
        url = "https://github.com/o/r/issues/9"
        said = [{"author": {"id": "bot"}, "content": bridge.close_plan("bug", "completed", url)[2]}]
        fake = FakeBridge(False, self.routes(said))
        quiet(fake.report_closed, {"77": closed_issue("completed")}, "2026-10-01T00:00:00Z")
        self.assertEqual(fake.calls, [])

    def test_old_closures_are_ignored(self):
        fake = FakeBridge(False, self.routes([]))
        quiet(fake.report_closed, {"77": closed_issue("completed", "2026-09-01T00:00:00Z")}, "2026-10-01T00:00:00Z")
        self.assertEqual(fake.calls, [])

    def test_dry_run_writes_nothing(self):
        fake = FakeBridge(True, self.routes([]))
        _, out = quiet(fake.report_closed, {"77": closed_issue("completed")}, "2026-10-01T00:00:00Z")
        self.assertEqual(fake.calls, [])
        self.assertIn("DRY-RUN would: reply in thread 77", out)

    def test_release_reply(self):
        routes = self.routes([])
        routes[("GET", "/repos/o/r/releases")] = [
            {"tag_name": "v1.2.1", "draft": False, "prerelease": False, "published_at": "2026-11-02T10:00:00Z",
             "html_url": "https://github.com/o/r/releases/tag/v1.2.1"},
            {"tag_name": "v1.2.0", "draft": False, "prerelease": False, "published_at": "2026-10-28T19:53:06Z",
             "html_url": "u"},
        ]
        fake = FakeBridge(False, routes)
        quiet(fake.report_releases, {"77": closed_issue("completed")}, "2026-10-29T00:00:00Z")
        posts = [c[3]["content"] for c in fake.calls if c[1] == "POST"]
        self.assertEqual(posts, ["🚀 Released in v1.2.1: https://github.com/o/r/releases/tag/v1.2.1"])
        calls = len(fake.calls)
        quiet(fake.report_releases, {"77": closed_issue("completed")}, "2026-10-29T00:00:00Z")
        self.assertEqual(len(fake.calls), calls)

    def test_one_failure_does_not_stop_the_rest(self):
        routes = self.routes([])
        routes[("GET", "/channels/66")] = sync.ApiError("GET", "/channels/66", 403, "Missing Access")
        fake = FakeBridge(False, routes)
        issues = {"66": closed_issue("completed", thread="66"), "77": closed_issue("completed")}
        _, out = quiet(fake.report_closed, issues, "2026-10-01T00:00:00Z")
        self.assertEqual(fake.failures, ["issue #9 (thread 66)"])
        self.assertIn("HTTP 403", out)
        self.assertTrue(any(c[1] == "POST" for c in fake.calls))


class ImportTest(unittest.TestCase):
    THREAD = {"id": "88", "parent_id": bridge.IDEA_FORUM, "name": "Spell Tracker", "applied_tags": ["B"],
              "owner_id": "4242"}

    def routes(self, starter, bot_messages=()):
        return {
            ("GET", "/channels/88/messages/88"): starter,
            ("GET", "/channels/88/messages"): list(bot_messages),
            ("GET", "/repos/o/r/labels"): [{"name": "discord"}, {"name": "enhancement"}],
            ("POST", "/repos/o/r/labels"): {},
            ("POST", "/repos/o/r/issues"): {"number": 5, "html_url": "https://github.com/o/r/issues/5"},
            ("POST", "/channels/88/messages"): {},
            ("GET", "/channels/88"): dict(self.THREAD),
            ("GET", "/guilds/1466376402986336550/members/4242"): {"nick": "Spell*Fan", "user": {"username": "u"}},
        }

    def test_new_post_becomes_issue_and_reply(self):
        fake = FakeBridge(False, self.routes({"content": "Track spell slots @here #3", "attachments": []}))
        quiet(fake.import_post, dict(self.THREAD))
        kinds = [(c[0], c[1], c[2]) for c in fake.calls]
        self.assertEqual(kinds, [("github", "POST", "/repos/o/r/labels"), ("github", "POST", "/repos/o/r/issues"),
                                 ("discord", "POST", "/channels/88/messages")])
        issue = fake.calls[1][3]
        self.assertEqual(issue["title"], "Spell Tracker")
        self.assertEqual(issue["labels"], ["builder", "discord", "enhancement"])
        self.assertTrue(issue["body"].startswith("<!-- discord-thread:88 -->\n**Suggested by** Spell\\*Fan on Discord\n"))
        self.assertNotIn("4242", issue["body"])
        self.assertEqual(fake.calls[2][3]["allowed_mentions"], {"parse": []})
        self.assertIn("@​here #​3", issue["body"])
        self.assertIn("https://github.com/o/r/issues/5", fake.calls[2][3]["content"])

    def test_empty_starter_is_a_failure_not_an_empty_issue(self):
        fake = FakeBridge(False, self.routes({"content": "", "attachments": []}))
        with self.assertRaises(sync.ApiError):
            quiet(fake.import_post, dict(self.THREAD))
        self.assertEqual(fake.calls, [])

    def test_thread_with_bot_link_but_no_issue_is_skipped(self):
        said = [{"author": {"id": "bot"}, "content": "tracked as https://github.com/o/r/issues/4"}]
        fake = FakeBridge(False, self.routes({"content": "x", "attachments": []}, said))
        quiet(fake.import_post, dict(self.THREAD))
        self.assertEqual(fake.calls, [])

    def test_retry_reply_for_recent_issue_without_link(self):
        now = datetime.now(timezone.utc)
        issue = {"number": 5, "html_url": "https://github.com/o/r/issues/5",
                 "created_at": now.strftime("%Y-%m-%dT%H:%M:%SZ")}
        fake = FakeBridge(False, self.routes({}))
        quiet(fake.retry_reply, dict(self.THREAD), issue)
        self.assertEqual(len(fake.calls), 1)
        quiet(fake.retry_reply, dict(self.THREAD), issue)
        self.assertEqual(len(fake.calls), 1)
        old = dict(issue, created_at=(now - timedelta(days=3)).strftime("%Y-%m-%dT%H:%M:%SZ"))
        fake = FakeBridge(False, self.routes({}))
        quiet(fake.retry_reply, dict(self.THREAD), old)
        self.assertEqual(fake.calls, [])


class CreditTest(unittest.TestCase):
    def test_name_prefers_nick_then_global_then_username(self):
        user = {"username": "ann_42", "global_name": "Ann"}
        self.assertEqual(bridge.poster_name({"nick": " Annie  B "}, user), "Annie B")
        self.assertEqual(bridge.poster_name({"nick": None}, user), "Ann")
        self.assertEqual(bridge.poster_name(None, {"username": "ann_42", "global_name": None}), "ann_42")
        self.assertEqual(bridge.poster_name(None, None), "")

    def test_name_is_neutralised(self):
        line = bridge.credit_line("bug", "@everyone <img src=x> **#1** [x](y)\nnext")
        self.assertNotIn("@everyone", line)
        self.assertNotIn("<img", line)
        self.assertNotIn("**#1**", line)
        self.assertNotIn("](", line)
        self.assertNotIn("\n", line)
        self.assertTrue(line.startswith("**Reported by** ") and line.endswith(" on Discord"))

    def test_discord_mentions_in_names_lose_their_ids(self):
        self.assertNotIn("123", bridge.credit_line("bug", "<@123>"))

    def test_empty_name_and_idea_verb(self):
        self.assertEqual(bridge.credit_line("bug", "  "), "**Reported by** a community member on Discord")
        self.assertEqual(bridge.credit_line("idea", "Bo"), "**Suggested by** Bo on Discord")

    def test_long_name_is_bounded(self):
        self.assertLess(len(bridge.credit_line("bug", "x" * 500)), 120)

    def test_with_credit_is_idempotent(self):
        old = bridge.marker("5") + "\n**Reported on Discord:** link\n\n> text"
        line = bridge.credit_line("bug", "Ann")
        once = bridge.with_credit(old, line)
        self.assertEqual(once.split("\n")[:3], [bridge.marker("5"), line, "**Reported on Discord:** link"])
        self.assertEqual(bridge.with_credit(once, line), once)
        renamed = bridge.with_credit(once, bridge.credit_line("bug", "Annie"))
        self.assertEqual(renamed.count("Reported by"), 1)
        self.assertEqual(bridge.credit_of(renamed), ("reported", "Annie"))
        self.assertEqual(bridge.thread_of(renamed), "5")

    def test_credit_in_post_text_is_not_a_credit(self):
        body = bridge.marker("5") + "\n> **Reported by** Mallory on Discord"
        self.assertIsNone(bridge.credit_of(body))

    def test_thanks_mention(self):
        url = "https://github.com/o/r/issues/3"
        self.assertTrue(bridge.close_plan("bug", "completed", url, "42")[2].endswith("\nThanks <@42> for reporting this!"))
        self.assertTrue(bridge.close_plan("idea", "completed", url, "42")[2].endswith("\nThanks <@42> for the idea!"))
        self.assertNotIn("<@", bridge.close_plan("idea", "not_planned", url, "42")[2])
        self.assertNotIn("<@", bridge.close_plan("bug", "completed", url, "not-an-id")[2])
        self.assertEqual(bridge.release_message("v1.2.1", "u", "bug", "42"), "🚀 Released in v1.2.1: u\nThanks <@42> for reporting this!")
        self.assertEqual(bridge.release_message("v1.2.1", "u"), "🚀 Released in v1.2.1: u")

    def test_credits_markdown(self):
        md = bridge.credits_markdown([(2, "Spell @Tracker", "suggested", "Bo"), (3, "Boons #1", "reported", "Ann")])
        lines = md.split("\n")
        self.assertIn("Community credits", lines[0])
        self.assertEqual(lines[2:], ["- Boons \\#\u200b1 (#3), reported by Ann", "- Spell @\u200bTracker (#2), suggested by Bo"])
        self.assertEqual(bridge.credits_markdown([]), "")


class CreditSyncTest(unittest.TestCase):
    URL = "https://github.com/o/r/issues/9"

    def routes(self, member=None, messages=()):
        routes = {
            ("GET", "/channels/77/messages"): list(messages),
            ("GET", "/channels/77"): {"id": "77", "parent_id": bridge.BUG_FORUM, "applied_tags": [], "owner_id": "4242"},
            ("PATCH", "/channels/77"): {},
            ("POST", "/channels/77/messages"): {},
            ("PATCH", "/repos/o/r/issues/9"): {},
            ("GET", "/users/4242"): {"username": "ann_42", "global_name": "Ann"},
        }
        key = ("GET", "/guilds/1466376402986336550/members/4242")
        routes[key] = member if member is not None else sync.ApiError("GET", "/guilds/x/members/y", 404)
        return routes

    def test_backfill_adds_credit_once(self):
        fake = FakeBridge(False, self.routes({"nick": "Annie", "user": {"username": "a"}}))
        issues = {"77": closed_issue("completed")}
        quiet(fake.credit_issues, issues)
        patch = [c for c in fake.calls if c[1] == "PATCH"]
        self.assertEqual(len(patch), 1)
        self.assertEqual(bridge.credit_of(patch[0][3]["body"]), ("reported", "Annie"))
        self.assertNotIn("4242", patch[0][3]["body"])
        quiet(fake.credit_issues, issues)
        self.assertEqual(len([c for c in fake.calls if c[1] == "PATCH"]), 1)

    def test_member_who_left_falls_back_to_user(self):
        fake = FakeBridge(False, self.routes())
        quiet(fake.credit_issues, {"77": closed_issue("completed")})
        self.assertEqual(bridge.credit_of(fake.calls[0][3]["body"]), ("reported", "Ann"))

    def test_backfill_dry_run_writes_nothing(self):
        fake = FakeBridge(True, self.routes({"nick": "Annie"}))
        _, out = quiet(fake.credit_issues, {"77": closed_issue("completed")})
        self.assertEqual(fake.calls, [])
        self.assertIn("**Reported by** Annie on Discord", out)

    def test_fixed_reply_pings_only_the_poster(self):
        fake = FakeBridge(False, self.routes())
        quiet(fake.report_closed, {"77": closed_issue("completed")}, "2026-10-01T00:00:00Z")
        post = [c[3] for c in fake.calls if c[1] == "POST"][0]
        self.assertIn("Thanks <@4242> for reporting this!", post["content"])
        self.assertEqual(post["allowed_mentions"], {"parse": [], "users": ["4242"]})

    def test_not_planned_reply_pings_nobody(self):
        fake = FakeBridge(False, self.routes())
        quiet(fake.report_closed, {"77": closed_issue("not_planned")}, "2026-10-01T00:00:00Z")
        post = [c[3] for c in fake.calls if c[1] == "POST"][0]
        self.assertEqual(post["allowed_mentions"], {"parse": []})

    def test_release_reply_thanks_the_poster(self):
        routes = self.routes()
        routes[("GET", "/repos/o/r/releases")] = [
            {"tag_name": "v1.2.1", "draft": False, "prerelease": False, "published_at": "2026-11-02T10:00:00Z",
             "html_url": "R"},
            {"tag_name": "v1.2.0", "draft": False, "prerelease": False, "published_at": "2026-10-28T19:53:06Z",
             "html_url": "u"},
        ]
        fake = FakeBridge(False, routes)
        quiet(fake.report_releases, {"77": closed_issue("completed")}, "2026-10-29T00:00:00Z")
        post = [c[3] for c in fake.calls if c[1] == "POST"][0]
        self.assertEqual(post["content"], "🚀 Released in v1.2.1: R\nThanks <@4242> for reporting this!")
        self.assertEqual(post["allowed_mentions"], {"parse": [], "users": ["4242"]})


class CreditsScriptTest(unittest.TestCase):
    RELEASES = ReleaseWindowTest.RELEASES

    def test_default_window_starts_at_latest_release(self):
        self.assertEqual(credits.window(self.RELEASES, None, None), ("2026-11-02T10:00:00Z", "9999"))
        self.assertEqual(credits.window(self.RELEASES, "v1.2.0", "v1.2.1"),
                         ("2026-10-28T19:53:06Z", "2026-11-02T10:00:00Z"))

    def test_unknown_or_draft_tag_is_an_error(self):
        for tag in ("v9.9.9", "v1.3.0", "models-x"):
            with self.assertRaises(ValueError):
                credits.window(self.RELEASES, tag, None)

    def test_entries_need_marker_credit_and_completion(self):
        credited = bridge.marker("1") + "\n" + bridge.credit_line("bug", "Ann") + "\nrest"
        issues = [
            {"number": 3, "title": "Boons", "state_reason": "completed", "closed_at": "2026-10-30T00:00:00Z",
             "body": credited},
            {"number": 4, "title": "No credit", "state_reason": "completed", "closed_at": "2026-10-30T00:00:00Z",
             "body": bridge.marker("2") + "\nrest"},
            {"number": 5, "title": "Not ours", "state_reason": "completed", "closed_at": "2026-10-30T00:00:00Z",
             "body": "**Reported by** X on Discord"},
            {"number": 6, "title": "Wontfix", "state_reason": "not_planned", "closed_at": "2026-10-30T00:00:00Z",
             "body": credited},
        ]
        self.assertEqual(credits.entries(issues, "2026-10-28T19:53:06Z", "9999"), [(3, "Boons", "reported", "Ann")])


class HttpTest(unittest.TestCase):
    def test_api_error_carries_no_headers(self):
        err = sync.ApiError("POST", "/repos/o/r/issues", 422, "Validation Failed")
        self.assertEqual(str(err), "POST /repos/o/r/issues -> HTTP 422 Validation Failed")


if __name__ == "__main__":
    unittest.main(verbosity=1)
