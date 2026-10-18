#!/usr/bin/env python3
"""Clone the public Aurora community repositories of tools/ci/aurora-repos.txt at their pinned
commits into <dest>/repos/<owner>__<repo>, the layout of the local corpus (BOREALIS_CORPUS_DIR),
and write <dest>/index.json with the Aurora character files the character corpus test reads.

Used by .github/workflows/acceptance.yml. Prints counts only. Python 3.9+, stdlib only.
Exit status: 0 every repository fetched, 1 a fetch failed, 2 usage error.
"""
import argparse
import json
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

MANIFEST = Path(__file__).resolve().parent / "aurora-repos.txt"
EMPTY = "-"  # a repository with no commits: an empty folder, as in the local corpus
ATTEMPTS = 3
PARALLEL = 8
BOM_AND_SPACE = b"\xef\xbb\xbf \r\n\t"


def read_manifest(path):
    """[(slug, sha)] from `<owner>/<repo> <sha|->` lines; '#' lines and blanks are ignored."""
    rows = []
    for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) != 2 or parts[0].count("/") != 1:
            raise ValueError(f"{path.name}:{n}: expected '<owner>/<repo> <sha|->'")
        slug, sha = parts
        if sha != EMPTY and (len(sha) != 40 or any(c not in "0123456789abcdef" for c in sha)):
            raise ValueError(f"{path.name}:{n}: not a full commit sha")
        rows.append((slug, sha))
    return rows


def git(args, cwd):
    return subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True).returncode == 0


def fetch(repos_dir, slug, sha):
    """Shallow-fetch one commit and check it out. Returns (slug, ok)."""
    dest = repos_dir / slug.replace("/", "__")
    dest.mkdir(parents=True, exist_ok=True)
    if sha == EMPTY:
        return slug, True
    url = f"https://github.com/{slug}.git"
    for _ in range(ATTEMPTS):
        if (git(["init", "-q"], dest)
                and git(["fetch", "-q", "--depth", "1", url, sha], dest)
                and git(["-c", "advice.detachedHead=false", "checkout", "-q", "FETCH_HEAD"], dest)):
            return slug, True
    return slug, False


def is_aurora_character(path):
    """An Aurora .dnd5e is XML; JSON .dnd5e files are another tool's (corpus/tools/index.py rule)."""
    return path.read_bytes().lstrip(BOM_AND_SPACE)[:1] == b"<"


def character_rows(root):
    rows = []
    repos = root / "repos"
    for repo in sorted(p.name for p in repos.iterdir() if p.is_dir()):
        for dirpath, dirnames, filenames in os.walk(repos / repo):
            dirnames[:] = sorted(d for d in dirnames if d != ".git")
            for name in sorted(filenames):
                path = Path(dirpath) / name
                if name.lower().endswith(".dnd5e") and is_aurora_character(path):
                    rel = path.relative_to(root).as_posix()
                    rows.append({"path": rel, "repo": repo, "kind": "character"})
    return rows


def count_files(root):
    total = 0
    for _, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d != ".git"]
        total += len(filenames)
    return total


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("dest", type=Path, help="corpus root to create (gets repos/ and index.json)")
    ap.add_argument("--manifest", type=Path, default=MANIFEST)
    args = ap.parse_args()
    try:
        rows = read_manifest(args.manifest)
    except (OSError, ValueError) as e:
        print(f"manifest error: {e}", file=sys.stderr)
        return 2
    repos_dir = args.dest / "repos"
    repos_dir.mkdir(parents=True, exist_ok=True)
    with ThreadPoolExecutor(PARALLEL) as pool:
        results = list(pool.map(lambda r: fetch(repos_dir, *r), rows))
    failed = [slug for slug, ok in results if not ok]
    chars = character_rows(args.dest)
    (args.dest / "index.json").write_text(json.dumps(chars), encoding="utf-8")
    files = count_files(repos_dir)
    empty = sum(1 for _, sha in rows if sha == EMPTY)
    print(f"aurora corpus: {len(rows) - len(failed)}/{len(rows)} repositories ({empty} empty), "
          f"{files} files, {len(chars)} Aurora character files")
    for slug in failed:  # public repository names, no content
        print(f"::error::could not fetch {slug} at its pinned commit")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
