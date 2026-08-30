#!/usr/bin/env python3
"""Serial-lab control server: relay between an agent (CLI) and a browser page.

Zero dependencies. The page (index.html, open in a real browser that holds the
Web Serial port) subscribes to /events (SSE) and executes command objects it
receives; it posts results back to /result and passive telemetry to /page-log.
The agent posts commands to /cmd and gets the page's correlated result as the
HTTP response (long-poll style, WebSocket-equivalent for this purpose).

  agent  --POST /cmd {op,...}-->  server  --SSE-->  page (Brave, holds serial)
  agent  <--result (blocks)-----  server  <--POST /result {id, ...}--

Endpoints:
  GET  /            the lab page
  GET  /events      SSE command stream (the page listens here)
  POST /cmd         {op, ..., timeoutMs?} -> blocks for the page's result
  POST /result      {id, ok, ...} from the page
  POST /page-log    {line} passive telemetry from the page
  GET  /log?n=100   tail of page telemetry + lifecycle events
  GET  /status      server-side view: page connected? pending cmds?
"""

import json
import os
import threading
import time
import queue
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

PORT = int(os.environ.get("SERIAL_LAB_PORT", "29188"))
HERE = Path(__file__).parent

_lock = threading.Lock()
_next_cmd_id = 1
_pending = {}  # cmd_id -> {"event": Event, "result": dict|None}
_subscribers = []  # list of queue.Queue for SSE connections
_log = []  # rolling [(ts, line)]
_LOG_CAP = 2000


def log_line(line: str) -> None:
    with _lock:
        _log.append((time.time(), line))
        del _log[:-_LOG_CAP]


def broadcast(obj: dict) -> int:
    """Queue a JSON object to every connected SSE subscriber; returns count."""
    with _lock:
        subs = list(_subscribers)
    for q in subs:
        q.put(obj)
    return len(subs)


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):  # quiet the default stderr spam
        pass

    # -- helpers ---------------------------------------------------------
    def _json(self, code: int, obj: dict) -> None:
        body = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.end_headers()
        self.wfile.write(body)

    def _read_body(self) -> dict:
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length) if length else b"{}"
        try:
            return json.loads(raw or b"{}")
        except json.JSONDecodeError:
            return {"_parse_error": raw.decode(errors="replace")}

    # -- routes ----------------------------------------------------------
    def do_GET(self):
        path = self.path.split("?")[0]
        if path == "/":
            body = (HERE / "index.html").read_bytes()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        elif path == "/events":
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            q: queue.Queue = queue.Queue()
            with _lock:
                _subscribers.append(q)
            log_line("[server] page subscribed to /events")
            try:
                while True:
                    try:
                        obj = q.get(timeout=15)
                        payload = f"data: {json.dumps(obj)}\n\n"
                    except queue.Empty:
                        payload = ": keepalive\n\n"
                    self.wfile.write(payload.encode())
                    self.wfile.flush()
            except (BrokenPipeError, ConnectionResetError, OSError):
                pass
            finally:
                with _lock:
                    if q in _subscribers:
                        _subscribers.remove(q)
                log_line("[server] page SSE disconnected")
        elif path == "/log":
            query = self.path.split("?")[1] if "?" in self.path else ""
            n = 100
            for part in query.split("&"):
                if part.startswith("n="):
                    n = int(part[2:])
            with _lock:
                tail = _log[-n:]
            self._json(200, {"log": [{"t": t, "line": l} for t, l in tail]})
        elif path == "/status":
            with _lock:
                self._json(200, {
                    "pages_connected": len(_subscribers),
                    "pending_cmds": list(_pending.keys()),
                })
        else:
            self._json(404, {"error": "not found"})

    def do_POST(self):
        global _next_cmd_id
        path = self.path.split("?")[0]
        body = self._read_body()
        if path == "/cmd":
            timeout_s = float(body.pop("timeoutMs", 30000)) / 1000.0
            with _lock:
                cmd_id = _next_cmd_id
                _next_cmd_id += 1
                entry = {"event": threading.Event(), "result": None}
                _pending[cmd_id] = entry
            body["id"] = cmd_id
            n = broadcast(body)
            log_line(f"[cmd {cmd_id}] {json.dumps(body)} -> {n} page(s)")
            if n == 0:
                with _lock:
                    _pending.pop(cmd_id, None)
                self._json(503, {"error": "no page connected", "id": cmd_id})
                return
            if entry["event"].wait(timeout=timeout_s):
                with _lock:
                    _pending.pop(cmd_id, None)
                self._json(200, entry["result"])
            else:
                with _lock:
                    _pending.pop(cmd_id, None)
                self._json(504, {"error": "page did not answer in time",
                                 "id": cmd_id})
        elif path == "/result":
            cmd_id = body.get("id")
            with _lock:
                entry = _pending.get(cmd_id)
            if entry:
                entry["result"] = body
                entry["event"].set()
                self._json(200, {"ok": True})
            else:
                log_line(f"[server] result for unknown cmd {cmd_id}")
                self._json(200, {"ok": False, "note": "no waiter"})
        elif path == "/page-log":
            log_line(f"[page] {body.get('line', json.dumps(body))}")
            self._json(200, {"ok": True})
        else:
            self._json(404, {"error": "not found"})


def main():
    server = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
    print(f"serial-lab control server on http://localhost:{PORT}")
    log_line("[server] started")
    server.serve_forever()


if __name__ == "__main__":
    main()
