"""Stand-ins for App Store Connect and the website repo, fed from a scenario file, for dry runs of the release
automation (auto.py --scenario). Reads come from the file; writes are only printed, as in any dry run.

A scenario is JSON with any of these keys (each replaces that part of the live state):
  tag {"sha", "date"} | null, main {"sha", "version"}, "ci:ci.yml" / "ci:acceptance.yml" / "ci:release.yml" status,
  release {"id", "draft", "html_url", "body", "name", "assets": [{"name"}]} | null, ios_state, website,
  notes {"text", "source"}, posted [keys already in #mod-chat],
  asc {"versions": [appStoreVersion objects], "builds": [build objects]}   -> FakeAsc
  website_repo {"branch_state": "ready" | problem, "merged": bool}           -> FakeSite
"""
from __future__ import annotations

from typing import Callable, Optional

import asc as asc_api
import website as website_mod


class FakeAsc(asc_api.Asc):
    def __init__(self, data: dict, log: Callable[[str], None] = print):
        super().__init__("scenario", "scenario", "/dev/null", True, log)
        self.data = data

    def token(self) -> str:
        return "scenario"

    def app_id(self) -> str:
        return "scenario-app"

    def versions(self) -> list[dict]:
        return list(self.data.get("versions", []))

    def build(self, target: str) -> Optional[dict]:
        return asc_api.pick_build(self.data.get("builds", []))

    def build_of(self, version_id: str) -> Optional[dict]:
        return None

    def get(self, path: str):
        return {"data": []}  # localizations, review submissions and items: none yet


class FakeSite(website_mod.Website):
    def __init__(self, data: dict, log: Callable[[str], None] = print):
        super().__init__(True, ("scenario", "scenario@example.invalid"), log)
        self.data = data

    def branch_state(self, version: str) -> str:
        return self.data.get("branch_state", "ready")

    def merged(self, version: str) -> bool:
        return bool(self.data.get("merged"))

    def ensure_branch(self, version: str) -> str:
        if self.branch_state(version) == "ready":
            return f"{website_mod.branch_name(version)} exists"
        self.log(f"DRY-RUN would: create {website_mod.branch_name(version)} with the {version} download links")
        return f"would create {website_mod.branch_name(version)}"

    def check(self, version: str) -> str:
        return self.branch_state(version)

    def publish(self, version: str) -> str:
        if self.merged(version):
            return f"{website_mod.branch_name(version)} is already merged into main"
        self.log(f"DRY-RUN would: npm run changelog:fetch on {website_mod.branch_name(version)}, commit, "
                 "merge it into main and push (Vercel deploys main)")
        return f"merged {website_mod.branch_name(version)} into main"

    def indexnow(self) -> str:
        self.log("DRY-RUN would: submit the site to IndexNow")
        return "dry run"
