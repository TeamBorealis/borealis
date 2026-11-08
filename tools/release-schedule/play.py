"""Google Play Developer API for the release automation (decision RL8, B-236): the signed bundle (.aab) that
release.yml builds goes to the closed testing track (alpha) and is rolled out to its testers. Never production:
that waits for Play's 12 testers x 14 days rule and for a person.

Credentials: PLAY_SERVICE_ACCOUNT_PATH, a file holding the service account's JSON key (the workflow writes the
`release` environment secret PLAY_SERVICE_ACCOUNT_JSON into a private temp file). The OAuth assertion is signed
with `openssl` (RS256), so this stays standard-library Python. Nothing here prints the key or a token.

With dry_run the reads run (Play reads need an edit: one is opened, read and deleted again, so nothing changes)
and every write is only printed.
"""
from __future__ import annotations

import base64
import json
import os
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Callable, Optional

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "discord-issues"))
import sync  # noqa: E402

PACKAGE = "app.borealis"
TRACK = "alpha"  # closed testing; its testers are the Google Groups set in Play Console
FORBIDDEN_TRACKS = frozenset({"production"})
API = f"https://androidpublisher.googleapis.com/androidpublisher/v3/applications/{PACKAGE}"
UPLOAD_API = f"https://androidpublisher.googleapis.com/upload/androidpublisher/v3/applications/{PACKAGE}"
SCOPE = "https://www.googleapis.com/auth/androidpublisher"
TOKEN_LIFETIME_S = 3600
UPLOAD_TIMEOUT_S = 1200  # Play answers the bundle upload only after processing it
NOTES_LANGUAGE = "en-GB"  # the store listing's default language (the 1.2.2 notes were en-GB)
NOTES_LIMIT = 500  # Play's limit per language
CHANGELOG = "https://borealisbuilder.com/changelog"
LIVE_STATUSES = frozenset({"completed", "inProgress"})


def artifact_name(version: str, sha: str) -> str:
    """The workflow artifact release.yml uploads the signed bundle as (version and commit, so a dry run of another
    commit is never taken for the tag's bundle)."""
    return f"play-bundle-{version}-{sha[:12]}"


def bundle_file(version: str) -> str:
    return f"Borealis_{version}_play.aab"


def release_notes(whats_new: str) -> str:
    """Play's "What's new in this release": the plain notes (notes.whats_new) cut to fit, and the changelog link.
    No GitHub download link: store builds never point at updates outside the store (B-85)."""
    tail = f"Full notes: {CHANGELOG}"
    room = NOTES_LIMIT - len(tail) - 2
    body = whats_new.strip()
    if len(body) > room:
        body = body[: room - 1].rstrip() + "…"
    return f"{body}\n\n{tail}" if body else tail


def track_release(version: str, version_code: int, notes: str, track: str = TRACK) -> dict:
    """The tracks.update body: one release, rolled out to every tester of the track."""
    if track in FORBIDDEN_TRACKS or track != TRACK:
        raise ValueError(f"the release automation only releases to the {TRACK} track, not {track}")
    return {"track": track, "releases": [{
        "name": version, "versionCodes": [str(version_code)], "status": "completed",
        "releaseNotes": [{"language": NOTES_LANGUAGE, "text": notes}]}]}


def release_status(track: dict, version_code: int) -> Optional[str]:
    """The status of the release on `track` that carries `version_code`, or None."""
    for rel in track.get("releases", []):
        if str(version_code) in [str(c) for c in rel.get("versionCodes", [])]:
            return rel.get("status")
    return None


def _b64(data: bytes) -> bytes:
    return base64.urlsafe_b64encode(data).rstrip(b"=")


class Play:
    def __init__(self, key_path: str, dry_run: bool, log: Callable[[str], None] = print):
        self.key_path, self.dry_run, self.log = key_path, dry_run, log
        self._token, self._token_at = "", 0.0

    @classmethod
    def from_env(cls, dry_run: bool, log: Callable[[str], None] = print) -> Optional["Play"]:
        path = os.environ.get("PLAY_SERVICE_ACCOUNT_PATH", "").strip()
        if not (path and Path(path).is_file()):
            return None
        return cls(path, dry_run, log)

    # --- plumbing -------------------------------------------------------------------------------------------

    def assertion(self, now: int) -> str:
        """The signed JWT that buys an access token (service-account flow)."""
        key = json.loads(Path(self.key_path).read_text(encoding="utf-8"))
        head = _b64(json.dumps({"alg": "RS256", "typ": "JWT"}).encode())
        body = _b64(json.dumps({"iss": key["client_email"], "scope": SCOPE, "aud": key["token_uri"],
                                "iat": now, "exp": now + TOKEN_LIFETIME_S}).encode())
        msg = head + b"." + body
        with tempfile.TemporaryDirectory() as tmp:
            pem = Path(tmp) / "key.pem"
            pem.touch(mode=0o600)
            pem.write_text(key["private_key"], encoding="utf-8")
            sig = subprocess.run(["openssl", "dgst", "-sha256", "-sign", str(pem)], input=msg,
                                 capture_output=True, check=True).stdout
        return (msg + b"." + _b64(sig)).decode()

    def token(self) -> str:
        if self._token and time.time() - self._token_at < TOKEN_LIFETIME_S - 300:
            return self._token
        token_uri = json.loads(Path(self.key_path).read_text(encoding="utf-8"))["token_uri"]
        form = urllib.parse.urlencode({"grant_type": "urn:ietf:params:oauth:grant-type:jwt-bearer",
                                       "assertion": self.assertion(int(time.time()))}).encode()
        req = urllib.request.Request(token_uri, data=form, method="POST",
                                     headers={"Content-Type": "application/x-www-form-urlencoded"})
        try:
            with urllib.request.urlopen(req, timeout=sync.TIMEOUT_S) as resp:
                self._token = json.loads(resp.read())["access_token"]
        except urllib.error.HTTPError as err:
            try:
                why = str(json.loads(err.read()).get("error_description", ""))[:200]
            except (ValueError, AttributeError):
                why = ""
            raise sync.ApiError("POST", "oauth2 token", err.code, why or "service account refused") from None
        self._token_at = time.time()
        return self._token

    def _headers(self) -> dict:
        return {"Authorization": f"Bearer {self.token()}", "User-Agent": "borealis-release-auto"}

    def call(self, method: str, path: str, body: Optional[dict] = None):
        return sync.http(method, API + path, self._headers(), body, log_path=path.split("?")[0])

    def write(self, method: str, path: str, body: Optional[dict], what: str):
        self.log(("DRY-RUN would: " if self.dry_run else "") + what)
        return None if self.dry_run else self.call(method, path, body)

    def upload_bundle(self, edit: str, aab: Path) -> dict:
        path = f"/edits/{edit}/bundles"
        req = urllib.request.Request(f"{UPLOAD_API}{path}?uploadType=media", data=aab.read_bytes(), method="POST",
                                     headers={**self._headers(), "Content-Type": "application/octet-stream"})
        try:
            with urllib.request.urlopen(req, timeout=UPLOAD_TIMEOUT_S) as resp:
                return json.loads(resp.read())
        except urllib.error.HTTPError as err:
            raise sync.ApiError("POST", f"upload {path}", err.code, sync._error_message(err.read())) from None

    # --- the release ----------------------------------------------------------------------------------------

    def track(self, edit: str) -> dict:
        try:
            return self.call("GET", f"/edits/{edit}/tracks/{TRACK}") or {}
        except sync.ApiError as err:
            if err.status == 404:
                return {}
            raise

    def release_to_testers(self, aab: Path, version: str, version_code: int, notes: str) -> str:
        """Upload the bundle (unless Play has it), put it on the alpha track rolled out to every tester, commit.
        Idempotent: a version already live on the track is left alone. Returns what was done."""
        edit = self.call("POST", "/edits", {})["id"]  # an edit is a scratch transaction until committed
        committed = False
        try:
            status = release_status(self.track(edit), version_code)
            if status in LIVE_STATUSES:
                return f"versionCode {version_code} is already on {TRACK} ({status})"
            bundles = (self.call("GET", f"/edits/{edit}/bundles") or {}).get("bundles", [])
            if version_code in [int(b.get("versionCode", 0)) for b in bundles]:
                self.log(f"Play already has the bundle with versionCode {version_code}")
            elif self.dry_run:
                size = f" ({aab.stat().st_size} bytes)" if aab.is_file() else ""
                self.log(f"DRY-RUN would: upload {aab.name}{size} to Play")
            else:
                got = self.upload_bundle(edit, aab)
                if int(got.get("versionCode", 0)) != version_code:
                    raise ValueError(f"the uploaded bundle has versionCode {got.get('versionCode')}, "
                                     f"expected {version_code}; nothing was released")
                self.log(f"uploaded {aab.name} to Play (versionCode {version_code}, sha256 {got.get('sha256')})")
            self.write("PUT", f"/edits/{edit}/tracks/{TRACK}", track_release(version, version_code, notes),
                       f"put {version} (versionCode {version_code}) on {TRACK}, rolled out to its testers")
            self.write("POST", f"/edits/{edit}:commit", None, f"commit the Play edit for {version}")
            committed = not self.dry_run
            return (f"would roll out versionCode {version_code} on {TRACK}" if self.dry_run
                    else f"versionCode {version_code} rolled out on {TRACK} (testers get it once Google approves)")
        finally:
            if not committed:
                try:
                    self.call("DELETE", f"/edits/{edit}")
                except sync.ApiError as err:  # an abandoned edit expires by itself
                    self.log(f"could not delete Play edit: {err}")
