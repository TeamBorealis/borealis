#!/usr/bin/env python3
"""Draft and check Borealis release notes (B-164, decision RL5a 3).

The notes of a release live in `release-notes/<version>.md`, committed before the freeze. The release automation
(auto.py) makes that file the GitHub release body (HTML comments removed), builds the App Store "What's New" text
from it and posts a summary to #mod-chat on the Tuesday. This script drafts the file from the issues closed as
completed since the last published release, the feat/fix commits since its tag, and the Discord credits
(tools/discord-issues/credits.py), in the shape of the 1.2.2 notes. Edit the draft; it is a starting point.

    GH_TOKEN=... python3 -I tools/release-schedule/notes.py 1.3.0            # writes release-notes/1.3.0.md
    GH_TOKEN=... python3 -I tools/release-schedule/notes.py 1.3.0 --stdout   # print it instead
    python3 -I tools/release-schedule/notes.py 1.3.0 --check                 # CS1 check of the committed file

CS1: the notes never name or link a source of non-SRD content; "5etools" appears only as "5etools-format".
Standard library only (Python 3.9+).
"""
from __future__ import annotations

import argparse
import os
import re
import sys
from datetime import date
from pathlib import Path
from typing import Optional, Sequence

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent / "discord-issues"))
import bridge  # noqa: E402
import credits  # noqa: E402
import sync  # noqa: E402

ROOT = HERE.parents[1]
NOTES_DIR = "release-notes"
REPO = "TeamBorealis/borealis"
SITE = "https://borealisbuilder.com"
DISCORD_INVITE = "https://discord.gg/r4K4D3AJpZ"
APK_CERT = "42:A7:E1:F7:3D:85:F9:FC:28:A9:C5:E0:29:F8:C6:EC:CC:C6:E7:15:13:F5:5F:F3:A3:85:EF:D1:E2:D6:0C:2C"
WHATS_NEW_LIMIT = 4000  # App Store Connect's limit
COMMENT_RE = re.compile(r"<!--.*?-->\n?", re.S)
COMMIT_RE = re.compile(r"^(feat|fix)(?:\(([^)]*)\))?!?:\s*(.+)$")
# Commits about the project's own tooling, not the app.
SKIP_SCOPES = {"ci", "release", "tools", "e2e", "test", "tests", "docs", "deps", "website", "discord", "bridge"}
ISSUE_REF_RE = re.compile(r"#(\d+)\b")
# Internal decision / backlog ids in commit subjects, e.g. "(IM14, review H1)" or "(B-86)": not for users.
INTERNAL_IDS_RE = re.compile(r"\s*\((?:\s*(?:review|[A-Z]{1,3}-?\d+[a-z]?)\s*,?)+\)")
FIVE_ETOOLS_RE = re.compile(r"5e\.?tools(?!-format)", re.I)
URL_RE = re.compile(r"https?://[^\s)>\]]+")
ALLOWED_URL_PREFIXES = (SITE, "https://www.borealisbuilder.com", f"https://github.com/{REPO}", DISCORD_INVITE)
# Names of non-SRD sources that must never appear (CS1); the URL rule catches links to any other site.
SOURCE_NAMES = ("dndbeyond", "d&d beyond", "dnd beyond", "player's handbook", "players handbook", "xanathar",
                "tasha", "monster manual", "dungeon master's guide", "volo's", "mordenkainen", "fizban", "bigby")


def strip_comments(text: str) -> str:
    """The text as published: HTML comments removed (they hold drafting hints)."""
    return COMMENT_RE.sub("", text).strip() + "\n"


def scrub_cs1(text: str) -> str:
    """'5etools' becomes '5etools-format' (CS1); used on issue titles and commit subjects."""
    return FIVE_ETOOLS_RE.sub("5etools-format", text)


def cs1_findings(text: str) -> list[str]:
    """What breaks CS1 in published notes: a bare '5etools', a link off the project's sites, a source's name."""
    body = strip_comments(text)
    found = []
    for m in FIVE_ETOOLS_RE.finditer(body):
        found.append(f"'{m.group(0)}' without '-format'")
    for url in URL_RE.findall(body):
        if not url.startswith(ALLOWED_URL_PREFIXES):
            found.append(f"link to {url}")
    lower = body.lower()
    for name in SOURCE_NAMES:
        if name in lower:
            found.append(f"names '{name}'")
    return found


def sentence(text: str) -> str:
    """A title or subject as a bullet's text: internal ids and the final period dropped, a capital first letter
    (unless the first word is spelled like iOS or macOS)."""
    text = INTERNAL_IDS_RE.sub("", text).strip().rstrip(".").strip()
    first = text.split(" ", 1)[0]
    if text and not any(c.isupper() for c in first[1:]):
        text = text[:1].upper() + text[1:]
    return text


def classify_commit(message: str) -> Optional[tuple[str, str]]:
    """('new' | 'fixed', subject) for a user-facing feat/fix commit, else None."""
    m = COMMIT_RE.match(message.splitlines()[0].strip() if message else "")
    if not m or (m.group(2) or "").lower() in SKIP_SCOPES:
        return None
    subject = sentence(m.group(3))
    if not subject or subject.lower().startswith(("release ", "bump ")):
        return None
    return ("new" if m.group(1) == "feat" else "fixed"), subject


def issue_kind(issue: dict) -> str:
    labels = {(lbl.get("name") or "").lower() for lbl in issue.get("labels", [])}
    return "fixed" if "bug" in labels or "crash" in labels else "new"


def platform_table(v: str) -> list[str]:
    return [
        "| Platform | File |",
        "|---|---|",
        f"| macOS (Apple silicon, macOS 10.15 or later) | `Borealis_{v}_aarch64.dmg` |",
        f"| Windows (x64) | `Borealis_{v}_x64-setup.exe` |",
        f"| Linux (x86_64), any distribution | `Borealis_{v}_amd64.AppImage` |",
        f"| Debian / Ubuntu | `Borealis_{v}_amd64.deb` |",
        f"| Fedora / openSUSE | `Borealis-{v}-1.x86_64.rpm` |",
        f"| Android (64-bit ARM), sideload | `Borealis_{v}_arm64.apk` |",
    ]


def render(version: str, since: str, issues: Sequence[tuple[int, str, str]], commits: Sequence[tuple[str, str]],
           credits_md: str, today: date, mac_notarised: bool = True) -> str:
    """The draft notes. issues: [(number, title, 'new'|'fixed')]; commits: [('new'|'fixed', subject)]."""
    v = version
    new = [f"- **{scrub_cs1(sentence(t))}** (#{n})." for n, t, k in issues if k == "new"]
    new += [f"- {scrub_cs1(s)}." for k, s in commits if k == "new"]
    fixed = [f"- **{scrub_cs1(sentence(t))}** (#{n})." for n, t, k in issues if k == "fixed"]
    fixed += [f"- {scrub_cs1(s)}." for k, s in commits if k == "fixed"]
    out = [
        f"<!-- Draft from tools/release-schedule/notes.py on {today.isoformat()}: issues closed and feat/fix "
        f"commits since {since}. Write the summary, merge related lines into plain user-facing sentences, "
        "and commit before the freeze. HTML comments never reach the release. -->",
        f"**Borealis** {v} brings {len(new)} new feature(s) and {len(fixed)} fix(es).",
        "",
        f"**Download for your platform at [borealisbuilder.com]({SITE})**, or pick a file below.",
        "",
        *platform_table(v),
        "",
    ]
    if mac_notarised:
        out += ["The macOS app is signed and notarised by Apple: it opens with a double-click.", ""]
    else:
        out += ["The macOS app is not notarised yet. The first time, right-click Borealis in Applications and "
                "choose **Open**; on macOS 15 or later, click **Open Anyway** in System Settings > Privacy & Security.",
                ""]
    out += [f"Installing {v} over an earlier version keeps your characters, content and settings.", ""]
    if new:
        out += [f"### New in {v}", *new, ""]
    if fixed:
        out += [f"### Fixed in {v}", *fixed, ""]
    if credits_md:
        out += [credits_md, ""]
    out += [
        "---",
        "",
        "Verify downloads with `SHA256SUMS.txt`. The Android APK is signed with the same certificate as every "
        "earlier release (SHA-256):",
        "",
        "```",
        APK_CERT,
        "```",
        "",
        f"Check it with `apksigner verify --print-certs Borealis_{v}_arm64.apk`.",
        "",
        "Licensed under MIT OR Apache-2.0. Bundled rules content is from the SRD 5.1 and SRD 5.2.1 (CC-BY-4.0). "
        "The optional semantic search model is EmbeddingGemma (Apache-2.0). Borealis is an independent project, "
        "not affiliated with or endorsed by Wizards of the Coast or the authors of Aurora Builder.",
        "",
    ]
    return "\n".join(out)


def _plain(line: str) -> str:
    line = re.sub(r"\[([^\]]+)\]\([^)]*\)", r"\1", line)
    line = re.sub(r"\*\*|__|`", "", line)
    line = re.sub(r"\s*\(#\d+(?:,\s*#\d+)*\)", "", line)
    line = re.sub(r"(?<![\w&])#(\d+)\b", "", line)
    return re.sub(r"\s{2,}", " ", line).strip()


def sections(text: str) -> dict[str, list[str]]:
    """{heading: [lines]} of the '### ' sections above the '---' line."""
    head, out, current = strip_comments(text).split("\n---", 1)[0], {}, None
    for line in head.splitlines():
        if line.startswith("### "):
            current = line[4:].strip()
            out[current] = []
        elif current is not None and line.strip():
            out[current].append(line.strip())
    return out


def bullets(lines: Sequence[str]) -> list[str]:
    """The list items of a section ("- " or "* "), without the marker."""
    return [line[2:].strip() for line in lines if line.startswith(("- ", "* "))]


def summary(text: str) -> str:
    """The first paragraph of the notes."""
    return strip_comments(text).split("\n\n", 1)[0].strip()


def whats_new(text: str, limit: int = WHATS_NEW_LIMIT) -> str:
    """Plain text for the App Store "What's New": the summary and the New / Fixed bullets, no Markdown."""
    parts = [_plain(summary(text))]
    for heading, lines in sections(text).items():
        if heading.lower().startswith(("new in", "fixed in")):
            parts.append(heading + "\n" + "\n".join("• " + _plain(item) for item in bullets(lines)))
    out = "\n\n".join(p for p in parts if p)
    return out if len(out) <= limit else out[: limit - 1].rstrip() + "…"


def notes_path(version: str, root: Path = ROOT) -> Path:
    return root / NOTES_DIR / f"{version}.md"


def gather(version: str, since: Optional[str], head: str, token: str, repo: str = REPO) -> str:
    """Draft notes from GitHub: closed issues, commits since the tag and credits."""
    headers = {"Authorization": f"Bearer {token}", "Accept": "application/vnd.github+json",
               "User-Agent": "borealis-release-notes"}
    api = f"{sync.GITHUB_API}/repos/{repo}"
    releases = sync.http("GET", f"{api}/releases?per_page=50", headers)
    start, end = credits.window(releases, since, None)
    tag = since or max(credits.stable_releases(releases), key=lambda r: r["published_at"])["tag_name"]
    closed = credits.github_pages(headers, f"/repos/{repo}/issues?state=closed&since={start}")
    issues = [i for i in bridge.released_issues(closed, start, end) if "pull_request" not in i]
    issue_rows = sorted(((i["number"], i["title"], issue_kind(i)) for i in issues), key=lambda r: r[0])
    numbers = {n for n, _, _ in issue_rows}
    compare = sync.http("GET", f"{api}/compare/{tag}...{head}?per_page=250", headers)
    commits = []
    for c in compare.get("commits", []):
        found = classify_commit(c["commit"]["message"])
        if found and not (set(int(n) for n in ISSUE_REF_RE.findall(found[1])) & numbers):
            commits.append(found)
    credits_md = bridge.credits_markdown(credits.entries(closed, start, end))
    return render(version, tag, issue_rows, commits, credits_md, date.today())


def main(argv: Optional[Sequence[str]] = None) -> int:
    p = argparse.ArgumentParser(description="Draft or check Borealis release notes")
    p.add_argument("version")
    p.add_argument("--since", help="previous release tag (default: the latest published release)")
    p.add_argument("--head", default="main", help="the branch or commit the release is cut from (default main)")
    p.add_argument("--stdout", action="store_true", help="print the draft instead of writing the file")
    p.add_argument("--force", action="store_true", help="replace an existing notes file")
    p.add_argument("--check", action="store_true", help="only check the committed notes file (CS1)")
    args = p.parse_args(argv)
    path = notes_path(args.version)
    if args.check:
        if not path.exists():
            print(f"error: {path} does not exist", file=sys.stderr)
            return 2
        findings = cs1_findings(path.read_text(encoding="utf-8"))
        print("\n".join(f"CS1: {f}" for f in findings) or "CS1: clean")
        return 1 if findings else 0
    token = (os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN") or "").strip()
    if not token:
        print("error: GH_TOKEN or GITHUB_TOKEN must be set", file=sys.stderr)
        return 2
    try:
        text = gather(args.version, args.since, args.head, token)
    except (sync.ApiError, ValueError) as err:
        print(f"error: {err}", file=sys.stderr)
        return 2
    for f in cs1_findings(text):
        print(f"warning: CS1: {f}", file=sys.stderr)
    if args.stdout:
        print(text)
        return 0
    if path.exists() and not args.force:
        print(f"error: {path} exists; pass --force to replace it", file=sys.stderr)
        return 2
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    print(f"wrote {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
