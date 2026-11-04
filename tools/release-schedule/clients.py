"""GitHub, git and Discord plumbing for the release automation (auto.py). Writes honour dry_run: they are
printed, never sent. Errors never carry tokens (sync.http reports method, path and status only)."""
from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Callable, Optional, Sequence

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "discord-issues"))
import sync  # noqa: E402

import decide  # noqa: E402
import remind  # noqa: E402

ROOT = HERE.parents[1]
MOD_CHAT = remind.MOD_CHAT
HISTORY_PAGES = 3  # #mod-chat messages read for posted keys: 3 x 100


def _say(dry_run: bool, log: Callable[[str], None], what: str) -> None:
    log(("DRY-RUN would: " if dry_run else "") + what)


@dataclass(frozen=True)
class Tag:
    name: str
    sha: str  # the commit
    date: datetime


class GitHub:
    def __init__(self, token: str, repo: str, dry_run: bool, log: Callable[[str], None] = print):
        self.repo, self.dry_run, self.log = repo, dry_run, log
        self._headers = {"Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28",
                         "User-Agent": "borealis-release-auto"}
        if token:  # a dry run without a token reads the public API anonymously
            self._headers["Authorization"] = f"Bearer {token}"

    def call(self, method: str, path: str, body: Optional[dict] = None):
        return sync.http(method, f"{sync.GITHUB_API}/repos/{self.repo}{path}", self._headers, body)

    def write(self, method: str, path: str, body: dict, what: str):
        _say(self.dry_run, self.log, what)
        return None if self.dry_run else self.call(method, path, body)

    def tag(self, name: str) -> Optional[Tag]:
        try:
            ref = self.call("GET", f"/git/ref/tags/{name}")
        except sync.ApiError as err:
            if err.status == 404:
                return None
            raise
        obj = ref["object"]
        if obj["type"] == "tag":
            tag = self.call("GET", f"/git/tags/{obj['sha']}")
            return Tag(name, tag["object"]["sha"], _when(tag["tagger"]["date"]))
        commit = self.call("GET", f"/git/commits/{obj['sha']}")
        return Tag(name, obj["sha"], _when(commit["committer"]["date"]))

    def runs(self, workflow: str, sha: str) -> list[dict]:
        return self.call("GET", f"/actions/workflows/{workflow}/runs?head_sha={sha}&per_page=30")["workflow_runs"]

    def ci(self, workflow: str, sha: str) -> str:
        return decide.ci_status(self.runs(workflow, sha))

    def release(self, tag: str) -> Optional[dict]:
        """The release (draft or published) for `tag`; drafts are listed only with push access."""
        return next((r for r in self.call("GET", "/releases?per_page=30") if r.get("tag_name") == tag), None)

    def dispatch(self, workflow: str, ref: str, inputs: Optional[dict] = None) -> None:
        body = {"ref": ref, **({"inputs": inputs} if inputs else {})}
        self.write("POST", f"/actions/workflows/{workflow}/dispatches", body,
                   f"run {workflow} on {ref}" + (f" with {json.dumps(inputs)}" if inputs else ""))


def _when(text: str) -> datetime:
    return datetime.fromisoformat(text.replace("Z", "+00:00"))


class Git:
    """The local checkout of the app repo (the workflow's, with credentials to push to origin)."""

    def __init__(self, root: Path, identity: tuple[str, str], dry_run: bool, log: Callable[[str], None] = print):
        self.root, self.identity, self.dry_run, self.log = root, identity, dry_run, log

    def _env(self) -> dict:
        name, email = self.identity
        return {**os.environ, "GIT_AUTHOR_NAME": name, "GIT_AUTHOR_EMAIL": email,
                "GIT_COMMITTER_NAME": name, "GIT_COMMITTER_EMAIL": email}

    def run(self, *args: str, cwd: Optional[Path] = None, check: bool = True) -> str:
        res = subprocess.run(["git", *args], cwd=str(cwd or self.root), env=self._env(), capture_output=True,
                             text=True)
        if check and res.returncode != 0:
            raise RuntimeError(f"git {args[0]} failed: {res.stderr.strip()[-300:]}")
        return res.stdout.strip()

    def fetch_main(self) -> str:
        self.run("fetch", "--quiet", "origin", "+refs/heads/main:refs/remotes/origin/main")
        return self.run("rev-parse", "origin/main")

    def show(self, rev: str, path: str) -> Optional[str]:
        res = subprocess.run(["git", "show", f"{rev}:{path}"], cwd=str(self.root), capture_output=True, text=True)
        return res.stdout if res.returncode == 0 else None

    def worktree(self) -> Path:
        path = Path(tempfile.mkdtemp(prefix="borealis-release-")) / "wt"
        self.run("worktree", "add", "--quiet", "--detach", str(path), "origin/main")
        return path

    def drop_worktree(self, path: Path) -> None:
        self.run("worktree", "remove", "--force", str(path), check=False)

    def push(self, wt: Path, *refspecs: str) -> None:
        _say(self.dry_run, self.log, f"push {' '.join(refspecs)}")
        if not self.dry_run:
            self.run("push", "--atomic", "origin", *refspecs, cwd=wt)

    def commit_to_main(self, message: str, change: Callable[[Path], Sequence[str]], attempts: int = 3) -> str:
        """Apply `change` (returns the changed paths) on a fresh worktree of origin/main, commit and push to main.
        Retries on a moved main. Returns the commit sha (or 'dry-run')."""
        for attempt in range(attempts):
            self.fetch_main()
            wt = self.worktree()
            try:
                paths = list(change(wt))
                if not paths:
                    return "unchanged"
                self.run("add", "--", *paths, cwd=wt)
                self.run("commit", "--quiet", "-m", message, cwd=wt)
                sha = self.run("rev-parse", "HEAD", cwd=wt)
                if self.dry_run:
                    _say(True, self.log, f"push {sha[:7]} \"{message}\" to main")
                    return "dry-run"
                res = subprocess.run(["git", "push", "origin", "HEAD:refs/heads/main"], cwd=str(wt),
                                     env=self._env(), capture_output=True, text=True)
                if res.returncode == 0:
                    self.log(f"pushed {sha[:7]} \"{message}\" to main")
                    return sha
                self.log(f"push to main refused (attempt {attempt + 1}): {res.stderr.strip()[-200:]}")
            finally:
                self.drop_worktree(wt)
        raise RuntimeError(f"could not push \"{message}\" to main after {attempts} attempts")


class Discord:
    """Idempotent #mod-chat notes: a note whose key is already in the channel is not posted again."""

    def __init__(self, token: Optional[str], dry_run: bool, log: Callable[[str], None] = print,
                 posted: Optional[frozenset[str]] = None):
        self.dry_run, self.log = dry_run, log
        self.bridge = sync.Bridge(dry_run, token, "", "TeamBorealis/borealis") if token else None
        self._posted = posted

    def posted(self) -> frozenset[str]:
        if self._posted is None:
            if self.bridge is None:
                self._posted = frozenset()
            else:
                bot = self.bridge.discord("GET", "/users/@me")["id"]
                messages, before = [], ""
                for _ in range(HISTORY_PAGES):
                    page = self.bridge.discord("GET", f"/channels/{MOD_CHAT}/messages?limit=100{before}")
                    messages += page
                    if len(page) < 100:
                        break
                    before = f"&before={page[-1]['id']}"
                self._posted = frozenset(k for k in remind.posted_keys(messages, bot) if k.startswith(decide.KEY_PREFIX))
        return self._posted

    def post(self, note: decide.Note) -> bool:
        """Post once; returns True when posted (or, in a dry run, when it would be)."""
        if note.key in self.posted():
            return False
        msg = note.message()
        if self.dry_run or self.bridge is None:
            self.log(f"DRY-RUN would post to #mod-chat [{note.key}]:\n  {msg['content']}\n  "
                     + msg["embeds"][0]["description"].replace("\n", "\n  "))
        else:
            self.bridge.discord("POST", f"/channels/{MOD_CHAT}/messages", msg)
            self.log(f"posted to #mod-chat: {note.key}")
        self._posted = self.posted() | {note.key}
        return True
