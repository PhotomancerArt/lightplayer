#!/usr/bin/env python3
"""Transcribing TCP proxy for esp-emu's --uart-tcp.

esp-emu serves the emulated UART0 on one TCP socket and accepts one client.
lp-cli can now speak `serial:tcp://host:port` itself, but then nothing else
sees the bytes. This sits in between — listens on --listen, connects to the
emulator on --target — and appends every byte to --log (device->host as-is,
host->device wrapped in `<<HOST ... >>`), so a walk leaves a transcript that
can be grepped for `[mem]` lines and diffed run to run.

    python3 uart-tcp-proxy.py --target 127.0.0.1:5555 --listen 127.0.0.1:5565 --log walk.uart.bin

Connects to the emulator immediately (retrying up to --connect-timeout s) so
the boot banner is captured even before a client attaches; bytes are
buffered (up to 4 MiB) until a client connects and then replayed, which is
what a serial port with a fresh boot behind it looks like to lp-cli.
"""
import argparse
import select
import socket
import sys
import time


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--target", required=True)
    ap.add_argument("--listen", required=True)
    ap.add_argument("--log")
    ap.add_argument("--connect-timeout", type=float, default=30.0)
    ap.add_argument("--replay", action="store_true", default=True)
    args = ap.parse_args()

    th, tp = args.target.rsplit(":", 1)
    lh, lp = args.listen.rsplit(":", 1)
    srv = socket.socket()
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((lh, int(lp)))
    srv.listen(1)
    print(f"LISTEN {args.listen}", flush=True)

    deadline = time.monotonic() + args.connect_timeout
    dev = None
    while dev is None:
        try:
            dev = socket.create_connection((th, int(tp)), timeout=2.0)
        except OSError as e:
            if time.monotonic() > deadline:
                print(f"connect {args.target}: {e}", file=sys.stderr)
                return 2
            time.sleep(0.2)
    dev.setblocking(False)
    print(f"CONNECTED {args.target}", flush=True)
    log = open(args.log, "ab") if args.log else None
    # Side file: wall-clock (s since the emulator connect) per chunk, with the
    # transcript byte offset, so "time to the hello line" is a lookup.
    tlog = open(args.log + ".times", "a") if args.log else None
    t0 = time.monotonic()
    offset = [0]

    def logw(direction, data):
        if not log:
            return
        rec = data if direction == "dev" else b"<<HOST " + data + b" >>"
        log.write(rec)
        log.flush()
        tlog.write(f"{time.monotonic() - t0:.3f} {direction} {offset[0]} {len(rec)}\n")
        tlog.flush()
        offset[0] += len(rec)

    client = None
    backlog = bytearray()
    try:
        while True:
            rl = [dev, srv] + ([client] if client else [])
            r, _, _ = select.select(rl, [], [], 1.0)
            if dev in r:
                try:
                    data = dev.recv(65536)
                except BlockingIOError:
                    data = b""
                if not data:
                    print("DEVICE CLOSED", flush=True)
                    return 0
                logw("dev", data)
                if client:
                    try:
                        client.sendall(data)
                    except OSError:
                        client.close(); client = None
                        print("CLIENT GONE", flush=True)
                else:
                    backlog += data
                    if len(backlog) > 4 << 20:
                        del backlog[: len(backlog) - (4 << 20)]
            if srv in r:
                c, addr = srv.accept()
                if client:
                    client.close()
                client = c
                client.setblocking(False)
                print(f"CLIENT {addr[0]}:{addr[1]} (replaying {len(backlog)} B)", flush=True)
                if backlog and args.replay:
                    try:
                        client.sendall(bytes(backlog))
                    except OSError:
                        pass
                backlog.clear()
            if client and client in r:
                try:
                    data = client.recv(65536)
                except BlockingIOError:
                    data = b""
                except OSError:
                    data = b""
                if not data:
                    print("CLIENT CLOSED", flush=True)
                    client.close(); client = None
                    continue
                logw("host", data)
                dev.sendall(data)
    except KeyboardInterrupt:
        return 0
    finally:
        if log:
            log.close()
        dev.close(); srv.close()
        if client:
            client.close()


if __name__ == "__main__":
    sys.exit(main())
