#!/usr/bin/env python3
"""BLE M4's desk check, scripted: everything after a human's one Join.

Preconditions (see ../README.md, "Wire mode"):
  - the board runs the product image (optionally `desk_espnow_meter`), was
    provisioned with `provision-access.py --password <pw>`, and has rebooted;
  - `server.py` is running with the board's console attached (`POST /serial`);
  - a page (phone over Tailscale, or a desktop browser) has joined `LP-…`.

    BLE_LAB_PORT=<port> python3 spikes/ble-lab/scripts/m4-desk-check.py \
        --password desk-lab [--idle-min 10] [--skip-drop]

Prints one JSON record per check and a summary. It does not decide pass or
fail for the ESP-NOW figures — those come from the `[COEX]` console lines,
read by M2's `coex.py`/`series.py` over the window this script prints.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import statistics
import time
import urllib.request

P = int(os.environ.get("BLE_LAB_PORT", "36666"))


def cmd(obj: dict, timeout_s: float = 60) -> dict:
    req = urllib.request.Request(
        f"http://localhost:{P}/cmd", data=json.dumps(obj).encode(), method="POST"
    )
    with urllib.request.urlopen(req, timeout=timeout_s + 10) as r:
        return json.load(r)


def console(n: int = 400) -> list[str]:
    with urllib.request.urlopen(f"http://localhost:{P}/serial?n={n}", timeout=10) as r:
        body = r.read().decode(errors="replace")
    return [str(line) for line in json.loads(body).get("serial", [])]


def record(name: str, **fields) -> None:
    print(json.dumps({"check": name, "t": round(time.time(), 1), **fields}), flush=True)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--password", required=True)
    ap.add_argument("--idle-min", type=float, default=10.0)
    ap.add_argument("--skip-drop", action="store_true", help="skip the 10 s unauthenticated drop")
    ap.add_argument("--rtt-n", type=int, default=20)
    args = ap.parse_args()

    status = cmd({"op": "status"})
    record("status", device=status.get("device"), connected=status.get("connected"))
    if not status.get("connected"):
        record("abort", reason="no BLE link: a human must Join first")
        return 1
    cmd({"op": "wire", "on": True})

    # 1. Refusal before login: a Play-tier request on an untrusted link.
    r = cmd({"op": "req", "msg": "listLoadedProjects"})
    record("refused-before-login", reply=r.get("reply"), rttMs=r.get("rttMs"))

    # 2. The 10 s unauthenticated drop (the link opened when the page
    #    subscribed; wait past the deadline without logging in).
    if not args.skip_drop:
        before = cmd({"op": "status", "n": 50})
        drops_before = before.get("drops", 0)
        time.sleep(13)
        after = cmd({"op": "status", "n": 50})
        record(
            "unauthenticated-drop",
            dropsBefore=drops_before,
            dropsAfter=after.get("drops"),
            events=[e for e in after.get("events", []) if e.get("kind") in ("disconnected", "reconnected")],
            console=[l for l in console(200) if "no login" in l or "closing at the server" in l][-4:],
        )
        # The page reconnects by itself; wait for it.
        for _ in range(30):
            if cmd({"op": "status"}).get("connected"):
                break
            time.sleep(1)
        cmd({"op": "wire", "on": True})

    # 3. Wrong password → refused (with a backoff); then the right one.
    wrong = cmd({"op": "login", "password": args.password + "-wrong"})
    record("login-wrong", ok=wrong.get("ok"), result=wrong.get("result"))
    retry_ms = (((wrong.get("result") or {}).get("refused") or {}).get("retryAfterMs")) or 0
    early = cmd({"op": "login", "password": args.password})
    record("login-right-during-backoff", ok=early.get("ok"), result=early.get("result"), retryAfterMs=retry_ms)
    if not early.get("ok"):
        time.sleep(retry_ms / 1000 + 0.5)
        right = cmd({"op": "login", "password": args.password})
        record(
            "login-right",
            ok=right.get("ok"),
            result=right.get("result"),
            deriveMs=right.get("deriveMs"),
            beginRttMs=right.get("beginRttMs"),
            answerRttMs=right.get("answerRttMs"),
        )

    # 4. After login the same request is answered.
    r = cmd({"op": "req", "msg": "listLoadedProjects"})
    record("granted-request", reply=r.get("reply"), rttMs=r.get("rttMs"))

    # 5. Round-trip time of a small Play-tier request (the knob-turn proxy;
    #    `lab.knob_rtt` does a real PanelWrite given the project's request).
    rtts = []
    for _ in range(args.rtt_n):
        d = cmd({"op": "req", "msg": "listLoadedProjects"})
        if d.get("rttMs") is not None:
            rtts.append(d["rttMs"])
    rtts.sort()
    record(
        "request-rtt",
        n=len(rtts),
        minMs=rtts[0] if rtts else None,
        medianMs=statistics.median(rtts) if rtts else None,
        maxMs=rtts[-1] if rtts else None,
    )

    # 6. The granted parameters, from the board's own read-back.
    record(
        "conn-params",
        console=[l for l in console(400) if "granted" in l or "asked for interval" in l or "at connect" in l][-6:],
    )

    # 7. The idle window: the link held, and the [COEX] lines to read loss off.
    start = time.time()
    idle_ms = int(args.idle_min * 60_000)
    # `timeoutMs` is the server's wait for the page; without it the server
    # gives up at its 30 s default and answers 504 mid-window.
    idle = cmd({"op": "idle", "ms": idle_ms, "timeoutMs": idle_ms + 30_000}, args.idle_min * 60 + 30)
    record(
        "idle",
        minutes=args.idle_min,
        windowStart=round(start, 1),
        windowEnd=round(time.time(), 1),
        connected=idle.get("connected"),
        events=idle.get("events"),
        heartbeats=[l for l in console(400) if '"heartbeat"' in l][-1:],
        coex=[l for l in console(400) if "[COEX]" in l][-2:],
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
