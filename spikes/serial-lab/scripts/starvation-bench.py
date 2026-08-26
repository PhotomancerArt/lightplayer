#!/usr/bin/env python3
"""Starvation bench: drive serial-lab to prove (or falsify) the exit criteria
of docs/debt/shared-uart-io-task-starvation.md on a real board.

Prereqs: `python3 spikes/serial-lab/server.py` running, the lab page open in a
real browser with the port granted (the human clicks Grant once), firmware
with the io_task fix flashed. Then:

    python3 spikes/serial-lab/scripts/starvation-bench.py [--project /path]

Checks, in order (summary table at the end):

  C1 liveness   — heartbeats/perf lines arrive at all.
  C2 load       — a project is ticking (tick >= --load-ms). If idle, tries
                  listAvailableProjects + loadProject (or --project) itself.
  C3 inbound    — a >=4 KB filesystem write lands intact UNDER LOAD and reads
                  back byte-identical (the debt entry's inbound criterion).
  C3b inbound   — the same at ~12 KiB: the ProjectRead-scale frame shape
                  Studio actually sends, kept under the ~16.7 KiB server
                  frame budget so the readback response fits the wire.
  C4 outbound   — request/response round-trips flow UNDER LOAD (the
                  responses=0 criterion): N pings, all answered.
  C5 idle       — regression control: the same ops stay perfect when idle
                  (stopAllProjects first). NOTE: leaves the device idle.
  C6 session    — a dead session's partial frame must not wedge the next
                  session: write a partial M! line with no newline, wait past
                  the firmware's stale-partial flush, then hello and expect
                  an answer (2026-08-21 wedge, hello-gate defect).

Evidence lines (FifoOverflowed, TX write timed out/failed, retry attempts,
stale-partial/hello-drain flushes) are counted per phase and printed.

Exit code 0 iff C1, C3, C3b, C4, C5, C6 all pass (C2 is advisory: if the
script cannot start a load, the bench human loads one and re-runs)."""

import argparse
import base64
import json
import re
import sys
import time
import urllib.request

DEFAULT_SERVER = "http://localhost:29188"
# 82 lines x 56 B = 4592 B > the 4 KiB exit criterion, and comfortably past
# the 128 B FIFO and the ~4.5 KB shape the 2026-08-21 baseline used.
PAYLOAD_LINES = 82
BENCH_PATH = "/bench-inbound.txt"
FRAME_ID_BASE = 9000  # clear of the lab page's own nextFrameId (100+)

EVIDENCE_PATTERNS = {
    "fifo_overflow": re.compile(r"FifoOverflowed|UART RX error"),
    "tx_fail": re.compile(r"UART (?:TX )?write (?:timed out|failed)|TX timed out"),
    "retry_win": re.compile(r"written on attempt"),
    "dropped_msg": re.compile(r"dropping message id"),
    "stale_flush": re.compile(r"discarding \d+ B partial line"),
    "hello_drain": re.compile(r"hello: dropped \d+ outbound lines"),
}
PERF_RE = re.compile(r"\[perf\].*?tick=(\d+)ms.*?responses=(\d+)")


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

    def entries_since(self, since, max_entries=2000):
        return self.cmd("buffer", since=since, max=max_entries).get("entries", [])

    def send_frame(self, msg, frame_id=None):
        if frame_id is None:
            frame_id = self.next_frame_id
            self.next_frame_id += 1
        self.cmd("frame", msg=msg, frameId=frame_id)
        return frame_id

    def await_response(self, frame_id, since, timeout_s=15.0):
        """Poll the buffer for a frame whose envelope id matches frame_id.
        Returns (frame_or_None, elapsed_s, all_entries_seen)."""
        t0 = time.time()
        seen = []
        while time.time() - t0 < timeout_s:
            entries = self.entries_since(since)
            seen = entries
            for e in entries:
                frame = e.get("frame")
                if frame and frame.get("id") == frame_id:
                    return frame, time.time() - t0, seen
            time.sleep(0.3)
        return None, time.time() - t0, seen


def count_evidence(entries, counters):
    for e in entries:
        text = e.get("text", "")
        for name, pat in EVIDENCE_PATTERNS.items():
            if pat.search(text):
                counters[name] = counters.get(name, 0) + 1


def last_perf(entries):
    """(tick_ms, responses) from the newest [perf] line, or None."""
    for e in reversed(entries):
        m = PERF_RE.search(e.get("text", ""))
        if m:
            return int(m.group(1)), int(m.group(2))
    return None


def find_string(obj, wanted):
    """Recursively search a decoded frame for `wanted`, tolerating the
    filesystem read response returning text either raw or base64 — the
    script must not hard-code the response schema."""
    if isinstance(obj, str):
        if obj == wanted:
            return True
        try:
            if base64.b64decode(obj, validate=True).decode() == wanted:
                return True
        except Exception:
            pass
        return False
    if isinstance(obj, dict):
        return any(find_string(v, wanted) for v in obj.values())
    if isinstance(obj, list):
        return any(find_string(v, wanted) for v in obj)
    return False


def payload_text(n_lines=PAYLOAD_LINES):
    lines = []
    for i in range(n_lines):
        line = f"bench line {i:03d} " + "abcdefghij" * 4
        lines.append(line[:63])
    return "\n".join(lines) + "\n"


# ~12 KiB: ProjectRead-scale, yet the readback response (payload + JSON
# framing) stays under the ~16.7 KiB server frame budget.
BIG_PAYLOAD_LINES = 215


def measure_tick(lab, seconds=6):
    since = lab.seq()
    lab.cmd("capture", ms=seconds * 1000, timeoutMs=seconds * 1000 + 8000)
    entries = lab.entries_since(since)
    return last_perf(entries), entries


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--server", default=DEFAULT_SERVER)
    ap.add_argument("--project", help="project path for loadProject if the device is idle")
    ap.add_argument("--load-ms", type=int, default=10,
                    help="tick >= this counts as 'under load' (dome-scale is ~41)")
    ap.add_argument("--pings", type=int, default=10)
    ap.add_argument("--skip-load", action="store_true",
                    help="measure whatever state the device is in; C2 informational")
    args = ap.parse_args()

    lab = Lab(args.server)
    results = {}   # name -> (ok, detail)
    evidence = {}

    # C0/C1 — port + liveness -------------------------------------------------
    status = lab.cmd("status")
    if not status.get("baud"):
        lab.cmd("adopt")
        opened = lab.cmd("open", baud=921600)
        if not opened.get("ok"):
            print(f"FATAL: cannot open port: {opened}", file=sys.stderr)
            return 2
    perf, entries = measure_tick(lab)
    heartbeats = [e for e in entries if e.get("frame", {}).get("id") == 0]
    count_evidence(entries, evidence)
    results["C1 liveness"] = (
        bool(heartbeats) or perf is not None,
        f"{len(heartbeats)} id=0 frames, perf={'yes' if perf else 'none'} in 6 s",
    )

    # C2 — load ---------------------------------------------------------------
    tick = perf[0] if perf else 0
    if tick < args.load_ms and not args.skip_load:
        path = args.project
        if not path:
            since = lab.seq()
            fid = lab.send_frame("listAvailableProjects")
            frame, _, _ = lab.await_response(fid, since)
            if frame:
                projects = []

                def collect(obj):
                    if isinstance(obj, dict):
                        for k, v in obj.items():
                            if k == "projects" and isinstance(v, list):
                                projects.extend(v)
                            else:
                                collect(v)
                    elif isinstance(obj, list):
                        for v in obj:
                            collect(v)

                collect(frame)
                for p in projects:
                    cand = p if isinstance(p, str) else (p.get("path") or p.get("name"))
                    if cand:
                        path = cand
                        break
        if path:
            since = lab.seq()
            fid = lab.send_frame({"loadProject": {"path": path}})
            lab.await_response(fid, since)
            time.sleep(3)
            perf, entries = measure_tick(lab)
            count_evidence(entries, evidence)
            tick = perf[0] if perf else 0
    results["C2 load"] = (
        tick >= args.load_ms,
        f"tick={tick}ms (need >={args.load_ms}ms; dome-scale ~41ms)",
    )
    under_load = tick >= args.load_ms

    payload = payload_text()

    def inbound_check(label, payload=payload, timeout_s=20):
        since = lab.seq()
        fid = lab.send_frame({"filesystem": {"write": {"path": BENCH_PATH, "data": payload}}})
        wframe, wlat, wentries = lab.await_response(fid, since, timeout_s=timeout_s)
        count_evidence(wentries, evidence)
        if not wframe:
            return False, f"write ({len(payload)} B): NO response in {wlat:.1f} s"
        since = lab.seq()
        fid = lab.send_frame({"filesystem": {"read": {"path": BENCH_PATH}}})
        rframe, rlat, rentries = lab.await_response(fid, since, timeout_s=timeout_s)
        count_evidence(rentries, evidence)
        if not rframe:
            return False, f"write ok {wlat:.1f}s; read: NO response in {rlat:.1f} s"
        if not find_string(rframe, payload):
            return False, f"write ok; read answered {rlat:.1f}s but content MISMATCH"
        return True, f"{len(payload)} B write {wlat:.1f}s + verified readback {rlat:.1f}s"

    def outbound_check(label, pings):
        got, lats = 0, []
        for _ in range(pings):
            since = lab.seq()
            fid = lab.send_frame("listLoadedProjects")
            frame, lat, entries = lab.await_response(fid, since, timeout_s=10)
            count_evidence(entries, evidence)
            if frame:
                got += 1
                lats.append(lat)
            time.sleep(0.1)
        detail = f"{got}/{pings} answered"
        if lats:
            detail += f", median {sorted(lats)[len(lats) // 2]:.2f}s"
        return got == pings, detail

    # C3/C3b/C4 — under load --------------------------------------------------
    results["C3 inbound/load"] = inbound_check("load") if under_load else (
        False, "SKIPPED: no load (see C2)")
    results["C3b inbound12k/load"] = (
        inbound_check("load-12k", payload=payload_text(BIG_PAYLOAD_LINES), timeout_s=40)
        if under_load
        else (False, "SKIPPED: no load (see C2)")
    )
    results["C4 outbound/load"] = outbound_check("load", args.pings) if under_load else (
        False, "SKIPPED: no load (see C2)")

    # C5 — idle control -------------------------------------------------------
    since = lab.seq()
    fid = lab.send_frame("stopAllProjects")
    lab.await_response(fid, since, timeout_s=10)
    time.sleep(1)
    ok_in, det_in = inbound_check("idle")
    ok_out, det_out = outbound_check("idle", 3)
    results["C5 idle control"] = (ok_in and ok_out, f"inbound: {det_in}; outbound: {det_out}")

    # C6 — dead-session partial frame must not wedge the next hello -----------
    lab.cmd("eval", js='return lab.writeText("M!{\\"id\\":8999,\\"msg\\":\\"listL")')
    time.sleep(1.6)  # > firmware STALE_PARTIAL_TIMEOUT (1 s)
    since = lab.seq()
    fid = lab.send_frame("hello")
    frame, lat, entries = lab.await_response(fid, since, timeout_s=10)
    count_evidence(entries, evidence)
    flushed = evidence.get("stale_flush", 0)
    results["C6 session"] = (
        frame is not None,
        f"hello after torn frame: {'answered %.1fs' % lat if frame else 'NO ANSWER'}"
        + f", stale-partial flush lines seen: {flushed}",
    )

    # Summary -----------------------------------------------------------------
    print("\n=== starvation-bench summary ===")
    width = max(len(k) for k in results)
    critical_ok = True
    for name, (ok, detail) in results.items():
        mark = "PASS" if ok else "FAIL"
        if name.startswith(("C1", "C3", "C3b", "C4", "C5", "C6")) and not ok:
            critical_ok = False
        print(f"  {name:<{width}}  {mark}  {detail}")
    print("  evidence:", json.dumps(evidence) if evidence else "none")
    print("  NOTE: device left idle (C5 ran stopAllProjects); reload the project"
          " before re-measuring under load.")
    return 0 if critical_ok else 1


if __name__ == "__main__":
    sys.exit(main())
