#!/usr/bin/env python3
"""Read one serial port raw into a file, changing nothing about the line state.

The bridge's whole claim is that a host reading its port sees the *other*
board's bytes and nothing else, so the reader has to be as inert as the bridge
is. `stty` and most terminal programs assert DTR/RTS on open, which on an
Espressif native-USB port is the reset sequence espflash uses — the reader
would reboot the board it is trying to observe, and the capture would be of its
own side effect.

So: `os.open` plus `termios`, raw, `HUPCL` cleared so closing does not drop
DTR either. The same open `scripts/spike/esp-emu/usb-tcp-bridge.py` makes, and
the same one Studio and lp-cli make.

    scripts/emu/tty-capture.py --dev /dev/cu.usbmodem1433201 \
        --out capture.txt --seconds 20

Prints a byte count to stderr and exits 0 when the time is up, or on SIGINT.
`--baud` is accepted and applied; a native-USB port ignores it, a real UART
does not.
"""

from __future__ import annotations

import argparse
import os
import select
import sys
import termios
import time
import tty

BAUD_CONSTANTS = {
    115200: termios.B115200,
    230400: termios.B230400,
    921600: 921600,  # macOS accepts arbitrary rates in the speed fields
}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dev", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--seconds", type=float, default=20.0)
    ap.add_argument("--baud", type=int, default=115200)
    ap.add_argument(
        "--quiet-exit",
        type=float,
        default=0.0,
        help="stop early after this many seconds with no bytes (0 = never)",
    )
    args = ap.parse_args()

    fd = os.open(args.dev, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    try:
        tty.setraw(fd)
        attrs = termios.tcgetattr(fd)
        attrs[2] &= ~termios.HUPCL  # do not drop DTR on close
        attrs[2] |= termios.CLOCAL | termios.CREAD
        speed = BAUD_CONSTANTS.get(args.baud, args.baud)
        attrs[4] = speed
        attrs[5] = speed
        termios.tcsetattr(fd, termios.TCSANOW, attrs)
        print(f"OPENED {args.dev} at {args.baud}", file=sys.stderr, flush=True)

        deadline = time.monotonic() + args.seconds
        last_byte = time.monotonic()
        total = 0
        with open(args.out, "wb") as out:
            while time.monotonic() < deadline:
                r, _, _ = select.select([fd], [], [], 0.25)
                if fd in r:
                    try:
                        data = os.read(fd, 65536)
                    except BlockingIOError:
                        data = b""
                    except OSError as e:
                        print(f"DEVICE ERROR {e}", file=sys.stderr)
                        break
                    if data:
                        out.write(data)
                        out.flush()
                        total += len(data)
                        last_byte = time.monotonic()
                if args.quiet_exit and time.monotonic() - last_byte > args.quiet_exit:
                    print("QUIET", file=sys.stderr, flush=True)
                    break
        print(f"CAPTURED {total} bytes -> {args.out}", file=sys.stderr, flush=True)
    except KeyboardInterrupt:
        pass
    finally:
        os.close(fd)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
