#!/usr/bin/env python3
"""BLE M4's desk check, scripted: everything after a human's one Join.

Preconditions (see ../README.md, "Wire mode"):
  - the board runs the product image (optionally `desk_espnow_meter`), was
    provisioned with `provision-access.py --password <pw>`, and has rebooted;
  - `server.py` is running with the board's console attached (`POST /serial`);
  - a page (phone over Tailscale, or a desktop browser) has joined `LP-…`.

    BLE_LAB_PORT=<port> python3 spikes/ble-lab/scripts/m4-desk-check.py \
        --password desk-lab [--idle-min 10] [--skip-drop]

    # just the knob-jump checks (docs/defects/2026-09-25-a-knob-jump-over-
    # bluetooth-kills-the-c6-ble-host.md); `--force-restart` needs an image
    # built with `desk_ble_fault`:
    BLE_LAB_PORT=<port> python3 spikes/ble-lab/scripts/m4-desk-check.py \
        --password desk-lab --only-knob-burst [--knob-jumps 10] [--force-restart]

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


def evaluate(js: str, timeout_ms: int) -> tuple[dict, list[str]]:
    """Run `js` in the lab page (the body of an async function given `lab`).
    Returns its value and the board's console lines while it ran."""
    d = cmd({"op": "eval", "js": js, "timeoutMs": timeout_ms}, timeout_ms / 1000)
    value = d.get("value") if d.get("ok") else {"error": d.get("error")}
    return value, [str(line) for line in d.get("serial", [])]


# The knob write Studio's Play mode sends, and its preview poll, shaped as it
# sends them: a 10-digit request id, the whole line written as ONE ATT write
# with response when it fits in 244 bytes. `size` pads the line to an exact
# length with JSON whitespace. See
# docs/defects/2026-09-25-a-knob-jump-over-bluetooth-kills-the-c6-ble-host.md:
# the C6 controller hands the host an ACL packet of 193–198 bytes (a write of
# 182–187 bytes) as two mbufs, and esp-radio 0.18 passed on only the first.
KNOB_JS = r"""
const W = lab.S.wire;
let nextId = 4294967296 + Math.floor(Math.random() * 1e6);
function line(msg, size) {
  let text = 'M!' + JSON.stringify({ id: nextId, msg });
  if (size && text.length + 1 < size) text = text.slice(0, -1) + ' '.repeat(size - 1 - text.length) + '}';
  return { id: nextId++, bytes: lab.enc.encode(text + '\n') };
}
function pending(id, ms) {
  const e = { frames: [] };
  const p = new Promise((res, rej) => { e.resolve = () => res(e); setTimeout(() => rej(new Error('no reply to ' + id)), ms); });
  W.pending.set(id, e);
  return p;
}
let q = Promise.resolve();
function send(l) { q = q.then(async () => { for (let i = 0; i < l.bytes.length; i += 244) await lab.S.rx.writeValueWithResponse(l.bytes.slice(i, i + 244)); }); return q; }
function knob(v) { return { projectCommand: { handle: 1, command: { panel_write: { request: { scope: { kind: 'module', owner: 0 }, channel: 'scale', value: { f32: v }, ttl_ms: null } } } } }; }
const poll = { projectRead: { handle: 1, request: { since: null, queries: [{ shapes: { level: 'detail' } }, { nodes: { level: 'detail', nodes: 'all', include_slots: true } }], probes: [{ output_frame: { geometry: 'always', samples: 'srgb8' } }] } } };
"""


def knob_jump_burst(n: int) -> tuple[dict, list[str]]:
    """`n` times: a preview poll in flight, the knob to 1, then straight to its
    end (4) — the Run M jump — with the jump's line padded through every
    length of the chained window in turn. Every reply must arrive on the same
    link."""
    js = KNOB_JS + r"""
const n = %d, out = []; let ok = 0;
const drops0 = lab.S.drops;
for (let i = 0; i < n; i++) {
  const size = 182 + (i %% 6);
  const polled = line(poll), low = line(knob(1)), jump = line(knob(4), size);
  const waits = [pending(polled.id, 20000), pending(low.id, 10000), pending(jump.id, 10000)];
  send(polled); send(low); send(jump);
  try { await Promise.all(waits); ok++; out.push([i, size, 'ok']); }
  catch (e) { out.push([i, size, String(e.message || e)]); break; }
  await lab.sleep(500);
}
return { n, ok, out, drops: lab.S.drops - drops0, connected: lab.S.connected };
""" % n
    return evaluate(js, n * 25_000 + 10_000)


def write_length_sweep(lo: int, hi: int) -> tuple[dict, list[str]]:
    """One knob write of every line length lo..hi (bytes, one ATT write each)."""
    js = KNOB_JS + r"""
const lo = %d, hi = %d, bad = []; let ok = 0;
// Short ids, so the shortest line reaches below the chained window.
nextId = 1000 + Math.floor(Math.random() * 8000);
const drops0 = lab.S.drops;
for (let size = lo; size <= hi; size++) {
  const l = line(knob(1 + (size %% 3)), size);
  if (l.bytes.length !== size) { bad.push([size, 'cannot pad to ' + size]); continue; }
  const wait = pending(l.id, 5000);
  send(l);
  try { await wait; ok++; } catch (e) { bad.push([size, String(e.message || e)]); break; }
}
return { lo, hi, ok, bad, drops: lab.S.drops - drops0, connected: lab.S.connected };
""" % (lo, hi)
    return evaluate(js, (hi - lo + 1) * 6_000 + 10_000)


def console_faults(lines: list[str]) -> list[str]:
    return [l for l in lines if "error parsing packet" in l or "host runner error" in l]


def knob_checks(args) -> int:
    """Knob jump + burst survives; every write length survives; and, on a
    `desk_ble_fault` image, a forced host-runner restart comes back. Returns 0
    when each check that ran passed."""
    failed = False
    serial: list[str] = []
    if args.knob_jumps:
        r, lines = knob_jump_burst(args.knob_jumps)
        serial += lines
        passed = r.get("ok") == args.knob_jumps and r.get("drops") == 0 and r.get("connected") is True
        failed |= not passed
        record("knob-jump-burst", passed=passed, **r)
    lo, hi = args.sweep
    if lo <= hi:
        r, lines = write_length_sweep(lo, hi)
        serial += lines
        passed = not r.get("bad") and r.get("drops") == 0 and r.get("connected") is True
        failed |= not passed
        record("write-length-sweep", passed=passed, **r)
    faults = console_faults(serial)
    failed |= bool(faults)
    record("knob-console", passed=not faults, faults=faults[:6])

    if args.force_restart:
        t0 = time.time()
        _, lines = evaluate(
            'lab.S.rx.writeValueWithResponse(lab.enc.encode("LP-DESK-FORCE-BLE-HOST-RESTART\\n"))'
            ".catch(() => 0); await lab.sleep(300); return 1;",
            10_000,
        )
        back_after = None
        for _ in range(60):
            st = cmd({"op": "status"})
            if st.get("connected") and any(
                e.get("kind") == "reconnected" and e["t"] / 1000 >= t0 for e in st.get("events", [])
            ):
                back_after = round(time.time() - t0, 1)
                break
            time.sleep(1)
        answered = None
        if back_after is not None:
            cmd({"op": "wire", "on": True})
            for _ in range(5):
                if cmd({"op": "login", "password": args.password}).get("ok"):
                    answered = cmd({"op": "req", "msg": "listLoadedProjects"}).get("reply")
                    break
                time.sleep(1)
        lines += console(400)
        closed = [l for l in lines if "controller reset: telling the host" in l][-2:]
        passed = back_after is not None and answered is not None and bool(closed)
        failed |= not passed
        record("forced-host-restart", passed=passed, backAfterS=back_after, answered=answered, console=closed)
    return 1 if failed else 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--password", required=True)
    ap.add_argument("--idle-min", type=float, default=10.0)
    ap.add_argument("--skip-drop", action="store_true", help="skip the 10 s unauthenticated drop")
    ap.add_argument("--rtt-n", type=int, default=20)
    ap.add_argument("--knob-jumps", type=int, default=10, help="knob-jump + burst rounds (0 skips)")
    ap.add_argument("--sweep", type=int, nargs=2, default=[178, 244], metavar=("LO", "HI"),
                    help="write-length sweep bounds in bytes")
    ap.add_argument("--force-restart", action="store_true",
                    help="desk_ble_fault images only: force a host-runner restart and prove it comes back")
    ap.add_argument("--only-knob-burst", action="store_true",
                    help="log in, run the knob-jump checks (and --force-restart), stop")
    args = ap.parse_args()

    status = cmd({"op": "status"})
    record("status", device=status.get("device"), connected=status.get("connected"))
    if not status.get("connected"):
        record("abort", reason="no BLE link: a human must Join first")
        return 1
    cmd({"op": "wire", "on": True})

    if args.only_knob_burst:
        r = cmd({"op": "login", "password": args.password})
        record("login-right", ok=r.get("ok"), result=r.get("result"))
        if not r.get("ok"):
            return 1
        return knob_checks(args)

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

    # 7. A knob jump with the preview poll in flight, and every write length
    #    (the 2026-09-25 defect).
    if knob_checks(args) != 0:
        return 1

    # 8. The idle window: the link held, and the [COEX] lines to read loss off.
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
