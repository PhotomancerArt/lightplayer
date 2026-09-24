#!/usr/bin/env python3
"""Write a board's device store (`/.lp/access.json`) over USB, then reboot it.

BLE M4's desk check needs a product board with BLE enabled and a password
installed, and M6 (the Studio UI for it) does not exist yet. This is the lab's
stand-in: it builds the store the way a client does (PBKDF2-HMAC-SHA256 of the
password over a random 16-byte salt → the 32-byte login key K; the board never
derives), writes it with the wire's own `filesystem.write` over the USB link —
the trusted link, where an access-file write is allowed at edit tier — and
sends `reboot`, because the board reads `bleEnabled` once at boot.

The port is opened the way `scripts/emu/tty-capture.py` opens one: raw
`termios`, `HUPCL` cleared, DTR/RTS never asserted, so opening it does not
reset the board. Release the lab server's console first
(`curl -X DELETE localhost:$P/serial`) — one opener at a time.

    python3 spikes/ble-lab/scripts/provision-access.py --dev <port by MAC> \
        --password 'desk-lab' --label desk --tier edit
    python3 spikes/ble-lab/scripts/provision-access.py --dev <port> --disable

`--disable` writes a store with BLE off and no secrets (a board's default).
The password is for the lab only; nothing here is how Studio will do it.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import secrets
import select
import sys
import termios
import time
import tty

PATH = "/.lp/access.json"


def device_store(password: str | None, label: str, tier: str, iterations: int, open_: bool, ble: bool) -> dict:
    entries = []
    if password is not None:
        salt = secrets.token_bytes(16)
        k = hashlib.pbkdf2_hmac("sha256", password.encode(), salt, iterations, 32)
        entries.append(
            {
                "label": label,
                "tier": tier,
                "salt": base64.b64encode(salt).decode(),
                "iterations": iterations,
                "k": base64.b64encode(k).decode(),
            }
        )
    return {"version": 1, "secrets": entries, "bleEnabled": ble, "open": open_}


def open_port(dev: str) -> int:
    fd = os.open(dev, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    tty.setraw(fd)
    attrs = termios.tcgetattr(fd)
    attrs[2] &= ~termios.HUPCL
    attrs[2] |= termios.CLOCAL | termios.CREAD
    termios.tcsetattr(fd, termios.TCSANOW, attrs)
    return fd


def request(fd: int, msg_id: int, msg, timeout: float = 5.0) -> dict | None:
    line = "M!" + json.dumps({"id": msg_id, "msg": msg}, separators=(",", ":")) + "\n"
    data = line.encode()
    # 64 bytes at a time, 2 ms apart: the USB-Serial-JTAG RX FIFO is small.
    for i in range(0, len(data), 64):
        os.write(fd, data[i : i + 64])
        time.sleep(0.002)
    deadline = time.monotonic() + timeout
    buf = b""
    needle = f'"id":{msg_id},'.encode()
    while time.monotonic() < deadline:
        r, _, _ = select.select([fd], [], [], 0.2)
        if fd not in r:
            continue
        try:
            buf += os.read(fd, 65536)
        except BlockingIOError:
            continue
        *lines, buf = buf.split(b"\n")
        for raw in lines:
            if raw.startswith(b"M!") and needle in raw:
                return json.loads(raw[2:])
    return None


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dev", required=True)
    ap.add_argument("--password")
    ap.add_argument("--label", default="desk")
    ap.add_argument("--tier", default="edit", choices=["play", "edit"])
    ap.add_argument("--iterations", type=int, default=10_000)
    ap.add_argument("--open", action="store_true", help="untrusted links hold play without login")
    ap.add_argument("--disable", action="store_true", help="BLE off, no secrets")
    ap.add_argument("--no-reboot", action="store_true")
    args = ap.parse_args()

    if args.disable:
        store = device_store(None, args.label, args.tier, args.iterations, False, False)
    else:
        if args.password is None:
            ap.error("--password is required unless --disable")
        store = device_store(args.password, args.label, args.tier, args.iterations, args.open, True)
    text = json.dumps(store, indent=2) + "\n"

    fd = open_port(args.dev)
    try:
        reply = request(fd, 9001, {"filesystem": {"write": {"path": PATH, "data": text}}})
        print(f"write {PATH}: {json.dumps(reply)[:200] if reply else 'NO REPLY'}")
        if reply is None:
            return 1
        if not args.no_reboot:
            reply = request(fd, 9002, "reboot")
            print(f"reboot: {json.dumps(reply)[:200] if reply else 'NO REPLY'}")
    finally:
        os.close(fd)
    safe = dict(store)
    safe["secrets"] = [{**s, "k": "<redacted>"} for s in store["secrets"]]
    print(json.dumps(safe))
    return 0


if __name__ == "__main__":
    sys.exit(main())
