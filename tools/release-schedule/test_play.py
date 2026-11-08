"""Tests for play.py (RL8, B-236): no network; the JWT test needs `openssl`.
python3 -I -m unittest discover -s tools/release-schedule"""
from __future__ import annotations

import base64
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Optional

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import play  # noqa: E402


def _unb64(part: str) -> bytes:
    return base64.urlsafe_b64decode(part + "=" * (-len(part) % 4))


class Recorder(play.Play):
    """A live (not dry-run) client whose calls are recorded and answered from `track` / `bundles`."""

    def __init__(self, track: dict, bundles: list, uploaded_code: int = 1004000):
        super().__init__("/dev/null", False, lambda _line: None)
        self.track_data, self.bundles, self.uploaded_code = track, bundles, uploaded_code
        self.calls: list[tuple[str, str, Optional[dict]]] = []

    def token(self) -> str:
        return "test"

    def call(self, method: str, path: str, body: Optional[dict] = None):
        self.calls.append((method, path, body))
        if (method, path) == ("POST", "/edits"):
            return {"id": "e1"}
        if method == "GET" and path.endswith("/tracks/alpha"):
            return self.track_data
        if method == "GET" and path.endswith("/bundles"):
            return {"bundles": self.bundles}
        return {}

    def upload_bundle(self, edit: str, aab: Path) -> dict:
        self.calls.append(("UPLOAD", f"/edits/{edit}/bundles", None))
        return {"versionCode": self.uploaded_code, "sha256": "x"}


class Notes(unittest.TestCase):
    def test_notes_fit_plays_limit_and_end_with_the_changelog(self):
        text = play.release_notes("Borealis 1.4.0 brings magic.\n\n" + "• A fix.\n" * 200)
        self.assertLessEqual(len(text), play.NOTES_LIMIT)
        self.assertTrue(text.endswith(f"Full notes: {play.CHANGELOG}"))
        self.assertIn("…", text)

    def test_short_notes_are_kept_whole_without_a_download_link(self):
        text = play.release_notes("Borealis 1.4.0 brings magic.")
        self.assertEqual(text, f"Borealis 1.4.0 brings magic.\n\nFull notes: {play.CHANGELOG}")
        self.assertNotIn("github.com", text)


class Track(unittest.TestCase):
    def test_release_goes_to_alpha_completed_with_the_version_code(self):
        body = play.track_release("1.4.0", 1004000, "notes")
        self.assertEqual(body["track"], "alpha")
        self.assertEqual(body["releases"], [{"name": "1.4.0", "versionCodes": ["1004000"], "status": "completed",
                                             "releaseNotes": [{"language": "en-GB", "text": "notes"}]}])

    def test_production_and_other_tracks_are_refused(self):
        for track in ("production", "beta", "internal"):
            with self.assertRaises(ValueError):
                play.track_release("1.4.0", 1004000, "notes", track)

    def test_release_status_finds_the_version_code(self):
        track = {"releases": [{"versionCodes": ["1002002"], "status": "completed"},
                              {"versionCodes": ["1004000"], "status": "draft"}]}
        self.assertEqual(play.release_status(track, 1004000), "draft")
        self.assertIsNone(play.release_status(track, 1005000))
        self.assertIsNone(play.release_status({}, 1004000))

    def test_artifact_name_pins_version_and_commit(self):
        self.assertEqual(play.artifact_name("1.4.0", "abcdef0123456789"), "play-bundle-1.4.0-abcdef012345")


class Release(unittest.TestCase):
    def test_uploads_updates_alpha_and_commits(self):
        p = Recorder({"releases": [{"versionCodes": ["1002002"], "status": "completed"}]}, [{"versionCode": 1002002}])
        did = p.release_to_testers(Path("b.aab"), "1.4.0", 1004000, "notes")
        self.assertIn("rolled out on alpha", did)
        steps = [(m, path) for m, path, _ in p.calls]
        self.assertEqual(steps, [("POST", "/edits"), ("GET", "/edits/e1/tracks/alpha"), ("GET", "/edits/e1/bundles"),
                                 ("UPLOAD", "/edits/e1/bundles"), ("PUT", "/edits/e1/tracks/alpha"),
                                 ("POST", "/edits/e1:commit")])
        self.assertNotIn("production", json.dumps(p.calls))

    def test_a_bundle_play_already_has_is_not_uploaded_again(self):
        p = Recorder({}, [{"versionCode": "1004000"}])
        p.release_to_testers(Path("b.aab"), "1.4.0", 1004000, "notes")
        self.assertNotIn("UPLOAD", [m for m, _, _ in p.calls])
        self.assertIn(("POST", "/edits/e1:commit", None), p.calls)

    def test_a_live_release_is_left_alone_and_the_edit_dropped(self):
        p = Recorder({"releases": [{"versionCodes": ["1004000"], "status": "completed"}]}, [])
        self.assertIn("already on alpha", p.release_to_testers(Path("b.aab"), "1.4.0", 1004000, "notes"))
        self.assertEqual([m for m, _, _ in p.calls], ["POST", "GET", "DELETE"])

    def test_a_wrong_version_code_releases_nothing(self):
        p = Recorder({}, [], uploaded_code=1003000)
        with self.assertRaises(ValueError):
            p.release_to_testers(Path("b.aab"), "1.4.0", 1004000, "notes")
        methods = [m for m, _, _ in p.calls]
        self.assertNotIn("PUT", methods)
        self.assertEqual(methods[-1], "DELETE")


class Assertion(unittest.TestCase):
    def test_jwt_is_rs256_signed_for_the_publisher_scope(self):
        with tempfile.TemporaryDirectory() as tmp:
            pem = subprocess.run(["openssl", "genpkey", "-algorithm", "RSA", "-pkeyopt", "rsa_keygen_bits:2048"],
                                 capture_output=True, check=True, text=True).stdout
            key = Path(tmp) / "sa.json"
            key.write_text(json.dumps({"client_email": "ci@example.iam.gserviceaccount.com", "private_key": pem,
                                       "token_uri": "https://oauth2.googleapis.com/token"}))
            jwt = play.Play(str(key), True).assertion(1_000_000)
            head, body, sig = jwt.split(".")
            self.assertEqual(json.loads(_unb64(head)), {"alg": "RS256", "typ": "JWT"})
            claims = json.loads(_unb64(body))
            self.assertEqual((claims["iss"], claims["scope"], claims["aud"], claims["exp"] - claims["iat"]),
                             ("ci@example.iam.gserviceaccount.com", play.SCOPE,
                              "https://oauth2.googleapis.com/token", play.TOKEN_LIFETIME_S))
            (Path(tmp) / "key.pem").write_text(pem)
            pub = subprocess.run(["openssl", "pkey", "-in", str(Path(tmp) / "key.pem"), "-pubout"],
                                 capture_output=True, check=True).stdout
            (Path(tmp) / "pub.pem").write_bytes(pub)
            (Path(tmp) / "sig").write_bytes(_unb64(sig))
            ok = subprocess.run(["openssl", "dgst", "-sha256", "-verify", str(Path(tmp) / "pub.pem"), "-signature",
                                 str(Path(tmp) / "sig")], input=f"{head}.{body}".encode(), capture_output=True)
            self.assertEqual(ok.returncode, 0, ok.stdout + ok.stderr)


if __name__ == "__main__":
    unittest.main()
