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


# The blocks generated today. P3 needs two: a flat one (uart0) and one with
# register arrays and clusters (systimer), which is what proves the
# generator handles both shapes. P5/P6 add rows here pointing at
# `lp-emu/esp/lp-emu-esp32c6/src/regs/` as they model each block.
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
    """`field -> element count` from the `struct RegisterBlock` declaration."""
    out: dict[str, int] = {}
    for line in text.splitlines():
        if line.startswith("}"):
            break
        m = ARRAY_FIELD.match(line)
        if m:
            out[m.group(1)] = int(m.group(3))
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
            f"pac-regnames: {len(TARGETS)} generated table(s) up to date "
            f"(source: {pac.origin})"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
