#!/usr/bin/env python3
"""Evaluate JavaScript in the iOS simulator's WKWebView through the WebKit remote inspector (the socket Safari's
Develop menu uses). A debug build of Borealis is inspectable; nothing in the app is changed.

    python3 -I rwi.py serve --port 9334 [--bundle app.borealis]   # HTTP bridge: POST /eval {"expr": "..."} -> {"value": ...}

The simulator's inspector socket is read from `simctl spawn <udid> launchctl getenv RWI_LISTEN_SOCKET`. Messages are a
4-byte big-endian length and a binary property list (`__selector`, `__argument`); the page's inspector protocol is JSON
inside `WIRSocketDataKey`, wrapped in `Target.sendMessageToTarget` for the page target.
"""

from __future__ import annotations

import argparse
import json
import os
import plistlib
import socket
import struct
import subprocess
import threading
import time
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

DEVELOPER_DIR = os.environ.get("DEVELOPER_DIR", "/Applications/Xcode.app/Contents/Developer")
TIMEOUT_S = 60.0


def socket_path(udid: str) -> str:
    out = subprocess.run(
        ["xcrun", "simctl", "spawn", udid, "launchctl", "getenv", "RWI_LISTEN_SOCKET"],
        capture_output=True, text=True, check=True, env={**os.environ, "DEVELOPER_DIR": DEVELOPER_DIR},
    ).stdout.strip()
    if not out:
        raise SystemExit(f"no RWI_LISTEN_SOCKET in simulator {udid} (is it booted?)")
    return out


class Inspector:
    """One connection to webinspectord_sim and one page of one application."""

    def __init__(self, path: str, bundle: str) -> None:
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(path)
        self.conn = str(uuid.uuid4()).upper()
        self.sender = str(uuid.uuid4()).upper()
        self.bundle = bundle
        self.apps: dict[str, dict] = {}
        self.listing: dict[str, dict] = {}
        self.app_id = ""
        self.page_id = 0
        self.target = ""
        self.next_id = 1
        self.replies: dict[int, dict] = {}
        self.cv = threading.Condition()
        self.buf = b""
        threading.Thread(target=self._reader, daemon=True).start()
        self._send("_rpc_reportIdentifier:", {"WIRConnectionIdentifierKey": self.conn})

    # -- wire --------------------------------------------------------------------------------------------------------
    def _send(self, selector: str, argument: dict) -> None:
        body = plistlib.dumps({"__selector": selector, "__argument": argument}, fmt=plistlib.FMT_BINARY)
        self.sock.sendall(struct.pack(">I", len(body)) + body)

    def _reader(self) -> None:
        while True:
            try:
                chunk = self.sock.recv(1 << 16)
            except OSError:
                return
            if not chunk:
                return
            self.buf += chunk
            while len(self.buf) >= 4:
                (n,) = struct.unpack(">I", self.buf[:4])
                if len(self.buf) < 4 + n:
                    break
                msg = plistlib.loads(self.buf[4 : 4 + n])
                self.buf = self.buf[4 + n :]
                self._dispatch(msg)

    def _dispatch(self, msg: dict) -> None:
        sel = msg.get("__selector", "")
        arg = msg.get("__argument", {})
        with self.cv:
            if sel == "_rpc_reportConnectedApplicationList:":
                self.apps = dict(arg.get("WIRApplicationDictionaryKey", {}))
            elif sel == "_rpc_applicationConnected:":
                self.apps[arg["WIRApplicationIdentifierKey"]] = arg
            elif sel == "_rpc_applicationSentListing:":
                if arg.get("WIRApplicationIdentifierKey") == self.app_id:
                    self.listing = dict(arg.get("WIRListingKey", {}))
            elif sel == "_rpc_applicationSentData:":
                self._protocol(json.loads(arg["WIRMessageDataKey"]))
            self.cv.notify_all()

    def _protocol(self, data: dict) -> None:
        method = data.get("method", "")
        if method == "Target.targetCreated":
            info = data["params"]["targetInfo"]
            if info.get("type") == "page":
                self.target = info["targetId"]
        elif method == "Target.dispatchMessageFromTarget":
            inner = json.loads(data["params"]["message"])
            if "id" in inner:
                self.replies[inner["id"]] = inner
        elif "id" in data:
            self.replies[data["id"]] = data

    def _wait(self, cond, what: str, timeout: float = TIMEOUT_S):
        end = time.monotonic() + timeout
        with self.cv:
            while not (value := cond()):
                left = end - time.monotonic()
                if left <= 0:
                    raise TimeoutError(what)
                self.cv.wait(left)
            return value

    # -- attach ------------------------------------------------------------------------------------------------------
    def attach(self) -> None:
        def app() -> str:
            for key, a in self.apps.items():
                if a.get("WIRApplicationBundleIdentifierKey") == self.bundle:
                    return key
            return ""

        self.app_id = self._wait(app, f"{self.bundle} in the inspector's application list")
        self._send("_rpc_forwardGetListing:", {"WIRConnectionIdentifierKey": self.conn, "WIRApplicationIdentifierKey": self.app_id})

        def page() -> int:
            for p in self.listing.values():
                if p.get("WIRTypeKey") in ("WIRTypeWeb", "WIRTypeWebPage", "WIRTypePage") or "WIRURLKey" in p:
                    return int(p["WIRPageIdentifierKey"])
            return 0

        self.page_id = self._wait(page, "an inspectable page")
        self._send(
            "_rpc_forwardSocketSetup:",
            {
                "WIRConnectionIdentifierKey": self.conn, "WIRApplicationIdentifierKey": self.app_id,
                "WIRPageIdentifierKey": self.page_id, "WIRSenderKey": self.sender, "WIRAutomaticallyPause": False,
            },
        )
        try:
            self._wait(lambda: self.target, "the page target", timeout=5.0)
        except TimeoutError:
            self.target = ""  # an older WebKit without targets: messages go to the page directly

    def close(self) -> None:
        """Detach from the page (so a later connection gets it) and close the socket."""
        try:
            if self.page_id:
                self._send(
                    "_rpc_forwardDidClose:",
                    {
                        "WIRConnectionIdentifierKey": self.conn, "WIRApplicationIdentifierKey": self.app_id,
                        "WIRPageIdentifierKey": self.page_id, "WIRSenderKey": self.sender,
                    },
                )
        finally:
            self.sock.close()

    def call(self, method: str, params: dict) -> dict:
        with self.cv:
            mid = self.next_id
            self.next_id += 1
        inner = json.dumps({"id": mid, "method": method, "params": params})
        if self.target:
            with self.cv:
                outer_id = self.next_id
                self.next_id += 1
            payload = json.dumps({"id": outer_id, "method": "Target.sendMessageToTarget", "params": {"targetId": self.target, "message": inner}})
        else:
            payload = inner
        self._send(
            "_rpc_forwardSocketData:",
            {
                "WIRConnectionIdentifierKey": self.conn, "WIRApplicationIdentifierKey": self.app_id,
                "WIRPageIdentifierKey": self.page_id, "WIRSenderKey": self.sender, "WIRSocketDataKey": payload.encode(),
            },
        )
        return self._wait(lambda: self.replies.pop(mid, None), f"reply to {method}")

    def evaluate(self, expr: str):
        r = self.call("Runtime.evaluate", {"expression": expr, "returnByValue": True, "awaitPromise": True})
        res = r.get("result", {})
        if r.get("error"):
            raise RuntimeError(json.dumps(r["error"]))
        if res.get("wasThrown"):
            raise RuntimeError(json.dumps(res.get("result", {}).get("description", res)))
        return res.get("result", {}).get("value")


def serve(udid: str, bundle: str, port: int) -> None:
    state: dict[str, Inspector] = {}
    lock = threading.Lock()

    def drop() -> None:
        with lock:
            old = state.pop("i", None)
        if old is not None:
            try:
                old.close()
            except OSError:
                pass

    def inspector() -> Inspector:
        with lock:
            if "i" not in state:
                i = Inspector(socket_path(udid), bundle)
                i.attach()
                state["i"] = i
            return state["i"]

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args) -> None:
            return

        def do_POST(self) -> None:  # noqa: N802
            body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", "0"))) or b"{}")
            try:
                if self.path == "/reset":
                    drop()
                    out = {"value": True}
                else:
                    out = {"value": inspector().evaluate(body["expr"])}
            except Exception as e:  # noqa: BLE001 (the caller gets every failure as text)
                if isinstance(e, (OSError, TimeoutError)):
                    drop()
                out = {"error": f"{type(e).__name__}: {e}"}
            data = json.dumps(out).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

    ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("serve")
    s.add_argument("--udid", required=True)
    s.add_argument("--bundle", default="app.borealis")
    s.add_argument("--port", type=int, default=9334)
    e = sub.add_parser("eval")
    e.add_argument("--udid", required=True)
    e.add_argument("--bundle", default="app.borealis")
    e.add_argument("expr")
    a = p.parse_args()
    if a.cmd == "serve":
        serve(a.udid, a.bundle, a.port)
    else:
        i = Inspector(socket_path(a.udid), a.bundle)
        i.attach()
        try:
            print(json.dumps(i.evaluate(a.expr)))
        finally:
            i.close()


if __name__ == "__main__":
    main()
