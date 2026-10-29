#!/usr/bin/env python3
"""Print release-notes Markdown crediting the Discord users whose bridged issues were closed as completed.

The window runs from the `--since` release (default: the latest published stable release) to the `--until`
release (default: now), so run it before publishing a release and paste the output into its notes. Names come
from each issue's credit line (written by sync.py), so only GitHub is read.

Env: GITHUB_TOKEN or GH_TOKEN, GITHUB_REPOSITORY (default TeamBorealis/borealis).
Usage: credits.py [--since TAG] [--until TAG]. Exit 0 (prints nothing when there is nobody to credit),
2 on a bad tag or API error. Python 3.9+, stdlib only.
"""
import argparse
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import bridge  # noqa: E402
import sync  # noqa: E402


def github_pages(headers, path):
    page, items = 1, []
    while True:
        batch = sync.http("GET", f"{sync.GITHUB_API}{path}&per_page=100&page={page}", headers)
        items += batch
        if len(batch) < 100:
            return items
        page += 1


def stable_releases(releases):
    return [r for r in releases if not r.get("draft") and not r.get("prerelease") and r.get("published_at")]


def window(releases, since, until):
    """(start, end) ISO times for the tags; raises ValueError for an unknown or unpublished tag."""
    stable = {r["tag_name"]: r["published_at"] for r in stable_releases(releases)}
    if not stable:
        raise ValueError("no published release")
    since = since or max(stable, key=stable.get)
    for tag in (since, until):
        if tag and tag not in stable:
            raise ValueError(f"{tag} is not a published stable release")
    return stable[since], (stable[until] if until else "9999")


def entries(issues, start, end):
    """[(number, title, verb, name)] for bridged issues closed as completed in (start, end] with a credit line."""
    found = []
    for issue in bridge.released_issues(issues, start, end):
        credit = bridge.credit_of(issue.get("body"))
        if bridge.thread_of(issue.get("body")) and credit and "pull_request" not in issue:
            found.append((issue["number"], issue["title"], *credit))
    return found


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--since", help="previous release tag (default: latest published release)")
    parser.add_argument("--until", help="release tag that ends the window (default: now)")
    args = parser.parse_args(argv)
    token = (os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN") or "").strip()
    if not token:
        print("GITHUB_TOKEN or GH_TOKEN must be set", file=sys.stderr)
        return 2
    repo = os.environ.get("GITHUB_REPOSITORY", "TeamBorealis/borealis")
    headers = {"Authorization": f"Bearer {token}", "Accept": "application/vnd.github+json",
               "User-Agent": "borealis-discord-credits"}
    try:
        releases = sync.http("GET", f"{sync.GITHUB_API}/repos/{repo}/releases?per_page=50", headers)
        start, end = window(releases, args.since, args.until)
        issues = github_pages(headers, f"/repos/{repo}/issues?state=closed&since={start}")
    except (sync.ApiError, ValueError) as err:
        print(f"error: {err}", file=sys.stderr)
        return 2
    markdown = bridge.credits_markdown(entries(issues, start, end))
    if markdown:
        print(markdown)
    else:
        print(f"nobody to credit: no Discord issue closed as completed after {start}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
