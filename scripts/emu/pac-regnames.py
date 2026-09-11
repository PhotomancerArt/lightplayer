#!/usr/bin/env python3
"""Generate `RegNames` tables from an Espressif PAC: register names and resets.

A bus log that says `UART0+0x01c` has to be decoded by hand against a PAC.
One that says `UART0+0x01c status` can be read. The names come from the same
place the M3 register inventory came from — the `#[doc = "0xNN - ..."]`
comments svd2rust writes above each register accessor — so they are derived
data, not transcription, and each generated file carries the provenance
header `docs/adr/2026-07-29-license-provenance-discipline.md` requires.

Each table also carries two more things svd2rust states per register, from
the same source and under the same provenance and `--check` lint:

- the **non-zero reset values**, from
  `impl crate::Resettable for <REG>_SPEC { const RESET_VALUE: u32 = … }`
  (an empty impl means zero). A `RegFile` seeds itself from them, so an
  accept block reads what the part reads before anyone writes it rather than
  what a boot was observed to need — the sweep
  `docs/defects/2026-09-07-accept-blocks-carry-only-the-reset-values-a-boot-needed.md`
  asked for.
- the **access**, from which of `impl crate::Readable` / `impl
  crate::Writable` the register has. Only the registers that are not plain
  read-write are listed. This is what lets an accept block grade itself:
  accept-and-remember IS the documented behaviour of a read-write register,
  and is a stand-in for hardware on a read-only one.

Two modes:

    scripts/emu/pac-regnames.py            regenerate the esp32c6's tables
    scripts/emu/pac-regnames.py --pac esp32
                                           regenerate the classic's tables
    scripts/emu/pac-regnames.py --pac esp32s3
                                           regenerate the S3's tables
    scripts/emu/pac-regnames.py --check    fail if ANY chip's table is stale

`just lint-emu-regnames` runs `--check`, so a hand edit is caught the way
`lint-vec-corpus` catches one in the shader corpus.

**One script, several chips.** [`CHIPS`] names each machine crate's PAC, its
block table, and the blocks that must NOT be emitted for it. `--pac` selects
one for a regenerate run and defaults to `esp32c6`, so no existing invocation
changes; `--check` ignores it and checks them all, because a lint that
depended on which chip you named would pass for the chip you were not
working on.

Source resolution, in order:

    1. an unpacked crate under $CARGO_HOME/registry/src/*/<pac>-<version>/
    2. the `.crate` tarball under $CARGO_HOME/registry/cache/*/
    3. `cargo fetch --locked`, then 1 and 2 again

If none of those produce the PAC, `--check` says so loudly and exits 0 (a
lint that wedges a CI runner which has never built a device-target crate is
worse than one that says "not checked here"), while a regenerate run fails:
writing a table from a source you could not read is the one outcome nobody
wants.
"""

from __future__ import annotations

import argparse
import glob
import io
import os
import re
import subprocess
import sys
import tarfile
from dataclasses import dataclass

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))

PAC_REPO = "https://github.com/esp-rs/esp-pacs"
PAC_LICENSE = "MIT OR Apache-2.0"
VENDORED_LICENSE = "licenses/esp-pacs-MIT.txt"


@dataclass(frozen=True)
class Target:
    """One generated table."""

    block: str  # PAC register-block module, e.g. "uart0"
    static: str  # the Rust `static` name, e.g. "UART0"
    out: str  # repo-relative output path


# The blocks generated today for the **C6**.
#
# The first two live in `lp-emu-esp-common/tests/` and are P3's proof that
# the generator handles both shapes: a flat block (uart0) and one with
# register arrays and clusters (systimer). They are fixtures, not tables the
# emulator uses.
#
# The rest live in the CHIP crate, because a register layout is chip-family
# data and the common crate holds no chip numbers. P4 adds the two blocks its
# boot trace actually names — `lp_clkrst`, whose `reset_cause` at +0x10 the
# mask ROM reads before `.bss` is zeroed, and `interrupt_core0`, whose 77 map
# entries `_setup_interrupts` writes. P5/P6/M4 add a row each as they model a
# block.
C6_TARGETS = [
    Target(
        block="uart0",
        static="UART0",
        out="lp-emu/esp/lp-emu-esp-common/tests/regs/uart0.rs",
    ),
    Target(
        block="systimer",
        static="SYSTIMER",
        out="lp-emu/esp/lp-emu-esp-common/tests/regs/systimer.rs",
    ),
    Target(
        block="lp_clkrst",
        static="LP_CLKRST",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/lp_clkrst.rs",
    ),
    Target(
        block="interrupt_core0",
        static="INTERRUPT_CORE0",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/interrupt_core0.rs",
    ),
    # P5: every block the no-radio boot path touches (modelled or accept).
    Target(
        block="plic_mx",
        static="PLIC_MX",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/plic_mx.rs",
    ),
    Target(
        block="plic_ux",
        static="PLIC_UX",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/plic_ux.rs",
    ),
    # M7: the two SDIO-slave blocks the mask ROM touches on its way to the
    # flash bootloader, and nothing else ever does.
    Target(
        block="hinf",
        static="HINF",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/hinf.rs",
    ),
    Target(
        block="slc",
        static="SLC",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/slc.rs",
    ),
    Target(
        block="sha",
        static="SHA",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/sha.rs",
    ),
    Target(
        block="lp_ana",
        static="LP_ANA",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/lp_ana.rs",
    ),
    Target(
        block="intpri",
        static="INTPRI",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/intpri.rs",
    ),
    Target(
        block="systimer",
        static="SYSTIMER",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/systimer.rs",
    ),
    Target(
        block="timg0",
        static="TIMG0",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/timg0.rs",
    ),
    Target(
        block="lp_wdt",
        static="LP_WDT",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/lp_wdt.rs",
    ),
    Target(
        block="pcr",
        static="PCR",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/pcr.rs",
    ),
    Target(
        block="pmu",
        static="PMU",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/pmu.rs",
    ),
    Target(
        block="lp_aon",
        static="LP_AON",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/lp_aon.rs",
    ),
    Target(
        block="lp_apm",
        static="LP_APM",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/lp_apm.rs",
    ),
    Target(
        block="lp_apm0",
        static="LP_APM0",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/lp_apm0.rs",
    ),
    Target(
        block="hp_apm",
        static="HP_APM",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/hp_apm.rs",
    ),
    Target(
        block="modem_syscon",
        static="MODEM_SYSCON",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/modem_syscon.rs",
    ),
    Target(
        block="modem_lpcon",
        static="MODEM_LPCON",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/modem_lpcon.rs",
    ),
    Target(
        block="i2c_ana_mst",
        static="I2C_ANA_MST",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/i2c_ana_mst.rs",
    ),
    Target(
        block="lp_i2c_ana_mst",
        static="LP_I2C_ANA_MST",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/lp_i2c_ana_mst.rs",
    ),
    Target(
        block="apb_saradc",
        static="APB_SARADC",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/apb_saradc.rs",
    ),
    Target(
        block="hp_sys",
        static="HP_SYS",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/hp_sys.rs",
    ),
    Target(
        block="tee",
        static="TEE",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/tee.rs",
    ),
    Target(
        block="lp_tee",
        static="LP_TEE",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/lp_tee.rs",
    ),
    Target(
        block="lp_io",
        static="LP_IO",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/lp_io.rs",
    ),
    Target(
        block="gpio",
        static="GPIO",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/gpio.rs",
    ),
    Target(
        block="io_mux",
        static="IO_MUX",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/io_mux.rs",
    ),
    Target(
        block="assist_debug",
        static="ASSIST_DEBUG",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/assist_debug.rs",
    ),
    Target(
        block="extmem",
        static="EXTMEM",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/extmem.rs",
    ),
    Target(
        block="usb_device",
        static="USB_DEVICE",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/usb_device.rs",
    ),
    Target(
        block="spi0",
        static="SPI0",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/spi0.rs",
    ),
    Target(
        block="spi1",
        static="SPI1",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/spi1.rs",
    ),
    Target(
        block="efuse",
        static="EFUSE",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/efuse.rs",
    ),
    Target(
        block="lp_peri",
        static="LP_PERI",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/lp_peri.rs",
    ),
    Target(
        block="uart0",
        static="UART0",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/uart0.rs",
    ),
    Target(
        block="lp_timer",
        static="LP_TIMER",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/lp_timer.rs",
    ),
    Target(
        block="rmt",
        static="RMT",
        out="lp-emu/esp/lp-emu-esp32c6/src/regs/rmt.rs",
    ),
]


def _v3(block: str, static: str) -> Target:
    return Target(
        block=block,
        static=static,
        out=f"lp-emu/esp/lp-emu-esp32v3/src/regs/{block}.rs",
    )


# The blocks generated for the **classic ESP32 (v3)**, from the inventory in
# `m3/notes.md` §4: every block the shipped image's boot path can reach,
# modelled or accepted, plus RMT — generated now so M4 adds a peripheral view
# rather than touching this script.
#
# Three of these serve more than one peripheral, because the PAC gives them
# one `RegisterBlock` type each: `spi0` is SPI0/SPI1/SPI2/SPI3, `timg0` is
# TIMG0/TIMG1, `uart0` is UART0/UART1/UART2. One table, several bases.
ESP32_TARGETS = [
    # The system block: cache control, the per-core interrupt maps, the
    # clock/reset gates, and core 1's stall (P4).
    _v3("dport", "DPORT"),
    # Reset cause, the two halves of the CPU stall key, and the RWDT (P5).
    _v3("rtc_cntl", "RTC_CNTL"),
    _v3("rtc_io", "RTC_IO"),
    _v3("sens", "SENS"),
    _v3("rtc_i2c", "RTC_I2C"),
    # The esp-rtos tick, and LACT as the classic's clock (P5).
    _v3("timg0", "TIMG0"),
    # The console: the ROM banner at 115200, the app at 921600 (P6).
    _v3("uart0", "UART0"),
    # The flash controller and the cache's own port (P7).
    _v3("spi0", "SPI0"),
    # The IDF bootloader hashes the app image with it (P7).
    _v3("sha", "SHA"),
    # MAC and chip revision (P5).
    _v3("efuse", "EFUSE"),
    # The 40-pad fabric (P8).
    _v3("gpio", "GPIO"),
    _v3("io_mux", "IO_MUX"),
    _v3("apb_ctrl", "APB_CTRL"),
    # WDEV_RND_REG, on the AHB bus (P3 found the window; P7 models the block).
    _v3("rng", "RNG"),
    _v3("frc_timer", "FRC_TIMER"),
    # ⚠️ **The RNG's entropy path, not an audio block.** The ESP-IDF
    # second-stage bootloader's `bootloader_random_enable()` drives I2S0's
    # ADC-sampling mode to stir the hardware RNG, so a ROM-up boot reaches
    # `0x3FF4_F0B0` five lines into the bootloader's log (P7). Nothing on
    # this chip's audio path is modelled and nothing here suggests it is.
    _v3("i2s0", "I2S0"),
    # M4's peripheral, generated here so M4 does not touch this script.
    _v3("rmt", "RMT"),
]


def _s3(block: str, static: str) -> Target:
    return Target(
        block=block,
        static=static,
        out=f"lp-emu/esp/lp-emu-esp32s3/src/regs/{block}.rs",
    )


# The blocks generated for the **ESP32-S3**, from M6 P01's static inventory of
# the shipped `fw-esp32s3` image
# (`docs/reports/2026-09-11-esp32s3-firmware-inventory.md` §6): every block the
# image's own MMIO census names, plus the four the ROM-up path will reach that
# the application never does.
#
# The census is the reason this list is not the C6's: the S3 image touches
# twenty-two blocks and none of them is a UART. `uart0` and `sha` are here
# anyway because the mask ROM's console and the IDF bootloader's image hash are
# the ROM-up path's, and a phase that met either would otherwise stop to
# regenerate this script rather than to model a block.
#
# Two tables serve more than one peripheral, because the PAC gives them one
# `RegisterBlock` type each: `timg0` is TIMG0 (`0x6001_f000`) and TIMG1
# (`0x6002_0000`), and `uart0` is UART0/UART1/UART2. `spi0` and `spi1` are
# separate modules on this chip, unlike the classic's single `spi0`.
#
# ⚠️ `interrupt_core0` and `interrupt_core1` are two PAC modules at ONE base —
# `esp32s3-0.35.2/src/lib.rs` gives both `0x600c_2000`, and that is not an SVD
# leak: they are two halves of one 4 KB window, core 0's registers at `+0x000`
# and core 1's at `+0x800`, which is why the generated `interrupt_core1` table
# starts at `+0x800` rather than at zero. Both are generated because the
# register names differ per core and a trace that read one half against the
# other's table would name every register wrongly.
ESP32S3_TARGETS = [
    # The clock gates, the reset gates, core 1's control, and the four
    # software interrupts (`cpu_intr_from_cpu[0..4]` at `0x30..0x40`, M6
    # notes §3.3) — 74 literal sites, the image's busiest block.
    _s3("system", "SYSTEM"),
    # Reset cause, the voltage/clock path esp-hal's `init` walks, and the
    # **RWDT the shipped image arms and feeds on every boot** (M6 notes §5.2).
    _s3("rtc_cntl", "RTC_CNTL"),
    # The esp-rtos tick (TIMG0) and the second group the boot path configures.
    _s3("timg0", "TIMG0"),
    # `esp_rtos::now` and esp-hal's S3 time driver. The classic has no
    # SYSTIMER; this chip's is the C6's shape.
    _s3("systimer", "SYSTIMER"),
    # The link. On the S3 the console is `jtag-serial` and there is no
    # `spike_uart0_link`, so this block is the only way bytes leave the chip
    # on the application path (M6 notes §5.3).
    _s3("usb_device", "USB_DEVICE"),
    # The strip. 4 TX channels, 48-word blocks, RAM at `+0x800`.
    _s3("rmt", "RMT"),
    # The 49-pad fabric (GPIO0..=GPIO48) and its mux.
    _s3("gpio", "GPIO"),
    _s3("io_mux", "IO_MUX"),
    # MAC, chip revision, and the dbias/voltage fields esp-hal's clock path
    # reads before it raises the core voltage.
    _s3("efuse", "EFUSE"),
    # The per-core source→CPU-interrupt maps. See the ⚠️ above.
    _s3("interrupt_core0", "INTERRUPT_CORE0"),
    _s3("interrupt_core1", "INTERRUPT_CORE1"),
    # The cache and its MMU. ⚠️ The enable polarity is INVERTED relative to
    # the C6's (M6 notes §3.4), and the flash-MMU **table** is not in this
    # block — it is a directly-addressed window whose address came out of the
    # vendored ROM's own `Cache_*` disassembly (the report's §8), never from
    # the PAC and never from a datasheet.
    _s3("extmem", "EXTMEM"),
    # The flash controller and the cache's own port.
    _s3("spi0", "SPI0"),
    _s3("spi1", "SPI1"),
    _s3("apb_ctrl", "APB_CTRL"),
    # The analog master behind `request_pll_clk` / `configure_cpu_clk` — the
    # classic's fifth strict stop, one chip over.
    _s3("i2c_ana_mst", "I2C_ANA_MST"),
    # The four RF-adjacent blocks `esp_hal::init` touches inline, one register
    # each, all inside the first few thousand cycles of a strict bring-up.
    # Nothing on this chip's radio is modelled and nothing here suggests it is.
    _s3("bb", "BB"),
    _s3("nrx", "NRX"),
    _s3("fe", "FE"),
    _s3("fe2", "FE2"),
    # ROM-up only. The shipped image touches neither: it calls no ROM UART
    # routine and no ROM SHA entry point (the report's §7). The mask ROM's own
    # console is UART0's, and the IDF second-stage bootloader hashes the app
    # image with SHA.
    _s3("uart0", "UART0"),
    _s3("sha", "SHA"),
]


@dataclass(frozen=True)
class Chip:
    """One machine crate's PAC, its block table and its exclusions."""

    pac: str  # the PAC crate name, e.g. "esp32c6"
    version: str  # the version `Cargo.lock` pins
    targets: list[Target]
    # `(block, reason)` — blocks this chip must NOT emit. A block named here
    # and also in `targets` is a bug, and `main` says so rather than
    # generating it.
    skip: tuple[tuple[str, str], ...] = ()
    # Where the PAC's `Interrupt` enum goes, or None if the chip has no such
    # enum. The classic has none: `esp32-0.40.2` ships no `src/interrupt.rs`,
    # because its interrupt sources are DPORT `core_N_intr_map` indices
    # rather than a PLIC-style numbered table.
    sources_out: str | None = None


CHIPS = {
    "esp32c6": Chip(
        pac="esp32c6",
        version="0.23.2",
        targets=C6_TARGETS,
        sources_out="lp-emu/esp/lp-emu-esp32c6/src/regs/interrupt_sources.rs",
    ),
    "esp32": Chip(
        pac="esp32",
        version="0.40.2",
        targets=ESP32_TARGETS,
        # `rng` sat in `skip` from P1 to P3 as "an SVD leak from another
        # family": esp32-0.40.2/src/lib.rs:647 gives it 0x6003_5000, which is
        # not in the DPORT peripheral window. M3 P3's strict boot found the
        # classic's SECOND peripheral window — the AHB bus at 0x6000_0000,
        # mirroring the DPORT blocks from 0x3FF4_0000 — and 0x6003_5000 is
        # WDEV's AHB address: `data` at +0x144 is 0x6003_5144, the classic's
        # WDEV_RND_REG. The PAC was right; the exclusion is gone and the
        # table is generated like every other (memmap::MMIO_AHB_BASE).
    ),
    "esp32s3": Chip(
        pac="esp32s3",
        version="0.35.2",
        targets=ESP32S3_TARGETS,
        # ⚠️ **The S3 HAS an interrupt-source enum and it is not generated
        # here.** `esp32s3-0.35.2` puts `pub enum Interrupt` in `src/lib.rs`
        # (line 325, 94 named variants numbered 0..=98 with gaps), not in a
        # `src/interrupt.rs` as the C6 does — and `collect_sources` reads
        # `src/interrupt.rs` by name. Teaching it a second path is a
        # generator change, and M6 P01's scope is the CHIPS entry; the phase
        # that first needs the table (P03's matrix, P04's blocks) makes it.
        # The four numbers a phase needs in the meantime are written down in
        # M6 notes §3.3: `RMT = 40`, `TG0_T0_LEVEL = 50`,
        # `SYSTIMER_TARGET0..2 = 57,58,59`, `USB_DEVICE = 96`.
        sources_out=None,
    ),
}

DEFAULT_PAC = "esp32c6"


# --------------------------------------------------------------------------
# reading the PAC


class PacSource:
    """Reads files out of the PAC crate, unpacked or still in its tarball."""

    def __init__(self, chip: Chip) -> None:
        self.chip = chip
        self.src_dir: str | None = None
        self.tar: tarfile.TarFile | None = None
        self.origin = "not found"

    @staticmethod
    def _cargo_home() -> str:
        return os.environ.get(
            "CARGO_HOME", os.path.join(os.path.expanduser("~"), ".cargo")
        )

    def _try_open(self) -> bool:
        home = self._cargo_home()
        stem = f"{self.chip.pac}-{self.chip.version}"
        dirs = glob.glob(os.path.join(home, "registry", "src", "*", stem))
        if dirs:
            self.src_dir = dirs[0]
            self.origin = self.src_dir
            return True
        crates = glob.glob(os.path.join(home, "registry", "cache", "*", f"{stem}.crate"))
        if crates:
            self.tar = tarfile.open(crates[0], "r:gz")
            self.origin = crates[0]
            return True
        return False

    def open(self) -> bool:
        if self._try_open():
            return True
        # Not unpacked and not cached: the workspace may simply never have
        # built anything that needs it. Ask cargo for it once.
        try:
            subprocess.run(
                ["cargo", "fetch", "--locked"],
                cwd=REPO,
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                timeout=600,
            )
        except (subprocess.SubprocessError, OSError):
            return False
        return self._try_open()

    def read(self, relpath: str) -> str | None:
        """`relpath` is relative to the crate root, e.g. `src/uart0.rs`."""
        if self.src_dir is not None:
            path = os.path.join(self.src_dir, relpath)
            if not os.path.exists(path):
                return None
            with open(path, encoding="utf-8") as f:
                return f.read()
        if self.tar is not None:
            member = f"{self.chip.pac}-{self.chip.version}/{relpath}"
            try:
                fh = self.tar.extractfile(member)
            except KeyError:
                return None
            if fh is None:
                return None
            return io.TextIOWrapper(fh, encoding="utf-8").read()
        return None

    def svd2rust_line(self) -> str:
        """The generator line svd2rust puts at the top of `src/lib.rs`."""
        text = self.read("src/lib.rs") or ""
        m = re.search(r"generated using (svd2rust v[\d.]+ \([0-9a-f]+ [\d-]+\))", text)
        if m:
            return m.group(1)
        return "svd2rust (version not recorded in the crate)"


# --------------------------------------------------------------------------
# parsing

# `#[doc = "0x1c - UART status register"]`
DOC_ONE = re.compile(r'#\[doc = "0x([0-9a-fA-F]+) - ')
# `#[doc = "0x0c..0x14 - Cluster UNIT0LOAD, containing ..."]`
DOC_RANGE = re.compile(r'#\[doc = "0x([0-9a-fA-F]+)\.\.0x([0-9a-fA-F]+) - ')
# `pub const fn status(&self) -> &STATUS {` — the concrete accessor for one
# register (or one cluster). svd2rust expands most register arrays into these
# under their datasheet names (`unit0_op`), which is what we want to see in a
# log.
ACCESSOR = re.compile(r"^\s*pub const fn (\w+)\(&self\) -> &(\w+) \{")
# `pub const fn trgt(&self, n: usize) -> &TRGT {` — the array accessor. Some
# arrays (`TRGT`, `REAL_TARGET` in SYSTIMER) get NO expanded siblings, so
# without this their registers would silently be missing from the table.
ARRAY_ACCESSOR = re.compile(r"^\s*pub const fn (\w+)\(&self, n: usize\) -> &(\w+) \{")
# `    trgt: [TRGT; 3],` in the `struct RegisterBlock` declaration: the only
# place the element count is written down.
ARRAY_FIELD = re.compile(r"^\s*(\w+): \[(\w+); (\d+)\],")
# `[(); 1][n];` — the bounds check inside an array accessor's body. A cluster
# array whose struct field svd2rust flattened into `_reserved_0_cpu: [u8; 0x9c]`
# (ASSIST_DEBUG's `cpu(n)`) has no `[TYPE; N]` field, so this is the only place
# its element count is written down.
ARRAY_BOUND = re.compile(r"^\s*\[\(\); (\d+)\]\[n\];")

IMPL_HEAD = re.compile(r"^impl (\w+) \{")
# `impl crate::Resettable for USER_SPEC {` — svd2rust writes one per register.
# An EMPTY body means "resets to 0"; a body carries the value:
#     const RESET_VALUE: u32 = 0x8000_0000;
RESETTABLE = re.compile(
    r"impl crate::Resettable for (\w+)_SPEC \{(?:\s*const RESET_VALUE: u32 = "
    r"([0-9a-fA-Fx_]+);)?\s*\}"
)
# `impl crate::Readable for STATUS_SPEC {}` / `impl crate::Writable for
# CONF0_SPEC {` — a register has one, the other, or both. A `status` register
# has only Readable; an `int_clr` only Writable.
READABLE = re.compile(r"impl crate::Readable for (\w+)_SPEC\b")
WRITABLE = re.compile(r"impl crate::Writable for (\w+)_SPEC\b")


@dataclass(frozen=True)
class Accessor:
    offset: int  # start offset from the doc comment
    end: int | None  # end offset, for a range doc (`0x1c..0x34`)
    name: str
    type_name: str
    array: bool  # `(&self, n: usize)` rather than `(&self)`


def parse_impl(text: str, type_name: str) -> list[Accessor]:
    """The register accessors of `impl <type_name>`, in source order."""
    out: list[Accessor] = []
    inside = False
    pending: tuple[int, int | None] | None = None
    for line in text.splitlines():
        if not inside:
            m = IMPL_HEAD.match(line)
            if m and m.group(1) == type_name:
                inside = True
            continue
        if line.startswith("}"):
            break
        m = DOC_RANGE.search(line)
        if m:
            pending = (int(m.group(1), 16), int(m.group(2), 16))
            continue
        m = DOC_ONE.search(line)
        if m:
            pending = (int(m.group(1), 16), None)
            continue
        m = ACCESSOR.match(line)
        if m and pending is not None:
            out.append(Accessor(pending[0], pending[1], m.group(1), m.group(2), False))
            pending = None
            continue
        m = ARRAY_ACCESSOR.match(line)
        if m and pending is not None:
            out.append(Accessor(pending[0], pending[1], m.group(1), m.group(2), True))
            pending = None
            continue
        if "pub fn " in line or "pub const fn " in line:
            # Another accessor shape (an iterator): its doc comment does not
            # describe a register.
            pending = None
    return out


def array_lengths(text: str) -> dict[str, int]:
    """`field -> element count`, from the `struct RegisterBlock` declaration
    or, for a cluster array svd2rust flattened into a `_reserved` byte field,
    from the `[(); N][n]` bounds check in the array accessor's body."""
    out: dict[str, int] = {}
    for line in text.splitlines():
        if line.startswith("}"):
            break
        m = ARRAY_FIELD.match(line)
        if m:
            out[m.group(1)] = int(m.group(3))
    pending: str | None = None
    for line in text.splitlines():
        m = ARRAY_ACCESSOR.match(line)
        if m:
            pending = m.group(1)
            continue
        m = ARRAY_BOUND.match(line)
        if m and pending is not None:
            out.setdefault(pending, int(m.group(1)))
            pending = None
    return out


def reset_value(pac: PacSource, relpath: str, type_name: str) -> int:
    """The PAC's reset value for one register, or 0.

    svd2rust writes `impl crate::Resettable for <REG>_SPEC {}` for a register
    that resets to zero and gives the impl a `const RESET_VALUE: u32` body
    when it does not. A missing file is 0 too: a register whose module we
    cannot read is one we have no reset for, and 0 is what the window held
    before this pass existed.
    """
    text = pac.read(relpath)
    if text is None:
        return 0
    for m in RESETTABLE.finditer(text):
        if m.group(1) != type_name:
            continue
        if m.group(2) is None:
            return 0
        return int(m.group(2).replace("_", ""), 0)
    return 0


def access_of(pac: PacSource, relpath: str, type_name: str) -> str:
    """`"rw"`, `"r"`, `"w"` or `"none"` for one register.

    svd2rust writes `impl crate::Readable` and/or `impl crate::Writable`
    beside every register, and which of the two it writes is the SVD's
    `access` attribute. A module we cannot read is `"rw"`: the neutral
    answer, and the one that claims nothing extra.
    """
    text = pac.read(relpath)
    if text is None:
        return "rw"
    r = any(m.group(1) == type_name for m in READABLE.finditer(text))
    w = any(m.group(1) == type_name for m in WRITABLE.finditer(text))
    if r and w:
        return "rw"
    if r:
        return "r"
    if w:
        return "w"
    return "none"


def collect(
    pac: PacSource, block: str
) -> tuple[list[tuple[int, str]], list[tuple[int, int]], list[tuple[int, str]]]:
    """Every `(offset, name)` in a register block, clusters flattened, and
    the non-zero `(offset, reset)` pairs beside them.

    Concrete accessors win over array ones: svd2rust expands most register
    arrays into per-index siblings under their datasheet names (`unit0_op`),
    and those read better in a log than a synthesised `unit_op0`. Arrays with
    no siblings (`TRGT`, `REAL_TARGET`) are expanded from the array accessor
    plus the element count in the struct declaration — without that pass
    their registers would be silently missing.
    """
    top = pac.read(f"src/{block}.rs")
    if top is None:
        raise SystemExit(f"pac-regnames: {pac.chip.pac} has no src/{block}.rs")

    accessors = parse_impl(top, "RegisterBlock")
    lengths = array_lengths(top)
    entries: dict[int, str] = {}
    resets: dict[int, int] = {}
    access: dict[int, str] = {}

    def cluster_members(type_name: str) -> list[Accessor]:
        text = pac.read(f"src/{block}/{type_name.lower()}.rs")
        # A cluster's own file has an `impl <TYPE>` listing its members; a
        # plain register's file has none.
        return parse_impl(text, type_name) if text else []

    def place(base: int, name: str, type_name: str) -> None:
        members = cluster_members(type_name)
        if members:
            # A cluster's member modules live one directory down, under the
            # cluster type's own name (`src/systimer/unitload/hi.rs`).
            for m in members:
                off = base + m.offset
                if off in entries:
                    continue
                entries[off] = f"{name}.{m.name}"
                rel = f"src/{block}/{type_name.lower()}/{m.type_name.lower()}.rs"
                rv = reset_value(pac, rel, m.type_name)
                if rv:
                    resets[off] = rv
                acc = access_of(pac, rel, m.type_name)
                if acc != "rw":
                    access[off] = acc
        else:
            if base in entries:
                return
            entries[base] = name
            rel = f"src/{block}/{type_name.lower()}.rs"
            rv = reset_value(pac, rel, type_name)
            if rv:
                resets[base] = rv
            acc = access_of(pac, rel, type_name)
            if acc != "rw":
                access[base] = acc

    for a in (x for x in accessors if not x.array):
        place(a.offset, a.name, a.type_name)

    for a in (x for x in accessors if x.array):
        count = lengths.get(a.name)
        if not count or a.end is None:
            continue
        stride = (a.end - a.offset) // count
        for i in range(count):
            place(a.offset + i * stride, f"{a.name}{i}", a.type_name)

    return sorted(entries.items()), sorted(resets.items()), sorted(access.items())


# --------------------------------------------------------------------------
# the interrupt-source table

# The peripheral interrupt source numbers. `esp32c6::Interrupt` is
# `#[repr(u16)]` with one `#[doc = "N - NAME"]` per variant; the numbers are
# the indices of `INTERRUPT_CORE0.core_0_intr_map[n]`, so they are as much
# SVD-derived data as a register offset and carry the same header.
#
# Not every chip has one: `Chip.sources_out` is None for a PAC that ships no
# `src/interrupt.rs`, which is the classic's case (its sources are DPORT
# `core_N_intr_map` indices rather than a numbered enum).
SOURCE_DOC = re.compile(r'#\[doc = "(\d+) - (\w+)"\]')


def collect_sources(pac: PacSource) -> list[tuple[int, str]]:
    text = pac.read("src/interrupt.rs")
    if text is None:
        raise SystemExit(f"pac-regnames: {pac.chip.pac} has no src/interrupt.rs")
    out = [(int(m.group(1)), m.group(2)) for m in SOURCE_DOC.finditer(text)]
    return sorted(out)


def render_sources(chip: Chip, entries: list[tuple[int, str]], svd2rust: str) -> str:
    lines = [
        "// Peripheral interrupt source numbers derived from esp-rs/esp-pacs:",
        f"//   {chip.pac}/src/interrupt.rs  (crate {chip.pac} {chip.version},",
        f"//   generated by {svd2rust})",
        f"// Repository: {PAC_REPO}",
        f"// {PAC_LICENSE}; MIT text vendored at {VENDORED_LICENSE}.",
        "// Generated by scripts/emu/pac-regnames.py — do not hand-edit.",
        "// Regenerate and check with `just lint-emu-regnames`.",
        "",
        f"/// Every peripheral interrupt source, `(number, name)`, sorted "
        f"({len(entries)} sources).",
        "///",
        "/// The number is the index into `INTERRUPT_CORE0.core_0_intr_map`.",
        "pub static INTERRUPT_SOURCES: &[(u16, &str)] = &[",
    ]
    for n, name in entries:
        lines.append(f'    ({n}, "{name}"),')
    lines += [
        "];",
        "",
        "/// The same numbers as named constants.",
        "#[allow(dead_code, reason = \"generated: every source, used or not\")]",
        "pub mod source {",
    ]
    for n, name in entries:
        lines.append(f"    pub const {name}: u16 = {n};")
    lines += ["}", ""]
    return "\n".join(lines)


# --------------------------------------------------------------------------
# rendering


def slice_literal(name: str, items: list[str]) -> list[str]:
    """`name: &[…],` the way rustfmt would leave it.

    `cargo fmt --all` covers the generated files, so a shape rustfmt would
    rewrite makes `fmt-check` and `lint-emu-regnames` disagree about the
    same file for ever. rustfmt keeps an array literal on one line while it
    fits in `array_width` (60 by default) and the line fits in `max_width`
    (100); otherwise one element per line. This is that rule, and the two
    lints agreeing is what proves it.
    """
    inner = ", ".join(items)
    one = f"    {name}: &[{inner}],"
    if len(inner) + 2 <= 60 and len(one) <= 100:
        return [one]
    out = [f"    {name}: &["]
    out += [f"        {it}," for it in items]
    out.append("    ],")
    return out


ACCESS_VARIANT = {"r": "ReadOnly", "w": "WriteOnly", "none": "NoAccess"}


def render(
    chip: Chip,
    target: Target,
    entries: list[tuple[int, str]],
    resets: list[tuple[int, int]],
    access: list[tuple[int, str]],
    svd2rust: str,
) -> str:
    lines = [
        "// Register offsets, names and reset values derived from esp-rs/esp-pacs:",
        f"//   {chip.pac}/src/{target.block}.rs  (crate {chip.pac} {chip.version},",
        f"//   generated by {svd2rust})",
        f"// Repository: {PAC_REPO}",
        f"// {PAC_LICENSE}; MIT text vendored at {VENDORED_LICENSE}.",
        "// Generated by scripts/emu/pac-regnames.py — do not hand-edit.",
        "// Regenerate and check with `just lint-emu-regnames`.",
        "",
        "use lp_emu_esp_common::regnames::{Access, RegNames};"
        if access
        else "use lp_emu_esp_common::regnames::RegNames;",
        "",
        f"/// Register names for the `{target.block}` block "
        f"({len(entries)} registers, {len(resets)} with a non-zero reset, "
        f"{len(access)} not plain read-write).",
        f"pub static {target.static}: RegNames = RegNames {{",
        f'    block: "{target.block}",',
    ]
    width = max((len(f"{off:#05x}") for off, _ in entries), default=5)
    lines += slice_literal(
        "entries", [f'({off:#0{width}x}, "{name}")' for off, name in entries]
    )
    lines += slice_literal(
        "resets", [f"({off:#0{width}x}, {value:#010x})" for off, value in resets]
    )
    lines += slice_literal(
        "access",
        [f"({off:#0{width}x}, Access::{ACCESS_VARIANT[a]})" for off, a in access],
    )
    lines += ["};", ""]
    return "\n".join(lines)


# --------------------------------------------------------------------------


def run_chip(chip: Chip, check: bool, stale: list[str]) -> int:
    """Generate (or check) every table for one chip. Returns a process code:
    0 to carry on, 1 to stop. `stale` collects the drifted paths under
    `--check`."""
    # A block named in both `targets` and `skip` would be an exclusion that
    # silently did nothing, which is the failure mode the SKIP table exists
    # to prevent. Say so before touching a file.
    skipped = {block for block, _ in chip.skip}
    both = sorted(skipped.intersection(t.block for t in chip.targets))
    if both:
        print(
            f"pac-regnames: {chip.pac}: {', '.join(both)} is in BOTH the target "
            "table and the SKIP table",
            file=sys.stderr,
        )
        return 1

    pac = PacSource(chip)
    if not pac.open():
        msg = (
            f"pac-regnames: the {chip.pac} {chip.version} sources are not in this\n"
            f"    machine's cargo registry (neither unpacked nor as a .crate), and\n"
            f"    `cargo fetch` did not produce them."
        )
        if check:
            print(msg)
            print(f"    SKIPPED: {chip.pac}'s generated tables were not checked here.")
            return 0
        print(msg, file=sys.stderr)
        return 1

    svd2rust = pac.svd2rust_line()
    stale_before = len(stale)
    for target in chip.targets:
        entries, resets, access = collect(pac, target.block)
        if not entries:
            print(
                f"pac-regnames: no registers parsed for `{target.block}` — the "
                f"{chip.pac} PAC's shape changed",
                file=sys.stderr,
            )
            return 1
        text = render(chip, target, entries, resets, access, svd2rust)
        path = os.path.join(REPO, target.out)
        current = None
        if os.path.exists(path):
            with open(path, encoding="utf-8") as f:
                current = f.read()
        if check:
            if current != text:
                stale.append(target.out)
            continue
        if current == text:
            print(
                f"  unchanged  {target.out} ({len(entries)} registers, "
                f"{len(resets)} resets, {len(access)} non-rw)"
            )
            continue
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as f:
            f.write(text)
        print(
            f"  wrote      {target.out} ({len(entries)} registers, "
            f"{len(resets)} resets, {len(access)} non-rw)"
        )

    # The interrupt-source table, same discipline — for the chips that have
    # one. See `Chip.sources_out`.
    if chip.sources_out is not None:
        sources = collect_sources(pac)
        if len(sources) < 2:
            print(
                f"pac-regnames: no interrupt sources parsed — the {chip.pac} PAC's "
                "shape changed",
                file=sys.stderr,
            )
            return 1
        text = render_sources(chip, sources, svd2rust)
        path = os.path.join(REPO, chip.sources_out)
        current = None
        if os.path.exists(path):
            with open(path, encoding="utf-8") as f:
                current = f.read()
        if check:
            if current != text:
                stale.append(chip.sources_out)
        elif current == text:
            print(f"  unchanged  {chip.sources_out} ({len(sources)} sources)")
        else:
            with open(path, "w", encoding="utf-8") as f:
                f.write(text)
            print(f"  wrote      {chip.sources_out} ({len(sources)} sources)")

    if check and len(stale) == stale_before:
        extra = " + the interrupt-source table" if chip.sources_out else ""
        print(
            f"pac-regnames: {chip.pac}: {len(chip.targets)} generated table(s)"
            f"{extra} up to date (source: {pac.origin})"
        )
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--check",
        action="store_true",
        help="fail if any chip's generated table differs from what this script "
        "produces. Checks EVERY chip, whatever --pac says.",
    )
    ap.add_argument(
        "--pac",
        choices=sorted(CHIPS),
        default=DEFAULT_PAC,
        help=f"which chip's tables to regenerate (default: {DEFAULT_PAC}). "
        "Ignored under --check.",
    )
    args = ap.parse_args()

    # `--check` covers every chip: a lint that only saw the chip you happened
    # to name would pass for the one you were not working on.
    chips = list(CHIPS.values()) if args.check else [CHIPS[args.pac]]

    stale: list[str] = []
    for chip in chips:
        code = run_chip(chip, args.check, stale)
        if code != 0:
            return code

    if args.check and stale:
        print("pac-regnames: these generated tables are out of date:")
        for s in stale:
            print(f"    {s}")
        print()
        print("They are generated from an Espressif PAC and must never be edited")
        print("by hand — a hand edit is reverted by the next regeneration and")
        print("takes its provenance with it. Run:")
        print()
        for chip in chips:
            print(f"    scripts/emu/pac-regnames.py --pac {chip.pac}")
        print()
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
