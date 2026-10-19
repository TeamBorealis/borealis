#!/usr/bin/env python3
"""Installed-app smoke through tauri-driver (WebDriver): the app starts, the main window shows the
start page, and the content screen opens.

Starts `tauri-driver` (Linux: WebKitWebDriver from webkit2gtk-driver, under Xvfb; Windows:
msedgedriver matching the WebView2 runtime, passed with --native-driver), opens a session on the
installed app, finds the window whose page shows "Additional content" (the splash window closes
while the app loads, so every window handle is tried), clicks that tab and waits for "Custom
content". Fresh install: only the bundled content is present, so nothing private reaches the log.

Usage: webdriver_smoke.py --tauri-driver PATH --app PATH [--native-driver PATH] [--port N]
                          [--timeout S] [--screenshot FILE]
Exit: 0 pass, 1 a check failed, 2 the driver did not start. Python 3.9+, stdlib only.
"""
import argparse
import base64
import json
import subprocess
import sys
import time
import urllib.error
import urllib.request

START_TEXT = r"^additional content$"
CONTENT_TEXT = r"custom content"
PAGE_HAS = ("const re = new RegExp(arguments[0], 'im');"
            "return !!document.body && re.test(document.body.innerText || '');")
# Clicks the deepest element whose whole text matches (the tab label), as a user's click lands there.
CLICK_TEXT = ("const re = new RegExp(arguments[0], 'i');"
              "const text = (e) => (e.innerText || '').trim();"
              "const hits = Array.from(document.querySelectorAll('body *')).filter((e) => re.test(text(e)));"
              "const leaf = hits.find((e) => !Array.from(e.children).some((c) => re.test(text(c))));"
              "if (!leaf) return false; leaf.click(); return true;")
POLL_S = 1.0
CLICK_TRIES = 4


class WebDriverError(RuntimeError):
    pass


def session_payload(app):
    """W3C new-session body for tauri-driver (tauri:options.application = the installed binary)."""
    caps = {"browserName": "wry", "tauri:options": {"application": app}}
    return {"capabilities": {"alwaysMatch": caps, "firstMatch": [{}]}}


def unwrap(body):
    """The `value` of a WebDriver reply; raises on a W3C error object."""
    value = body.get("value") if isinstance(body, dict) else None
    if isinstance(value, dict) and "error" in value:
        raise WebDriverError(f"{value['error']}: {str(value.get('message', ''))[:200]}")
    return value


class Client:
    def __init__(self, base):
        self.base = base.rstrip("/")
        self.sid = None

    def call(self, method, path, body=None, timeout=60):
        data = None if body is None else json.dumps(body).encode("utf-8")
        req = urllib.request.Request(self.base + path, data=data, method=method,
                                     headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                return unwrap(json.loads(resp.read().decode("utf-8") or "{}"))
        except urllib.error.HTTPError as e:
            return unwrap(json.loads(e.read().decode("utf-8") or "{}"))

    def ready(self):
        try:
            value = self.call("GET", "/status", timeout=5)
        except (OSError, ValueError, WebDriverError):
            return False
        return bool(value.get("ready", True)) if isinstance(value, dict) else True

    def new_session(self, app, timeout):
        value = self.call("POST", "/session", session_payload(app), timeout=timeout)
        self.sid = value["sessionId"]

    def s(self, method, path, body=None, timeout=60):
        return self.call(method, f"/session/{self.sid}{path}", body, timeout)

    def script(self, source, *args):
        return self.s("POST", "/execute/sync", {"script": source, "args": list(args)})

    def handles(self):
        return self.s("GET", "/window/handles")

    def switch(self, handle):
        self.s("POST", "/window", {"handle": handle})

    def screenshot(self, path):
        with open(path, "wb") as f:
            f.write(base64.b64decode(self.s("GET", "/screenshot")))

    def end(self):
        if self.sid:
            try:
                self.s("DELETE", "", timeout=30)
            except (OSError, ValueError, WebDriverError):
                pass
            self.sid = None


def wait_until(check, timeout):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if check():
            return True
        time.sleep(POLL_S)
    return False


def window_showing(client, pattern):
    """Switch to the first window whose page matches `pattern`; False when none does yet."""
    try:
        handles = client.handles()
    except (OSError, ValueError, WebDriverError):
        handles = []
    for handle in handles or [None]:
        try:
            if handle is not None:
                client.switch(handle)
            if client.script(PAGE_HAS, pattern):
                return True
        except (OSError, ValueError, WebDriverError):
            continue  # the splash window closed under us: list the handles again
    return False


def open_content(client, timeout):
    """Click the Additional content tab until the content screen shows (first clicks can be lost)."""
    for _ in range(CLICK_TRIES):
        try:
            client.script(CLICK_TEXT, START_TEXT)
        except (OSError, ValueError, WebDriverError):
            pass
        if wait_until(lambda: window_showing(client, CONTENT_TEXT), timeout / CLICK_TRIES):
            return True
    return False


def smoke(client, app, timeout, screenshot=None):
    """Runs the checks; returns the failed check's name, or None."""
    t0 = time.monotonic()
    client.new_session(app, timeout)
    print(f"smoke: session on the installed app after {time.monotonic() - t0:.0f} s", flush=True)
    try:
        if not wait_until(lambda: window_showing(client, START_TEXT), timeout):
            return "main window shows the start page"
        print(f"smoke: main window shows the start page after {time.monotonic() - t0:.0f} s", flush=True)
        if not open_content(client, timeout):
            return "content screen opens"
        print("smoke: the content screen opens", flush=True)
        return None
    finally:
        if screenshot:
            try:
                client.screenshot(screenshot)
            except (OSError, ValueError, WebDriverError) as e:
                print(f"smoke: no screenshot ({type(e).__name__})")
        client.end()


def main(argv=None):
    ap = argparse.ArgumentParser(description="installed-app smoke through tauri-driver")
    ap.add_argument("--tauri-driver", required=True)
    ap.add_argument("--app", required=True)
    ap.add_argument("--native-driver")
    ap.add_argument("--port", type=int, default=4444)
    ap.add_argument("--timeout", type=float, default=180.0)
    ap.add_argument("--screenshot")
    args = ap.parse_args(argv)
    cmd = [args.tauri_driver, "--port", str(args.port)]
    if args.native_driver:
        cmd += ["--native-driver", args.native_driver]
    driver = subprocess.Popen(cmd)
    client = Client(f"http://127.0.0.1:{args.port}")
    try:
        if not wait_until(client.ready, 60):
            print("::error::tauri-driver did not answer on its port within 60 s")
            return 2
        failed = smoke(client, args.app, args.timeout, args.screenshot)
    except (OSError, ValueError, KeyError, WebDriverError) as e:
        failed = f"WebDriver session ({type(e).__name__}: {str(e)[:200]})"
    finally:
        driver.terminate()
        try:
            driver.wait(timeout=30)
        except subprocess.TimeoutExpired:
            driver.kill()
    if failed:
        print(f"::error::installed-app smoke failed: {failed}")
        return 1
    print("smoke: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
