#!/usr/bin/env python3
"""Recursively push a local directory to a device path over serial-lab.

The 2026-08-29 small-dome install went out flat (the upload skipped
subdirectories), so Studio reported "definition not found:
/doors/module.json". This script walks a local tree, writes every file
over the wire (`FsRequest::Write`; firmware `ensure_parent_dirs` creates
the directories), then verifies with a recursive `ListDir` plus a
byte-identical read-back of every file.

Prereqs: `python3 spikes/serial-lab/server.py` running, lab page open with
the port granted, port open at 921600 (the script opens it if closed).

    python3 spikes/serial-lab/scripts/push-dir.py \
        --src /path/to/examples/small-dome --dest /projects/small-dome \
        [--no-stop] [--load] [--server http://localhost:29188]

Text files ship RAW (never pre-base64 text: the smart codec would store
base64-of-text as the literal base64 text — see lpc-wire serde_base64).
Binary files are refused; use a chunked path if that ever comes up.

--no-stop skips the stopAllProjects safety (flash writes wedge under
multi-wire playback: docs/defects/2026-08-29-flash-write-wedges-under-
zook-playback.md). --load issues LoadProject{dest} after a clean verify.

Exit 0 iff every write acked, ListDir matches, every read-back is
byte-identical (and --load, when given, got a response frame).
"""

import argparse
import json
import sys
import time
import urllib.request
from pathlib import Path

DEFAULT_SERVER = "http://localhost:29188"
FRAME_ID_BASE = 7000  # clear of the lab page (100+) and starvation-bench (9000+)


class Lab:
    def __init__(self, server):
        self.server = server
        self.next_frame_id = FRAME_ID_BASE

    def cmd(self, op, timeout=30, **kw):
        body = json.dumps({"op": op, **kw}).encode()
        req = urllib.request.Request(
            self.server + "/cmd", data=body, headers={"Content-Type": "application/json"}
        )
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read())

    def seq(self):
        return self.cmd("status").get("seq", 0)

    def send_frame(self, msg):
        frame_id = self.next_frame_id
        self.next_frame_id += 1
        self.cmd("frame", msg=msg, frameId=frame_id)
        return frame_id

    def await_response(self, frame_id, since, timeout_s=20.0):
        t0 = time.time()
        while time.time() - t0 < timeout_s:
            entries = self.cmd("buffer", since=since, max=2000).get("entries", [])
            for e in entries:
                frame = e.get("frame")
                if frame and frame.get("id") == frame_id:
                    return frame, time.time() - t0
            time.sleep(0.3)
        return None, time.time() - t0

    def request(self, msg, timeout_s=20.0):
        """Send one request frame and return (response_frame|None, elapsed)."""
        since = self.seq()
        fid = self.send_frame(msg)
        return self.await_response(fid, since, timeout_s)


def fs_error(frame):
    """The error field of an FsResponse frame, or a note when unparsable."""
    msg = (frame or {}).get("msg")
    if not isinstance(msg, dict):
        return "no parsed msg in response frame"
    body = msg.get("filesystem")
    if not isinstance(body, dict):
        return f"not a filesystem response: {json.dumps(msg)[:120]}"
    (_, payload), = body.items()
    return payload.get("error")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--src", required=True)
    ap.add_argument("--dest", required=True, help="absolute device prefix, e.g. /projects/small-dome")
    ap.add_argument("--server", default=DEFAULT_SERVER)
    ap.add_argument("--no-stop", action="store_true")
    ap.add_argument("--load", action="store_true", help="LoadProject{dest} after verify")
    args = ap.parse_args()

    src = Path(args.src)
    dest = args.dest.rstrip("/")
    if not src.is_dir():
        sys.exit(f"--src is not a directory: {src}")
    if not dest.startswith("/"):
        sys.exit(f"--dest must be absolute: {dest}")

    files = sorted(p for p in src.rglob("*") if p.is_file())
    plan = []
    for p in files:
        data = p.read_bytes()
        try:
            data.decode("utf-8")
        except UnicodeDecodeError:
            sys.exit(f"binary file (no chunked path here): {p}")
        plan.append((f"{dest}/{p.relative_to(src).as_posix()}", data))
    print(f"pushing {len(plan)} files, {sum(len(d) for _, d in plan)} B total → {dest}")

    lab = Lab(args.server)
    st = lab.cmd("status")
    if not st.get("ok"):
        sys.exit(f"lab page not responding: {st}")
    if st.get("baud") is None:
        print("port closed; opening at 921600")
        lab.cmd("adopt")
        opened = lab.cmd("open", baud=921600)
        if not opened.get("ok"):
            sys.exit(f"open failed: {opened}")

    if not args.no_stop:
        frame, dt = lab.request("stopAllProjects")
        print(f"stopAllProjects: {'ok' if frame else 'NO RESPONSE'} ({dt:.1f}s)")
        if frame is None:
            sys.exit("device unresponsive to stopAllProjects; aborting before writes")

    failures = []
    for path, data in plan:
        frame, dt = lab.request(
            {"filesystem": {"write": {"path": path, "data": data.decode("utf-8")}}}
        )
        err = None if frame is None else fs_error(frame)
        status = "NO RESPONSE" if frame is None else (err or "ok")
        print(f"  write {path} ({len(data)} B): {status} ({dt:.1f}s)")
        if frame is None or err:
            failures.append(f"write {path}: {status}")

    # Verify: recursive listing covers every pushed path.
    frame, dt = lab.request({"filesystem": {"listDir": {"path": dest, "recursive": True}}})
    listed = set()
    if frame is None:
        failures.append("listDir: NO RESPONSE")
    else:
        err = fs_error(frame)
        if err:
            failures.append(f"listDir: {err}")
        else:
            listed = set(frame["msg"]["filesystem"]["listDir"].get("entries", []))
    missing = {p for p, _ in plan} - listed
    if missing:
        failures.append(f"listDir missing {len(missing)}: {sorted(missing)}")
    print(f"listDir: {len(listed)} entries, {len(missing)} missing ({dt:.1f}s)")

    # Verify: byte-identical read-back.
    for path, data in plan:
        frame, dt = lab.request({"filesystem": {"read": {"path": path}}})
        err = None if frame is None else fs_error(frame)
        got = None
        if frame is not None and not err:
            raw = frame["msg"]["filesystem"]["read"].get("data")
            got = raw.encode("utf-8") if isinstance(raw, str) else raw
        ok = got == data
        print(f"  readback {path}: {'identical' if ok else 'MISMATCH' if not err else err} ({dt:.1f}s)")
        if not ok:
            failures.append(f"readback {path}: {err or 'content mismatch'}")

    if args.load and not failures:
        frame, dt = lab.request({"loadProject": {"path": dest}}, timeout_s=30.0)
        print(f"loadProject {dest}: {'ok' if frame else 'NO RESPONSE'} ({dt:.1f}s)")
        if frame is None:
            failures.append("loadProject: NO RESPONSE")
        else:
            print(f"  response: {json.dumps(frame.get('msg'))[:400]}")

    print()
    if failures:
        print(f"FAIL ({len(failures)}):")
        for f in failures:
            print(f"  - {f}")
        sys.exit(1)
    print(f"PASS: {len(plan)} files pushed and verified under {dest}")


if __name__ == "__main__":
    main()
