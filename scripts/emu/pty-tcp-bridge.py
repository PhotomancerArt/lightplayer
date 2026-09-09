#!/usr/bin/env python3
"""A PTY in front of the emulator's byte socket, so a real flasher can open it.

`lp-emu-esp32c6 --usb-sj tcp:<addr>` LISTENS and speaks raw bytes. `espflash`
speaks to a *serial port* — it takes `--port <path>` and opens it with the
`serialport` crate — and there is no TCP form. So the two cannot meet without
something that is a serial port on one side and a socket on the other.

That something is a pseudo-terminal: this opens one, puts the slave in raw
mode, prints its path, connects to the emulator and pumps bytes both ways.
`espflash --port <that path>` then talks to the mask ROM's download console.

    python3 pty-tcp-bridge.py --target 127.0.0.1:5600 --log flash.wire.bin
    PTY /dev/ttys014
    READY

# Why a third bridge

`scripts/emu/uart-tcp-proxy.py` is TCP→TCP (it transcribes a walk) and
`scripts/spike/esp-emu/usb-tcp-bridge.py` is tty→TCP in the other direction
(it puts a *board's* tty on a socket). Neither makes a device node, which is
the one thing a flasher needs; this is the missing third shape, not a fourth
copy of an existing one.

# What a PTY cannot carry

Modem control lines. `espflash --before default-reset` / `--after hard-reset`
drive DTR and RTS to reset the chip, and a pty has no such lines: on macOS and
on Linux the ioctl is accepted and goes nowhere, so the toggles are silently
lost rather than reported. A caller therefore passes `--before no-reset` and
performs the same dance on the emulator's `--control` channel, where
USB_DEVICE models it (`signals dtr=… rts=…` → the reset the serial bridge
asks for). `scripts/emu/flash-over-socket.sh` does exactly that and says so.

The log is the WIRE, both directions interleaved as they happened, with no
framing of our own: `<<HOST` and `>>DEV` markers would corrupt a SLIP stream
that a later reader may want to parse. Use `--log-host` / `--log-dev` for the
split form when a direction has to be read on its own.
"""
import argparse
import os
import select
import signal
import socket
import sys
import termios
import time
import tty


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--target", required=True, help="host:port the emulator listens on")
    ap.add_argument("--link", help="symlink to create pointing at the pty slave")
    ap.add_argument("--log", help="every byte both ways, in wire order")
    ap.add_argument("--log-host", help="only what the flasher sent")
    ap.add_argument("--log-dev", help="only what the chip sent")
    ap.add_argument("--connect-timeout", type=float, default=30.0)
    ap.add_argument(
        "--write-timeout",
        type=float,
        default=2.0,
        help="seconds to keep trying to hand a chunk to the pty before dropping it",
    )
    ap.add_argument(
        "--idle-exit",
        type=float,
        default=0.0,
        help="exit after this many seconds with no traffic either way (0 = never)",
    )
    args = ap.parse_args()

    host, port = args.target.rsplit(":", 1)
    deadline = time.monotonic() + args.connect_timeout
    dev = None
    while dev is None:
        try:
            dev = socket.create_connection((host, int(port)), timeout=2.0)
        except OSError as e:
            if time.monotonic() > deadline:
                print(f"CONNECT FAILED {args.target}: {e}", flush=True)
                return 2
            time.sleep(0.05)
    dev.setblocking(False)

    master, slave = os.openpty()
    # Raw on the SLAVE: one termios per pty, and the line discipline is what
    # would otherwise echo the flasher's own bytes back at it and translate
    # \n into \r\n in the middle of a SLIP frame.
    tty.setraw(slave)
    attrs = termios.tcgetattr(slave)
    attrs[2] |= termios.CLOCAL | termios.CREAD
    termios.tcsetattr(slave, termios.TCSANOW, attrs)
    path = os.ttyname(slave)
    # The slave fd stays OPEN for the life of the bridge. Without it, the
    # master reads EIO the moment espflash closes the port, and a second
    # espflash invocation against the same path would find a dead pty.
    os.set_blocking(master, False)

    if args.link:
        try:
            os.remove(args.link)
        except FileNotFoundError:
            pass
        os.symlink(path, args.link)

    logs = {
        "both": open(args.log, "wb", buffering=0) if args.log else None,
        "host": open(args.log_host, "wb", buffering=0) if args.log_host else None,
        "dev": open(args.log_dev, "wb", buffering=0) if args.log_dev else None,
    }

    print(f"PTY {path}", flush=True)
    print("READY", flush=True)

    stop = False

    def on_signal(_sig, _frame):
        nonlocal stop
        stop = True

    signal.signal(signal.SIGINT, on_signal)
    signal.signal(signal.SIGTERM, on_signal)

    last = time.monotonic()
    dropped = 0
    to_dev = b""
    try:
        while not stop:
            want_write = [dev] if to_dev else []
            r, w, _ = select.select([master, dev], want_write, [], 0.2)
            moved = False
            if master in r:
                try:
                    data = os.read(master, 65536)
                except (BlockingIOError, InterruptedError):
                    data = b""
                except OSError:
                    # EIO: every slave holder closed. Ours is still open, so
                    # this is the pty going away rather than a normal close.
                    data = b""
                if data:
                    moved = True
                    for name in ("both", "host"):
                        if logs[name]:
                            logs[name].write(data)
                    to_dev += data
            if dev in r:
                try:
                    data = dev.recv(65536)
                except (BlockingIOError, InterruptedError):
                    data = b""
                if data == b"":
                    try:
                        # A real recv of zero is the emulator's socket closing.
                        dev.getpeername()
                    except OSError:
                        print("DEVICE CLOSED", flush=True)
                        break
                if data:
                    moved = True
                    for name in ("both", "dev"):
                        if logs[name]:
                            logs[name].write(data)
                    # Bounded, and this bound is load-bearing. The pty stays
                    # alive after the flasher exits (the bridge holds the
                    # slave open on purpose), so nothing is READING it any
                    # more and its buffer fills after a few kilobytes. An
                    # unbounded retry loop wedges there and never sees the
                    # signal that would stop it — which is exactly how the
                    # first cut of this script hung a recipe run after the
                    # chip rebooted and started talking. A serial port whose
                    # application has closed drops what it cannot deliver;
                    # so does this, and it says how much.
                    pending = data
                    deadline = time.monotonic() + args.write_timeout
                    while pending and not stop:
                        try:
                            n = os.write(master, pending)
                        except BlockingIOError:
                            n = 0
                        pending = pending[n:]
                        if not pending:
                            break
                        if time.monotonic() > deadline:
                            dropped += len(pending)
                            print(
                                f"DROPPED {len(pending)} (nothing is reading the pty; "
                                f"{dropped} total)",
                                flush=True,
                            )
                            break
                        select.select([], [master], [], 0.2)
            if to_dev and dev in w:
                try:
                    n = dev.send(to_dev)
                except BlockingIOError:
                    n = 0
                except OSError as e:
                    print(f"DEVICE ERROR {e}", flush=True)
                    break
                to_dev = to_dev[n:]
                moved = True
            now = time.monotonic()
            if moved:
                last = now
            elif args.idle_exit and now - last > args.idle_exit:
                print("IDLE", flush=True)
                break
    finally:
        for f in logs.values():
            if f:
                f.close()
        if args.link:
            try:
                os.remove(args.link)
            except FileNotFoundError:
                pass
        os.close(master)
        os.close(slave)
        dev.close()
        print("CLOSED", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
