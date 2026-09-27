#!/usr/bin/env python3
"""A native soak reader that opens the port the way Chromium's Web Serial does.

`lp-cli link soak` opens a serial port raw (serialport's cfmakeraw). Chromium
(services/device/serial/serial_io_handler_posix.cc, ConfigurePortImpl) does
not: it clears ICANON ECHO ECHOE ECHONL ISIG and IGNBRK BRKINT ISTRIP INLCR
IGNCR ICRNL IXON, clears OPOST, and SETS PARMRK (with IGNPAR when there is no
parity), leaving IEXTEN as it was. With PARMRK set the tty marks errors
in-band and passes a data byte 0xFF as 0xFF 0xFF, which Chromium's
CheckReceiveError folds back.

This reader reproduces that open (`--termios chrome`) or a plain raw one
(`--termios raw`), optionally stops reading on a schedule (a busy page), folds
0xFF 0xFF like Chromium when the tty doubled it, and writes the capture for
`lp-cli link soak-verify`. It drives the soak_link board itself: hello, the
packed opt-in, `SOAK! on=1 ...`, then `SOAK! on=0`.

    scripts/link/tty-soak.py --dev /dev/cu.usbmodem1101 --out cap.bin \\
        --termios chrome --encoding packed --seconds 20 --stall-every-ms 1000 --stall-ms 100

Never touches DTR/RTS (HUPCL cleared). Investigation tooling (plan
lp2025/2026-09-26-1720-reliable-device-link, M1c).
"""

import argparse
import json
import os
import select
import sys
import termios
import time


def configure(fd, mode):
    attrs = termios.tcgetattr(fd)
    iflag, oflag, cflag, lflag, ispeed, ospeed, cc = attrs
    if mode in ("raw", "raw-parmrk"):
        # cfmakeraw
        iflag &= ~(termios.IGNBRK | termios.BRKINT | termios.PARMRK | termios.ISTRIP
                   | termios.INLCR | termios.IGNCR | termios.ICRNL | termios.IXON)
        oflag &= ~termios.OPOST
        lflag &= ~(termios.ECHO | termios.ECHONL | termios.ICANON | termios.ISIG | termios.IEXTEN)
        cflag &= ~(termios.CSIZE | termios.PARENB)
        cflag |= termios.CS8
        if mode == "raw-parmrk":
            # cfmakeraw plus the one flag Chromium adds.
            iflag |= termios.PARMRK
    else:
        # Chromium's ConfigurePortImpl, no parity, 8N1, no flow control.
        lflag &= ~(termios.ICANON | termios.ECHO | termios.ECHOE | termios.ECHONL | termios.ISIG)
        iflag &= ~(termios.IGNBRK | termios.BRKINT | termios.ISTRIP | termios.INLCR
                   | termios.IGNCR | termios.ICRNL | termios.IXON)
        iflag |= termios.PARMRK
        oflag &= ~termios.OPOST
        cflag &= ~(termios.CSIZE | termios.PARENB | termios.PARODD | termios.CSTOPB | termios.CRTSCTS)
        cflag |= termios.CS8 | termios.CREAD | termios.CLOCAL
        iflag |= termios.IGNPAR
        iflag &= ~termios.INPCK
        if mode == "chrome-ignbrk":
            # Chromium's flags plus IGNBRK: with IGNBRK and IGNPAR both set,
            # PARMRK no longer doubles 0xFF and the port may bypass ttyinput.
            iflag |= termios.IGNBRK
    cflag &= ~termios.HUPCL
    cc[termios.VMIN] = 1
    cc[termios.VTIME] = 0
    termios.tcsetattr(fd, termios.TCSANOW, [iflag, oflag, cflag, lflag, ispeed, ospeed, cc])
    return {"iflag": hex(iflag), "oflag": hex(oflag), "cflag": hex(cflag), "lflag": hex(lflag)}


class Folder:
    """Chromium's CheckReceiveError, for the no-parity case: 0xFF 0xFF -> 0xFF,
    0xFF 0x00 0x00 -> (a break: dropped, counted)."""

    def __init__(self):
        self.state = 0  # 0 none, 1 saw FF, 2 saw FF 00
        self.pending = bytearray()
        self.breaks = 0
        self.folded = 0

    def feed(self, data):
        out = bytearray()
        for b in data:
            if self.state == 0:
                if b == 0xFF:
                    self.state = 1
                    self.pending = bytearray([b])
                else:
                    out.append(b)
            elif self.state == 1:
                if b == 0x00:
                    self.state = 2
                    self.pending.append(b)
                elif b == 0xFF:
                    out.append(0xFF)
                    self.folded += 1
                    self.state = 0
                else:
                    out += self.pending
                    out.append(b)
                    self.state = 0
            else:
                if b == 0x00:
                    self.breaks += 1
                    self.state = 0
                elif b == 0xFF:
                    out += self.pending
                    self.pending = bytearray([b])
                    self.state = 1
                else:
                    out += self.pending
                    out.append(b)
                    self.state = 0
        return bytes(out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dev", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--termios", choices=["chrome", "chrome-ignbrk", "raw", "raw-parmrk"], default="chrome")
    ap.add_argument("--fold", choices=["auto", "on", "off"], default="auto",
                    help="fold 0xFF 0xFF like Chromium (auto: with --termios chrome)")
    ap.add_argument("--encoding", choices=["packed", "json"], default="packed")
    ap.add_argument("--seconds", type=float, default=20)
    ap.add_argument("--stall-every-ms", type=int, default=0)
    ap.add_argument("--stall-ms", type=int, default=0)
    ap.add_argument("--read-size", type=int, default=255,
                    help="bytes per read() (Chromium asks for what its data pipe has room for)")
    ap.add_argument("--min", type=int, default=16)
    ap.add_argument("--max", type=int, default=16384)
    a = ap.parse_args()

    fd = os.open(a.dev, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    flags = configure(fd, a.termios)
    fold = a.fold == "on" or (a.fold == "auto" and a.termios in ("chrome", "raw-parmrk"))
    folder = Folder()
    raw = bytearray()
    out = bytearray()

    def write(text):
        data = text.encode()
        while data:
            try:
                n = os.write(fd, data)
                data = data[n:]
            except BlockingIOError:
                time.sleep(0.001)

    def pump(until):
        while time.monotonic() < until:
            r, _, _ = select.select([fd], [], [], 0.01)
            if not r:
                continue
            try:
                chunk = os.read(fd, a.read_size)
            except BlockingIOError:
                continue
            raw.extend(chunk)
            out.extend(folder.feed(chunk) if fold else chunk)

    start = time.monotonic()
    write('M!{"id":1,"msg":"hello"}\n')
    pump(start + 0.7)
    if a.encoding == "packed":
        write('M!{"id":9007199254740991,"msg":{"setEncoding":{"encoding":"packed","format":2}}}\n')
    else:
        # the board keeps a packed agreement until the link resets
        write('M!{"id":2,"msg":{"setEncoding":{"encoding":"json","format":2}}}\n')
    pump(time.monotonic() + 0.5)
    write(f"\nSOAK! on=1 min={a.min} max={a.max} rate=0 logs=0 seed=1 budget=20 count=0\n")
    t0 = time.monotonic()
    end = t0 + a.seconds
    stalls = 0
    next_stall = t0 + a.stall_every_ms / 1000 if a.stall_every_ms else None
    while time.monotonic() < end:
        if next_stall and time.monotonic() >= next_stall:
            time.sleep(a.stall_ms / 1000)
            stalls += 1
            next_stall += a.stall_every_ms / 1000
        pump(min(end, next_stall) if next_stall else end)
    write("\nSOAK! on=0\n")
    pump(time.monotonic() + 2.5)
    os.close(fd)
    with open(a.out, "wb") as f:
        f.write(out)
    meta = {"dev": a.dev, "termios": a.termios, "flags": flags, "fold": fold,
            "encoding": a.encoding, "seconds": a.seconds, "stall_every_ms": a.stall_every_ms,
            "stall_ms": a.stall_ms, "stalls": stalls, "read_size": a.read_size,
            "raw_bytes": len(raw), "bytes": len(out), "ff_pairs_folded": folder.folded,
            "breaks_dropped": folder.breaks, "raw_ff": raw.count(0xFF)}
    with open(a.out + ".meta.json", "w") as f:
        json.dump(meta, f, indent=1)
    print(json.dumps(meta))


if __name__ == "__main__":
    main()
