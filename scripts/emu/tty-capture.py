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

`--until <text>` stops at the END of the first line containing <text>, not at
the match: the validation runner's sentinels are line prefixes
(`[stack] heartbeat: high-water`) and the figures that make the line worth
capturing come after them, so stopping at the match would cut the transcript
mid-number. Nothing is dropped — bytes already read past the sentinel are
written out too; the reader simply stops asking for more.
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
    ap.add_argument(
        "--until",
        default=None,
        help="stop at the end of the first line containing this text",
    )
    args = ap.parse_args()
    until = args.until.encode() if args.until else None

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
        # Only the tail since the last newline needs re-scanning for the
        # sentinel; a match is not acted on until its line is terminated.
        pending = b""
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
                        if until is not None:
                            pending += data
                            done = False
                            for line in pending.split(b"\n")[:-1]:
                                if until in line:
                                    done = True
                                    break
                            pending = pending.rsplit(b"\n", 1)[-1]
                            if done:
                                print("UNTIL", file=sys.stderr, flush=True)
                                break
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
