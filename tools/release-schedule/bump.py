#!/usr/bin/env python3
"""Bump the Borealis version in the eight files every release commit touches (decision RL5a, B-164).

The files and their spots are the ones of "chore: release 1.2.2" (cb131e7): the workspace version in Cargo.toml,
the workspace crates in Cargo.lock, the README version line, tauri.conf.json, the iOS Info.plist and project.yml
(CFBundleShortVersionString and CFBundleVersion), ui/package.json and the two top entries of ui/package-lock.json.
Store build numbers are not stored: they come from the version (`build_number`, 1.2.2 -> 1002002).

Every replacement must match exactly the expected number of times, so a changed file layout fails loudly instead
of producing a half-bumped tree.

    python3 -I tools/release-schedule/bump.py 1.3.0            # bump the checkout this file lives in
    python3 -I tools/release-schedule/bump.py 1.3.0 --check    # only report what would change

Standard library only (Python 3.9+).
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Callable, Mapping, Optional, Sequence

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]  # the app checkout

FILES = (
    "Cargo.toml",
    "Cargo.lock",
    "README.md",
    "src-tauri/tauri.conf.json",
    "src-tauri/gen/apple/borealis_iOS/Info.plist",
    "src-tauri/gen/apple/project.yml",
    "ui/package.json",
    "ui/package-lock.json",
)
VERSION_RE = re.compile(r"^(\d+)\.(\d+)\.(\d+)$")


class BumpError(ValueError):
    pass


def parse(version: str) -> tuple[int, int, int]:
    m = VERSION_RE.match(version)
    if not m:
        raise BumpError(f"version must look like 1.3.0, got {version!r}")
    return int(m.group(1)), int(m.group(2)), int(m.group(3))


def build_number(version: str) -> int:
    """The store build number (iOS CFBundleVersion, Android versionCode): 1.2.2 -> 1002002."""
    major, minor, patch = parse(version)
    if minor > 999 or patch > 999:
        raise BumpError(f"{version}: minor and patch must stay below 1000 for the build number")
    return major * 1_000_000 + minor * 1_000 + patch


def _replace(text: str, pattern: str, new: str, count: int, where: str) -> str:
    regex = re.compile(pattern, re.M)
    found = len(regex.findall(text))
    if found != count:
        raise BumpError(f"{where}: expected {count} match(es) of {pattern!r}, found {found}")
    return regex.sub(lambda m: m.group(1) + new + m.group(2), text)


def _cargo_lock(text: str, old: str, new: str) -> str:
    """Workspace crates only: [[package]] blocks without a `source` line whose version is `old`."""
    blocks = text.split("\n[[package]]\n")
    changed = 0
    out = []
    for block in blocks:
        local = "\nsource = " not in f"\n{block}" and re.search(rf'^version = "{re.escape(old)}"$', block, re.M)
        if local and block.startswith("name = "):
            block = re.sub(rf'^version = "{re.escape(old)}"$', f'version = "{new}"', block, count=1, flags=re.M)
            changed += 1
        out.append(block)
    if changed == 0:
        raise BumpError(f"Cargo.lock: no workspace crate at version {old}")
    return "\n[[package]]\n".join(out)


def _rules(old: str) -> Mapping[str, Callable[[str, str], str]]:
    o = re.escape(old)
    return {
        "Cargo.toml": lambda t, n: _replace(t, rf'(^\[workspace\.package\]\n(?:[^\[\n].*\n|\n)*?version = "){o}(")',
                                            n, 1, "Cargo.toml"),
        "Cargo.lock": lambda t, n: _cargo_lock(t, old, n),
        "README.md": lambda t, n: _replace(t, rf"(<sub>Version ){o}( &nbsp;)", n, 1, "README.md"),
        "src-tauri/tauri.conf.json": lambda t, n: _replace(t, rf'(^  "version": "){o}(",$)', n, 1, "tauri.conf.json"),
        "src-tauri/gen/apple/borealis_iOS/Info.plist": lambda t, n: _replace(
            t, rf"(<key>CFBundle(?:ShortVersionString|Version)</key>\s*<string>){o}(</string>)", n, 2, "Info.plist"),
        "src-tauri/gen/apple/project.yml": lambda t, n: _replace(
            _replace(t, rf"(^\s+CFBundleShortVersionString: ){o}($)", n, 1, "project.yml"),
            rf'(^\s+CFBundleVersion: "){o}("$)', n, 1, "project.yml"),
        "ui/package.json": lambda t, n: _replace(t, rf'(^  "version": "){o}(",$)', n, 1, "ui/package.json"),
        "ui/package-lock.json": lambda t, n: _replace(
            t, rf'("name": "borealis-ui",\n\s*"version": "){o}(")', n, 2, "ui/package-lock.json"),
    }


def bump_texts(texts: Mapping[str, str], old: str, new: str) -> dict[str, str]:
    """New contents for every file in FILES (a pure function; `texts` maps path -> content)."""
    if parse(new) <= parse(old):
        raise BumpError(f"new version {new} must be greater than the current {old}")
    missing = [f for f in FILES if f not in texts]
    if missing:
        raise BumpError(f"missing files: {', '.join(missing)}")
    return {path: rule(texts[path], new) for path, rule in _rules(old).items()}


def current_version(root: Path = ROOT) -> str:
    return json.loads((root / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))["version"]


def bump_tree(root: Path, new: str, write: bool = True) -> list[str]:
    """Bump the checkout at `root`; returns the changed paths. With write=False nothing is written."""
    old = current_version(root)
    texts = {f: (root / f).read_text(encoding="utf-8") for f in FILES}
    bumped = bump_texts(texts, old, new)
    changed = [f for f in FILES if bumped[f] != texts[f]]
    if write:
        for f in changed:
            (root / f).write_text(bumped[f], encoding="utf-8")
    return changed


def main(argv: Optional[Sequence[str]] = None) -> int:
    p = argparse.ArgumentParser(description="Bump the Borealis version in the release files")
    p.add_argument("version", help="the new version, for example 1.3.0")
    p.add_argument("--check", action="store_true", help="only report what would change")
    p.add_argument("--root", type=Path, default=ROOT, help="the app checkout (default: this one)")
    args = p.parse_args(argv)
    try:
        old = current_version(args.root)
        changed = bump_tree(args.root, args.version, write=not args.check)
    except (BumpError, OSError, KeyError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    verb = "would bump" if args.check else "bumped"
    print(f"{verb} {old} -> {args.version} (build {build_number(args.version)}) in {len(changed)} files:")
    print("\n".join(f"  {f}" for f in changed))
    return 0


if __name__ == "__main__":
    sys.exit(main())
