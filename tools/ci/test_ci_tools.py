#!/usr/bin/env python3
"""Tests for the acceptance CI helpers: python3 -I tools/ci/test_ci_tools.py (stdlib only)."""
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import fetch_aurora_corpus as fetch  # noqa: E402
import match_hashes as mh  # noqa: E402
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
        ident = "acceptance/f-url/s2/ID_5ET_SOME_FEAT"
        printed = self.t.take(f"OUTCOME\tPASS\t{ident}\t")
        self.assertEqual(printed, f"OUTCOME_HASH acceptance/f-url PASS {rf.outcome_hash(ident)}")
        self.assertNotIn("ID_5ET", printed)
        self.assertNotIn("s2", printed)
        failed = self.t.take("OUTCOME\tFAIL\tacceptance/f-url/s1/class\tU=2")
        self.assertRegex(failed, r"^OUTCOME_HASH acceptance/f-url FAIL [0-9a-f]{12}$")
        self.assertNotIn("U=2", failed)
        self.assertEqual(self.t.outcomes[("acceptance/f-url", "PASS")], 1)
        self.assertEqual(self.t.outcomes[("acceptance/f-url", "FAIL")], 1)

    def test_outcome_hash_is_the_salted_sha256_prefix(self):
        import hashlib
        ident = "acceptance/a-core/s1/Class"
        want = hashlib.sha256((rf.OUTCOME_SALT + ident).encode()).hexdigest()[:12]
        self.assertEqual(rf.outcome_hash(ident), want)
        self.assertNotEqual(rf.outcome_hash(ident), rf.outcome_hash(ident + "x"))

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


class MatchTests(unittest.TestCase):
    LOCAL = ["noise line\n",
             "OUTCOME\tPASS\tacceptance/a-core/s1/Class\tU=0\n",
             "OUTCOME\tFAIL\tacceptance/a-core/s2/Feat/ID_X\tnot offered\n",
             "OUTCOME\tPASS\tacceptance/a-core/s2/Feat/ID_LOCAL_ONLY\t\n",
             "OUTCOME\tPASS\tacceptance/f-url/s1/class\tU=0\n"]

    def ci_log(self, rows):
        """CI log text as run_filtered prints it, with a GitHub timestamp prefix."""
        t = rf.Tally()
        out = ["2026-10-18T08:00:00.0000000Z running 1 test\n"]
        for line in rows:
            text = t.take(line)
            if text is not None:
                out.append(f"2026-10-18T08:00:01.0000000Z {text}\n")
        return out

    def test_identical_sides_match(self):
        local = mh.local_rows(self.LOCAL)
        self.assertEqual(mh.compare(local, mh.ci_rows(self.ci_log(self.LOCAL))), {})

    def test_lists_rows_on_one_side_and_verdict_changes_per_group(self):
        ci_side = [self.LOCAL[1],
                   "OUTCOME\tPASS\tacceptance/a-core/s2/Feat/ID_X\t\n",
                   "OUTCOME\tFAIL\tacceptance/a-core/s2/Feat/ID_CI_ONLY\t\n",
                   self.LOCAL[4]]
        diffs = mh.compare(mh.local_rows(self.LOCAL), mh.ci_rows(self.ci_log(ci_side)))
        self.assertEqual(list(diffs), ["acceptance/a-core"])
        d = diffs["acceptance/a-core"]
        self.assertEqual(d["only_local"], [("acceptance/a-core/s2/Feat/ID_LOCAL_ONLY", "PASS")])
        self.assertEqual(d["only_ci"],
                         [(rf.outcome_hash("acceptance/a-core/s2/Feat/ID_CI_ONLY"), "FAIL")])
        self.assertEqual(d["differs"], [("acceptance/a-core/s2/Feat/ID_X", "FAIL", "PASS")])
        text = "\n".join(mh.report(diffs))
        self.assertIn("only local 1, only CI 1, verdict differs 1", text)
        self.assertNotIn("ID_CI_ONLY", text)

    def test_a_group_missing_on_one_side_is_reported(self):
        diffs = mh.compare(mh.local_rows(self.LOCAL), mh.ci_rows(self.ci_log(self.LOCAL[:4])))
        self.assertEqual(list(diffs), ["acceptance/f-url"])
        self.assertEqual(diffs["acceptance/f-url"]["ci"], 0)

    def test_main_exit_codes(self):
        with tempfile.TemporaryDirectory() as d:
            local, ci, empty = Path(d) / "local.txt", Path(d) / "ci.log", Path(d) / "e.txt"
            local.write_text("".join(self.LOCAL), encoding="utf-8")
            empty.write_text("nothing\n", encoding="utf-8")
            ci.write_text("".join(self.ci_log(self.LOCAL)), encoding="utf-8")
            self.assertEqual(mh.main([str(local), str(ci)]), 0)
            ci.write_text("".join(self.ci_log(self.LOCAL[:3])), encoding="utf-8")
            self.assertEqual(mh.main([str(local), str(ci)]), 1)
            self.assertEqual(mh.main([str(empty), str(ci)]), 2)
            self.assertEqual(mh.main([str(local), str(Path(d) / "missing.log")]), 2)


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
