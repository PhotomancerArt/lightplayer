#!/usr/bin/env python3
"""Reset the classic ESP32 through its CH340K cable and capture the console.

The classic on this desk (DOM-Z-102, ESP32 v3.1, MAC `30:76:f5:ec:f6:34`) has
**no USB-Serial-JTAG peripheral**: its console is UART0, and the host's side of
UART0 is a CH340K bridge whose DTR/RTS lines are wired to EN and IO0. So a
reset, a download-mode strap and a console read are all the same handle, and
this script is the instrument that drives it.

    scripts/emu/classic-reset-and-capture.py --list
    scripts/emu/classic-reset-and-capture.py --baud 115200 --seconds 8 \\
        --out cap_115200.bin --decoded cap_115200.txt

Four facts out of the bench (`bench.md`, L0/L1a/L1 of the Xtensa emulator
plan), each of which cost a sitting to find, and each of which this file
implements rather than describes:

1. **The WCH dext ignores single-bit `TIOCMBIS`/`TIOCMBIC`.** `cn.wch.
   CH34xVCPDriver` moves nothing for them, which is what pyserial's `.dtr` /
   `.rts` setters issue — so pyserial would not work here even if this host
   had it (it does not). The lines move only under a **whole-status
   `TIOCMSET`**: `0x4` (RTS only) = EN low = reset held, `0x0` = both released
   = run, `0x2` (DTR only) = IO0 low with EN released. Product code records
   the same at `lp-app/lpa-client/src/stream/serialport_stream.rs:51-60`.
2. **macOS tty close DRAINS.** `close()` blocked for over a minute after an
   8-second read at 921600. `tcflush(TCIOFLUSH)` before close, always — this
   script does it on every path including the error path.
3. **One baud per capture.** The mask ROM and the ESP-IDF second-stage
   bootloader talk at **115200**; the application reprograms `clkdiv` in
   `board::esp32v3::init` and talks at **921600**. One image therefore needs
   *two* captures, and each one reads the other half's bytes as line noise.
   `--decoded` writes the decodable span and the raw read is preserved whole
   (DD46): see `decodable_span`, which is what the committed transcripts under
   `lp-emu/transcripts/esp32v3/` were cut with.
4. **The port name moves.** `/dev/cu.wchusbserial143330` (L0) became
   `/dev/cu.wchusbserial11330` (L1a) when the hub re-enumerated, same cable,
   same board. Resolve by **USB identity** — CH340K `1a86:7522`, which carries
   no USB serial number, so the locationID and the node it names are the
   identity — never by a remembered path, and never by taking the first port.

## Why there are two reset-and-capture scripts

`scripts/emu/reset-and-capture.py` is the **C6's** and stays as it is. It
drives an Espressif native USB-Serial-JTAG handle with `TIOCMBIS`/`TIOCMBIC`,
which that device honours and this cable does not (fact 1), and it relies on a
USB-SJ session surviving a chip reset, which a CH340 bridge has no equivalent
of. Neither script is a generalisation of the other; they are two different
cables.

## Modes

    --reset run        TIOCMSET 0x4, 100 ms, TIOCMSET 0x0   → the board boots
    --reset download   TIOCMSET 0x4, 100 ms, TIOCMSET 0x2, 80 ms, TIOCMSET 0x0
                       → the ROM downloader (L1 measured `boot:0x3` this way)
    --reset none       the lines are released and nothing is pulsed

`--download-mode` and `--no-reset` are spellings of the last two. The three
`--reset` words are the ones the committed sidecars quote, so they are the
canonical ones and they do not change.

## What this never does

It never guesses a port, never opens one that another process holds (`lsof`
first — Brave takes the port on a Web Serial reload), never flashes, and never
leaves a line asserted. `--self-test` exercises the argument parsing, the
`--send-after` matcher, the span cutter and the port-name rule **with no port
open and no board present**, because a script whose only proof is a board is a
script nobody can change.
"""

from __future__ import annotations

import argparse
import array
import fcntl
import os
import plistlib
import select
import subprocess
import sys
import termios
import time
from pathlib import Path

# --------------------------------------------------------------------------
# The cable
# --------------------------------------------------------------------------

# <sys/ioccom.h> encodings, the same constants `scripts/emu/reset-and-capture.py`
# uses; only the *way they are issued* differs between the two cables.
TIOCMGET = 0x4004746A
TIOCMSET = 0x8004746D
TIOCM_DTR = 0x002
TIOCM_RTS = 0x004
# IOKit's _IOW('T', 2, speed_t). The 4-byte form, as pyserial uses on Darwin.
IOSSIOSPEED = 0x80045402

# CH340K. No USB serial number is exposed, so unlike `board-port.py`'s
# Espressif boards these cannot be told apart by identity alone — only by
# where they are plugged. Hence `--location`, and hence the refusal to guess.
CH34X_VID = 0x1A86
CH34X_PID = 0x7522

#: Whole-status words, and the dwell after each, for every reset mode.
#: Kept as data so `--self-test` can assert the numbers rather than a comment.
RESET_SEQUENCES: dict[str, tuple[tuple[int, float], ...]] = {
    "run": ((TIOCM_RTS, 0.100), (0x0, 0.0)),
    "download": ((TIOCM_RTS, 0.100), (TIOCM_DTR, 0.080), (0x0, 0.0)),
    "none": (),
}

#: What a byte has to be to be part of a capture's decodable span: printable
#: ASCII, the three whitespace bytes a console emits, and ESC for the SGR
#: colour codes the ESP-IDF bootloader writes (which the transcripts keep).
DECODABLE = frozenset({0x09, 0x0A, 0x0D, 0x1B}) | frozenset(range(0x20, 0x7F))


def decodable_span(raw: bytes) -> tuple[int, int]:
    """The `[start, end)` of `raw` that is this capture's console output.

    A capture at one baud contains the other half's bytes as line noise
    (fact 3): the 115200 capture ends in the application's 921600 traffic
    misread, and the 921600 capture *begins* with the ROM and bootloader's
    115200 traffic misread. Both are high-bit and NUL bytes with short runs
    of accidentally-printable ones between them.

    The rule, which reproduces all four committed transcripts from their
    committed `.raw.bin` byte for byte: take the **longest unbroken run of
    decodable bytes**, then cut the end back to its last newline — one noise
    byte after the final `\\r\\n` is printable often enough to matter, and a
    transcript ends at a line.
    """
    best_start, best_end = 0, 0
    start: int | None = None
    for i, byte in enumerate(raw):
        if byte in DECODABLE:
            if start is None:
                start = i
        elif start is not None:
            if i - start > best_end - best_start:
                best_start, best_end = start, i
            start = None
    if start is not None and len(raw) - start > best_end - best_start:
        best_start, best_end = start, len(raw)

    cut = raw.rfind(b"\n", best_start, best_end)
    if cut >= 0:
        best_end = cut + 1
    return best_start, best_end


# --------------------------------------------------------------------------
# The board, by identity
# --------------------------------------------------------------------------


def port_for(location_id: int) -> str:
    """The device node the WCH dext builds from a locationID.

    `board-port.py` documents the Espressif form (`usbmodem` + the hex with
    trailing zeros stripped + the interface number `01`). The CH340 is a
    single-interface device, so the same rule ends in `0`:
    `0x00143330` → `wchusbserial143330`, `0x01133000` → `wchusbserial11330`.
    Both of those are bench-observed (`bench.md` L0 and L1a).
    """
    return f"/dev/cu.wchusbserial{format(location_id, 'x').rstrip('0')}0"


def bridges() -> list[tuple[int, str]]:
    """`(locationID, port)` for every CH340K on the bus. Opens nothing."""
    raw = subprocess.run(
        ["ioreg", "-a", "-r", "-c", "IOUSBHostDevice", "-l", "-w0"],
        capture_output=True,
        check=True,
    ).stdout
    if not raw:
        return []
    found: dict[int, tuple[int, str]] = {}

    def walk(node: object) -> None:
        if isinstance(node, dict):
            if node.get("idVendor") == CH34X_VID and node.get("idProduct") == CH34X_PID:
                loc = node.get("locationID")
                if isinstance(loc, int):
                    found[loc] = (loc, port_for(loc))
            for value in node.values():
                walk(value)
        elif isinstance(node, list):
            for value in node:
                walk(value)

    walk(plistlib.loads(raw))
    return sorted(found.values())


def resolve_port(location: int | None) -> str:
    """One named cable, or a refusal. **Never the first port.**"""
    present = bridges()
    if not present:
        raise SystemExit(
            f"no CH340K ({CH34X_VID:04x}:{CH34X_PID:04x}) is on the bus. "
            "Check the cable, and `--list` to see what is."
        )
    if location is not None:
        for loc, port in present:
            if loc == location:
                return confirmed(port, loc)
        listing = "  ".join(f"0x{loc:x}" for loc, _ in present)
        raise SystemExit(f"no CH340K at location 0x{location:x}. Present: {listing}")
    if len(present) > 1:
        listing = "\n".join(f"  0x{loc:x}  {port}" for loc, port in present)
        raise SystemExit(
            "more than one CH340K is on the bus and they carry no serial "
            f"number to tell them apart. Name one with --location:\n{listing}"
        )
    loc, port = present[0]
    return confirmed(port, loc)


def confirmed(port: str, loc: int) -> str:
    if not os.path.exists(port):
        raise SystemExit(
            f"the CH340K at location 0x{loc:x} is on the bus but {port} does not "
            "exist — the device-node naming rule has changed. Do not guess a port; "
            "`ls /dev/cu.*` and fix this script's port_for()."
        )
    return port


def refuse_if_held(port: str) -> None:
    """Brave holds a USB enumeration handle and takes the port on a Web Serial
    reload (`bench.md`, standing rules). Opening it underneath would give a
    half-readable capture and no error."""
    try:
        proc = subprocess.run(["lsof", "--", port], capture_output=True, text=True)
    except FileNotFoundError:  # pragma: no cover - lsof is in macOS's base
        print("  (lsof is not on PATH; the held-port check did not run)", file=sys.stderr)
        return
    lines = [l for l in proc.stdout.splitlines()[1:] if l.strip()]
    if lines:
        holders = "\n".join(f"  {l}" for l in lines)
        raise SystemExit(f"{port} is held. Close it and try again:\n{holders}")


# --------------------------------------------------------------------------
# The scripted sends
# --------------------------------------------------------------------------


def parse_send_after(spec: str) -> tuple[bytes, float, str]:
    """`TRIGGER:DELAY_MS:TEXT` → `(trigger, delay_s, text)`.

    Two colons, left to right, so the TEXT may (and does) contain colons:
    L1's own invocation is
    `--send-after '[INIT] I/O task spawned:1:M!{"id":1,"msg":"stopAllProjects"}'`.
    """
    parts = spec.split(":", 2)
    if len(parts) != 3:
        raise ValueError(f"--send-after wants TRIGGER:DELAY_MS:TEXT, got {spec!r}")
    trigger, delay_ms, text = parts
    if not trigger:
        raise ValueError(f"--send-after needs a trigger line: {spec!r}")
    return trigger.encode(), float(delay_ms) / 1000.0, text


def parse_send_at(spec: str) -> tuple[float, str]:
    """`SECONDS:TEXT` → `(seconds, text)`, wall-clock from the reset."""
    parts = spec.split(":", 1)
    if len(parts) != 2:
        raise ValueError(f"--send-at wants SECONDS:TEXT, got {spec!r}")
    return float(parts[0]), parts[1]


class Sender:
    """When each scripted request goes out. Pure, so it can be tested dry.

    `feed(buf, now)` is handed the whole capture so far and returns the lines
    that are due, in order. A `--send-after` fires **once**, `delay` after the
    first byte of its trigger has arrived; how soon after is bounded by the
    read loop's poll interval (`--poll`), and that latency is the host's, not
    the board's — the emulated twin's send lands 1 ms of emulated time after
    the same trigger, and nothing in the timing class is compared across it.
    """

    def __init__(
        self,
        after: list[tuple[bytes, float, str]],
        at: list[tuple[float, str]],
        t0: float,
    ) -> None:
        self._after = [[trig, delay, text, None] for trig, delay, text in after]
        self._at = [[when, text, False] for when, text in at]
        self._t0 = t0

    def feed(self, buf: bytes, now: float) -> list[tuple[str, str]]:
        """`(text, why)` for every request due at `now`."""
        due: list[tuple[str, str]] = []
        for item in self._after:
            trigger, delay, text, fire_at = item
            if fire_at is None:
                if trigger in buf:
                    item[3] = now + delay
            elif fire_at != float("inf") and now >= fire_at:
                item[3] = float("inf")
                due.append((text, f"after {trigger.decode(errors='replace')!r}"))
        for item in self._at:
            when, text, done = item
            if not done and now - self._t0 >= when:
                item[2] = True
                due.append((text, f"at {when}s"))
        return due

    def unfired(self) -> list[str]:
        """Requests whose trigger never arrived — a capture that ends with one
        of these outstanding did not see what it came for."""
        out = [
            f"after {trig.decode(errors='replace')!r}: {text}"
            for trig, _delay, text, fire_at in self._after
            if fire_at is None
        ]
        out += [f"at {when}s: {text}" for when, text, done in self._at if not done]
        return out


# --------------------------------------------------------------------------
# The port
# --------------------------------------------------------------------------


def modem_set(fd: int, bits: int) -> None:
    fcntl.ioctl(fd, TIOCMSET, array.array("i", [bits]))


def modem_get(fd: int) -> int:
    buf = array.array("i", [0])
    fcntl.ioctl(fd, TIOCMGET, buf, 1)
    return buf[0]


def configure(fd: int, baud: int) -> None:
    """Raw, 8N1, no flow control, `HUPCL` cleared — the same open Studio and
    `lp-cli` make (`serialport_stream.rs`)."""
    iflag, oflag, cflag, lflag, _ispeed, _ospeed, cc = termios.tcgetattr(fd)
    iflag = 0
    oflag = 0
    lflag = 0
    # No HUPCL: the close must not drop DTR under the board.
    cflag = termios.CS8 | termios.CREAD | termios.CLOCAL
    cc = list(cc)
    cc[termios.VMIN] = 0
    cc[termios.VTIME] = 0
    # macOS termios takes the literal rate as the B constant, so a numeric
    # speed is legal; IOSSIOSPEED below is the belt to that's braces, and
    # 921600 needs it on some dext builds.
    termios.tcsetattr(fd, termios.TCSANOW, [iflag, oflag, cflag, lflag, baud, baud, cc])
    try:
        fcntl.ioctl(fd, IOSSIOSPEED, array.array("i", [baud]), 1)
    except OSError as exc:  # pragma: no cover - diagnostic only
        print(f"  (IOSSIOSPEED {baud} refused: {exc}; the termios rate stands)", file=sys.stderr)


def pulse(fd: int, mode: str) -> None:
    for bits, dwell in RESET_SEQUENCES[mode]:
        modem_set(fd, bits)
        print(f"  TIOCMSET 0x{bits:x} -> 0x{modem_get(fd):x}")
        if dwell:
            time.sleep(dwell)


def write_outputs(args: argparse.Namespace, raw: bytes) -> None:
    """The raw read whole, and the decodable span beside it (DD46)."""
    Path(args.out).write_bytes(raw)
    print(f"  captured {len(raw)} bytes -> {args.out}")
    start, end = decodable_span(raw)
    span = raw[start:end]
    print(
        f"  decodable span: [{start}, {end}) = {len(span)} bytes "
        f"({len(raw) - len(span)} bytes of the other baud's traffic trimmed)"
    )
    if args.decoded:
        Path(args.decoded).write_bytes(span)
        print(f"  decoded view -> {args.decoded}")
    if args.expect_mac:
        text = span.decode("utf-8", errors="replace")
        if args.expect_mac.lower() in text.lower():
            print(f"  board confirmed: {args.expect_mac} is in the capture")
        else:
            raise SystemExit(
                f"{args.expect_mac} is NOT in this capture. Either this is not the "
                "board you meant, or the capture does not reach the line that prints "
                "the MAC (the ROM and the bootloader do not; the application does). "
                "Nothing was flashed; check the board before going on."
            )


def capture(args: argparse.Namespace, port: str) -> int:
    after = [parse_send_after(spec) for spec in args.send_after]
    at = [parse_send_at(spec) for spec in args.send_at]

    fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    buf = bytearray()
    sender: Sender | None = None
    try:
        configure(fd, args.baud)
        # An open asserts DTR+RTS, and RTS asserted holds EN low: the board
        # sits in reset until the next line runs.
        modem_set(fd, 0x0)
        time.sleep(0.05)
        print(f"  {port} @ {args.baud}, lines released: TIOCMGET = 0x{modem_get(fd):x}")
        print(f"  reset mode: {args.reset}")
        pulse(fd, args.reset)

        t0 = time.monotonic()
        sender = Sender(after, at, t0)
        deadline = t0 + args.seconds
        while True:
            now = time.monotonic()
            if now >= deadline:
                break
            ready, _, _ = select.select([fd], [], [], min(args.poll, deadline - now))
            if ready:
                try:
                    chunk = os.read(fd, 65536)
                except BlockingIOError:  # pragma: no cover - non-blocking race
                    chunk = b""
                if chunk:
                    buf.extend(chunk)
            now = time.monotonic()
            for text, why in sender.feed(bytes(buf), now):
                os.write(fd, (text + "\n").encode())
                print(f"  sent @{now - t0:6.3f}s ({why}): {text}")
    finally:
        # ⚠️ macOS tty close DRAINS (fact 2). Flush both directions first, on
        # every path, including this one.
        termios.tcflush(fd, termios.TCIOFLUSH)
        try:
            modem_set(fd, 0x0)
        except OSError:  # pragma: no cover - the port went away under us
            pass
        os.close(fd)
    print("  port closed, lines deasserted")

    write_outputs(args, bytes(buf))
    if sender is not None:
        for missed in sender.unfired():
            print(f"  ⚠️  NEVER FIRED: {missed}", file=sys.stderr)
    return 0


# --------------------------------------------------------------------------
# The self-test: everything above except the port itself
# --------------------------------------------------------------------------

#: The four transcripts committed under `lp-emu/transcripts/esp32v3/`, as
#: `(stem, raw start, decoded length)`. The offsets are not guesses: they are
#: where each committed `.txt` sits inside its committed `.raw.bin`, and they
#: are what `decodable_span` has to reproduce.
COMMITTED_SPANS = (
    ("silicon-esp32v3-2026-09-10-2e21b6226-115200", 0, 2236),
    ("silicon-esp32v3-2026-09-10-2e21b6226-921600", 6437, 6982),
    ("silicon-esp32v3-2026-09-10-75486b114-115200", 32, 2236),
    ("silicon-esp32v3-2026-09-10-75486b114-921600", 6454, 5427),
)


def self_test() -> int:
    """Everything that is not the cable, proved with no board and no port."""
    checks = 0

    def check(what: str, got: object, want: object) -> None:
        nonlocal checks
        if got != want:
            raise SystemExit(f"SELF-TEST FAILED: {what}\n  got  {got!r}\n  want {want!r}")
        checks += 1
        print(f"  ok  {what}")

    # 1. The reset sequences, as numbers rather than as prose.
    check("run = 0x4, 100 ms, 0x0", RESET_SEQUENCES["run"], ((0x4, 0.100), (0x0, 0.0)))
    check(
        "download = 0x4, 100 ms, 0x2, 80 ms, 0x0",
        RESET_SEQUENCES["download"],
        ((0x4, 0.100), (0x2, 0.080), (0x0, 0.0)),
    )
    check("none pulses nothing", RESET_SEQUENCES["none"], ())

    # 2. The port-name rule, against both locationIDs this cable has had.
    check("L0's locationID", port_for(0x00143330), "/dev/cu.wchusbserial143330")
    check("L1a's locationID", port_for(0x01133000), "/dev/cu.wchusbserial11330")

    # 3. The scripted-request grammar, in L1's own spellings — the trigger has
    #    no colon, the request text has two.
    stop_all = 'M!{"id":1,"msg":"stopAllProjects"}'
    check(
        "--send-after splits on the first two colons only",
        parse_send_after(f"[INIT] I/O task spawned:1:{stop_all}"),
        (b"[INIT] I/O task spawned", 0.001, stop_all),
    )
    check(
        "--send-at splits on the first colon only",
        parse_send_at('8.0:M!{"id":2,"msg":"stopAllProjects"}'),
        (8.0, 'M!{"id":2,"msg":"stopAllProjects"}'),
    )
    for bad in ("no-colons", "one:colon"):
        try:
            parse_send_after(bad)
        except ValueError:
            checks += 1
            print(f"  ok  --send-after refuses {bad!r}")
        else:
            raise SystemExit(f"SELF-TEST FAILED: --send-after accepted {bad!r}")

    # 4. The matcher, against a fixture byte stream. No port, no board.
    sender = Sender([(b"[INIT] I/O task spawned", 0.001, stop_all)], [(8.0, "later")], 0.0)
    check("nothing fires before the trigger", sender.feed(b"[INIT] fw-esp32v3 boot\n", 1.0), [])
    stream = b"[INIT] fw-esp32v3 boot\n[INIT] I/O task spawned\n"
    check("the trigger only arms", sender.feed(stream, 1.0), [])
    check(
        "and fires once the delay is up",
        sender.feed(stream, 1.002),
        [(stop_all, "after '[INIT] I/O task spawned'")],
    )
    check("and never twice", sender.feed(stream + b"[INIT] I/O task spawned\n", 2.0), [])
    check("--send-at fires on the clock", sender.feed(stream, 8.5), [("later", "at 8.0s")])
    check("nothing is left outstanding", sender.unfired(), [])
    check(
        "an unseen trigger is reported",
        Sender([(b"never", 0.0, "x")], [], 0.0).unfired(),
        ["after 'never': x"],
    )

    # 5. The span cutter, against the committed captures — the one part of
    #    this script whose real inputs are already in the tree.
    root = Path(__file__).resolve().parent.parent.parent
    base = root / "lp-emu" / "transcripts" / "esp32v3" / "boot-idle"
    if not base.is_dir():
        print(f"  SKIP the committed-capture checks: {base} is not here")
    else:
        for stem, start, length in COMMITTED_SPANS:
            raw = (base / "raw" / f"{stem}.raw.bin").read_bytes()
            txt = (base / f"{stem}.txt").read_bytes()
            check(f"{stem}: span", decodable_span(raw), (start, start + length))
            check(f"{stem}: is the committed transcript", raw[start : start + length], txt)

    check("an all-noise capture yields an empty span", decodable_span(b"\x80\x00\xff"), (0, 0))
    print(f"\nself-test: {checks} checks passed, no port opened")
    return 0


# --------------------------------------------------------------------------


def build_parser() -> argparse.ArgumentParser:
    ap = argparse.ArgumentParser(
        description=__doc__.splitlines()[0],
        epilog="See this file's header for the four bench facts it implements.",
    )
    ap.add_argument("--port", help="the device node; omit to resolve by USB identity")
    ap.add_argument(
        "--location",
        type=lambda s: int(s, 0),
        help="pin the CH340K by USB locationID (e.g. 0x1133000); required when "
        "more than one is on the bus",
    )
    ap.add_argument("--list", action="store_true", help="print every CH340K on the bus and exit")
    ap.add_argument("--self-test", action="store_true", help="check this script, open nothing")
    ap.add_argument(
        "--baud",
        type=int,
        help="115200 for the ROM banner and the second-stage bootloader, "
        "921600 for the application (one baud per capture)",
    )
    ap.add_argument("--seconds", type=float, help="how long to read for")
    ap.add_argument("--out", help="the raw read, written whole")
    ap.add_argument("--decoded", help="the decodable span of the raw read")
    ap.add_argument(
        "--reset",
        choices=tuple(RESET_SEQUENCES),
        default="run",
        help="run (boot), download (the ROM downloader), none (read only)",
    )
    ap.add_argument(
        "--download-mode",
        dest="reset",
        action="store_const",
        const="download",
        help="the same as --reset download",
    )
    ap.add_argument(
        "--no-reset",
        dest="reset",
        action="store_const",
        const="none",
        help="the same as --reset none",
    )
    ap.add_argument(
        "--send-after",
        action="append",
        default=[],
        metavar="TRIGGER:DELAY_MS:TEXT",
        help="send TEXT (plus a newline) DELAY_MS after TRIGGER first appears; repeatable",
    )
    ap.add_argument(
        "--send-at",
        action="append",
        default=[],
        metavar="SECONDS:TEXT",
        help="send TEXT SECONDS after the reset; repeatable",
    )
    ap.add_argument(
        "--poll",
        type=float,
        default=0.05,
        help="the read loop's poll interval, which bounds how soon after a "
        "trigger a --send-after can go out (default 0.05)",
    )
    ap.add_argument(
        "--expect-mac",
        help="fail unless this MAC appears in the capture — the board printing "
        "its own name is the confirmation a locationID cannot give",
    )
    return ap


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)

    if args.self_test:
        return self_test()

    if args.list:
        present = bridges()
        if not present:
            print(f"no CH340K ({CH34X_VID:04x}:{CH34X_PID:04x}) on the bus", file=sys.stderr)
        for loc, port in present:
            missing = "" if os.path.exists(port) else "   (NO DEVICE NODE)"
            print(f"loc=0x{loc:x}  {port}{missing}")
        return 0

    missing = [n for n in ("baud", "seconds", "out") if getattr(args, n) is None]
    if missing:
        build_parser().error("a capture needs " + ", ".join(f"--{n}" for n in missing))

    port = args.port or resolve_port(args.location)
    refuse_if_held(port)
    return capture(args, port)


if __name__ == "__main__":
    raise SystemExit(main())
