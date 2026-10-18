#!/usr/bin/env python3
"""Tests for the acceptance CI helpers: python3 -I tools/ci/test_ci_tools.py (stdlib only)."""
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import fetch_aurora_corpus as fetch  # noqa: E402
import run_filtered as rf  # noqa: E402


class FilterTests(unittest.TestCase):
    def setUp(self):
        self.t = rf.Tally()

    def test_passes_libtest_progress_and_results(self):
        for line in ("running 5 tests", "test acceptance_f_url ... ok",
                     "test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"):
            self.assertEqual(self.t.take(line + "\n"), line)
        self.assertEqual(self.t.tests["ok"], 1)

    def test_passes_numeric_metric_lines(self):
        line = "METRIC\tf-url\ts1\tclass\tU\t0"
        self.assertEqual(self.t.take(line), line)
        self.assertEqual(self.t.metrics, [["f-url", "s1", "class", "U", "0"]])

    def test_withholds_metric_with_text_or_non_numeric_value(self):
        self.assertIsNone(self.t.take("METRIC\tf-url\ts1\tclass\tU\tmany"))
        self.assertIsNone(self.t.take("METRIC\tf-url\ts1\t<b>A \"quoted\" name</b>\tU\t3"))
        self.assertEqual(self.t.withheld, 2)

    def test_counts_outcomes_by_group_and_never_prints_ids(self):
        self.assertIsNone(self.t.take("OUTCOME\tPASS\tacceptance/f-url/s2/ID_5ET_SOME_FEAT\t"))
        self.assertIsNone(self.t.take("OUTCOME\tFAIL\tacceptance/f-url/s1/class\tU=2"))
        self.assertEqual(self.t.outcomes[("acceptance/f-url", "PASS")], 1)
        self.assertEqual(self.t.outcomes[("acceptance/f-url", "FAIL")], 1)

    def test_withholds_content_bearing_lines(self):
        for line in ("      3  skipped: Fireball needs a material component",
                     "thread 'x' panicked at src/a.rs:1:2: download: https://example.com/x",
                     "  host ids Dnd2014: fighter Some(\"ID_X\")",
                     "test result: see https://example.com",
                     "SKIPPED f-url: some text with a comma, and a URL https://x.y"):
            self.assertIsNone(self.t.take(line), line)
        self.assertEqual(self.t.withheld, 5)

    def test_passes_fixed_skip_lines(self):
        for line in ("SKIPPED f-url: BOREALIS_LIVE_5ETOOLS_URL not set",
                     "SKIPPED a-core: not in BOREALIS_ACCEPT_ONLY",
                     "SKIPPED: BOREALIS_CORPUS_DIR is not set"):
            self.assertEqual(self.t.take(line), line)

    def test_unexpected_outcome_group_is_other(self):
        self.assertEqual(rf.outcome_group("acceptance/a b <c>/x"), "other")

    def test_summary_counts(self):
        self.t.take("test a ... ok")
        self.t.take("OUTCOME\tFAIL\tacceptance/a-core/s1/x\t")
        head, rows = rf.summary_lines("acc", 0, self.t)
        self.assertIn("tests ok 1", head)
        self.assertIn("FAIL 1", head)
        self.assertEqual(rows, [("acceptance/a-core", 0, 1)])


class FetchTests(unittest.TestCase):
    def test_committed_manifest_is_valid(self):
        rows = fetch.read_manifest(fetch.MANIFEST)
        self.assertEqual(len(rows), len({slug for slug, _ in rows}))
        self.assertGreater(len(rows), 0)

    def test_manifest_rejects_short_sha(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "m.txt"
            p.write_text("owner/repo abc123\n", encoding="utf-8")
            with self.assertRaises(ValueError):
                fetch.read_manifest(p)

    def test_character_rows_keep_xml_dnd5e_only(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            repo = root / "repos" / "o__r" / "chars"
            repo.mkdir(parents=True)
            (repo / "a.dnd5e").write_bytes(b"\xef\xbb\xbf\r\n<character/>")
            (repo / "b.dnd5e").write_bytes(b'{"json": true}')
            (repo / "c.xml").write_bytes(b"<elements/>")
            rows = fetch.character_rows(root)
        self.assertEqual(rows, [{"path": "repos/o__r/chars/a.dnd5e", "repo": "o__r",
                                 "kind": "character"}])


if __name__ == "__main__":
    unittest.main()
