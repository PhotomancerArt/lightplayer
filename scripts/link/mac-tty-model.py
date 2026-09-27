#!/usr/bin/env python3
"""A model of the macOS receive path Chromium's Web Serial reads through, fed
with a board's byte stream: does it lose bytes, and in what shape?

The model is written from the two open-source files that implement the path
(read as behaviour, not copied; both APSL-2.0):

- IOSerialFamily `IOSerialBSDClient::getData` hands the tty at most
  `TTY_HIGHWATER - (rawq + canq)` bytes per pass, TTY_HIGHWATER = TTYHOG - 4
  = 1020, i.e. it counts one queue slot per byte. The difference is taken into
  an unsigned 32-bit variable, so when the queue already holds MORE than 1020
  the subtraction wraps, `MIN(…, 1024)` makes it a full 1 KB, and the
  "no room, block" test (`<= 0` on an unsigned) never fires.
- A port that cannot bypass the line discipline (Chromium's termios: PARMRK
  set, IGNBRK clear) goes through xnu `ttyinput` one byte at a time. There a
  data byte 0xFF with PARMRK set (and not both IGNBRK and IGNPAR) is queued
  as 0xFF 0xFF: two slots for one byte. A byte that arrives with the queue at
  MAX_INPUT (1024) is dropped (`input_overflow`, no IMAXBEL: silently).

So with 0xFF bytes in the stream the queue can pass 1020 while the reader is
behind, and then every later pass hands the tty a full 1 KB that ttyinput
throws away, until the reader drains the queue. JSON text has no 0xFF, so its
queue never passes 1020 and the client's own byte count is exact backpressure.

Time is in ticks of 1 ms. The board delivers `--rate` bytes per ms in 64 B
packets while it has data (the USB side never blocks: the driver's queue takes
them); the reader (Chromium's read plus the page) drains the tty queue once
per tick except inside its stalls. The reader folds 0xFF 0xFF back to 0xFF
the way Chromium's CheckReceiveError does.

    scripts/link/mac-tty-model.py <capture.bin> <out.bin> [--rate 140] [--stall-every 1000 --stall 100]
    lp-cli link soak-verify <out.bin>

Investigation tooling (plan lp2025/2026-09-26-1720-reliable-device-link, M1c).
"""
import argparse, collections, json

TTYHOG = 1024
MAX_INPUT = 1024
HIGHWATER = TTYHOG - 4
M32 = 1 << 32


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("capture"); ap.add_argument("out")
    ap.add_argument("--rate", type=int, default=140, help="board bytes per ms")
    ap.add_argument("--stall-every", type=int, default=0, help="ms between reader stalls")
    ap.add_argument("--stall", type=int, default=0, help="stall length, ms")
    ap.add_argument("--bypass", action="store_true",
                    help="a raw-mode port (cfmakeraw): no doubling, b_to_q")
    a = ap.parse_args()
    data = open(a.capture, "rb").read()

    driver = collections.deque()      # the CDC driver's own queue (never full here)
    rawq = bytearray()                # the tty's queue, slots
    out = bytearray()
    dropped = 0
    wraps = 0
    pos = 0
    t = 0
    fold_pending = False
    while pos < len(data) or driver or rawq:
        # the board: this tick's packets
        take = data[pos:pos + a.rate]
        pos += len(take)
        driver.extend(take)
        # IOSerialBSDClient's rx thread: passes until it blocks or runs dry
        while driver:
            size = (HIGHWATER - len(rawq)) % M32
            if size > HIGHWATER:
                wraps += 1
            size = min(size, 1024)
            if size == 0:
                break
            n = min(size, len(driver))
            chunk = [driver.popleft() for _ in range(n)]
            if a.bypass:
                rawq.extend(chunk)
                continue
            for c in chunk:
                if len(rawq) >= MAX_INPUT:
                    dropped += 1
                    continue
                if c == 0xFF:
                    rawq.append(0xFF)
                if len(rawq) < MAX_INPUT:
                    rawq.append(c)
                else:
                    dropped += 1
        # the reader
        stalled = a.stall_every and a.stall and (t % a.stall_every) < a.stall
        if not stalled:
            for b in rawq:
                if a.bypass:
                    out.append(b)
                elif b == 0xFF:
                    if fold_pending:
                        out.append(0xFF)  # 0xFF 0xFF -> one data 0xFF
                    fold_pending = not fold_pending
                else:
                    if fold_pending:
                        out.append(0xFF)  # a lone 0xFF (its twin was dropped)
                        fold_pending = False
                    out.append(b)
            rawq.clear()
        t += 1
    open(a.out, "wb").write(out)
    print(json.dumps({"in": len(data), "out": len(out), "dropped_bytes": dropped,
                      "wrapped_passes": wraps, "ticks_ms": t, "ff_in": data.count(0xFF)}))


if __name__ == "__main__":
    main()
