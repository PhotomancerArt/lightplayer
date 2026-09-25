#!/usr/bin/env python3
"""BLE-lab control server: relay between an agent (CLI) and a browser page.

spikes/serial-lab/server.py, copied and extended for the BLE spike (vision
`ble-remote-control`, 2026-09-23). The page (index.html, open in a real
browser that holds the Web Bluetooth link) subscribes to /events (SSE) and
executes command objects it receives; it posts results back to /result and
passive telemetry to /page-log. The agent posts commands to /cmd and gets the
page's correlated result as the HTTP response.

  agent  --POST /cmd {op,...}-->  server  --SSE-->  page (Brave, holds BLE)
  agent  <--result (blocks)-----  server  <--POST /result {id, ...}--

The one addition: the server also owns the board's USB serial console,
through scripts/emu/tty-capture.py (it never asserts DTR/RTS, which on an
Espressif native-USB port is the reset sequence). Every /cmd response carries
the console lines that arrived while it ran, so one call shows both sides.

Endpoints:
  GET  /            the lab page
  GET  /events      SSE command stream (the page listens here)
  POST /cmd         {op, ..., timeoutMs?} -> blocks for the page's result,
                    plus "serial": [console lines during the command]
  POST /result      {id, ok, ...} from the page
  POST /page-log    {line} passive telemetry from the page
  GET  /log?n=100   tail of page telemetry + lifecycle events
  GET  /status      server-side view: page connected? pending cmds? console?
  POST /serial      {dev}  start the console reader on that port
  DELETE /serial    stop it (before flashing: one holder per port)
  GET  /serial?n=50 tail of the console

Port: BLE_LAB_PORT, else scripts/dev-port.sh's stable per-worktree choice.
Binds 127.0.0.1 only.
"""

import json
import os
import threading
import time
import queue
import subprocess
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HERE = Path(__file__).parent
REPO = HERE.parent.parent
if "BLE_LAB_PORT" in os.environ:
    PORT = int(os.environ["BLE_LAB_PORT"])
else:
    PORT = int(subprocess.run([str(REPO / "scripts" / "dev-port.sh"), "ble-lab"],
                              capture_output=True, text=True, check=True).stdout.strip())

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


# -- the board's console ----------------------------------------------------
_serial = []  # rolling [(ts, line)]
_serial_seq = 0
_reader = {"proc": None, "dev": None}


def _pump(proc) -> None:
    global _serial_seq
    buf = b""
    while True:
        chunk = proc.stdout.read1(4096) if hasattr(proc.stdout, "read1") else proc.stdout.read(1)
        if not chunk:
            break
        buf += chunk
        while b"\n" in buf:
            raw, buf = buf.split(b"\n", 1)
            line = raw.rstrip(b"\r").decode("utf-8", errors="replace")
            with _lock:
                _serial.append((time.time(), line))
                _serial_seq += 1
                del _serial[:-_LOG_CAP]
    log_line(f"[server] console reader on {_reader['dev']} exited")


def start_serial(dev: str) -> None:
    stop_serial()
    proc = subprocess.Popen(
        ["/opt/homebrew/bin/python3", str(REPO / "scripts" / "emu" / "tty-capture.py"),
         "--dev", dev, "--out", "/dev/stdout", "--seconds", str(24 * 3600)],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    _reader.update(proc=proc, dev=dev)
    threading.Thread(target=_pump, args=(proc,), daemon=True).start()
    log_line(f"[server] console reader started on {dev}")


def stop_serial() -> bool:
    proc = _reader.get("proc")
    if not proc:
        return False
    proc.send_signal(2)  # SIGINT: tty-capture.py closes the port cleanly
    try:
        proc.wait(timeout=3)
    except subprocess.TimeoutExpired:
        proc.kill()
    log_line(f"[server] console reader stopped on {_reader['dev']}")
    _reader.update(proc=None, dev=None)
    return True


def serial_since(mark: int) -> list:
    """Console lines after the running count `mark`."""
    with _lock:
        n = _serial_seq - mark
        return [l for _, l in _serial[-n:]] if n > 0 else []


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
        elif path == "/serial":
            query = self.path.split("?")[1] if "?" in self.path else ""
            n = 50
            for part in query.split("&"):
                if part.startswith("n="):
                    n = int(part[2:])
            with _lock:
                tail = [l for _, l in _serial[-n:]]
            self._json(200, {"dev": _reader["dev"], "serial": tail})
        elif path == "/status":
            with _lock:
                self._json(200, {
                    "pages_connected": len(_subscribers),
                    "pending_cmds": list(_pending.keys()),
                    "console": _reader["dev"],
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
            with _lock:
                mark = _serial_seq
            n = broadcast(body)
            log_line(f"[cmd {cmd_id}] {json.dumps(body)} -> {n} page(s)")
            if n == 0:
                with _lock:
                    _pending.pop(cmd_id, None)
                self._json(503, {"error": "no page connected", "id": cmd_id})
                return
            answered = entry["event"].wait(timeout=timeout_s)
            with _lock:
                _pending.pop(cmd_id, None)
            time.sleep(0.3)  # let the board's console catch up with the page
            if answered:
                self._json(200, {**entry["result"], "serial": serial_since(mark)})
            else:
                self._json(504, {"error": "page did not answer in time",
                                 "id": cmd_id, "serial": serial_since(mark)})
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
        elif path == "/serial":
            dev = body.get("dev")
            if not dev:
                self._json(400, {"error": "need {dev}"})
                return
            start_serial(dev)
            self._json(200, {"ok": True, "dev": dev})
        elif path == "/page-log":
            log_line(f"[page] {body.get('line', json.dumps(body))}")
            self._json(200, {"ok": True})
        else:
            self._json(404, {"error": "not found"})

    def do_DELETE(self):
        if self.path.split("?")[0] == "/serial":
            self._json(200, {"ok": True, "stopped": stop_serial()})
        else:
            self._json(404, {"error": "not found"})


def main():
    server = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
    print(f"ble-lab control server on http://localhost:{PORT}", flush=True)
    log_line("[server] started")
    server.serve_forever()


if __name__ == "__main__":
    main()
