#!/usr/bin/env python3
"""The S3's update desk sitting on FS3 (S3 updates plan, M7 P11): the split
image and USB updates on silicon, and the spike's five silicon questions.

    scripts/ota/s3-desk-sitting.py usb  --as "<lease holder>" --images <dir> --out <dir> [--dry-run]
    scripts/ota/s3-desk-sitting.py cuts --as "<lease holder>" --images <dir> --out <dir> [--dry-run]

<dir> of --images holds `scripts/ota/build-image.sh --chip esp32s3` outputs:
`x`, `y`, `y-dies` (`fixture-trial-dies`) and `y-engine-dies`
(`fixture-engine-dies`). `--known-good` is the merged
image main ships for the S3 (`lp-cli firmware package esp32s3-8mb`'s
`*-merged.bin`; default: x's own package, a main build). The S3 boots
through the ESP-IDF door only, so there is one door to walk.

Two halves, because FS3's power and its USB arrive at different times:

  usb   — runs on any port. Known-good first; X with its boot transcript
          (Q-S1/Q-S2: the engine mapped at MMU entry 0x100 and running;
          Q-S3: the bootloader's banner; Q-S5: a requested reboot's code);
          push projects/test/basic; X -> Y over USB; read the boot records
          (Q-S4: attempted/started/confirmed written); an engine-less board
          healed from the host's copy (Q-S1: a heal remaps); X -> Y-dies
          rolled back and refused (Q-S5: a panic's code); an RTS reset
          (Q-S5); Y -> Y-engine-dies: core-only "engine keeps crashing",
          then a good build installed over it; known-good again before the
          lease is dropped.
  cuts  — needs a port whose power `board power-*` really switches. It
          first proves the cut: power off, the MAC must leave the bus (an LED
          panel that back-powers the XIAO keeps it there, and the half
          stops). Then Q-S5's power-on code, and real power cuts mid-core and
          mid-engine (`hw-power-cut.py --chip esp32s3` through
          `board power-cycle`).

Rules it keeps (AGENTS.md, the planning dir's fs3-queue.md): the board is
resolved by MAC, never a port; every step needs the lease (`board check`
as --as); flashing writes the PACKAGED image (bootloader, table, app — not
`lpfs`, which holds FS3's saved network and the other lanes' files); if FS3
leaves the bus and does not come back, the sitting stops and says so (hands
needed — never retry). `--dry-run` prints every command and runs none.

Everything lands in --out: one console per step and `summary.md`, the walk
record's raw material (configuration `silicon:esp32s3`). Nothing here is a
gate: a disagreement with the emulator is a fidelity defect to file.
"""

import argparse
import json
import re
import shlex
import struct
import subprocess
import sys
import time
import zlib
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
FS3_MAC = "D8:3B:DA:47:29:70"
FS3_SLUG = "fixture-s3"
# The emulator's bootloader banner (espflash 3.3's bundled IDF bootloader),
# what Q-S3 compares the board's against.
EMU_BOOTLOADER = ("ESP-IDF v5.1-beta1-378-gea5e0ff298-dirt 2nd stage bootloader",
                  "compile time Jun  7 2023 08:07:32")
RECORDS = 0x16000


class Sitting:
    def __init__(self, a):
        self.a = a
        self.out = Path(a.out)
        self.out.mkdir(parents=True, exist_ok=True)
        self.lines = []
        self.lp_cli = a.lp_cli

    # --- plumbing ---------------------------------------------------------------------------

    def note(self, text):
        print(text, flush=True)
        self.lines.append(text)

    def run(self, cmd, check=False, **kw):
        self.note("$ " + " ".join(shlex.quote(str(c)) for c in cmd))
        if self.a.dry_run:
            return subprocess.CompletedProcess(cmd, 0, "", "")
        r = subprocess.run([str(c) for c in cmd], capture_output=True, text=True, **kw)
        if check and r.returncode != 0:
            self.stop(f"`{cmd[0]}` failed ({r.returncode}): {(r.stdout + r.stderr)[-400:]}")
        return r

    def stop(self, why):
        self.note(f"\n**STOPPED** — {why}")
        self.write_summary()
        sys.exit(1)

    def port(self, wait=20.0):
        if self.a.dry_run:
            return "<port of " + self.a.mac + ">"
        end = time.time() + wait
        while time.time() < end:
            r = subprocess.run([sys.executable, str(REPO / "scripts/emu/board-port.py"), self.a.mac],
                               capture_output=True, text=True)
            p = r.stdout.strip()
            if r.returncode == 0 and p.startswith("/dev/"):
                return p
            time.sleep(0.5)
        return None

    def board_port(self, step):
        p = self.port()
        if not p:
            self.stop(f"{step}: FS3 ({self.a.mac}) is not on USB and did not come back — hands "
                      "needed (fs3-queue.md rule 5: message the director, do not retry)")
        return p

    def leased(self):
        r = self.run(["board", "check", FS3_SLUG, "--as", self.a.holder])
        if r.returncode != 0:
            self.stop(f"the lease: `board check {FS3_SLUG} --as …` said {r.returncode} "
                      f"({(r.stdout + r.stderr).strip()[-200:]}). Queue first: "
                      f"board take {FS3_SLUG} --as \"{self.a.holder}\" --for \"M7 P11\" --wait --timeout 240")

    def capture(self, name, seconds, exit_on=None, request=None, offer=None, heal_cache=None):
        port = self.board_port(name)
        console = self.out / f"{name}.txt"
        cmd = [self.lp_cli, "link", "capture", port, "--console", console, "--seconds", seconds]
        if exit_on:
            cmd += ["--exit-on", exit_on]
        if request:
            cmd += ["--request", request]
        if offer:
            cmd += ["--ota-offer", offer]
        if heal_cache:
            cmd += ["--ota-heal-only", "--ota-cache", heal_cache]
        self.run(cmd)
        return "" if self.a.dry_run else console.read_text(errors="replace")

    def flash(self, name, image):
        port = self.board_port(name)
        self.run(["espflash", "write-bin", "--chip", "esp32s3", "--port", port, "0x0", image],
                 check=True)

    def expect(self, step, text, needle, what):
        if self.a.dry_run:
            self.note(f"- {step}: would check — {what} (`{needle}`)")
            return True
        ok = needle in text
        self.note(f"- {step}: {'PASS' if ok else 'MISSING'} — {what} (`{needle}`)")
        return ok

    def write_summary(self):
        (self.out / "summary.md").write_text(
            f"# FS3 sitting ({time.strftime('%Y-%m-%d %H:%M')}) — silicon:esp32s3\n\n"
            "FS3 (XIAO ESP32-S3 Plus, 16 MB), antenna attached.\n\n"
            + "\n".join(self.lines) + "\n")

    # --- images ---------------------------------------------------------------------------

    def image(self, name):
        d = Path(self.a.images) / name
        split = json.loads((d / "split.json").read_text())
        if split.get("chip") != "esp32s3":
            self.stop(f"{d} is not an esp32s3 split")
        package = next((d / "package").glob("*-merged.bin"))
        return d, split["buildId"], package

    def y_cache(self, y_dir):
        """A host cache holding Y's engine, what a heal serves from."""
        cache = self.out / "y-cache"
        cache.mkdir(exist_ok=True)
        manifest = json.loads((y_dir / "ota/ota-manifest.json").read_text())
        (cache / f"{manifest['engine']['sha256']}.bin").write_bytes(
            (y_dir / "ota/engine.bin").read_bytes())
        return cache

    def hello_of(self, text, build):
        for line in reversed(text.splitlines()):
            if '"hello":{' in line and f'"buildId":"{build}"' in line:
                return line
        return ""

    # --- the records (Q-S4) ---------------------------------------------------------------

    def records(self, step):
        port = self.board_port(step)
        out = self.out / f"{step}.records.bin"
        # One packet in flight: espflash 3.3's default window (64) fails reads
        # on FS3 (docs/defects/2026-10-25-espflash-pipelined-reads-fail-on-the-s3s-usb.md).
        self.run(["espflash", "read-flash", "--chip", "esp32s3", "--port", port,
                  "--max-in-flight", "1", hex(RECORDS), "0x2000", out], check=True)
        # After a read FS3 has been seen sitting in the ROM, silent, until a
        # host reset: put it back in its firmware.
        self.run(["espflash", "reset", "--chip", "esp32s3", "--port", self.board_port(step)])
        if self.a.dry_run:
            return
        data = out.read_bytes()
        for i in range(2):
            rec = data[i * 0x1000: i * 0x1000 + 44]
            magic, version, flags, seq, core_off, core_len, build, crc = struct.unpack(
                "<IHHIIIII", rec[:28])
            if magic != int.from_bytes(b"LPBR", "little") or zlib.crc32(rec[:24]) != crc:
                self.note(f"- {step}: record {i}: none")
                continue
            # A mark reads set when any bit of its word is cleared.
            mark = lambda off: int.from_bytes(rec[off:off + 4], "little") != 0xFFFF_FFFF
            tally = bin(~int.from_bytes(rec[40:44], "little") & 0xFFFF_FFFF).count("1")
            self.note(f"- {step}: record {i}: seq {seq} core @{core_off:#x} +{core_len} "
                      f"build {build:#010x} {'trial' if flags & 1 else 'proven'} · "
                      f"attempted {mark(28)} confirmed {mark(32)} started {mark(36)} "
                      f"cold tally {tally}")

    # --- the halves -----------------------------------------------------------------------

    def usb(self):
        a = self.a
        steps = set(a.steps.split(","))
        x_dir, x_build, x_pkg = self.image("x")
        y_dir, y_build, _ = self.image("y")
        yd_dir, yd_build, _ = self.image("y-dies")
        ye_dir, ye_build, _ = self.image("y-engine-dies") if "engine-dies" in steps else (None, None, None)
        known_good = Path(a.known_good) if a.known_good else x_pkg
        self.note(f"## The USB half — X {x_build}, Y {y_build}, Y-dies {yd_build}\n")
        self.leased()

        self.note("\n### Known-good first (fs3-queue.md rule 2)")
        self.flash("known-good", known_good)
        text = self.capture("known-good", 60, exit_on='"hello":{')
        self.expect("known-good", text, '"hello":{', "the known-good image says hello")

        if "boot" in steps:
            self.note("\n### X, and its boot transcript (Q-S1, Q-S2, Q-S3, Q-S5 requested reboot)")
            self.flash("x", x_pkg)
            self.capture("x-first", 60, exit_on='"hello":{')
            # A requested reboot: the whole boot from the ROM, on this console
            # (USB-Serial-JTAG stays up across a software reset). No --exit-on:
            # the hello before the reboot would match any line about X.
            text = self.capture("x-reboot", 25, request="reboot")
            self.expect("boot", text, "[LOADER] core @0x20000 (proven)", "the loader boots X's core")
            self.expect("boot", text, '"state":"running"', "an engine says hello (Q-S1, Q-S2)")
            for word in EMU_BOOTLOADER:
                self.expect("Q-S3", text, word, "the bootloader is espflash 3.3's bundled one")
            self.expect("Q-S5", text, "rst:0x3 ", "a requested reboot reads RTC_SW_SYS_RST (warm)")

        if "push" in steps:
            self.note("\n### A project (the other lanes' files stay in lpfs; this adds `basic`)")
            port = self.board_port("push")
            self.run([self.lp_cli, "upload", REPO / "projects/test/basic", f"serial:{port}"])

        if "update" in steps:
            self.note("\n### X -> Y over USB (U6)")
            text = self.capture("x-to-y", 900, offer=y_dir / "ota", exit_on="[host-ota] done:")
            self.expect("update", text, "done: UpToDate", "the host's update ends UpToDate")
            text = self.capture("y-hello", 60, exit_on=f'"buildId":"{y_build}"')
            hello = self.hello_of(text, y_build)
            self.expect("update", hello, '"state":"running"', "Y's engine says hello")
            self.records("update")

        if "heal" in steps:
            self.note("\n### An engine-less board healed from the host's copy (Q-S1: a heal remaps)")
            self.run([REPO / "scripts/ota/make-engine-less.sh", "--chip", "esp32s3", a.mac],
                     check=True, env={**__import__("os").environ, "BOARD_HOLDER": a.holder})
            text = self.capture("heal", 600, offer=y_dir / "ota", heal_cache=self.y_cache(y_dir),
                                exit_on="[host-ota] done:")
            self.expect("heal", text, "[OTA] core-only: no engine", "the board came up core-only")
            text = self.capture("heal-hello", 60, exit_on=f'"buildId":"{y_build}"')
            self.expect("heal", self.hello_of(text, y_build), '"state":"running"',
                        "Y's engine runs again")

        if "dies" in steps:
            self.note("\n### Y -> Y-dies: rolled back and refused (Q-S5: a panic)")
            text = self.capture("y-to-dies", 900, offer=yd_dir / "ota",
                                exit_on="[host-ota] done:")
            self.expect("dies", text, "fixture-trial-dies", "the trial core panics")
            self.expect("dies", text, "(rolled back)", "the loader rolls back")
            self.expect("dies", text, "RefusedBuild", "the host stops at a refused build")
            self.expect("Q-S5", text, "rst:0x3 ", "a panic's software reset reads 0x3 (warm)")
            # Heal Y back from the read-back the host kept.
            text = self.capture("dies-heal", 600, offer=y_dir / "ota",
                                heal_cache=self.y_cache(y_dir), exit_on="[host-ota] done:")
            text = self.capture("dies-hello", 60, exit_on=f'"buildId":"{y_build}"')
            self.expect("dies", self.hello_of(text, y_build), '"state":"running"',
                        "Y runs again after the heal")
            self.records("dies")

        if "engine-dies" in steps:
            self.note("\n### Y -> Y-engine-dies: an engine that keeps crashing stays reachable")
            text = self.capture("y-to-engine-dies", 900, offer=ye_dir / "ota",
                                exit_on="[host-ota] done:")
            self.expect("engine-dies", text, "fixture-engine-dies", "the new engine panics")
            self.expect("engine-dies", text, "engine keeps crashing (4 incomplete boots)",
                        "the core stays core-only after four incomplete boots")
            self.expect("engine-dies", text, "ReportCrashing", "the host reports it, no heal")
            text = self.capture("engine-dies-offer-y", 900, offer=y_dir / "ota",
                                exit_on="[host-ota] done:")
            self.expect("engine-dies", text, "done: UpToDate", "a good build installs over it")

        if "rts" in steps:
            self.note("\n### An RTS reset from the host (Q-S5)")
            port = self.board_port("rts")
            self.run(["espflash", "reset", "--chip", "esp32s3", "--port", port])
            text = self.capture("rts", 60, exit_on='"heartbeat"')
            # The ROM's `rst:` line goes out before the capture opens the port.
            self.expect("Q-S5", text, '"resetReason":"user-reset"',
                        "a host chip reset reads user-reset (USB_UART/JTAG_CHIP_RESET, cold)")

        self.note("\n### Known-good again before the lease is dropped (fs3-queue.md rule 4)")
        self.flash("restore", known_good)
        text = self.capture("restore", 60, exit_on='"hello":{')
        self.expect("restore", text, '"hello":{', "FS3 is back on the known-good image")
        self.note(f"\nDrop the lease: board drop {FS3_SLUG} --as \"{a.holder}\"")
        self.write_summary()

    def cuts(self):
        a = self.a
        x_dir, _, _ = self.image("x")
        y_dir, y_build, _ = self.image("y")
        self.note("## The power-cut half\n")
        self.leased()
        self.note("### Does the port cut FS3's power? (and does the LED panel back-power it?)")
        self.run(["board", "power-off", FS3_SLUG, "--as", a.holder], check=True)
        gone = self.a.dry_run
        if not self.a.dry_run:
            time.sleep(3)
            gone = self.port(wait=2) is None
        self.run(["board", "power-on", FS3_SLUG, "--as", a.holder], check=True)
        self.board_port("power-on")
        if not gone:
            self.stop("FS3 stayed on the bus with its port powered off: this port does not cut "
                      "it, or the LED panel back-powers the XIAO. Power cuts would be fiction.")
        self.note("- the port cuts FS3's power: "
                  + ("would check (it must leave the bus)" if self.a.dry_run else "PASS (it left the bus)"))
        text = self.capture("power-on", 60, exit_on='"hello":{')
        self.expect("Q-S5", text, "rst:0x1 ", "a power cut reads POWERON (cold)")
        cuts = a.cuts or "0.5,3,8,15,25,40,60,90"
        cmd = [sys.executable, REPO / "scripts/ota/hw-power-cut.py", "--chip", "esp32s3",
               "--mac", a.mac, "--x", x_dir, "--y", y_dir, "--out", self.out / "cuts",
               "--lp-cli", self.lp_cli, "--cuts", cuts, "--power-cycle-cmd",
               f'board power-cycle {FS3_SLUG} --as "{a.holder}" --off-secs 2']
        r = self.run(cmd)
        self.note(r.stdout[-1500:] if r.stdout else "")
        self.write_summary()


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("half", choices=["usb", "cuts"])
    ap.add_argument("--as", dest="holder", required=True, help="the lease holder (your session)")
    ap.add_argument("--images", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--known-good", help="the main-built merged image to start and end on")
    ap.add_argument("--mac", default=FS3_MAC)
    ap.add_argument("--lp-cli", default=str(REPO / "target/release/lp-cli"))
    ap.add_argument("--steps", default="boot,push,update,heal,dies,engine-dies,rts",
                    help="the usb half's steps, comma-separated")
    ap.add_argument("--cuts", help="seconds after the update starts (the cuts half)")
    ap.add_argument("--dry-run", action="store_true", help="print the commands, run none")
    a = ap.parse_args()
    s = Sitting(a)
    getattr(s, a.half)()


if __name__ == "__main__":
    main()
