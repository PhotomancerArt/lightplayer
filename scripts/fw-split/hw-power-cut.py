#!/usr/bin/env python3
"""Real power cuts during a real X -> Y update, on a C6 behind a switchable hub.

    hw-power-cut.py --mac 10:BD:A3:B0:BD:A8 --hub 1-1.3 --hub-port 2 \\
        --x <split-build-x> --y <split-build-y> --lp-cli <lp-cli> --out <dir> \\
        --cuts 0.5,3,7,11,...

Per cut T (seconds after the offer starts):
  1. write X's app.bin at 0x10000 (loader, ONE boot record, X core, X engine;
     the second record sector is written erased), and wait for X's heartbeat;
  2. start a host offering Y; after T seconds switch the hub port's power
     off for --off-secs, then on;
  3. host again, offering Y, for --recover-secs; read the console.
PASS = the last boot logged is Y's core and a heartbeat follows it (Y's engine
running). Also reported: what the first boot after the cut was.

Needs uhubctl and a hub with per-port power switching. Spike-grade tooling.
"""

import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]


def sh(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def port_of(mac, wait=15.0):
    end = time.time() + wait
    while time.time() < end:
        r = sh([sys.executable, str(REPO / "scripts/emu/board-port.py"), mac])
        p = r.stdout.strip()
        if r.returncode == 0 and p.startswith("/dev/"):
            return p
        time.sleep(0.3)
    return None


def power(hub, port, on):
    sh(["uhubctl", "-l", hub, "-p", str(port), "-a", "on" if on else "off", "-e"])


def capture(lp_cli, port, console, seconds, offer=None, exit_on=None):
    cmd = [lp_cli, "link", "capture", port, "--console", str(console), "--seconds", str(seconds)]
    if offer:
        cmd += ["--ota-offer", str(offer)]
    if exit_on:
        cmd += ["--exit-on", exit_on]
    return cmd


def story(text):
    """[(kind, line)] for the lines that tell what happened, in order."""
    out = []
    for line in text.splitlines():
        if "[CORE] build" in line:
            m = re.search(r"build \S+\+(\w+) @(0x[0-9a-f]+)( \(trial\))?", line)
            if m:
                out.append(("core", f"{m.group(1)}@{m.group(2)}{' trial' if m.group(3) else ''}"))
        elif "no engine at" in line:
            out.append(("core-only", ""))
        elif "rolled back" in line and "[OTA]" in line:
            out.append(("rolled-back", ""))
        elif "not trusted" in line or "refused" in line:
            out.append(("refused", line.split("[OTA]")[-1].strip()[:60]))
        elif '"heartbeat":{' in line:
            out.append(("heartbeat", ""))
    # Log lines can arrive twice (raw + ring); keep order, drop adjacent repeats.
    dedup = []
    for s in out:
        if not dedup or dedup[-1] != s:
            dedup.append(s)
    return dedup


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mac", required=True)
    ap.add_argument("--hub", required=True)
    ap.add_argument("--hub-port", type=int, required=True)
    ap.add_argument("--x", required=True)
    ap.add_argument("--y", required=True)
    ap.add_argument("--lp-cli", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--cuts", required=True, help="comma list of seconds after the offer starts")
    ap.add_argument("--off-secs", type=float, default=2.0)
    ap.add_argument("--recover-secs", type=int, default=90)
    a = ap.parse_args()
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    rows = []
    for t in [float(c) for c in a.cuts.split(",")]:
        tag = f"cut-{t:05.1f}"
        port = port_of(a.mac)
        if not port:
            print(f"{tag}: board not on the bus — stopping", flush=True)
            break
        # 1. back to X
        r = sh(["espflash", "write-bin", "--chip", "esp32c6", "--port", port, "0x10000",
                str(Path(a.x) / "app.bin")])
        if r.returncode != 0:
            print(f"{tag}: flashing X failed: {r.stderr[-300:]}", flush=True)
            break
        port = port_of(a.mac)
        pre = out / f"{tag}.pre.txt"
        sh(capture(a.lp_cli, port, pre, 20, exit_on='"heartbeat":{'))
        if '"heartbeat":{' not in pre.read_text(errors="replace"):
            print(f"{tag}: X never heartbeat after flashing — skipping", flush=True)
            continue
        # 2. the offer, and the cut
        cut_console = out / f"{tag}.cut.txt"
        host = subprocess.Popen(capture(a.lp_cli, port, cut_console, int(t) + 60, offer=a.y),
                                stdout=subprocess.DEVNULL, stderr=open(out / f"{tag}.cut.err", "w"))
        time.sleep(t)
        power(a.hub, a.hub_port, False)
        host.kill()
        host.wait()
        time.sleep(a.off_secs)
        power(a.hub, a.hub_port, True)
        # 3. recover
        port = port_of(a.mac, wait=20)
        if not port:
            rows.append({"cut": t, "verdict": "FAIL", "why": "board did not come back on USB"})
            print(json.dumps(rows[-1]), flush=True)
            continue
        rec = out / f"{tag}.rec.txt"
        subprocess.run(capture(a.lp_cli, port, rec, a.recover_secs, offer=a.y),
                       stdout=subprocess.DEVNULL, stderr=open(out / f"{tag}.rec.err", "w"))
        before = story(cut_console.read_text(errors="replace")) if cut_console.exists() else []
        after = story(rec.read_text(errors="replace"))
        cores = [s for s in after if s[0] == "core"]
        last_core_i = max((i for i, s in enumerate(after) if s[0] == "core"), default=-1)
        ok = (last_core_i >= 0 and after[last_core_i][1].startswith("y@")
              and any(s[0] == "heartbeat" for s in after[last_core_i:]))
        reached = [s for s in before if s[0] in ("core", "core-only", "rolled-back")]
        rows.append({
            "cut": t,
            "verdict": "PASS" if ok else "FAIL",
            "before_cut": " → ".join(f"{k}:{v}" if v else k for k, v in reached[-3:]),
            "first_boot_after": cores[0][1] if cores else "?",
            "after": " → ".join(f"{k}:{v}" if v else k for k, v in after if k != "heartbeat")[:200],
        })
        print(json.dumps(rows[-1]), flush=True)
    (out / "summary.json").write_text(json.dumps(rows, indent=1))
    passed = sum(r["verdict"] == "PASS" for r in rows)
    print(f"pass: {passed} / {len(rows)}")


if __name__ == "__main__":
    main()
