#!/usr/bin/env python3
"""pty <-> TCP bridge for esp-emu's --uart-tcp.

esp-emu exposes the emulated UART0 as a TCP *server* (mirrors QEMU's
`-serial tcp::PORT,server,nowait`). lp-cli only speaks `serial:<path>`, and
socat is not installed on this machine, so this opens a pty pair, prints the
slave path (`/dev/ttysNNN`) for lp-cli to open, and shovels bytes both ways.

    python3 pty-tcp-bridge.py 127.0.0.1:5555 [--log capture.bin] [--pty-file path]

Every byte in each direction is also appended to --log (device->host as-is,
host->device wrapped in `<<HOST ... >>` markers) so a run leaves a transcript
that can be diffed for determinism. --pty-file writes the slave path to a file
so a shell script can read it without parsing stdout.

Retries the TCP connect for up to --connect-timeout seconds so it can be
started before (or slightly after) esp-emu. Exits when the socket closes.
"""
import argparse
import os
import select
import socket
import sys
import termios
import time
import tty


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("target", help="host:port of esp-emu --uart-tcp")
    ap.add_argument("--log", help="append a byte transcript here")
    ap.add_argument("--pty-file", help="write the slave pty path here")
    ap.add_argument("--connect-timeout", type=float, default=30.0)
    args = ap.parse_args()

    host, port = args.target.rsplit(":", 1)
    port = int(port)

    master, slave = os.openpty()
    slave_path = os.ttyname(slave)
    # Non-blocking master: with no reader on the slave the tty buffer fills
    # after ~1 KB and a blocking write would wedge the whole bridge (seen
    # 2026-09-06: capture stopped at 1,056 bytes). Bytes the pty cannot take
    # are dropped from the pty only; the --log transcript keeps every byte.
    os.set_blocking(master, False)
    pending = bytearray()
    dropped = 0
    # Raw mode on the slave so the tty layer never echoes or translates.
    tty.setraw(slave)
    attrs = termios.tcgetattr(slave)
    attrs[3] &= ~(termios.ECHO | termios.ICANON | termios.ISIG)  # lflag
    termios.tcsetattr(slave, termios.TCSANOW, attrs)
    print(f"PTY {slave_path}", flush=True)
    if args.pty_file:
        with open(args.pty_file, "w") as f:
            f.write(slave_path + "\n")

    deadline = time.monotonic() + args.connect_timeout
    sock = None
    while sock is None:
        try:
            sock = socket.create_connection((host, port), timeout=2.0)
        except OSError as e:
            if time.monotonic() > deadline:
                print(f"connect to {host}:{port} failed: {e}", file=sys.stderr)
                return 2
            time.sleep(0.2)
    sock.setblocking(False)
    print(f"CONNECTED {host}:{port}", flush=True)

    log = open(args.log, "ab") if args.log else None

    def logw(direction: str, data: bytes) -> None:
        if not log:
            return
        if direction == "dev":
            log.write(data)
        else:
            log.write(b"<<HOST " + data + b" >>")
        log.flush()

    try:
        while True:
            r, _, _ = select.select([master, sock], [], [], 1.0)
            if sock in r:
                try:
                    data = sock.recv(65536)
                except BlockingIOError:
                    data = b""
                if not data:
                    print("SOCKET CLOSED", flush=True)
                    return 0
                logw("dev", data)
                pending += data
                try:
                    n = os.write(master, bytes(pending))
                    del pending[:n]
                except BlockingIOError:
                    pass
                if len(pending) > 1 << 20:
                    dropped += len(pending) - (1 << 20)
                    del pending[: len(pending) - (1 << 20)]
            if pending and master not in r:
                try:
                    n = os.write(master, bytes(pending))
                    del pending[:n]
                except BlockingIOError:
                    pass
            if master in r:
                try:
                    data = os.read(master, 65536)
                except OSError:
                    data = b""
                if data:
                    logw("host", data)
                    sock.sendall(data)
    except KeyboardInterrupt:
        return 0
    finally:
        if dropped:
            print(f"DROPPED {dropped} bytes at the pty (no reader)", flush=True)
        if log:
            log.close()
        sock.close()
        os.close(master)
        os.close(slave)


if __name__ == "__main__":
    sys.exit(main())
