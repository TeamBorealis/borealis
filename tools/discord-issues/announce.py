#!/usr/bin/env python3
"""Post a published GitHub release, with its notes, to the Discord #announcements channel.

The notes become one message of embeds: the intro, then one embed per heading. Tables, code blocks, images and HTML
comments are dropped, and so is everything after the first `---` divider line (checksums, licence). #N
becomes a link to the issue. Notes that are too long lose text from their longest sections, so the credits stay.
Nobody is pinged. A release already announced (an embed linking its page) is skipped, so a re-run is safe.

Env: DISCORD_BOT_TOKEN, GITHUB_TOKEN, GITHUB_REPOSITORY (default TeamBorealis/borealis).
Usage: announce.py [--tag TAG] [--dry-run]. Default tag: the latest published release. Exit 0 when posted or
already announced, 1 on an API error, 2 on bad input. Python 3.9+, stdlib only.
"""
import argparse
import os
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import sync  # noqa: E402

ANNOUNCEMENTS = "1564901763343982723"
SITE = "https://borealisbuilder.com"
COLOUR = 0x2BA68A
DESCRIPTION_LIMIT = 4096  # Discord's limit per embed description
TOTAL_LIMIT = 6000  # Discord's limit on all embed text in one message
MAX_EMBEDS = 10
TITLE_LIMIT = 256
RECENT_MESSAGES = 50

COMMENT_RE = re.compile(r"<!--.*?-->", re.S)
FENCE_RE = re.compile(r"^```.*?^```[ \t]*$", re.S | re.M)
IMAGE_RE = re.compile(r"!\[[^\]]*\]\([^)]*\)")
DIVIDER_RE = re.compile(r"^---[ \t]*$", re.M)
HEADING_RE = re.compile(r"^(#{1,6})\s+(.*?)\s*#*\s*$")
ISSUE_RE = re.compile(r"(?<![\w/\[#])#(\d+)\b")


def clean_notes(body):
    """The notes as Discord-friendly Markdown: no tables, code, images, comments or text after a divider."""
    body = (body or "").replace("\r\n", "\n")
    body = DIVIDER_RE.split(body, 1)[0]
    body = FENCE_RE.sub("", COMMENT_RE.sub("", body))
    body = IMAGE_RE.sub("", body)
    lines = [line for line in body.split("\n") if not line.lstrip().startswith("|")]
    return re.sub(r"\n{3,}", "\n\n", "\n".join(lines)).strip()


def link_issues(text, repo):
    return ISSUE_RE.sub(lambda m: f"[#{m[1]}](https://github.com/{repo}/issues/{m[1]})", text)


def sections(notes):
    """[(title or None, body)]: the text before the first heading, then one entry per heading."""
    found, title, lines = [], None, []
    for line in notes.split("\n"):
        heading = HEADING_RE.match(line)
        if heading:
            found.append((title, "\n".join(lines).strip()))
            title, lines = heading[2], []
        else:
            lines.append(line)
    found.append((title, "\n".join(lines).strip()))
    return [(t, b) for t, b in found if t or b]


def _cut(text, limit, more):
    """`text` cut at a line break so that it plus `more` fits in `limit` characters."""
    if len(text) <= limit:
        return text
    room = limit - len(more)
    head = text[:room]
    head = head[: head.rfind("\n")] if "\n" in head else head
    return head.rstrip() + more


def _fit(parts, more):
    """Shorten the longest descriptions until all embed text fits Discord's per-message limit, so a short section
    at the end (the community credits) is never the one that is cut."""
    parts = [[title, _cut(text, DESCRIPTION_LIMIT, more)] for title, text in parts]
    while True:
        over = sum(len(t) + len(d) for t, d in parts) + len(FOOTER) - TOTAL_LIMIT
        if over <= 0:
            return parts
        longest = max(parts, key=lambda p: len(p[1]))
        target = max(len(longest[1]) - over, len(more) + 1)
        longest[1] = _cut(longest[1].removesuffix(more), target, more)


FOOTER = "Full notes and every download are on GitHub"


def announcement(release, repo):
    """The Discord message for a release: a short line plus embeds holding the notes."""
    tag, url = release["tag_name"], release["html_url"]
    name = (release.get("name") or "").strip() or f"Borealis {tag}"
    more = f"\n…\n[Read the full release notes]({url})"
    found = sections(clean_notes(release.get("body")))[:MAX_EMBEDS]
    parts = [((name if i == 0 and title is None else title or "")[:TITLE_LIMIT], link_issues(body, repo))
             for i, (title, body) in enumerate(found)]
    embeds = [{"title": title, "description": text, "color": COLOUR} for title, text in _fit(parts, more)]
    if not embeds:
        embeds = [{"title": name[:TITLE_LIMIT], "description": f"[Release notes]({url})", "color": COLOUR}]
    embeds[0]["url"] = url
    embeds[-1]["footer"] = {"text": FOOTER}
    content = f"**{name} is out!** Download it at <{SITE}> or from <{url}>."
    return {"content": content, "embeds": embeds, "allowed_mentions": {"parse": []}}


def already_announced(messages, url):
    return any(embed.get("url") == url for message in messages for embed in message.get("embeds", []))


def pick_release(releases, tag):
    stable = [r for r in releases if not r.get("draft") and not r.get("prerelease") and r.get("published_at")]
    if tag:
        stable = [r for r in stable if r["tag_name"] == tag]
    return max(stable, key=lambda r: r["published_at"]) if stable else None


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--tag", help="release tag (default: the latest published release)")
    parser.add_argument("--dry-run", action="store_true", help="print the message instead of posting it")
    args = parser.parse_args(argv)
    discord_token = os.environ.get("DISCORD_BOT_TOKEN", "").strip()
    github_token = os.environ.get("GITHUB_TOKEN", "").strip()
    if not discord_token or not github_token:
        print("::error::DISCORD_BOT_TOKEN and GITHUB_TOKEN must be set")
        return 2
    repo = os.environ.get("GITHUB_REPOSITORY", "TeamBorealis/borealis")
    bridge = sync.Bridge(args.dry_run, discord_token, github_token, repo)
    try:
        release = pick_release(bridge.github("GET", f"/repos/{repo}/releases?per_page=30"), args.tag)
        if release is None:
            print(f"::error::no published stable release {args.tag or ''}".rstrip())
            return 2
        recent = bridge.discord("GET", f"/channels/{ANNOUNCEMENTS}/messages?limit={RECENT_MESSAGES}")
        if already_announced(recent, release["html_url"]):
            print(f"{release['tag_name']} is already announced")
            return 0
        message = announcement(release, repo)
        if args.dry_run:
            print(f"DRY-RUN would post to #announcements:\n{message['content']}")
            for embed in message["embeds"]:
                print(f"--- {embed['title']}\n{embed['description']}")
            return 0
        bridge.discord("POST", f"/channels/{ANNOUNCEMENTS}/messages", message)
        print(f"announced {release['tag_name']}")
        return 0
    except sync.ApiError as err:
        print(f"::error::{err}")
        return 1


if __name__ == "__main__":
    sys.exit(main())
