#!/usr/bin/env python3
"""Tests for the acceptance CI helpers: python3 -I tools/ci/test_ci_tools.py (stdlib only)."""
import contextlib
import ctypes
import io
import json
import os
import subprocess
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import check_thresholds as ct  # noqa: E402
import fetch_aurora_corpus as fetch  # noqa: E402
import webdriver_smoke as ws  # noqa: E402


def quiet(fn, *args):
    """(return value, captured stdout+stderr) of fn(*args). The tests drive the tools' main() through
    failure paths, and their `::error::` lines must not reach a CI log, where GitHub would turn them into
    error annotations on the self-test step."""
    out = io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(out):
        value = fn(*args)
    return value, out.getvalue()
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
            self.assertEqual(quiet(mh.main, [str(local), str(ci)])[0], 0)
            ci.write_text("".join(self.ci_log(self.LOCAL[:3])), encoding="utf-8")
            self.assertEqual(quiet(mh.main, [str(local), str(ci)])[0], 1)
            self.assertEqual(quiet(mh.main, [str(empty), str(ci)])[0], 2)
            self.assertEqual(quiet(mh.main, [str(local), str(Path(d) / "missing.log")])[0], 2)


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


class ThresholdTests(unittest.TestCase):
    LIMITS = {"version": 1, "desktop-linux": {"a-core/s1": {"wall_s": 100},
                                              "process/acceptance": {"peak_mem_mb": 4000}}}
    LOG = ["IM4 acceptance (ubuntu-latest)\tAcceptance\t2026-10-19T10:18:03.7Z METRIC\ta-core\ts1\t*\twall_s\t54\n",
           "2026-10-19T10:18:03.8Z METRIC\ta-core\ts1\tClass\twall_s\t999\n",
           "METRIC\tprocess\tacceptance\t*\tpeak_mem_mb\t3500\n",
           "METRIC\ta-user\ts1\t*\twall_s\t2\n",
           "METRIC\ta-user\ts1\t*\tfolder_errors\t0\n",
           "test result: ok. 8 passed; 0 failed\n"]

    def run_main(self, limits, log_lines, platform="desktop-linux"):
        with tempfile.TemporaryDirectory() as d:
            t, log = Path(d) / "t.json", Path(d) / "a.log"
            t.write_text(json.dumps(limits), encoding="utf-8")
            log.write_text("".join(log_lines), encoding="utf-8")
            code, self.out = quiet(ct.main, ["--platform", platform, "--thresholds", str(t), str(log)])
            return code

    def test_committed_thresholds_are_numbers_only_for_every_platform(self):
        data = ct.load_thresholds(ct.THRESHOLDS)
        self.assertEqual(set(data) - {"version"},
                         {"desktop-linux", "desktop-windows", "android", "ios"})
        for platform in ("android", "ios"):
            self.assertIn("peak_mem_mb", data[platform]["process/m4"])
            self.assertLessEqual(data[platform]["f-full/import"]["wall_s"], 180)

    def test_reads_aggregate_rows_behind_any_log_prefix(self):
        rows = ct.metric_rows(self.LOG)
        self.assertEqual(rows, [("a-core/s1", "wall_s", 54), ("process/acceptance", "peak_mem_mb", 3500),
                                ("a-user/s1", "wall_s", 2), ("a-user/s1", "folder_errors", 0)])

    def test_value_over_its_limit_is_a_breach_and_watched_rows_without_limit_are_listed(self):
        checked, breaches, unlimited = ct.check(
            [("a-core/s1", "wall_s", 101), ("a-user/s1", "wall_s", 2), ("a-user/s1", "folder_errors", 0)],
            self.LIMITS["desktop-linux"])
        self.assertEqual(breaches, [("a-core/s1", "wall_s", 101, 100)])
        self.assertEqual(len(checked), 1)
        self.assertEqual(unlimited, [("a-user/s1", "wall_s", 2, None)])

    def test_value_equal_to_the_limit_passes(self):
        _, breaches, _ = ct.check([("a-core/s1", "wall_s", 100)], self.LIMITS["desktop-linux"])
        self.assertEqual(breaches, [])

    def test_main_exit_codes(self):
        self.assertEqual(self.run_main(self.LIMITS, self.LOG), 0)
        over = self.LOG + ["METRIC\tprocess\tacceptance\t*\tpeak_mem_mb\t4001\n"]
        self.assertEqual(self.run_main(self.LIMITS, over), 1)
        self.assertIn("::error::desktop-linux process/acceptance peak_mem_mb 4001", self.out)
        self.assertEqual(self.run_main(self.LIMITS, self.LOG, platform="desktop-macos"), 2)
        self.assertEqual(self.run_main(self.LIMITS, self.LOG, platform="version"), 2)
        self.assertEqual(quiet(ct.main, ["--platform", "ios", "/nonexistent/log"])[0], 2)

    def test_rejects_thresholds_that_are_not_positive_numbers(self):
        for bad in ({"desktop-linux": {"a-core/s1": {"wall_s": "90"}}},
                    {"desktop-linux": {"a-core/s1": {"wall_s": True}}},
                    {"desktop-linux": {"a-core/s1": {"wall_s": 0}}},
                    {"desktop-linux": {"a-core": {"wall_s": 9}}},
                    {"desktop-linux": {"a-core/s1": {}}}):
            self.assertEqual(self.run_main(bad, self.LOG), 2, bad)

    # Lines exactly as e2e/support/mobile-metrics.ts writes them for the S5 (m4) run.
    M4_LOG = ["METRIC\ta-core\timport\t*\twall_s\t201\n", "METRIC\ta-user\timport\t*\twall_s\t1\n",
              "METRIC\tf-full\timport\t*\twall_s\t5\n", "METRIC\tapp\trestart_digest\t*\twall_s\t2\n",
              "METRIC\tprocess\tm4\t*\tpeak_mem_mb\t{peak}\n"]

    def m4_code(self, platform, peak, wall=None):
        lines = [l.format(peak=peak) for l in self.M4_LOG]
        if wall:
            lines[0] = f"METRIC\ta-core\timport\t*\twall_s\t{wall}\n"
        return self.run_main(ct.load_thresholds(ct.THRESHOLDS), lines, platform=platform)

    def test_s5_mobile_run_is_judged_by_the_committed_android_and_ios_rows(self):
        self.assertEqual(self.m4_code("android", 2500, wall=200), 0)
        self.assertEqual(self.m4_code("ios", 1500, wall=340), 0)
        self.assertEqual(self.m4_code("android", 2501), 1)
        self.assertIn("::error::android process/m4 peak_mem_mb 2501 is over the limit 2500", self.out)
        self.assertEqual(self.m4_code("ios", 1501), 1)
        # Same a-core time: inside the iOS limit (340 s), over the Android one (200 s).
        self.assertEqual(self.m4_code("ios", 100, wall=300), 0)
        self.assertEqual(self.m4_code("android", 100, wall=300), 1)
        self.assertIn("android a-core/import wall_s 300 is over the limit 200", self.out)


class PeakMemoryTests(unittest.TestCase):
    def test_job_structures_match_the_x64_layout(self):
        if ctypes.sizeof(ctypes.c_void_p) != 8:
            self.skipTest("64-bit layout")
        self.assertEqual(ctypes.sizeof(rf.JobBasicLimits), 64)
        self.assertEqual(ctypes.sizeof(rf.JobExtendedLimits), 144)
        self.assertEqual(rf.JobExtendedLimits.PeakProcessMemoryUsed.offset, 128)

    def test_maxrss_units(self):
        self.assertEqual(rf.maxrss_to_mib(2048 * 1024, "linux"), 2048)
        self.assertEqual(rf.maxrss_to_mib(3 * rf.MIB, "darwin"), 3)

    def test_peak_line_passes_the_counts_only_filter(self):
        line = rf.peak_metric_line("acceptance", 812)
        t = rf.Tally()
        self.assertEqual(t.take(line), line)
        self.assertEqual(ct.metric_rows([line]), [("process/acceptance", "peak_mem_mb", 812)])

    def test_peak_stage_is_a_short_slug(self):
        self.assertEqual(rf.peak_stage("aurora-sweep"), "aurora-sweep")
        for bad in ("A b", "x\ty", "", "a" * 33):
            with self.assertRaises(Exception):
                rf.peak_stage(bad)

    @unittest.skipIf(os.name == "nt", "POSIX rusage path")
    def test_meter_sees_a_child_that_ran_after_it(self):
        meter = rf.PeakMeter()
        subprocess.run([sys.executable, "-I", "-c", "b = bytearray(64 * 1024 * 1024)"], check=True)
        self.assertGreaterEqual(meter.peak_mib(), 64)


class FakeTauriDriver(BaseHTTPRequestHandler):
    """A WebDriver endpoint with a splash window that is gone and a main window with the start page."""
    state = {}

    def log_message(self, *args):
        pass

    def reply(self, value, status=200):
        body = json.dumps({"value": value}).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/status":
            return self.reply({"ready": True})
        if self.path.endswith("/window/handles"):
            return self.reply(["splash", "main"])
        if self.path.endswith("/screenshot"):
            return self.reply("aGk=")
        return self.reply({"error": "unknown command", "message": self.path}, 404)

    def do_DELETE(self):
        self.state["deleted"] = True
        self.reply(None)

    def do_POST(self):
        st = self.state
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])) or b"{}")
        if self.path == "/session":
            st["caps"] = body
            return self.reply({"sessionId": "s1", "capabilities": {}})
        if self.path.endswith("/window"):
            st["window"] = body["handle"]
            return self.reply(None)
        if self.path.endswith("/execute/sync"):
            if st.get("window") == "splash":
                return self.reply({"error": "no such window", "message": "closed"}, 404)
            if body["script"] == ws.CLICK_TEXT:
                st["clicks"] = st.get("clicks", 0) + 1
                return self.reply(True)
            pattern = body["args"][0]
            showing = pattern == ws.START_TEXT or (pattern == ws.CONTENT_TEXT and st.get("clicks", 0) >= 2)
            return self.reply(showing)
        return self.reply({"error": "unknown command", "message": self.path}, 404)


class WebDriverSmokeTests(unittest.TestCase):
    def setUp(self):
        FakeTauriDriver.state = {}
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), FakeTauriDriver)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.client = ws.Client(f"http://127.0.0.1:{self.server.server_address[1]}")
        self.poll, ws.POLL_S = ws.POLL_S, 0.01

    def tearDown(self):
        ws.POLL_S = self.poll
        self.server.shutdown()
        self.server.server_close()

    def test_session_names_the_installed_app(self):
        caps = ws.session_payload("C:/Users/x/AppData/Local/Borealis/borealis.exe")
        self.assertEqual(caps["capabilities"]["alwaysMatch"]["tauri:options"]["application"],
                         "C:/Users/x/AppData/Local/Borealis/borealis.exe")

    def test_direct_msedgedriver_session_is_the_webview2_mapping(self):
        caps = ws.webview2_payload("C:/app/borealis.exe")["capabilities"]["alwaysMatch"]
        self.assertEqual(caps["browserName"], "webview2")
        self.assertEqual(caps["ms:edgeOptions"]["binary"], "C:/app/borealis.exe")
        self.assertIn("webviewOptions", caps["ms:edgeOptions"])

    def test_driver_log_summary_keeps_driver_lines_and_drops_devtools_traffic(self):
        lines = ["[1.0][INFO]: Launching WebView2 app\n",
                 "[1.1][INFO]: DevTools WebSocket Response: Runtime.evaluate (id=7) {\"value\": \"page text\"}\n",
                 "[1.2][SEVERE]: DevToolsActivePort file doesn't exist\n",
                 "page text without a level\n",
                 "[1.3][DEBUG]: DevTools HTTP Request: http://localhost:1/json/version\n"]
        out = ws.driver_log_summary(lines)
        self.assertEqual(out[0], "driver log: 5 lines, 2 driver lines")
        self.assertEqual(out[1:], ["[1.0][INFO]: Launching WebView2 app",
                                   "[1.2][SEVERE]: DevToolsActivePort file doesn't exist"])
        self.assertNotIn("page text", "\n".join(out))

    def test_direct_mode_sends_the_webview2_session(self):
        client = ws.Client(self.client.base, ws.webview2_payload)
        quiet(ws.smoke, client, "C:/app/borealis.exe", 4)
        caps = FakeTauriDriver.state["caps"]["capabilities"]["alwaysMatch"]
        self.assertEqual(caps["ms:edgeOptions"]["binary"], "C:/app/borealis.exe")

    def test_attach_session_names_only_the_debugger_address(self):
        caps = ws.attach_payload("127.0.0.1:9222")["capabilities"]["alwaysMatch"]
        self.assertEqual(caps, {"browserName": "webview2",
                                "ms:edgeOptions": {"debuggerAddress": "127.0.0.1:9222"}})

    def test_launch_arguments_line_is_kept_longer(self):
        line = "[1.0][INFO]: WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = " + "--x " * 200
        out = ws.driver_log_summary([line])
        self.assertEqual(len(out[1]), 600)

    def test_debugger_address_needs_msedgedriver(self):
        with self.assertRaises(SystemExit):
            quiet(ws.main, ["--tauri-driver", "t", "--debugger-address", "127.0.0.1:9222"])
        with self.assertRaises(SystemExit):
            quiet(ws.main, ["--msedgedriver", "m"])

    def test_w3c_error_raises(self):
        with self.assertRaises(ws.WebDriverError):
            ws.unwrap({"value": {"error": "no such window", "message": "gone"}})
        self.assertEqual(ws.unwrap({"value": 3}), 3)

    def test_smoke_skips_the_closed_splash_and_retries_a_lost_click(self):
        with tempfile.TemporaryDirectory() as d:
            shot = Path(d) / "s.png"
            failed, _ = quiet(ws.smoke, self.client, "/usr/bin/borealis", 4, str(shot))
            self.assertIsNone(failed)
            self.assertEqual(shot.read_bytes(), b"hi")
        st = FakeTauriDriver.state
        self.assertEqual(st["caps"]["capabilities"]["alwaysMatch"]["tauri:options"]["application"],
                         "/usr/bin/borealis")
        self.assertEqual(st["clicks"], 2)
        self.assertTrue(st["deleted"])
        self.assertTrue(self.client.ready())

    def test_smoke_names_the_failed_check(self):
        FakeTauriDriver.state = {"clicks": -100}
        self.assertEqual(quiet(ws.smoke, self.client, "/usr/bin/borealis", 0.4)[0], "content screen opens")


if __name__ == "__main__":
    unittest.main()
