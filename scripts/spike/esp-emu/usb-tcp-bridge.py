#!/usr/bin/env python3
"""Raw tty <-> TCP bridge for the desk half of the esp-emu spike.

Opens ONE serial device raw (no pyserial needed: os.open + termios; the
ESP32-C6's USB-Serial-JTAG ignores baud), listens on one TCP port and pumps
bytes both ways. Put uart-tcp-proxy.py in front of it and point lp-cli at the
proxy with `serial:tcp://…`, and the desk walk leaves the same `.uart.bin`
transcript the emulator walk did — the board's bytes on the same wire shape.

    python3 usb-tcp-bridge.py --dev /dev/cu.usbmodem1433201 --listen 127.0.0.1:5581

Never toggles DTR/RTS (an espflash-style sequence on a native-USB C6 can park
it); opening the port is the only line-state change, the same one Studio and
lp-cli make. Exits when the client closes (or on SIGINT), closing the tty.
"""
import argparse
import os
import select
import socket
import sys
import termios
import tty


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dev", required=True)
    ap.add_argument("--listen", required=True)
    ap.add_argument("--linger", type=float, default=0.0,
                    help="seconds to keep pumping after the client closes")
    args = ap.parse_args()

    fd = os.open(args.dev, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    attrs = termios.tcgetattr(fd)
    tty.setraw(fd)
    attrs = termios.tcgetattr(fd)
    attrs[2] &= ~termios.HUPCL  # do not drop DTR on close
    attrs[2] |= termios.CLOCAL | termios.CREAD
    termios.tcsetattr(fd, termios.TCSANOW, attrs)
    print(f"OPENED {args.dev}", flush=True)

    lh, lp = args.listen.rsplit(":", 1)
    srv = socket.socket()
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((lh, int(lp)))
    srv.listen(1)
    print(f"LISTEN {args.listen}", flush=True)

    client = None
    pending = b""
    try:
        while True:
            rl = [fd, srv] + ([client] if client else [])
            r, _, _ = select.select(rl, [], [], 1.0)
            if fd in r:
                try:
                    data = os.read(fd, 65536)
                except BlockingIOError:
                    data = b""
                except OSError as e:
                    print(f"DEVICE ERROR {e}", flush=True)
                    return 1
                if data and client:
                    try:
                        client.sendall(data)
                    except OSError:
                        client.close(); client = None
                        print("CLIENT GONE", flush=True)
                        return 0
            if srv in r:
                c, addr = srv.accept()
                if client:
                    client.close()
                client = c
                client.setblocking(False)
                print(f"CLIENT {addr[0]}:{addr[1]}", flush=True)
            if client and client in r:
                try:
                    data = client.recv(65536)
                except (BlockingIOError, InterruptedError):
                    data = b""
                except OSError:
                    data = b""
                if not data:
                    print("CLIENT CLOSED", flush=True)
                    client.close(); client = None
                    return 0
                pending += data
                while pending:
                    try:
                        n = os.write(fd, pending)
                    except BlockingIOError:
                        select.select([], [fd], [], 1.0)
                        continue
                    pending = pending[n:]
    except KeyboardInterrupt:
        print("SIGINT", flush=True)
        return 0
    finally:
        if client:
            client.close()
        os.close(fd)
        print("CLOSED", flush=True)


if __name__ == "__main__":
    sys.exit(main())
