#!/usr/bin/env python3
"""Cut a sacrificial board's power N times and capture every boot's scan.

The silicon half of the `flash-tears` payload's capture
(`Capture::PowerCuts` in `lp-emu/lp-emu-validate/src/payload.rs`). The
validation runner flashes the image and then runs this as its own plan step:

    scripts/emu/flash-tears-cuts.py --port /dev/cu.usbmodem112101 \
        --capture target/validate/flash-tears.cap --cuts 50 \
        --min-ms 50 --max-ms 2000 --until '[flash-tears] === SCAN DONE ==='

For the boot after the flash and then after each of `--cuts` power cuts:

1. find the board's port again (a power cycle re-enumerates it; the port is
   resolved by MAC through `board show`, never by a port-name pattern);
2. open it the way `tty-capture.py` does — `os.open` and raw termios, HUPCL
   cleared, DTR/RTS untouched, so opening is not the reset dance;
3. send one byte (`g`) every 300 ms until the header arrives — the payload
   writes nothing until it has heard from the host — and read to the end of
   the first line holding `--until`;
4. close the port, wait a random `--min-ms..=--max-ms`, and cut the power with
   `board power-cycle <MAC> --as $BOARD_HOLDER` — never uhubctl.

Every byte read goes into `--capture`, in order, untouched: the transcript is
the port's bytes. What this script decided (the seed, each wait, each power
cycle's output) goes to stderr, which the sitting log keeps.

**It refuses any board but the allocated sacrificial one** (M4 director ruling
DD6): `board show` must name the port's board with tag `sacrificial`, MAC
`--mac` (CX1's by default), and a lease held by `$BOARD_HOLDER`. Without
`board` installed there is no registry to check, and it refuses.
"""

from __future__ import annotations

import argparse
import json
import os
import random
import select
import subprocess
import sys
import termios
import time
import tty

CX1_MAC = "14:C1:9F:E6:54:90"
HEADER = b"[fw-checks-header]"


def log(msg: str) -> None:
    print(f"[flash-tears-cuts] {msg}", file=sys.stderr, flush=True)


def board_show(board: str) -> dict:
    out = subprocess.run(
        ["board", "show", board, "--json"], capture_output=True, text=True, check=False
    )
    if out.returncode != 0:
        raise SystemExit(f"REFUSED: `board show {board}` failed: {out.stderr.strip()}")
    return json.loads(out.stdout)


def check_board(port: str, mac: str, holder: str | None) -> dict:
    """Refuse unless the registry names a sacrificial board with this MAC,
    leased by `holder` (no lease check when `holder` is None)."""
    info = board_show(port)
    tags = info.get("tags") or []
    if "sacrificial" not in tags:
        raise SystemExit(
            f"REFUSED: {info.get('mark')} {info.get('slug')} on {port} is not tagged "
            f"`sacrificial` (tags: {tags}); this payload destroys a board's filesystem"
        )
    if (info.get("mac") or "").upper() != mac.upper():
        raise SystemExit(
            f"REFUSED: the board on {port} is {info.get('mac')}, not the allocated {mac}"
        )
    if not info.get("present"):
        raise SystemExit(f"REFUSED: {info.get('slug')} is not plugged in")
    lease = info.get("lease") or {}
    if holder is not None and lease.get("holder") != holder:
        raise SystemExit(
            f"REFUSED: {info.get('slug')} is not leased by `{holder}` (lease: {lease}); "
            f"take it first: board take {info.get('slug')} --as \"{holder}\" --for ..."
        )
    return info


def find_port(mac: str, deadline: float) -> str:
    """The board's port once it has enumerated again."""
    while time.monotonic() < deadline:
        info = board_show(mac)
        port = info.get("port")
        if info.get("present") and port and os.path.exists(port):
            return port
        time.sleep(0.2)
    raise SystemExit(f"FAILED: {mac} did not come back on USB")


def open_port(dev: str) -> int:
    fd = os.open(dev, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    tty.setraw(fd)
    attrs = termios.tcgetattr(fd)
    attrs[2] &= ~termios.HUPCL  # do not drop DTR on close
    attrs[2] |= termios.CLOCAL | termios.CREAD
    termios.tcsetattr(fd, termios.TCSANOW, attrs)
    return fd


def capture_boot(dev: str, out, until: bytes, seconds: float) -> bool:
    """Read one boot to the end of the line holding `until`."""
    fd = open_port(dev)
    try:
        seen = b""
        last_poke = 0.0
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            now = time.monotonic()
            if HEADER not in seen and now - last_poke > 0.3:
                try:
                    os.write(fd, b"g")
                except BlockingIOError:
                    pass
                last_poke = now
            r, _, _ = select.select([fd], [], [], 0.05)
            if not r:
                continue
            try:
                chunk = os.read(fd, 4096)
            except BlockingIOError:
                continue
            if not chunk:
                continue
            out.write(chunk)
            out.flush()
            seen += chunk
            at = seen.find(until)
            if at >= 0 and b"\n" in seen[at:]:
                return True
        return False
    finally:
        os.close(fd)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "--check-board",
        metavar="BOARD",
        help="only check that BOARD is the allocated sacrificial board, then exit",
    )
    ap.add_argument("--port")
    ap.add_argument("--capture")
    ap.add_argument("--cuts", type=int)
    ap.add_argument("--min-ms", type=int, default=50)
    ap.add_argument("--max-ms", type=int, default=2000)
    ap.add_argument("--until")
    ap.add_argument("--mac", default=CX1_MAC)
    ap.add_argument("--seed", type=int, default=None)
    ap.add_argument("--boot-seconds", type=float, default=60.0)
    args = ap.parse_args()
    if args.check_board:
        info = check_board(args.check_board, args.mac, None)
        print(f"board {info.get('mark')} {info.get('slug')} {info.get('mac')} tags={info.get('tags')}")
        return 0
    if not (args.port and args.capture and args.cuts is not None and args.until):
        ap.error("--port, --capture, --cuts and --until are required")

    holder = os.environ.get("BOARD_HOLDER")
    if not holder:
        raise SystemExit("REFUSED: set BOARD_HOLDER to the session holding the board's lease")
    info = check_board(args.port, args.mac, holder)
    seed = args.seed if args.seed is not None else int(time.time())
    rng = random.Random(seed)
    log(f"board {info.get('mark')} {info.get('slug')} {info.get('mac')} on {args.port}")
    log(f"seed {seed}; {args.cuts} cuts, waits {args.min_ms}..{args.max_ms} ms")

    until = args.until.encode()
    port = args.port
    with open(args.capture, "wb") as out:
        for boot in range(args.cuts + 1):
            what = "the boot after the flash" if boot == 0 else f"the boot after cut {boot}"
            port = find_port(args.mac, time.monotonic() + args.boot_seconds)
            if not capture_boot(port, out, until, args.boot_seconds):
                log(f"FAILED: {what} never reached the sentinel")
                return 1
            log(f"captured {what}")
            if boot == args.cuts:
                break
            wait_ms = rng.randint(args.min_ms, args.max_ms)
            time.sleep(wait_ms / 1000.0)
            cycle = subprocess.run(
                ["board", "power-cycle", args.mac, "--as", holder],
                capture_output=True,
                text=True,
                check=False,
            )
            log(f"cut {boot + 1}: waited {wait_ms} ms; {cycle.stdout.strip()} {cycle.stderr.strip()}")
            if cycle.returncode != 0:
                log(f"FAILED: board power-cycle exited {cycle.returncode}")
                return 1
    log(f"done: {args.cuts} cuts, {args.cuts + 1} boots captured into {args.capture}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
