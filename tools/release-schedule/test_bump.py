"""Tests for bump.py: python3 -I -m unittest discover -s tools/release-schedule"""
from __future__ import annotations

import subprocess
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import bump  # noqa: E402

TEXTS = {
    "Cargo.toml": '[workspace]\nmembers = ["a"]\n\n[workspace.package]\nversion = "1.2.2"\nedition = "2021"\n'
                  '\n[workspace.dependencies]\nserde = { version = "1.2.2" }\n',
    "Cargo.lock": 'version = 4\n\n[[package]]\nname = "borealis"\nversion = "1.2.2"\ndependencies = [\n "x",\n]\n'
                  '\n[[package]]\nname = "x"\nversion = "1.2.2"\nsource = "registry+https://example"\n'
                  '\n[[package]]\nname = "borealis-core"\nversion = "1.2.2"\n',
    "README.md": "<sub>Version 1.2.2 &nbsp;·&nbsp; rules</sub>\n",
    "src-tauri/tauri.conf.json": '{\n  "productName": "Borealis",\n  "version": "1.2.2",\n  "identifier": "a"\n}\n',
    "src-tauri/gen/apple/borealis_iOS/Info.plist": "<key>CFBundleShortVersionString</key>\n\t<string>1.2.2</string>\n"
                                                    "\t<key>CFBundleVersion</key>\n\t<string>1.2.2</string>\n",
    "src-tauri/gen/apple/project.yml": '        CFBundleShortVersionString: 1.2.2\n        CFBundleVersion: "1.2.2"\n',
    "ui/package.json": '{\n  "name": "borealis-ui",\n  "version": "1.2.2",\n  "type": "module"\n}\n',
    "ui/package-lock.json": '{\n  "name": "borealis-ui",\n  "version": "1.2.2",\n  "packages": {\n    "": {\n'
                            '      "name": "borealis-ui",\n      "version": "1.2.2",\n    },\n'
                            '    "node_modules/x": {\n      "version": "1.2.2"\n    }\n  }\n}\n',
}


class BuildNumber(unittest.TestCase):
    def test_build_number_from_the_version(self):
        self.assertEqual(bump.build_number("1.2.2"), 1002002)
        self.assertEqual(bump.build_number("1.3.0"), 1003000)
        self.assertEqual(bump.build_number("1.2.10"), 1002010)


class Texts(unittest.TestCase):
    def test_bumps_only_the_release_spots(self):
        out = bump.bump_texts(TEXTS, "1.2.2", "1.3.0")
        self.assertIn('[workspace.package]\nversion = "1.3.0"', out["Cargo.toml"])
        self.assertIn('serde = { version = "1.2.2" }', out["Cargo.toml"])
        self.assertEqual(out["Cargo.lock"].count('version = "1.3.0"'), 2)  # the two workspace crates
        self.assertIn('name = "x"\nversion = "1.2.2"\nsource', out["Cargo.lock"])
        self.assertEqual(out["ui/package-lock.json"].count('"version": "1.3.0"'), 2)
        self.assertIn('"node_modules/x": {\n      "version": "1.2.2"', out["ui/package-lock.json"])
        self.assertEqual(out["src-tauri/gen/apple/borealis_iOS/Info.plist"].count("1.3.0"), 2)
        self.assertIn("Version 1.3.0 &nbsp;", out["README.md"])

    def test_refuses_a_lower_version_and_a_changed_layout(self):
        with self.assertRaises(bump.BumpError):
            bump.bump_texts(TEXTS, "1.2.2", "1.2.1")
        broken = {**TEXTS, "README.md": "Version 1.2.2\n"}
        with self.assertRaises(bump.BumpError):
            bump.bump_texts(broken, "1.2.2", "1.3.0")


def _git(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", *args], cwd=HERE, capture_output=True, text=True)


@unittest.skipUnless(_git("cat-file", "-e", "cb131e7^{commit}").returncode == 0, "needs the repository history")
class ReleaseCommit(unittest.TestCase):
    def test_reproduces_the_1_2_2_release_commit(self):
        before = {f: _git("show", f"cb131e7^:{f}").stdout for f in bump.FILES}
        after = {f: _git("show", f"cb131e7:{f}").stdout for f in bump.FILES}
        self.assertEqual(bump.bump_texts(before, "1.2.1", "1.2.2"), after)


if __name__ == "__main__":
    unittest.main()
