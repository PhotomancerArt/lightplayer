#!/usr/bin/env -S uv run --quiet --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["bleak>=0.22"]
# ///
"""Host side of the `test_ble` spike (vision `ble-remote-control`).

Finds an `LP-BLE-*` board, connects, subscribes to the Nordic-UART TX
characteristic, and runs three checks:

  1. echo:  write a line, expect the same bytes back;
  2. burst: ask the board for N 180-byte notifications, time their arrival;
  3. write: stream N writes-without-response board-ward, which is the
            direction a project upload goes.

    scripts/ble/nus-probe.py                 # first LP-BLE board found
    scripts/ble/nus-probe.py --name LP-BLE-b48c --burst 200

macOS asks once for Bluetooth permission for the app running this (the
terminal, or Claude); say yes, then re-run.
"""

import argparse
import asyncio
import time

from bleak import BleakClient, BleakScanner

NUS_RX = "6e400002-b5a3-f393-e0a9-e50e24dcca9e"  # central -> board
NUS_TX = "6e400003-b5a3-f393-e0a9-e50e24dcca9e"  # board -> central


async def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--name", help="exact advertised name (default: first LP-BLE-*)")
    ap.add_argument("--burst", type=int, default=100)
    ap.add_argument("--writes", type=int, default=100)
    ap.add_argument("--scan-secs", type=float, default=10.0)
    args = ap.parse_args()

    print(f"[probe] scanning {args.scan_secs:.0f}s for LP-BLE boards...")
    t_scan = time.monotonic()
    device = await BleakScanner.find_device_by_filter(
        lambda d, ad: (ad.local_name or d.name or "").startswith(args.name or "LP-BLE-")
        and (args.name is None or (ad.local_name or d.name) == args.name),
        timeout=args.scan_secs,
    )
    if device is None:
        raise SystemExit("[probe] no LP-BLE board found")
    print(f"[probe] found {device.name} ({device.address}) in {time.monotonic() - t_scan:.2f}s")

    inbox: asyncio.Queue[bytes] = asyncio.Queue()

    t_conn = time.monotonic()
    async with BleakClient(device) as client:
        print(f"[probe] connected in {time.monotonic() - t_conn:.2f}s, mtu={client.mtu_size}")
        await client.start_notify(NUS_TX, lambda _c, data: inbox.put_nowait(bytes(data)))

        # 1. echo
        msg = b'M!{"hello":"from the mac"}\n'
        t0 = time.monotonic()
        await client.write_gatt_char(NUS_RX, msg, response=True)
        got = await asyncio.wait_for(inbox.get(), 5)
        rtt = (time.monotonic() - t0) * 1000
        print(f"[probe] echo {'OK' if got == msg else 'MISMATCH'} rtt={rtt:.1f}ms got={got!r}")

        # 2. burst, board -> central
        await client.write_gatt_char(NUS_RX, f"burst {args.burst}".encode(), response=True)
        t0 = time.monotonic()
        total = 0
        count = 0
        try:
            while count < args.burst:
                total += len(await asyncio.wait_for(inbox.get(), 5))
                count += 1
        except asyncio.TimeoutError:
            pass
        secs = max(time.monotonic() - t0, 1e-6)
        print(f"[probe] burst board->mac {count}/{args.burst} pkts {total} B in {secs:.2f}s = {total / secs:.0f} B/s")

        # 3. writes-without-response, central -> board (the upload direction).
        # Each one is echoed, so drain the echoes as we go.
        payload = bytes(range(32, 32 + 180))
        t0 = time.monotonic()
        for _ in range(args.writes):
            await client.write_gatt_char(NUS_RX, payload, response=False)
        echoed = 0
        try:
            while echoed < args.writes:
                await asyncio.wait_for(inbox.get(), 5)
                echoed += 1
        except asyncio.TimeoutError:
            pass
        secs = max(time.monotonic() - t0, 1e-6)
        sent = args.writes * len(payload)
        print(f"[probe] writes mac->board {args.writes} x {len(payload)} B = {sent} B in {secs:.2f}s = {sent / secs:.0f} B/s (echoes back {echoed})")


if __name__ == "__main__":
    asyncio.run(main())
