#!/usr/bin/env python3
"""ESP32-C6 LP analog I2C clock — the bench tool behind the first-flash fix.

A factory-fresh C6 (Seeed XIAO) runs an ESP-IDF app that gates LPPERI_CLK_EN
bit 29, the LP analog I2C clock. The second-stage bootloader in our merged
image drives the analog bus through the LP aperture, and every reset a
flasher can send is HP-only, so after the first LightPlayer flash it hangs at
0x4086ed7a until someone replugs the board. Studio's flashers now restore the
clock before their closing reset (lpa-link, P1 of the plan); this tool is how
that state is induced, inspected and undone on the desk, and how the
detection copy is exercised.

    scripts/c6-lp-ana-i2c.py induce <port>            # download mode, clear bit 29
    scripts/c6-lp-ana-i2c.py fix <port> [--connect default-reset]
    scripts/c6-lp-ana-i2c.py dump <port> <label>      # registers -> <label>.txt
    scripts/c6-lp-ana-i2c.py watch <port>             # Studio's normal reset, then boot lines

Runs under esptool's own Python (it needs the esptool package): the shebang
is generic, so invoke it as `"$ESPTOOL_PYTHON" scripts/c6-lp-ana-i2c.py …`
or let scripts/c6-bootloader-hang-walk.sh resolve the interpreter next to the
`esptool` binary (Homebrew: <cellar>/esptool/<v>/libexec/bin/python).

Facts that bite (all bench, 2026-09-06):
- `induce`/`fix` talk to the ROM/stub in download mode. Closing the port
  afterwards toggles DTR/RTS and REBOOTS a USB-Serial-JTAG chip — after
  `induce` that reboot is the hang.
- Opening the port with pyserial resets the chip the same way; `watch` does
  it on purpose, `dump` uses connect_mode no-reset and expects the chip to be
  in download mode already.
- Setting the clock alone does not clear a latched busy; `fix` also pulses
  LPPERI_RESET_EN bit 29.
- The TG0 watchdog loop is not guaranteed — the induced hang is often
  silent. The reliable signature is a ROM boot line whose `Saved PC` lies in
  the bootloader's code segments (0x4086e610..0x40871378 for the shipped
  bootloader).

See docs/defects/2026-09-06-c6-first-flash-bootloader-hang-lp-analog-i2c-clock.md.
"""

import argparse
import os
import sys
import time

LPPERI_CLK_EN = 0x600B2800
LPPERI_RESET_EN = 0x600B2804
LP_ANA_I2C_BIT = 1 << 29
LP_I2C_ANA_MST_CTRL = 0x600B2400
LP_I2C_ANA_MST_BUSY = 1 << 25

# LP-domain / clock registers worth diffing before and after a power cycle.
DUMP_RANGES = [
    ("PMU", 0x600B0000, 0x600B0200),
    ("LP_CLKRST", 0x600B0400, 0x600B0420),
    ("LP_AON", 0x600B1000, 0x600B1040),
    ("LP_I2C_ANA_MST", 0x600B2400, 0x600B2418),
    ("LP_I2C_ANA_MST.date", 0x600B27FC, 0x600B2800),
    ("LPPERI", 0x600B2800, 0x600B2810),
    ("LP_ANA_PERI", 0x600B2C00, 0x600B2C40),
    ("MODEM_LPCON", 0x600AF000, 0x600AF030),
    ("I2C_ANA_MST(hp)", 0x600AF800, 0x600AF830),
    ("MODEM_SYSCON", 0x600A9800, 0x600A9840),
    ("PCR", 0x60096000, 0x60096140),
]


def connect(port, mode):
    try:
        from esptool.cmds import detect_chip
    except ImportError:
        sys.exit(
            "esptool is not importable: run this under esptool's own Python "
            "(see the module docstring) or `pip install esptool`."
        )
    return detect_chip(port, 115200, connect_mode=mode, connect_attempts=5)


def show(esp, tag):
    clk = esp.read_reg(LPPERI_CLK_EN)
    rst = esp.read_reg(LPPERI_RESET_EN)
    ctrl = esp.read_reg(LP_I2C_ANA_MST_CTRL)
    busy = bool(ctrl & LP_I2C_ANA_MST_BUSY)
    print(
        f"{tag}: LPPERI_CLK_EN={clk:08x} lp_ana_i2c_ck_en={(clk >> 29) & 1} "
        f"RESET_EN={rst:08x} LP_I2C_ANA_MST.ctrl={ctrl:08x} busy={busy}"
    )
    return clk, ctrl


def cmd_induce(args):
    esp = connect(args.port, args.connect)
    clk, _ = show(esp, "before")
    esp.write_reg(LPPERI_CLK_EN, clk & ~LP_ANA_I2C_BIT)
    show(esp, "after")
    print("# closing the port reboots the chip; the bootloader will now hang")


def cmd_fix(args):
    esp = connect(args.port, args.connect)
    clk, ctrl = show(esp, "before")
    if clk & LP_ANA_I2C_BIT and not ctrl & LP_I2C_ANA_MST_BUSY:
        print("clean: clock on, master idle — nothing to do")
        return
    esp.write_reg(LPPERI_CLK_EN, clk | LP_ANA_I2C_BIT)
    rst = esp.read_reg(LPPERI_RESET_EN)
    esp.write_reg(LPPERI_RESET_EN, rst | LP_ANA_I2C_BIT)
    time.sleep(0.01)
    esp.write_reg(LPPERI_RESET_EN, rst & ~LP_ANA_I2C_BIT)
    time.sleep(0.05)
    _, ctrl = show(esp, "after")
    if ctrl & LP_I2C_ANA_MST_BUSY:
        sys.exit("busy still set after the reset pulse — the board needs a replug")
    print("fixed: the next reset boots the app")


def cmd_dump(args):
    esp = connect(args.port, args.connect)
    lines = [f"# chip {esp.CHIP_NAME} {esp.get_chip_description()}"]
    for name, lo, hi in DUMP_RANGES:
        lines.append(f"## {name}")
        for addr in range(lo, hi, 4):
            try:
                lines.append(f"{addr:08x} {esp.read_reg(addr):08x}")
            except Exception as error:  # noqa: BLE001 — a dump keeps going
                lines.append(f"{addr:08x} ERR {error}")
    text = "\n".join(lines) + "\n"
    out = f"{args.label}.txt"
    with open(out, "w", encoding="utf-8") as handle:
        handle.write(text)
    print(text, end="")
    print(f"# wrote {out}")


def cmd_watch(args):
    try:
        import serial
    except ImportError:
        sys.exit("pyserial is not importable: run under esptool's Python")
    port = args.port
    with serial.Serial(port, 115200, timeout=0.2) as tty:
        # Studio's "normal" reset: D0 W100 R1 W100 R0.
        tty.dtr = False
        time.sleep(0.1)
        tty.rts = True
        time.sleep(0.1)
        tty.rts = False
    print("# reset sent; waiting for the port to re-enumerate")
    deadline = time.time() + 8
    tty = None
    while time.time() < deadline and tty is None:
        try:
            tty = serial.Serial(port, 115200, timeout=0.2)
            tty.dtr = False
            tty.rts = False
        except Exception:  # noqa: BLE001 — not back yet
            time.sleep(0.2)
    if tty is None:
        sys.exit(f"{port} did not come back within 8 s")
    end = time.time() + args.seconds
    buf = b""
    while time.time() < end:
        buf += tty.read(4096)
    tty.close()
    text = buf.decode("utf-8", "replace")
    print(text)
    saved = [line for line in text.splitlines() if line.startswith("Saved PC:")]
    hung = [line for line in saved if in_bootloader(line)]
    if hung:
        print(f"# HUNG BOOTLOADER: {hung[-1]}")
        sys.exit(2)
    if "fw-esp32 initialized, starting server loop" in text or "M!{" in text:
        print("# app booted")
        return
    print("# no app output — inconclusive")
    sys.exit(3)


BOOTLOADER_CODE_RANGES = [(0x4086E610, 0x40871378), (0x40875720, 0x40876F20)]


def in_bootloader(saved_pc_line):
    try:
        pc = int(saved_pc_line.split(":", 1)[1].strip(), 16)
    except ValueError:
        return False
    return any(lo <= pc < hi for lo, hi in BOOTLOADER_CODE_RANGES)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)

    induce = sub.add_parser("induce", help="enter download mode and gate the LP analog I2C clock")
    induce.add_argument("port")
    induce.add_argument("--connect", default="default-reset", choices=["default-reset", "no-reset"])
    induce.set_defaults(run=cmd_induce)

    fix = sub.add_parser("fix", help="restore the clock and pulse the master's reset")
    fix.add_argument("port")
    fix.add_argument("--connect", default="no-reset", choices=["default-reset", "no-reset"])
    fix.set_defaults(run=cmd_fix)

    dump = sub.add_parser("dump", help="dump LP-domain/clock registers (chip already in download mode)")
    dump.add_argument("port")
    dump.add_argument("label")
    dump.add_argument("--connect", default="no-reset", choices=["default-reset", "no-reset"])
    dump.set_defaults(run=cmd_dump)

    watch = sub.add_parser("watch", help="Studio's normal reset, then print boot output")
    watch.add_argument("port")
    watch.add_argument("--seconds", type=float, default=6.0)
    watch.set_defaults(run=cmd_watch)

    args = parser.parse_args()
    if not os.path.exists(args.port):
        sys.exit(f"{args.port} does not exist (scripts/emu/board-port.py <MAC> resolves it)")
    args.run(args)


if __name__ == "__main__":
    main()
