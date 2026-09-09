#!/usr/bin/env python3
"""Reset a board from its own USB-Serial-JTAG handle and read its boot log on that
same handle.

    scripts/emu/reset-and-capture.py /dev/cu.usbmodem1433201 boot.txt 12

This is the way to see a native-USB board's boot, and the two obvious
alternatives both fail:

* `tty-capture.py` reads without disturbing anything, which is right for
  watching a board that is already talking — but it cannot make one boot, so it
  never sees a banner.
* `espflash monitor --before default-reset` resets the board and then shows
  nothing. Its reset drops the USB device; the port it reopens is a new session
  and the banner has already gone by.

USB-Serial-JTAG keeps its session across a *chip* reset, so if the handle is
already open when the reset lands, the ROM banner, the IDF bootloader table and
the app's first lines all arrive on it. Hence: open, pulse, keep reading.

The pulse is esptool's `hard_reset` dance — **DTR low throughout** (DTR is the
download strap; asserting it would land the chip in the ROM downloader instead
of booting), RTS high for 100 ms, then release. `HUPCL` is cleared so closing
the handle does not drop DTR afterwards.

Written by the director's session, 2026-09-06, while diagnosing the fixture;
kept here because a procedure that lives in a scratchpad is a procedure that
has to be reinvented.
"""

import fcntl
import os
import select
import struct
import sys
import termios
import time

# <sys/ioccom.h> encodings for the modem-line ioctls on macOS.
TIOCMBIS, TIOCMBIC = 0x8004746C, 0x8004746B
TIOCM_RTS, TIOCM_DTR = 0x004, 0x002


def main() -> int:
    if len(sys.argv) != 4:
        print(
            "usage: reset-and-capture.py <dev> <out> <seconds>", file=sys.stderr
        )
        return 2
    dev, out, secs = sys.argv[1], sys.argv[2], float(sys.argv[3])

    fd = os.open(dev, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    try:
        attrs = termios.tcgetattr(fd)
        attrs[0] = attrs[1] = attrs[3] = 0  # raw in, raw out, no line discipline
        attrs[2] = termios.CS8 | termios.CREAD | termios.CLOCAL  # and no HUPCL
        termios.tcsetattr(fd, termios.TCSANOW, attrs)

        fcntl.ioctl(fd, TIOCMBIC, struct.pack("I", TIOCM_DTR))  # strap released
        fcntl.ioctl(fd, TIOCMBIS, struct.pack("I", TIOCM_RTS))  # reset asserted
        time.sleep(0.1)
        fcntl.ioctl(fd, TIOCMBIC, struct.pack("I", TIOCM_RTS))  # and let go

        deadline, total = time.time() + secs, 0
        with open(out, "wb") as f:
            while time.time() < deadline:
                r, _, _ = select.select([fd], [], [], 0.2)
                if not r:
                    continue
                try:
                    chunk = os.read(fd, 4096)
                except BlockingIOError:
                    continue
                except OSError as e:
                    print(f"DEVICE ERROR {e}", file=sys.stderr)
                    break
                if chunk:
                    f.write(chunk)
                    f.flush()
                    total += len(chunk)
        print(f"CAPTURED {total} bytes -> {out}", file=sys.stderr)
    finally:
        os.close(fd)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
