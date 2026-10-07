#!/usr/bin/env python3
"""Real power cuts during a real X -> Y over-the-air update, on a C6 behind a
switchable USB hub.

    scripts/ota/hw-power-cut.py --mac A0:F2:62:87:B4:8C \\
        --hub 1-1.2,1-2.2 --hub-port 4 \\
        --x <x-image> --y <y-image> --out <dir> --cuts 0.5,3,7,11,...

<x-image>/<y-image> are `scripts/ota/build-image.sh` outputs. Per cut T
(seconds after the update starts):

  1. write X's packaged image at 0x0 (`espflash write-bin`; the package
     ends on a sector) and wait for X's heartbeat;
  2. start `lp-cli link capture --ota-offer <Y>/ota`; after T seconds switch
     the hub port's power off for --off-secs, then on;
  3. capture again, offering Y, until the host says `done`.

PASS = the recovery ends `UpToDate` and its last core line is Y's build with
an engine. Also reported: the first boot after the cut, and what the
recovery served (the resume evidence).

The board is resolved by MAC (scripts/emu/board-port.py), never by guessing a
port. A VIA-style hub is a USB 2 and a USB 3 hub on one chip: VBUS drops only
when BOTH twins' ports are off, so --hub takes both. Needs uhubctl.

On the desk, cut through the lease tool instead of uhubctl:
`--power-cycle-cmd 'board power-cycle fixture-c6 --as "<who>" --off-secs 2'`
replaces --hub/--hub-port (the command is split shell-style and run as is).
"""

import argparse
import json
import re
import shlex
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


def power(hubs, port, on):
    for hub in hubs.split(","):
        sh(["uhubctl", "-l", hub, "-p", str(port), "-a", "on" if on else "off", "-e"])


def capture(lp_cli, port, console, seconds, offer=None, exit_on=None):
    cmd = [lp_cli, "link", "capture", port, "--console", str(console), "--seconds", str(seconds)]
    if offer:
        cmd += ["--ota-offer", str(offer)]
    if exit_on:
        cmd += ["--exit-on", exit_on]
    return cmd


def last_core(text):
    lines = [l for l in text.splitlines() if "[CORE] core @" in l]
    return lines[-1] if lines else ""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mac", required=True)
    ap.add_argument("--hub", help="the hub and its USB 3 twin, e.g. 1-1.2,1-2.2")
    ap.add_argument("--hub-port", type=int)
    ap.add_argument("--power-cycle-cmd",
                    help="a command that cuts the board's power and restores it (instead of --hub)")
    ap.add_argument("--x", required=True)
    ap.add_argument("--y", required=True)
    ap.add_argument("--lp-cli", default=str(REPO / "target/release/lp-cli"))
    ap.add_argument("--out", required=True)
    ap.add_argument("--cuts", required=True, help="seconds after the update starts, comma-separated")
    ap.add_argument("--off-secs", type=float, default=2.0)
    ap.add_argument("--recover-secs", type=int, default=180)
    ap.add_argument("--no-z", action="store_true", help="offer raw chunks only")
    a = ap.parse_args()
    if not a.power_cycle_cmd and (not a.hub or a.hub_port is None):
        ap.error("give --hub and --hub-port, or --power-cycle-cmd")
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    x, y = Path(a.x), Path(a.y)
    image = next((x / "package").glob("*-merged.bin"))
    y_build = json.loads((y / "split.json").read_text())["buildId"]
    rows = []
    for t in [float(c) for c in a.cuts.split(",")]:
        tag = f"cut-{t:05.1f}"
        port = port_of(a.mac)
        if not port:
            print(f"{tag}: board {a.mac} not on the bus — stopping", flush=True)
            break
        r = sh(["espflash", "write-bin", "--chip", "esp32c6", "--port", port, "0x0", str(image)])
        if r.returncode != 0:
            print(f"{tag}: flashing X failed: {r.stderr[-300:]}", flush=True)
            break
        port = port_of(a.mac)
        pre = out / f"{tag}.pre.txt"
        sh(capture(a.lp_cli, port, pre, 30, exit_on='"heartbeat":{'))
        if '"heartbeat":{' not in pre.read_text(errors="replace"):
            print(f"{tag}: X never heartbeat after flashing — skipping", flush=True)
            continue
        offer = y / "ota"
        cut_console = out / f"{tag}.cut.txt"
        cmd = capture(a.lp_cli, port, cut_console, int(t) + 60, offer=offer)
        if a.no_z:
            cmd.append("--ota-no-z")
        host = subprocess.Popen(cmd, stdout=subprocess.DEVNULL,
                                stderr=open(out / f"{tag}.cut.err", "w"))
        time.sleep(t)
        if a.power_cycle_cmd:
            r = sh(shlex.split(a.power_cycle_cmd))
            if r.returncode != 0:
                print(f"{tag}: power cycle failed: {(r.stdout + r.stderr)[-300:]}", flush=True)
            host.kill()
            host.wait()
        else:
            power(a.hub, a.hub_port, False)
            host.kill()
            host.wait()
            time.sleep(a.off_secs)
            power(a.hub, a.hub_port, True)
        port = port_of(a.mac, wait=20)
        if not port:
            rows.append({"cut": t, "verdict": "FAIL", "why": "the board did not come back on USB"})
            print(json.dumps(rows[-1]), flush=True)
            continue
        rec = out / f"{tag}.rec.txt"
        cmd = capture(a.lp_cli, port, rec, a.recover_secs, offer=offer, exit_on="[host-ota] done")
        if a.no_z:
            cmd.append("--ota-no-z")
        rec_err = out / f"{tag}.rec.err"
        subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=open(rec_err, "w"))
        text = rec.read_text(errors="replace")
        final = last_core(text)
        cores = re.findall(r"\[CORE\] core @\S+ \+\d+ \(([a-z ]+)\) build (\S+)", text)
        served = re.findall(r"D \d+ chunk\(s\) / \d+ B; Z \d+ chunk\(s\) / \d+ B",
                            rec_err.read_text(errors="replace"))
        ok = "done: UpToDate" in text and f"build {y_build}" in final and " engine " in final
        rows.append({
            "cut": t,
            "verdict": "PASS" if ok else "FAIL",
            "first_boot_after": " ".join(cores[0]) if cores else "?",
            "recovery_served": served[-1] if served else "?",
            "final": final[final.find("[CORE]"):][:160],
        })
        print(json.dumps(rows[-1]), flush=True)
    (out / "summary.json").write_text(json.dumps(rows, indent=1))
    passed = sum(r["verdict"] == "PASS" for r in rows)
    print(f"pass: {passed} / {len(rows)}")


if __name__ == "__main__":
    main()
