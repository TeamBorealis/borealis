"""App Store Connect API for the release automation (B-164): the iOS version, its build, review and release.

Credentials come from the environment: ASC_KEY_ID, ASC_ISSUER_ID and ASC_KEY_PATH (a file holding the .p8 key; the
workflow decodes the base64 secret ASC_API_KEY_P8 of the `release` environment into a private temp file). The JWT is signed with `openssl`, so this stays
standard-library Python. Nothing here prints the key or the token.

With dry_run every GET runs and every write is only printed.
"""
from __future__ import annotations

import base64
import json
import os
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Optional, Sequence

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "discord-issues"))
import sync  # noqa: E402

import decide  # noqa: E402

API = "https://api.appstoreconnect.apple.com"
BUNDLE_ID = "app.borealis"
TOKEN_LIFETIME_S = 600
PLACEHOLDER = "<new>"  # a dry run's stand-in for an object it would have created


def der_to_raw(der: bytes) -> bytes:
    """openssl's DER ECDSA signature (SEQUENCE of two INTEGERs) -> the raw r||s that JWS ES256 wants."""
    if der[0] != 0x30:
        raise ValueError("not a DER sequence")
    i = 2 if der[1] < 0x80 else 2 + (der[1] & 0x7F)
    out = b""
    for _ in range(2):
        if der[i] != 0x02:
            raise ValueError("not a DER integer")
        n = der[i + 1]
        out += der[i + 2:i + 2 + n].lstrip(b"\x00").rjust(32, b"\x00")
        i += 2 + n
    return out


def _b64(data: bytes) -> bytes:
    return base64.urlsafe_b64encode(data).rstrip(b"=")


def state_of(version: dict) -> str:
    a = version.get("attributes", {})
    return a.get("appStoreState") or a.get("appVersionState") or "UNKNOWN"


@dataclass(frozen=True)
class VersionChoice:
    action: str  # use | rename | create | blocked
    version: Optional[dict]
    reason: str


def choose_version(versions: Sequence[dict], target: str) -> VersionChoice:
    """Which iOS appStoreVersion carries `target`. Only one version can be in progress at a time: an editable one
    is renamed; one of another version in review or awaiting release blocks the submission (ios-supersede)."""
    ios = [v for v in versions if v.get("attributes", {}).get("platform", "IOS") == "IOS"]
    same = [v for v in ios if v["attributes"].get("versionString") == target]
    if same:
        return VersionChoice("use", same[0], f"iOS version {target} exists ({state_of(same[0])})")
    editable = [v for v in ios if state_of(v) in decide.IOS_EDITABLE]
    if editable:
        old = editable[0]["attributes"].get("versionString")
        return VersionChoice("rename", editable[0], f"rename the editable iOS version {old} to {target}")
    busy = [v for v in ios if state_of(v) in decide.IOS_IN_REVIEW | decide.IOS_APPROVED]
    if busy:
        old = busy[0]["attributes"].get("versionString")
        return VersionChoice("blocked", busy[0], f"iOS version {old} is {state_of(busy[0])}; it must be withdrawn "
                                                 f"before {target} can be submitted (step ios-supersede)")
    return VersionChoice("create", None, f"create iOS version {target}")


def pick_build(builds: Sequence[dict]) -> Optional[dict]:
    """The newest processed, unexpired build, else None."""
    ok = [b for b in builds if b.get("attributes", {}).get("processingState") == "VALID"
          and not b["attributes"].get("expired")]
    return max(ok, key=lambda b: b["attributes"].get("uploadedDate", ""), default=None)


class Asc:
    def __init__(self, key_id: str, issuer_id: str, key_path: str, dry_run: bool,
                 log: Callable[[str], None] = print):
        self.key_id, self.issuer_id, self.key_path = key_id, issuer_id, key_path
        self.dry_run, self.log = dry_run, log
        self._token, self._token_at = "", 0.0
        self._app_id: Optional[str] = None

    @classmethod
    def from_env(cls, dry_run: bool, log: Callable[[str], None] = print) -> Optional["Asc"]:
        key_id, issuer, path = (os.environ.get(n, "").strip() for n in ("ASC_KEY_ID", "ASC_ISSUER_ID", "ASC_KEY_PATH"))
        if not (key_id and issuer and path and Path(path).is_file()):
            return None
        return cls(key_id, issuer, path, dry_run, log)

    @classmethod
    def from_config(cls, config: Path, dry_run: bool, log: Callable[[str], None] = print) -> "Asc":
        """The local key (~/.config/borealis-appstore/config.env: KEY_ID, ISSUER_ID, KEY_PATH), for local dry runs."""
        env = {}
        for line in config.read_text(encoding="utf-8").splitlines():
            if "=" in line and not line.lstrip().startswith("#"):
                k, v = line.split("=", 1)
                env[k.strip()] = v.strip().strip('"').strip("'")
        return cls(env["KEY_ID"], env["ISSUER_ID"], os.path.expanduser(env["KEY_PATH"]), dry_run, log)

    # --- plumbing -------------------------------------------------------------------------------------------

    def token(self) -> str:
        if self._token and time.time() - self._token_at < TOKEN_LIFETIME_S - 60:
            return self._token
        now = int(time.time())
        head = _b64(json.dumps({"alg": "ES256", "kid": self.key_id, "typ": "JWT"}).encode())
        body = _b64(json.dumps({"iss": self.issuer_id, "iat": now, "exp": now + TOKEN_LIFETIME_S,
                                "aud": "appstoreconnect-v1"}).encode())
        msg = head + b"." + body
        sig = subprocess.run(["openssl", "dgst", "-sha256", "-sign", self.key_path], input=msg,
                             capture_output=True, check=True).stdout
        self._token, self._token_at = (msg + b"." + _b64(der_to_raw(sig))).decode(), time.time()
        return self._token

    def _call(self, method: str, path: str, body: Optional[dict] = None):
        headers = {"Authorization": f"Bearer {self.token()}", "User-Agent": "borealis-release-auto"}
        return sync.http(method, API + path, headers, body, log_path=path.split("?")[0])

    def get(self, path: str):
        return self._call("GET", path)

    def write(self, method: str, path: str, body: dict, what: str):
        self.log(("DRY-RUN would: " if self.dry_run else "") + what)
        return None if self.dry_run else self._call(method, path, body)

    # --- reads ----------------------------------------------------------------------------------------------

    def app_id(self) -> str:
        if self._app_id is None:
            apps = self.get(f"/v1/apps?filter[bundleId]={BUNDLE_ID}")["data"]
            if not apps:
                raise ValueError(f"no App Store Connect app for {BUNDLE_ID}")
            self._app_id = apps[0]["id"]
        return self._app_id

    def versions(self) -> list[dict]:
        return self.get(f"/v1/apps/{self.app_id()}/appStoreVersions?filter[platform]=IOS&limit=20")["data"]

    def version(self, target: str) -> Optional[dict]:
        return next((v for v in self.versions() if v["attributes"].get("versionString") == target), None)

    def ios_state(self, target: str) -> Optional[str]:
        v = self.version(target)
        return state_of(v) if v else None

    def build(self, target: str) -> Optional[dict]:
        """The newest processed build of `target` (its number is normally build_number(target); release.yml may
        upload again with an override such as 1003000.1)."""
        builds = self.get(f"/v1/builds?filter[app]={self.app_id()}&filter[preReleaseVersion.version]={target}"
                          "&sort=-uploadedDate&limit=10")["data"]
        return pick_build(builds)

    def build_of(self, version_id: str) -> Optional[dict]:
        if version_id == PLACEHOLDER:
            return None
        return self.get(f"/v1/appStoreVersions/{version_id}/build").get("data")

    # --- writes ---------------------------------------------------------------------------------------------

    def ensure_version(self, target: str) -> tuple[str, str]:
        """(version id, what was done); raises ValueError when another version blocks it."""
        choice = choose_version(self.versions(), target)
        if choice.action == "blocked":
            raise ValueError(choice.reason)
        if choice.action == "use":
            v = choice.version
            if v["attributes"].get("releaseType") != "MANUAL" and state_of(v) in decide.IOS_EDITABLE:
                self.write("PATCH", f"/v1/appStoreVersions/{v['id']}", {"data": {
                    "type": "appStoreVersions", "id": v["id"], "attributes": {"releaseType": "MANUAL"}}},
                    f"set iOS {target} to manual release")
            return v["id"], choice.reason
        if choice.action == "rename":
            v = choice.version
            self.write("PATCH", f"/v1/appStoreVersions/{v['id']}", {"data": {
                "type": "appStoreVersions", "id": v["id"],
                "attributes": {"versionString": target, "releaseType": "MANUAL"}}}, choice.reason)
            return v["id"], choice.reason
        made = self.write("POST", "/v1/appStoreVersions", {"data": {
            "type": "appStoreVersions",
            "attributes": {"platform": "IOS", "versionString": target, "releaseType": "MANUAL"},
            "relationships": {"app": {"data": {"type": "apps", "id": self.app_id()}}}}}, choice.reason)
        return (made["data"]["id"] if made else PLACEHOLDER), choice.reason

    def attach_build(self, version_id: str, build: dict) -> None:
        attrs = build["attributes"]
        if attrs.get("usesNonExemptEncryption") is None:
            # Same answer as every earlier build: standard HTTPS/TLS only, exempt.
            self.write("PATCH", f"/v1/builds/{build['id']}", {"data": {
                "type": "builds", "id": build["id"], "attributes": {"usesNonExemptEncryption": False}}},
                f"answer export compliance (exempt) for build {attrs.get('version')}")
        current = self.build_of(version_id)
        if current and current["id"] == build["id"]:
            return
        self.write("PATCH", f"/v1/appStoreVersions/{version_id}/relationships/build",
                   {"data": {"type": "builds", "id": build["id"]}}, f"attach build {attrs.get('version')}")

    def set_whats_new(self, version_id: str, text: str) -> None:
        if version_id == PLACEHOLDER:
            self.log(f"DRY-RUN would: set What's New ({len(text)} characters)")
            return
        locs = self.get(f"/v1/appStoreVersions/{version_id}/appStoreVersionLocalizations")["data"]
        for loc in locs:
            if loc["attributes"].get("whatsNew") == text:
                continue
            try:
                self.write("PATCH", f"/v1/appStoreVersionLocalizations/{loc['id']}", {"data": {
                    "type": "appStoreVersionLocalizations", "id": loc["id"], "attributes": {"whatsNew": text}}},
                    f"set What's New ({loc['attributes'].get('locale')}, {len(text)} characters)")
            except sync.ApiError as err:
                # The app's first App Store version has no What's New; Apple answers 409.
                self.log(f"What's New not set for {loc['attributes'].get('locale')}: {err}")

    def submit(self, version_id: str) -> str:
        """Add the version to a review submission and submit it (idempotent)."""
        app = self.app_id()
        subs = self.get(f"/v1/reviewSubmissions?filter[app]={app}&filter[platform]=IOS"
                        "&filter[state]=READY_FOR_REVIEW&limit=5")["data"]
        if subs:
            sub_id = subs[0]["id"]
        else:
            made = self.write("POST", "/v1/reviewSubmissions", {"data": {
                "type": "reviewSubmissions", "attributes": {"platform": "IOS"},
                "relationships": {"app": {"data": {"type": "apps", "id": app}}}}}, "create a review submission")
            sub_id = made["data"]["id"] if made else PLACEHOLDER
        items = [] if sub_id == PLACEHOLDER else self.get(
            f"/v1/reviewSubmissions/{sub_id}/items?include=appStoreVersion")["data"]
        linked = {((i.get("relationships", {}).get("appStoreVersion") or {}).get("data") or {}).get("id")
                  for i in items}
        if version_id not in linked:
            self.write("POST", "/v1/reviewSubmissionItems", {"data": {
                "type": "reviewSubmissionItems", "relationships": {
                    "reviewSubmission": {"data": {"type": "reviewSubmissions", "id": sub_id}},
                    "appStoreVersion": {"data": {"type": "appStoreVersions", "id": version_id}}}}},
                "add the iOS version to the review submission")
        self.write("PATCH", f"/v1/reviewSubmissions/{sub_id}", {"data": {
            "type": "reviewSubmissions", "id": sub_id, "attributes": {"submitted": True}}}, "submit for review")
        return sub_id

    def release(self, version_id: str, target: str) -> None:
        self.write("POST", "/v1/appStoreVersionReleaseRequests", {"data": {
            "type": "appStoreVersionReleaseRequests",
            "relationships": {"appStoreVersion": {"data": {"type": "appStoreVersions", "id": version_id}}}}},
            f"release iOS {target} on the App Store")

    def supersede(self, target: str) -> str:
        """Withdraw another version's waiting review so `target` can take its place (manual step only)."""
        choice = choose_version(self.versions(), target)
        if choice.action != "blocked":
            return f"nothing to withdraw: {choice.reason}"
        old = choice.version
        if state_of(old) in decide.IOS_APPROVED:
            return (f"iOS {old['attributes']['versionString']} is approved and waiting; the API cannot withdraw it. "
                    "In App Store Connect open that version and choose to reject the binary, then run submit again.")
        subs = self.get(f"/v1/reviewSubmissions?filter[app]={self.app_id()}&filter[platform]=IOS"
                        "&filter[state]=WAITING_FOR_REVIEW,IN_REVIEW,UNRESOLVED_ISSUES&limit=5")["data"]
        if not subs:
            return f"no open review submission found for iOS {old['attributes']['versionString']}"
        for sub in subs:
            self.write("PATCH", f"/v1/reviewSubmissions/{sub['id']}", {"data": {
                "type": "reviewSubmissions", "id": sub["id"], "attributes": {"canceled": True}}},
                f"cancel the review submission of iOS {old['attributes']['versionString']}")
        return f"withdrew iOS {old['attributes']['versionString']}; the next submit renames it to {target}"
