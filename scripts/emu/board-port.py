#!/usr/bin/env python3
"""Resolve an Espressif native-USB board's serial port from its MAC. Passively.

Every ESP32-C6 and -S3 enumerates as `303a:1001`, so a port list cannot tell
two of them apart — but the USB *serial number* is the board's MAC, and macOS
builds the device node's name from the USB locationID. This walks IOKit's own
registry for the match and prints the port. It opens nothing, resets nothing,
and never probes; `hardware list --probe` is what resets idle boards, and this
exists so a bench with two identical boards on it never has to.

    scripts/emu/board-port.py A0:F2:62:86:7E:44
    /dev/cu.usbmodem1433201

    scripts/emu/board-port.py --list
    A0:F2:62:86:7E:44  loc=0x143320  /dev/cu.usbmodem1433201
    A0:F2:62:87:49:A0  loc=0x143330  /dev/cu.usbmodem1433301

Exit 0 with the port on stdout, 1 with a reason on stderr. `--list` prints
every Espressif native-USB board on the bus and exits 0.

The port name is `usbmodem` + the locationID in hex with trailing zeros
stripped + `01` (the interface number). The device node is then confirmed to
exist, so a rename in a future macOS shows up as a failure here rather than as
a flash onto nothing.
"""

from __future__ import annotations

import argparse
import os
import plistlib
import subprocess
import sys

# Espressif's native USB-Serial-JTAG. One PID for every chip that has it.
VID = 0x303A
PID = 0x1001


def boards() -> list[tuple[str, int, str]]:
    """(MAC, locationID, port) for every Espressif native-USB device present."""
    raw = subprocess.run(
        ["ioreg", "-a", "-r", "-c", "IOUSBHostDevice", "-l", "-w0"],
        capture_output=True,
        check=True,
    ).stdout
    if not raw:
        return []
    found: dict[str, tuple[str, int, str]] = {}

    def walk(node: object) -> None:
        if isinstance(node, dict):
            if node.get("idVendor") == VID and node.get("idProduct") == PID:
                mac = node.get("USB Serial Number")
                loc = node.get("locationID")
                if isinstance(mac, str) and isinstance(loc, int):
                    found[mac.upper()] = (mac.upper(), loc, port_for(loc))
            for value in node.values():
                walk(value)
        elif isinstance(node, list):
            for value in node:
                walk(value)

    walk(plistlib.loads(raw))
    return sorted(found.values())


def port_for(location_id: int) -> str:
    return f"/dev/cu.usbmodem{format(location_id, 'x').rstrip('0')}01"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("mac", nargs="?", help="the board's MAC, as its USB serial number")
    ap.add_argument("--list", action="store_true", help="print every board and exit")
    args = ap.parse_args()

    present = boards()
    if args.list:
        for mac, loc, port in present:
            exists = "" if os.path.exists(port) else "   (NO DEVICE NODE)"
            print(f"{mac}  loc=0x{loc:x}  {port}{exists}")
        return 0

    if not args.mac:
        ap.error("give a MAC, or --list")

    want = args.mac.upper()
    for mac, _loc, port in present:
        if mac == want:
            if not os.path.exists(port):
                print(
                    f"{want} is on the bus but {port} does not exist — the device "
                    f"node naming rule has changed; do not flash by guess",
                    file=sys.stderr,
                )
                return 1
            print(port)
            return 0

    print(f"{want} is not on the bus. Present:", file=sys.stderr)
    for mac, loc, port in present:
        print(f"  {mac}  loc=0x{loc:x}  {port}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
