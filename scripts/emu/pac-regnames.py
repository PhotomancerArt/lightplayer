#!/usr/bin/env python3
"""Generate `RegNames` tables from the esp32c6 PAC's svd2rust offset comments.

A bus log that says `UART0+0x01c` has to be decoded by hand against a PAC.
One that says `UART0+0x01c status` can be read. The names come from the same
place the M3 register inventory came from — the `#[doc = "0xNN - ..."]`
comments svd2rust writes above each register accessor — so they are derived
data, not transcription, and each generated file carries the provenance
header `docs/adr/2026-07-29-license-provenance-discipline.md` requires.

Two modes:

    scripts/emu/pac-regnames.py            regenerate every table
    scripts/emu/pac-regnames.py --check    fail if any table is out of date

`just lint-emu-regnames` runs `--check`, so a hand edit is caught the way
`lint-vec-corpus` catches one in the shader corpus.

Source resolution, in order:

    1. an unpacked crate under $CARGO_HOME/registry/src/*/esp32c6-<version>/
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

PAC_CRATE = "esp32c6"
PAC_VERSION = "0.23.2"
PAC_REPO = "https://github.com/esp-rs/esp-pacs"
PAC_LICENSE = "MIT OR Apache-2.0"
VENDORED_LICENSE = "licenses/esp-pacs-MIT.txt"


@dataclass(frozen=True)
class Target:
    """One generated table."""

    block: str  # PAC register-block module, e.g. "uart0"
    static: str  # the Rust `static` name, e.g. "UART0"
    out: str  # repo-relative output path


# The blocks generated today.
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
TARGETS = [
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


# --------------------------------------------------------------------------
# reading the PAC


class PacSource:
    """Reads files out of the PAC crate, unpacked or still in its tarball."""

    def __init__(self) -> None:
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
        stem = f"{PAC_CRATE}-{PAC_VERSION}"
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
            member = f"{PAC_CRATE}-{PAC_VERSION}/{relpath}"
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


def collect(pac: PacSource, block: str) -> list[tuple[int, str]]:
    """Every `(offset, name)` in a register block, clusters flattened.

    Concrete accessors win over array ones: svd2rust expands most register
    arrays into per-index siblings under their datasheet names (`unit0_op`),
    and those read better in a log than a synthesised `unit_op0`. Arrays with
    no siblings (`TRGT`, `REAL_TARGET`) are expanded from the array accessor
    plus the element count in the struct declaration — without that pass
    their registers would be silently missing.
    """
    top = pac.read(f"src/{block}.rs")
    if top is None:
        raise SystemExit(f"pac-regnames: {PAC_CRATE} has no src/{block}.rs")

    accessors = parse_impl(top, "RegisterBlock")
    lengths = array_lengths(top)
    entries: dict[int, str] = {}

    def cluster_members(type_name: str) -> list[Accessor]:
        text = pac.read(f"src/{block}/{type_name.lower()}.rs")
        # A cluster's own file has an `impl <TYPE>` listing its members; a
        # plain register's file has none.
        return parse_impl(text, type_name) if text else []

    def place(base: int, name: str, type_name: str) -> None:
        members = cluster_members(type_name)
        if members:
            for m in members:
                entries.setdefault(base + m.offset, f"{name}.{m.name}")
        else:
            entries.setdefault(base, name)

    for a in (x for x in accessors if not x.array):
        place(a.offset, a.name, a.type_name)

    for a in (x for x in accessors if x.array):
        count = lengths.get(a.name)
        if not count or a.end is None:
            continue
        stride = (a.end - a.offset) // count
        for i in range(count):
            place(a.offset + i * stride, f"{a.name}{i}", a.type_name)

    return sorted(entries.items())


# --------------------------------------------------------------------------
# the interrupt-source table

# Where the peripheral interrupt source numbers go. `esp32c6::Interrupt` is
# `#[repr(u16)]` with one `#[doc = "N - NAME"]` per variant; the numbers are
# the indices of `INTERRUPT_CORE0.core_0_intr_map[n]`, so they are as much
# SVD-derived data as a register offset and carry the same header.
SOURCES_OUT = "lp-emu/esp/lp-emu-esp32c6/src/regs/interrupt_sources.rs"
SOURCE_DOC = re.compile(r'#\[doc = "(\d+) - (\w+)"\]')


def collect_sources(pac: PacSource) -> list[tuple[int, str]]:
    text = pac.read("src/interrupt.rs")
    if text is None:
        raise SystemExit(f"pac-regnames: {PAC_CRATE} has no src/interrupt.rs")
    out = [(int(m.group(1)), m.group(2)) for m in SOURCE_DOC.finditer(text)]
    return sorted(out)


def render_sources(entries: list[tuple[int, str]], svd2rust: str) -> str:
    lines = [
        "// Peripheral interrupt source numbers derived from esp-rs/esp-pacs:",
        f"//   {PAC_CRATE}/src/interrupt.rs  (crate {PAC_CRATE} {PAC_VERSION},",
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


def render(target: Target, entries: list[tuple[int, str]], svd2rust: str) -> str:
    lines = [
        "// Register offsets and names derived from esp-rs/esp-pacs:",
        f"//   {PAC_CRATE}/src/{target.block}.rs  (crate {PAC_CRATE} {PAC_VERSION},",
        f"//   generated by {svd2rust})",
        f"// Repository: {PAC_REPO}",
        f"// {PAC_LICENSE}; MIT text vendored at {VENDORED_LICENSE}.",
        "// Generated by scripts/emu/pac-regnames.py — do not hand-edit.",
        "// Regenerate and check with `just lint-emu-regnames`.",
        "",
        "use lp_emu_esp_common::regnames::RegNames;",
        "",
        f"/// Register names for the `{target.block}` block "
        f"({len(entries)} registers).",
        f"pub static {target.static}: RegNames = RegNames {{",
        f'    block: "{target.block}",',
        "    entries: &[",
    ]
    width = max((len(f"{off:#05x}") for off, _ in entries), default=5)
    for off, name in entries:
        lines.append(f'        ({off:#0{width}x}, "{name}"),')
    lines += ["    ],", "};", ""]
    return "\n".join(lines)


# --------------------------------------------------------------------------


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--check",
        action="store_true",
        help="fail if a generated table differs from what this script produces",
    )
    args = ap.parse_args()

    pac = PacSource()
    if not pac.open():
        msg = (
            f"pac-regnames: the {PAC_CRATE} {PAC_VERSION} sources are not in this\n"
            f"    machine's cargo registry (neither unpacked nor as a .crate), and\n"
            f"    `cargo fetch` did not produce them."
        )
        if args.check:
            print(msg)
            print("    SKIPPED: the generated tables were not checked here.")
            return 0
        print(msg, file=sys.stderr)
        return 1

    svd2rust = pac.svd2rust_line()
    stale: list[str] = []
    for target in TARGETS:
        entries = collect(pac, target.block)
        if not entries:
            print(
                f"pac-regnames: no registers parsed for `{target.block}` — the PAC's "
                "shape changed",
                file=sys.stderr,
            )
            return 1
        text = render(target, entries, svd2rust)
        path = os.path.join(REPO, target.out)
        current = None
        if os.path.exists(path):
            with open(path, encoding="utf-8") as f:
                current = f.read()
        if args.check:
            if current != text:
                stale.append(target.out)
            continue
        if current == text:
            print(f"  unchanged  {target.out} ({len(entries)} registers)")
            continue
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as f:
            f.write(text)
        print(f"  wrote      {target.out} ({len(entries)} registers)")

    # The interrupt-source table, same discipline.
    sources = collect_sources(pac)
    if len(sources) < 2:
        print("pac-regnames: no interrupt sources parsed — the PAC's shape changed", file=sys.stderr)
        return 1
    text = render_sources(sources, svd2rust)
    path = os.path.join(REPO, SOURCES_OUT)
    current = None
    if os.path.exists(path):
        with open(path, encoding="utf-8") as f:
            current = f.read()
    if args.check:
        if current != text:
            stale.append(SOURCES_OUT)
    elif current == text:
        print(f"  unchanged  {SOURCES_OUT} ({len(sources)} sources)")
    else:
        with open(path, "w", encoding="utf-8") as f:
            f.write(text)
        print(f"  wrote      {SOURCES_OUT} ({len(sources)} sources)")

    if args.check:
        if stale:
            print("pac-regnames: these generated tables are out of date:")
            for s in stale:
                print(f"    {s}")
            print()
            print("They are generated from the esp32c6 PAC and must never be edited")
            print("by hand — a hand edit is reverted by the next regeneration and")
            print("takes its provenance with it. Run:")
            print()
            print("    scripts/emu/pac-regnames.py")
            print()
            return 1
        print(
            f"pac-regnames: {len(TARGETS)} generated table(s) + the interrupt-source "
            f"table up to date (source: {pac.origin})"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
