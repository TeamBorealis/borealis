"""The website side of the release automation (B-164, RL5a 2): the private repo TeamBorealis/borealis-website,
deployed by Vercel from its `main` branch. Publishing the site = merging `release-<version>` into `main`.

The workflow passes a write deploy key (secret WEBSITE_DEPLOY_KEY, written to a private temp file whose path is in
WEBSITE_DEPLOY_KEY_PATH). WEBSITE_REMOTE overrides the remote (a local dry run can use the HTTPS URL and the
developer's own git credentials). With dry_run, clones, fetches, tests and builds run; pushes are only printed.
"""
from __future__ import annotations

import os
import re
import subprocess
import tempfile
import time
import urllib.request
from pathlib import Path
from typing import Callable, Optional, Sequence

import plan

REMOTE = "git@github.com:TeamBorealis/borealis-website.git"
SITE = "https://borealisbuilder.com"
CONFIG = "src/site.config.ts"
CONFIG_VERSION_RE = re.compile(r'^(const version = ")(\d+\.\d+\.\d+)(";)$', re.M)
SCRIPT_RE = re.compile(r'src="(/assets/index-[^"]+\.js)"')
LIVE_TIMEOUT_S = 20 * 60
LIVE_POLL_S = 30


def branch_name(version: str) -> str:
    return f"release-{version}"


def site_version(config_text: str) -> Optional[str]:
    m = CONFIG_VERSION_RE.search(config_text)
    return m.group(2) if m else None


def bump_config(config_text: str, version: str) -> str:
    if len(CONFIG_VERSION_RE.findall(config_text)) != 1:
        raise ValueError(f"{CONFIG}: expected one `const version = \"x.y.z\";` line")
    return CONFIG_VERSION_RE.sub(lambda m: m.group(1) + version + m.group(3), config_text)


def base_branch(branches: Sequence[str], unmerged: Sequence[str], version: str) -> str:
    """Where a new release branch starts: the newest older release branch not yet merged into main (its work, such
    as guide screenshots, carries over when a release folds into this one), else main."""
    older = [b for b in branches if b in unmerged and b.startswith("release-")
             and re.fullmatch(r"release-\d+\.\d+\.\d+", b)
             and plan.parse_version(b[8:]) < plan.parse_version(version)]
    return max(older, key=lambda b: plan.parse_version(b[8:]), default="main")


def live_has_version(html: str, script: str, version: str) -> bool:
    return f'"{version}"' in script or f"Borealis_{version}_" in html or f"Borealis {version}" in html


class Website:
    def __init__(self, dry_run: bool, identity: tuple[str, str], log: Callable[[str], None] = print,
                 remote: Optional[str] = None, key_path: Optional[str] = None):
        self.dry_run, self.identity, self.log = dry_run, identity, log
        self.remote = remote or os.environ.get("WEBSITE_REMOTE") or REMOTE
        self.key_path = key_path or os.environ.get("WEBSITE_DEPLOY_KEY_PATH") or None
        self._dir: Optional[Path] = None

    @classmethod
    def available(cls) -> bool:
        return bool(os.environ.get("WEBSITE_REMOTE") or os.environ.get("WEBSITE_DEPLOY_KEY_PATH"))

    # --- git ------------------------------------------------------------------------------------------------

    def _env(self) -> dict:
        env = dict(os.environ)
        if self.key_path:
            env["GIT_SSH_COMMAND"] = (f"ssh -i {self.key_path} -o IdentitiesOnly=yes "
                                      "-o StrictHostKeyChecking=accept-new")
        name, email = self.identity
        env.update(GIT_AUTHOR_NAME=name, GIT_AUTHOR_EMAIL=email, GIT_COMMITTER_NAME=name, GIT_COMMITTER_EMAIL=email)
        return env

    def git(self, *args: str, check: bool = True) -> str:
        cwd = str(self.dir) if args[0] != "clone" else None
        res = subprocess.run(["git", *args], cwd=cwd, env=self._env(), capture_output=True, text=True)
        if check and res.returncode != 0:
            raise RuntimeError(f"website: git {args[0]} failed: {res.stderr.strip()[-300:]}")
        return res.stdout.strip()

    @property
    def dir(self) -> Path:
        if self._dir is None:
            self._dir = Path(tempfile.mkdtemp(prefix="borealis-website-")) / "site"
            self.git("clone", "--quiet", "--no-single-branch", self.remote, str(self._dir))
        return self._dir

    def run(self, *cmd: str) -> None:
        res = subprocess.run(list(cmd), cwd=self.dir, capture_output=True, text=True)
        if res.returncode != 0:
            raise RuntimeError(f"website: {' '.join(cmd)} failed: {(res.stdout + res.stderr).strip()[-500:]}")

    def push(self, *refspecs: str) -> None:
        what = f"push {' '.join(refspecs)} to borealis-website"
        self.log(("DRY-RUN would: " if self.dry_run else "") + what)
        if not self.dry_run:
            self.git("push", "--atomic", "origin", *refspecs)

    def branches(self) -> list[str]:
        out = self.git("branch", "-r", "--format=%(refname:short)")
        return [b.split("/", 1)[1] for b in out.splitlines() if "/" in b and not b.endswith("/HEAD")]

    def unmerged(self) -> list[str]:
        out = self.git("branch", "-r", "--no-merged", "origin/main", "--format=%(refname:short)")
        return [b.split("/", 1)[1] for b in out.splitlines() if "/" in b]

    # --- release steps --------------------------------------------------------------------------------------

    def branch_state(self, version: str) -> str:
        """'ready' when release-<version> exists and links that version's downloads, else what is wrong."""
        branch = branch_name(version)
        if branch not in self.branches():
            return f"no {branch} branch"
        found = site_version(self.git("show", f"origin/{branch}:{CONFIG}"))
        return "ready" if found == version else f"{branch} links version {found}, not {version}"

    def merged(self, version: str) -> bool:
        branch = branch_name(version)
        if branch not in self.branches():
            return False
        res = subprocess.run(["git", "merge-base", "--is-ancestor", f"origin/{branch}", "origin/main"],
                             cwd=self.dir, env=self._env())
        return res.returncode == 0

    def ensure_branch(self, version: str) -> str:
        """Create release-<version> with the download links bumped, when it is missing. Returns what it did."""
        branch = branch_name(version)
        if branch in self.branches():
            return f"{branch} exists"
        base = base_branch(self.branches(), self.unmerged(), version)
        self.git("checkout", "--quiet", "-B", branch, f"origin/{base}")
        path = self.dir / CONFIG
        path.write_text(bump_config(path.read_text(encoding="utf-8"), version), encoding="utf-8")
        self.git("commit", "--quiet", "-am", f"chore: Borealis {version} download links")
        self.push(f"HEAD:refs/heads/{branch}")
        return f"created {branch} from {base} with the {version} download links"

    def check(self, version: str) -> str:
        """'ready' when the branch has the version and passes `npm test` and `npm run build`."""
        state = self.branch_state(version)
        if state != "ready":
            return state
        self.git("checkout", "--quiet", "-B", branch_name(version), f"origin/{branch_name(version)}")
        try:
            self.run("npm", "ci", "--no-audit", "--no-fund")
            self.run("npm", "test")
            self.run("npm", "run", "build")
        except RuntimeError as err:
            return str(err)
        return "ready"

    def publish(self, version: str) -> str:
        """Refresh the changelog on the release branch, then merge it into main (Vercel deploys main)."""
        branch = branch_name(version)
        if self.merged(version):
            return f"{branch} is already merged into main"
        self.git("checkout", "--quiet", "-B", branch, f"origin/{branch}")
        self.run("npm", "ci", "--no-audit", "--no-fund")
        self.run("npm", "run", "changelog:fetch")
        if self.git("status", "--porcelain"):
            self.git("commit", "--quiet", "-am", f"chore: changelog for Borealis {version}")
        self.git("checkout", "--quiet", "-B", "main", "origin/main")
        ff = subprocess.run(["git", "merge", "--ff-only", "--quiet", branch], cwd=self.dir, env=self._env())
        if ff.returncode != 0:
            self.git("merge", "--no-ff", "--quiet", "-m", f"chore: publish Borealis {version}", branch)
        self.push("refs/heads/main:refs/heads/main", f"refs/heads/{branch}:refs/heads/{branch}")
        return f"merged {branch} into main"

    def wait_live(self, version: str, timeout_s: int = LIVE_TIMEOUT_S) -> bool:
        if self.dry_run:
            self.log(f"DRY-RUN would: wait for {SITE} to serve {version}")
            return True
        deadline = time.time() + timeout_s
        while time.time() < deadline:
            if self.is_live(version):
                return True
            time.sleep(LIVE_POLL_S)
        return False

    @staticmethod
    def is_live(version: str) -> bool:
        try:
            html = _fetch(SITE + "/download")
            m = SCRIPT_RE.search(html) or SCRIPT_RE.search(_fetch(SITE + "/"))
            script = _fetch(SITE + m.group(1)) if m else ""
        except OSError:
            return False
        return live_has_version(html, script, version)

    def indexnow(self) -> str:
        what = "submit the site to IndexNow"
        self.log(("DRY-RUN would: " if self.dry_run else "") + what)
        if self.dry_run:
            return "dry run"
        self.git("checkout", "--quiet", "-B", "main", "origin/main")
        res = subprocess.run(["node", "scripts/indexnow.mjs"], cwd=self.dir, capture_output=True, text=True)
        out = (res.stdout + res.stderr).strip()
        if res.returncode != 0:
            raise RuntimeError(f"website: IndexNow failed: {out[-300:]}")
        return out.splitlines()[-1] if out else "IndexNow done"


def _fetch(url: str) -> str:
    req = urllib.request.Request(url, headers={"User-Agent": "borealis-release-auto", "Cache-Control": "no-cache"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        return resp.read().decode("utf-8", "replace")
