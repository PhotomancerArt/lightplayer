#!/usr/bin/env python3
"""A whole-SRAM ledger for a linked firmware ELF: where every byte of RAM goes.

    scripts/ram-ledger.py <elf> [--chip esp32c6|esp32v3|esp32s3]
                          [--json out.json] [--top N] [--blobs DIR]...
                          [--max-unknown-pct P]
    just ram-ledger <elf> [args]

Prints a Markdown ledger on stdout: the chip's whole SRAM by category, summing
to the chip's RAM size, then per category the owners (crate, or the radio
blob archive a C symbol came from) and the top symbols. `--json` writes the
same numbers plus every coalesced extent and every data/bss/code symbol over
`--min-symbol` bytes (default 256), for scripts that census the statics.

Method (all of it from the ELF and the files named below, nothing guessed):

1. The chip's SRAM is a list of address windows (`CHIPS`, each cited to the
   linker script or firmware source it came from). Windows the image owns are
   "elf" windows; the ROM's, the flash cache's, the JIT code region's and the
   heap spans the firmware registers by constant at boot (which the ELF cannot
   say) are "fixed", with the citation beside them.
2. Each allocated section that lands in an elf window claims its bytes;
   sized symbols inside it then claim theirs (innermost wins, so a static
   inside a compiler-merged `.L_MergedGlobals` block keeps its own name).
   Heap arenas are the big `.bss` statics named `HEAP*`; `.stack` is the
   stack; `.dram2_uninit` is a reclaimed bootloader segment.
3. A symbol's owner is its crate (v0 and legacy Rust mangling are decoded
   here), or, for an unmangled C symbol, the blob archive that defines it:
   every `lib*.a` of the chip's `esp-wifi-sys-*` crate is read (the `ar`
   format and each member's ELF symbol table) and joined by name.
4. Bytes inside a section that no sized symbol covers are not "unknown":
   they stay with the section ("unsymbolized") and are counted as such. Gaps
   of under 16 B between claims are alignment padding. Only bytes that no
   section and no window accounts for are "unknown".

Pure stdlib (the ELF reader is `scripts/elf32.py`, shared with
`scripts/heap-budget-stack-layout.py`). What it cannot see: a stack's
high-water mark and a heap's live bytes are runtime facts, not link facts —
the ledger says where the regions are, never how full they get.
"""

import argparse
import glob
import hashlib
import json
import os
import re
import sys
from dataclasses import dataclass, field
from itertools import groupby
from typing import Dict, List, Optional, Tuple

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from elf32 import (  # noqa: E402
    Elf32,
    ElfError,
    Section,
    Symbol,
    STT_FILE,
    STT_FUNC,
    STT_NOTYPE,
    STT_OBJECT,
)

KIB = 1024
PAD_MAX = 16  # an unclaimed run shorter than this between claims is alignment

# --------------------------------------------------------------------------
# Categories

CATEGORIES: List[Tuple[str, str]] = [
    ("rom", "ROM-reserved"),
    ("cache", "Flash/instruction cache reserve"),
    ("code", "Our code in RAM (.vectors/.trap/.rwtext)"),
    ("radio_code", "Radio blob code in RAM (.rwtext.wifi)"),
    ("jit", "JIT code region (reserved by the firmware)"),
    ("data", ".data"),
    ("bss", ".bss statics (heap arenas excluded)"),
    ("heap", "Heap regions"),
    ("stack", "Main stack"),
    ("idle", "Idle: no section, no heap region"),
    ("padding", "Alignment padding"),
    ("unknown", "UNKNOWN"),
]
CAT_LABEL = dict(CATEGORIES)

# --------------------------------------------------------------------------
# Chip memory maps. Every address is cited.


@dataclass
class Seg:
    start: int
    end: int
    label: str
    kind: str  # "fixed" | "elf"
    category: str = ""  # fixed: the category; elf: unused
    owner: str = ""  # fixed: the owner/heap-region name
    note: str = ""
    src: str = ""
    fill: str = "unknown"  # elf: what an unclaimed tail of this window is

    @property
    def size(self) -> int:
        return self.end - self.start


@dataclass
class Chip:
    name: str
    title: str
    ram_bytes: int
    segs: List[Seg]
    rtc: List[Seg]
    # (ibus_start, ibus_end, delta added to reach the data-bus view)
    aliases: List[Tuple[int, int, int]] = field(default_factory=list)
    skip_sections: Dict[str, str] = field(default_factory=dict)
    flash_ranges: List[Tuple[int, int]] = field(default_factory=list)
    blob_crate: str = ""
    machine: int = 0
    arena_label: str = ""  # what the image's one big .bss heap arena is called, if not by its symbol


MEMORY_X_C6 = "third_party/esp-hal/ld/esp32c6/memory.x"
MEMORY_X_V3 = "third_party/esp-hal/ld/esp32/memory.x"
MEMORY_X_S3 = "third_party/esp-hal/ld/esp32s3/memory.x"
CODEMEM = "lp-shader/lpvm-native/src/codemem_esp32.rs"
V3_MAIN = "lp-fw/fw-esp32v3/src/main.rs"
S3_MAIN = "lp-fw/fw-esp32s3/src/main.rs"
V3_BUDGET = "docs/reports/2026-09-04-classic-ram-budget.md"

CHIP_C6 = Chip(
    name="esp32c6",
    title="ESP32-C6 (RV32, 512 KiB HP SRAM)",
    ram_bytes=0x40880000 - 0x40800000,
    machine=0xF3,
    blob_crate="esp-wifi-sys-esp32c6",
    segs=[
        Seg(0x40800000, 0x4087E610, "app RAM (image-owned)", "elf",
            src=f"{MEMORY_X_C6}: RAM 0x40800000+0x6E610 and dram2_seg 0x4086E610..0x4087E610"),
        Seg(0x4087E610, 0x40880000, "ROM data/stack", "fixed", "rom", "ROM",
            "the ROM's own data and stack; leave it",
            f"{MEMORY_X_C6}: dram2_seg ends at 0x4087e610; MEMORY_MAP RAM ends 0x40880000"),
    ],
    rtc=[Seg(0x50000000, 0x50004000, "LP SRAM", "elf", fill="idle",
             src=f"{MEMORY_X_C6}: RTC_FAST 0x50000000, 16K; MEMORY_MAP RTC_RAM 0x50000000..0x50004000")],
    flash_ranges=[(0x42000000, 0x44000000)],
)

CHIP_V3 = Chip(
    name="esp32v3",
    title="ESP32 classic (Xtensa LX6, 520 KiB SRAM)",
    ram_bytes=(0x40080000 - 0x40070000) + (0x400A0000 - 0x40080000)
    + (0x40000000 - 0x3FFE0000) + (0x3FFE0000 - 0x3FFAE000),
    machine=0x5E,
    arena_label="heap region 1: dram_seg arena",
    segs=[
        Seg(0x3FFAE000, 0x3FFB0000, "SRAM2 ROM block", "fixed", "rom", "ROM",
            "8 KiB reserved for the ROM at the bottom of SRAM2",
            f"{MEMORY_X_V3}: dram_seg ORIGIN = 0x3FFAE000 + 8K ('8K reserved for usage by the ROM'); {V3_BUDGET}"),
        Seg(0x3FFB0000, 0x3FFE0000, "SRAM2 dram_seg (image-owned)", "elf",
            src=f"{MEMORY_X_V3}: dram_seg 0x3FFB0000, 192K"),
        Seg(0x3FFE0000, 0x3FFE0440, "SRAM1 ROM PRO data", "fixed", "rom", "ROM",
            "reserved_rom_data_pro, 1,088 B; ROM routines the image still calls read it",
            f"{MEMORY_X_V3}: reserved_rom_data_pro 0x3ffe0000 len 1088; {CODEMEM}: SRAM1_ROM_PRO_STACK_SPAN"),
        Seg(0x3FFE0440, 0x3FFE3F20, "SRAM1 ROM PRO stack + hole (reclaimed)", "fixed", "heap",
            "heap region 0: ROM PRO stack (reclaimed)",
            "handed to esp-alloc FIRST; dead from the first Rust instruction",
            f"{V3_MAIN}: add_rom_pro_stack_region(); {CODEMEM}: SRAM1_ROM_PRO_STACK_SPAN = (0x3FFE0440, 0x3FFE3F20-0x3FFE0440)"),
        Seg(0x3FFE3F20, 0x3FFE4350, "SRAM1 ROM APP data", "fixed", "rom", "ROM",
            "reserved_rom_data_app, 1,072 B",
            f"{MEMORY_X_V3}: reserved_rom_data_app 0x3ffe3f20 len 1072; {CODEMEM}: SRAM1_ROM_APP_STACK_BASE = 0x3FFE4350"),
        Seg(0x3FFE4350, 0x3FFE8000, "SRAM1 ROM APP stack + dram2 head (reclaimed)", "fixed", "heap",
            "heap region 3: ROM APP stack (reclaimed, after core bind)",
            "registered LAST: live during boot until the APP core is bound",
            f"{V3_MAIN}: add_rom_app_stack_region() (0x3FFE4350 .. sram1_claim_base 0x3FFE8000)"),
        Seg(0x3FFE8000, 0x40000000, "SRAM1 tail", "fixed", "heap",
            "heap region 2: SRAM1 tail",
            "the whole tail; the JIT region lives in SRAM0 since 2026-09-05",
            f"{V3_MAIN}: add_sram1_heap_region(); {CODEMEM}: ESP32_SRAM1_TAIL_BASE..ESP32_DRAM2_END (98,304 B)"),
        Seg(0x40070000, 0x40080000, "SRAM0 flash cache (PRO+APP)", "fixed", "cache", "flash cache",
            "64 KiB; the APP half is held for the cache too, nothing can use it as RAM",
            f"{MEMORY_X_V3}: reserved_cache_seg 0x40070000 len 64k; {V3_BUDGET}"),
        Seg(0x40080000, 0x40088000, "SRAM0 IRAM below the JIT region", "elf", fill="idle",
            src=f"image-owned: {MEMORY_X_V3}: vectors_seg 0x40080000 1k, iram_seg 0x40080400; {CODEMEM}: ESP32_DEFAULT ibus_base 0x40088000"),
        Seg(0x40088000, 0x40098000, "SRAM0 JIT code region", "fixed", "jit", "JIT code region",
            "64 KiB word-access IRAM the JIT writes code into; the ELF cannot say how much is used "
            "(corpus peak model ~16.8 KB)",
            f"{CODEMEM}: CodeRegion::ESP32_DEFAULT (ibus_base 0x4008_8000, len 0x1_0000)"),
        Seg(0x40098000, 0x400A0000, "SRAM0 IRAM above the JIT region", "fixed", "idle", "idle IRAM",
            "32 KiB spare; word-access only, so no heap can use it",
            f"{CODEMEM}: ESP32_DEFAULT leaves 32 KiB above; SRAM0_IRAM_END 0x400A0000"),
    ],
    rtc=[
        Seg(0x3FF80000, 0x3FF82000, "RTC fast RAM", "elf", fill="idle",
            src=f"{MEMORY_X_V3}: rtc_fast_dram_seg 0x3FF80000, 8k"),
        Seg(0x50000000, 0x50002000, "RTC slow RAM", "elf", fill="idle",
            src=f"{MEMORY_X_V3}: rtc_slow_seg 0x50000000, 8k"),
    ],
    flash_ranges=[(0x3F400000, 0x3F800000), (0x400C2000, 0x40C00000)],
)

# Per-image: the S3's ICache reserve is read from the ELF (`RESERVE_ICACHE`).
CHIP_S3 = Chip(
    name="esp32s3",
    title="ESP32-S3 (Xtensa LX7, 512 KiB SRAM)",
    ram_bytes=(0x40378000 - 0x40370000) + (0x3FD00000 - 0x3FC88000),
    machine=0x5E,
    aliases=[(0x40378000, 0x403E0000, -0x6F0000)],
    arena_label="dram_seg arena",
    skip_sections={
        ".rwdata_dummy": "the data-bus shadow of .vectors+.rwtext (same SRAM1 bytes, seen at 0x3FC88000)",
    },
    segs=[
        Seg(0x40370000, 0x40378000, "SRAM0 instruction-cache reserve", "fixed", "cache",
            "instruction cache",
            "RESERVE_ICACHE = 0x8000 (32 KiB), a symbol in the ELF",
            f"{MEMORY_X_S3}: RESERVE_ICACHE; vectors_seg starts at 0x40370000 + RESERVE_ICACHE"),
        Seg(0x3FC88000, 0x3FCDB700, "SRAM1 dram_seg (image-owned)", "elf",
            src=f"{MEMORY_X_S3}: dram_seg 0x3FC88000..dram2_seg; .vectors/.rwtext appear here via the +0x6F0000 alias"),
        Seg(0x3FCDB700, 0x3FCED710, "SRAM1 dram2_seg (bootloader's, unregistered)", "fixed", "idle",
            "idle bootloader segment",
            "72 KB the bootloader used and the firmware never registers; fw-esp32s3 has ONE heap arena. "
            f"{S3_MAIN} L172: 'the next lever ... is dram2_seg as a second esp_alloc region'",
            f"{MEMORY_X_S3}: dram2_seg 0x3FCDB700..0x3FCED710; {S3_MAIN}: only heap_allocator!(size: HEAP_SIZE)"),
        Seg(0x3FCED710, 0x3FCF0000, "SRAM1 top: ROM data", "fixed", "rom", "ROM",
            "10,480 B above dram2_seg; the ROM's own data symbols (e.g. phy_param_rom @0x3FCEF81C) live here",
            f"{MEMORY_X_S3}: dram2_seg end 0x3FCED710, SRAM1 end 0x3FCF0000; the ELF's absolute ROM symbols 0x3FCEF81C.."),
        Seg(0x3FCF0000, 0x3FCF8000, "SRAM2 low half (unregistered)", "fixed", "idle", "idle SRAM2",
            "esp-hal leaves it 'to the heap' but fw-esp32s3 registers no region here (single .bss arena)",
            f"{MEMORY_X_S3}: comment on D-cache; {S3_MAIN}: only heap_allocator!(size: HEAP_SIZE)"),
        Seg(0x3FCF8000, 0x3FD00000, "SRAM2 top half: data cache (ASSUMED 32 KiB)", "fixed", "cache",
            "data cache",
            "ASSUMED 32 KiB: the D-cache size is set by the bootloader image, not by this ELF; "
            "if it is 16 KiB, 16 KiB of this row is idle",
            f"{MEMORY_X_S3}: 'D cache use the memory from high address ... 16K/32K'"),
    ],
    rtc=[
        Seg(0x600FE000, 0x60100000, "RTC fast RAM", "elf", fill="idle",
            src=f"{MEMORY_X_S3}: rtc_fast_seg 0x600fe000, 8k"),
        Seg(0x50000000, 0x50002000, "RTC slow RAM", "elf", fill="idle",
            src=f"{MEMORY_X_S3}: rtc_slow_seg 0x50000000, 8k"),
    ],
    flash_ranges=[(0x3C000000, 0x3E000000), (0x42000000, 0x44000000)],
)

CHIPS = {c.name: c for c in (CHIP_C6, CHIP_V3, CHIP_S3)}

# Which radio blob archives belong to which family of the radio stack.
BLOB_FAMILY = {
    "libnet80211.a": "Wi-Fi", "libpp.a": "Wi-Fi", "libwpa_supplicant.a": "Wi-Fi",
    "libespnow.a": "Wi-Fi", "libmesh.a": "Wi-Fi", "libsmartconfig.a": "Wi-Fi",
    "libwapi.a": "Wi-Fi", "libregulatory.a": "Wi-Fi", "libcore.a": "Wi-Fi",
    "libble_app.a": "BLE controller", "libbtbb.a": "BLE controller",
    "libphy.a": "PHY", "libcoexist.a": "Coexistence", "libprintf.a": "C support (printf)",
}

# --------------------------------------------------------------------------
# Rust symbol names: owning crate and a readable form.

_BASIC = {
    "a": "i8", "b": "bool", "c": "char", "d": "f64", "e": "str", "f": "f32", "h": "u8",
    "i": "isize", "j": "usize", "l": "i32", "m": "u32", "n": "i128", "o": "u128",
    "p": "_", "s": "i16", "t": "u16", "u": "()", "v": "...", "x": "i64", "y": "u64", "z": "!",
}
_B62 = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"


class _V0:
    """Just enough of Rust's v0 mangling (RFC 2603) to name a symbol and find
    the crate that owns it. Raises on anything it does not understand."""

    def __init__(self, s: str) -> None:
        self.s, self.i, self.depth = s, 0, 0

    def peek(self) -> str:
        return self.s[self.i] if self.i < len(self.s) else ""

    def eat(self, c: str) -> bool:
        if self.peek() == c:
            self.i += 1
            return True
        return False

    def need(self, c: str) -> None:
        if not self.eat(c):
            raise ValueError(f"expected {c!r} at {self.i}")

    def base62(self) -> int:
        n = 0
        if self.eat("_"):
            return 0
        while True:
            c = self.peek()
            self.i += 1
            if c == "_":
                return n + 1
            n = n * 62 + _B62.index(c)

    def decimal(self) -> int:
        j = self.i
        while self.peek().isdigit():
            self.i += 1
        if j == self.i:
            raise ValueError("no digits")
        return int(self.s[j : self.i])

    def disambiguator(self) -> int:
        return self.base62() + 1 if self.eat("s") else 0

    def ident(self) -> Tuple[str, int]:
        d = self.disambiguator()
        self.eat("u")
        n = self.decimal()
        self.eat("_")
        name = self.s[self.i : self.i + n]
        if len(name) != n:
            raise ValueError("short ident")
        self.i += n
        return name, d

    def backref(self) -> int:
        off = self.base62()
        if off >= self.i:
            raise ValueError("forward backref")
        return off

    def _at(self, off: int, fn):
        save, self.i = self.i, off
        self.depth += 1
        if self.depth > 24:
            raise ValueError("backref depth")
        try:
            return fn()
        finally:
            self.i = save
            self.depth -= 1

    def path(self) -> Tuple[str, str]:
        c = self.peek()
        self.i += 1
        if c == "C":
            name, _ = self.ident()
            return name, name
        if c == "N":
            ns = self.peek()
            self.i += 1
            parent, crate = self.path()
            name, d = self.ident()
            if ns == "C":
                name = f"{{closure#{d}}}"
            elif ns == "S":
                name = f"{{shim:{name}#{d}}}"
            return (f"{parent}::{name}" if name else parent), crate
        if c == "M":
            self.disambiguator()
            _, crate = self.path()
            ty = self.type()
            return f"<{ty}>", crate
        if c == "X":
            self.disambiguator()
            _, crate = self.path()
            ty = self.type()
            tr, _ = self.path()
            return f"<{ty} as {tr}>", crate
        if c == "Y":
            ty = self.type()
            tr, crate = self.path()
            return f"<{ty} as {tr}>", crate
        if c == "I":
            p, crate = self.path()
            args = []
            while not self.eat("E"):
                args.append(self.generic_arg())
            return f"{p}::<{', '.join(args)}>", crate
        if c == "B":
            return self._at(self.backref(), lambda: self._path_again())
        raise ValueError(f"path {c!r}")

    def _path_again(self) -> Tuple[str, str]:
        return self.path()

    def generic_arg(self) -> str:
        if self.eat("L"):
            self.base62()
            return "'_"
        if self.eat("K"):
            return self.const()
        return self.type()

    def const(self) -> str:
        if self.eat("p"):
            return "_"
        if self.peek() == "B":
            self.i += 1
            return self._at(self.backref(), self.const)
        self.type()
        neg = self.eat("n")
        j = self.i
        while self.peek() != "_":
            self.i += 1
        v = self.s[j : self.i]
        self.need("_")
        return ("-" if neg else "") + (str(int(v, 16)) if v else "0")

    def type(self) -> str:
        c = self.peek()
        if c in _BASIC and c.islower():
            self.i += 1
            return _BASIC[c]
        if c == "B":
            self.i += 1
            return self._at(self.backref(), self.type)
        if c == "A":
            self.i += 1
            t = self.type()
            return f"[{t}; {self.const()}]"
        if c == "S":
            self.i += 1
            return f"[{self.type()}]"
        if c == "T":
            self.i += 1
            ts = []
            while not self.eat("E"):
                ts.append(self.type())
            return f"({', '.join(ts)})"
        if c in "RQ":
            self.i += 1
            if self.eat("L"):
                self.base62()
            return ("&" if c == "R" else "&mut ") + self.type()
        if c in "PO":
            self.i += 1
            return ("*const " if c == "P" else "*mut ") + self.type()
        if c == "F":
            self.i += 1
            if self.eat("G"):
                self.base62()
            self.eat("U")
            if self.eat("K"):
                if not self.eat("C"):
                    self.ident()
            ts = []
            while not self.eat("E"):
                ts.append(self.type())
            ret = ts.pop() if ts else "()"
            return f"fn({', '.join(ts)}) -> {ret}"
        if c == "D":
            self.i += 1
            if self.eat("G"):
                self.base62()
            ts = []
            while not self.eat("E"):
                p, _ = self.path()
                while self.eat("p"):
                    self.ident()
                    self.type()
                ts.append(p)
            self.need("L")
            self.base62()
            return "dyn " + " + ".join(ts)
        p, _ = self.path()
        return p


_LEGACY_ESC = {
    "$LT$": "<", "$GT$": ">", "$u20$": " ", "$RF$": "&", "$LP$": "(", "$RP$": ")",
    "$C$": ",", "$BP$": "*", "$u5b$": "[", "$u5d$": "]", "$u7b$": "{", "$u7d$": "}",
    "$SP$": "@", "$u3b$": ";", "$u27$": "'", "$u2b$": "+",
}


def _demangle_legacy(name: str) -> Optional[Tuple[str, str]]:
    if not name.startswith("_ZN"):
        return None
    i, segs = 3, []
    while i < len(name) and name[i].isdigit():
        j = i
        while name[j].isdigit():
            j += 1
        n = int(name[i:j])
        segs.append(name[j : j + n])
        i = j + n
    if not segs:
        return None
    if re.fullmatch(r"h[0-9a-f]{16}", segs[-1]):
        segs.pop()
    out = []
    for s in segs:
        if s.startswith("_$"):
            s = s[1:]
        for k, v in _LEGACY_ESC.items():
            s = s.replace(k, v)
        out.append(s.replace("..", "::"))
    pretty = "::".join(out)
    first = out[0].lstrip("<").split("::")[0].split(" ")[0] if out else "?"
    return pretty, first or "?"


@dataclass
class Named:
    pretty: str  # a readable name
    owner: str  # the crate, or a bucket label
    mangled: bool  # a Rust symbol (as opposed to an unmangled C one)


_NAME_CACHE: Dict[str, Named] = {}


def rust_name(raw: str) -> Optional[Named]:
    """Pretty name and crate of a Rust-mangled symbol, else None."""
    if raw.startswith("_R"):
        try:
            p = _V0(raw[2:])
            pretty, crate = p.path()
            return Named(pretty, crate, True)
        except (ValueError, IndexError):
            m = re.match(r"_R(?:N[A-Za-z]|[IMXY])*C(?:s[0-9a-zA-Z]*_)?(\d+)", raw)
            if m:
                k = int(m.group(1))
                st = m.end()
                crate = raw[st : st + k] if st + k <= len(raw) else "?"
                return Named(raw, crate, True)
            return Named(raw, "(unparsed Rust symbol)", True)
    leg = _demangle_legacy(raw)
    if leg:
        return Named(leg[0], leg[1], True)
    return None


# --------------------------------------------------------------------------
# Blob archives (`ar`), joined to the ELF by symbol name.


@dataclass
class BlobIndex:
    # symbol name -> (archive file name, member name)
    defs: Dict[str, Tuple[str, str]] = field(default_factory=dict)
    ambiguous: Dict[str, List[str]] = field(default_factory=dict)
    archives: List[Tuple[str, int, int]] = field(default_factory=list)  # name, bytes, symbols
    dirs: List[str] = field(default_factory=list)

    def add_archive(self, path: str) -> None:
        with open(path, "rb") as f:
            blob = f.read()
        aname = os.path.basename(path)
        if blob[:8] != b"!<arch>\n":
            raise ValueError(f"{path}: not an ar archive")
        pos, longnames, nsyms = 8, b"", 0
        while pos + 60 <= len(blob):
            hdr = blob[pos : pos + 60]
            name = hdr[:16].decode("ascii", "replace").strip()
            size = int(hdr[48:58].decode("ascii").strip())
            body = blob[pos + 60 : pos + 60 + size]
            pos += 60 + size + (size & 1)
            if name == "//":
                longnames = body
                continue
            if name in ("/", "/SYM64/"):
                continue
            if name.startswith("/") and name[1:].isdigit():
                off = int(name[1:])
                end = longnames.index(b"\n", off)
                name = longnames[off:end].decode("ascii", "replace").rstrip("/")
            else:
                name = name.rstrip("/")
            if body[:4] != b"\x7fELF":
                continue
            try:
                obj = Elf32(body, f"{aname}({name})")
            except ElfError:
                continue
            for sym in obj.symbols():
                if sym.shndx == 0 or sym.shndx >= 0xFF00 or sym.name.startswith(".L"):
                    continue
                if sym.kind not in (STT_FUNC, STT_OBJECT, STT_NOTYPE) or sym.kind == STT_FILE:
                    continue
                nsyms += 1
                prev = self.defs.get(sym.name)
                if prev is None:
                    self.defs[sym.name] = (aname, name)
                elif prev[0] != aname:
                    self.ambiguous.setdefault(sym.name, [prev[0]]).append(aname)
        self.archives.append((aname, len(blob), nsyms))

    def add_dir(self, d: str) -> int:
        n = 0
        for p in sorted(glob.glob(os.path.join(d, "lib*.a"))):
            self.add_archive(p)
            n += 1
        if n:
            self.dirs.append(d)
        return n


def find_blob_dirs(crate: str, repo_root: str) -> List[str]:
    """`<cargo registry>/<crate>-<version>/libs`, the version Cargo.lock pins."""
    home = os.environ.get("CARGO_HOME", os.path.expanduser("~/.cargo"))
    found = sorted(glob.glob(os.path.join(home, "registry", "src", "*", crate + "-*", "libs")))
    if not found:
        return []
    pinned = None
    try:
        with open(os.path.join(repo_root, "Cargo.lock")) as f:
            lock = f.read()
        m = re.search(r'name = "%s"\nversion = "([^"]+)"' % re.escape(crate), lock)
        pinned = m.group(1) if m else None
    except OSError:
        pass
    if pinned:
        want = [d for d in found if os.path.basename(os.path.dirname(d)) == f"{crate}-{pinned}"]
        if want:
            return want[:1]
    return found[-1:]


# --------------------------------------------------------------------------
# The ledger


@dataclass
class Claim:
    category: str
    owner: str
    section: str
    name: str = ""  # a symbol's readable name; "" = section-level
    size: int = 0  # the symbol's own size
    addr: int = 0
    source: str = ""  # where a blob symbol came from, "libx.a(member.o)"
    family: str = ""
    # "symbol" (a sized symbol), "section" (a section no sized symbol covers),
    # "inferred" (a gap between two symbols of one owner), "fixed" (the map
    # decides), "pad", "idle", "unknown"
    kind: str = "symbol"
    between: str = ""  # for section-only gaps: the symbols either side


class Painter:
    """A per-byte ownership map over a list of windows."""

    def __init__(self, windows: List[Tuple[int, int]]) -> None:
        self.windows = sorted(windows)
        self.offsets: List[int] = []
        total = 0
        for lo, hi in self.windows:
            self.offsets.append(total)
            total += hi - lo
        self.claim_ids: List[int] = [0] * total  # 0 = unclaimed
        self.claims: List[Optional[Claim]] = [None]
        self._index: Dict[tuple, int] = {}

    def intern(self, c: Claim) -> int:
        key = (c.category, c.owner, c.section, c.name, c.size, c.addr, c.source, c.family, c.kind, c.between)
        cid = self._index.get(key)
        if cid is None:
            cid = len(self.claims)
            self.claims.append(c)
            self._index[key] = cid
        return cid

    def paint(self, lo: int, hi: int, cid: int) -> int:
        """Claim [lo, hi) wherever it lies in a window. Returns bytes painted."""
        n = 0
        for k, (wlo, whi) in enumerate(self.windows):
            a, b = max(lo, wlo), min(hi, whi)
            if a < b:
                o = self.offsets[k]
                self.claim_ids[o + (a - wlo) : o + (b - wlo)] = [cid] * (b - a)
                n += b - a
        return n

    def runs(self):
        """(start, end, claim id) maximal runs in address order."""
        for k, (wlo, whi) in enumerate(self.windows):
            o = self.offsets[k]
            pos = wlo
            for cid, grp in groupby(self.claim_ids[o : o + (whi - wlo)]):
                n = sum(1 for _ in grp)
                yield pos, pos + n, cid
                pos += n


def section_category(name: str) -> Optional[str]:
    if name in (".vectors", ".trap", ".rwtext", ".iram0.text") or name.startswith(".rwtext.") and name != ".rwtext.wifi":
        return "code"
    if name == ".rwtext.wifi":
        return "radio_code"
    if name in (".data", ".data.wifi") or name.startswith(".data."):
        return "data"
    if name in (".bss", ".noinit") or name.startswith(".bss."):
        return "bss"
    if name == ".stack":
        return "stack"
    if name == ".dram2_uninit":
        return "heap"
    return None


HEAP_RE = re.compile(r"(?:^|::)(HEAP(?:_[A-Z0-9]+)*)$")


@dataclass
class Ledger:
    chip: Chip
    elf_path: str
    sha256: str
    painter: Painter
    rtc_painter: Painter
    warnings: List[str]
    notes: List[str]
    heap_regions: List[dict]
    skipped: List[dict]
    blob: Optional[BlobIndex]
    stack: Optional[dict]
    quality: Dict[str, int]
    rtc_quality: Dict[str, int]
    sections: List[dict]


def detect_chip(elf: Elf32) -> Chip:
    wanted = elf.symbol_values(["_stack_start", "_stack_end"])
    top = wanted["_stack_start"]
    if elf.e_machine == CHIP_C6.machine:
        return CHIP_C6
    if top is None:
        raise SystemExit(f"{elf.label}: cannot tell the chip (no _stack_start); pass --chip")
    for c in (CHIP_V3, CHIP_S3):
        for s in c.segs:
            if s.kind == "elf" and s.start < top <= s.end:
                return c
    raise SystemExit(f"{elf.label}: _stack_start 0x{top:08x} is in no known chip's image window; pass --chip")


def to_dbus(chip: Chip, addr: int) -> int:
    for lo, hi, delta in chip.aliases:
        if lo <= addr < hi:
            return addr + delta
    return addr


def in_ranges(addr: int, ranges: List[Tuple[int, int]]) -> bool:
    return any(lo <= addr < hi for lo, hi in ranges)


def build_ledger(elf: Elf32, elf_path: str, chip: Chip, blob: Optional[BlobIndex]) -> Ledger:
    with open(elf_path, "rb") as f:
        sha = hashlib.sha256(f.read()).hexdigest()
    warnings: List[str] = []
    notes: List[str] = []

    segs = list(chip.segs)
    # The S3's cache reserve is a symbol in the image, not a constant of the map.
    if chip.name == "esp32s3":
        ic = elf.symbol_values(["RESERVE_ICACHE"])["RESERVE_ICACHE"]
        if ic is not None and ic != 0x8000:
            warnings.append(f"RESERVE_ICACHE is 0x{ic:x}, the map assumes 0x8000 — the S3 rows are wrong")

    ram_windows = [(s.start, s.end) for s in segs]
    painter = Painter(ram_windows)
    rtc_painter = Painter([(s.start, s.end) for s in chip.rtc])

    # 1. fixed segments
    for s in segs:
        if s.kind == "fixed":
            painter.paint(
                s.start, s.end,
                painter.intern(Claim(s.category, s.owner or s.label, s.label, "", s.size, s.start, family=s.note, kind="fixed")),
            )

    syms_by_sec: Dict[int, List[Symbol]] = {}
    all_syms = list(elf.symbols())
    for sym in all_syms:
        if sym.size > 0 and sym.shndx < len(elf.sections) and sym.kind in (STT_OBJECT, STT_FUNC, STT_NOTYPE):
            syms_by_sec.setdefault(sym.shndx, []).append(sym)

    named_cache: Dict[str, Named] = {}

    def name_of(sym: Symbol) -> Named:
        raw = sym.name
        n = named_cache.get(raw)
        if n is not None:
            return n
        n = _name_of(raw, blob)
        named_cache[raw] = n
        return n

    skipped: List[dict] = []
    sections_out: List[dict] = []
    heap_regions: List[dict] = []
    elf_segs = [s for s in segs if s.kind == "elf"]
    elf_ranges = [(s.start, s.end) for s in elf_segs]

    def paint_section(sec: Section, p: Painter, windows: List[Tuple[int, int]], rtc: bool) -> None:
        lo = to_dbus(chip, sec.addr)
        hi = lo + sec.size
        cat = "rtc" if rtc else section_category(sec.name)
        if cat is None:
            cat = "bss" if sec.nobits else "data"
            warnings.append(f"section {sec.name} (0x{sec.addr:08x}, {sec.size} B) has no category rule; counted as {cat}")
        if cat == "stack":
            sec_claim = p.intern(Claim(cat, "main stack", sec.name, "_stack_end.._stack_start", sec.size, lo, kind="fixed"))
        else:
            sec_claim = p.intern(Claim(cat, f"(unsymbolized {sec.name})", sec.name, "", sec.size, lo, kind="section"))
        got = p.paint(lo, hi, sec_claim)
        if not rtc:
            sections_out.append({"section": sec.name, "addr": lo, "size": sec.size, "category": cat,
                                 "window": next(s.label for s in segs if s.start <= lo < s.end)})
        if got != sec.size:
            warnings.append(f"section {sec.name}: only {got} of {sec.size} B fall in the chip's RAM windows")
        # fixed-segment collisions: the image owning bytes the map says are not its own
        # (checked against the claim map before the section painted over it — see below)
        syms = syms_by_sec.get(sec.index, [])
        containers = [y for y in syms if y.name.startswith(".L_MergedGlobals")]
        regular = [y for y in syms if not y.name.startswith(".L_MergedGlobals")]
        order = containers + sorted(regular, key=lambda y: -y.size)
        for y in order:
            ylo = to_dbus(chip, y.value)
            yhi = ylo + y.size
            if y.name.startswith(".L_MergedGlobals"):
                c = Claim(cat, "(compiler-merged statics)", sec.name, y.name, y.size, ylo)
            else:
                c = _claim_for(y, sec, cat, ylo, name_of(y), blob, rtc)
                hm = HEAP_RE.search(name_of(y).pretty) if not rtc else None
                if hm and y.size >= 4096 and y.kind == STT_OBJECT:
                    label = (f"{chip.arena_label} ({hm.group(1)} in {sec.name})" if chip.arena_label
                             else f"{hm.group(1)} (arena in {sec.name})")
                    c = Claim("heap", label, sec.name, name_of(y).pretty, y.size, ylo)
                    heap_regions.append({"name": label, "start": ylo, "size": y.size, "section": sec.name,
                                         "symbol": name_of(y).pretty, "source": "ELF symbol"})
            p.paint(ylo, yhi, p.intern(c))

    # 2. ELF sections
    for sec in elf.sections:
        if not sec.alloc or sec.size == 0:
            continue
        if sec.name in chip.skip_sections:
            skipped.append({"section": sec.name, "addr": sec.addr, "size": sec.size, "why": chip.skip_sections[sec.name]})
            continue
        lo = to_dbus(chip, sec.addr)
        if in_ranges(lo, elf_ranges):
            hi = lo + sec.size
            seg = next(s for s in elf_segs if s.start <= lo < s.end)
            if hi > seg.end:
                warnings.append(f"section {sec.name} (0x{lo:08x}+{sec.size}) runs past its window end 0x{seg.end:08x}")
            paint_section(sec, painter, ram_windows, False)
        elif in_ranges(sec.addr, [(s.start, s.end) for s in chip.rtc]):
            paint_section(sec, rtc_painter, [], True)
        elif in_ranges(lo, ram_windows):
            warnings.append(
                f"section {sec.name} (0x{lo:08x}, {sec.size} B) lies in a window the map says the image does not own "
                f"({next(s.label for s in segs if s.start <= lo < s.end)}); the map or the image is wrong"
            )
        elif in_ranges(sec.addr, chip.flash_ranges):
            skipped.append({"section": sec.name, "addr": sec.addr, "size": sec.size, "why": "flash (XIP), not RAM"})
        else:
            warnings.append(f"section {sec.name} (0x{sec.addr:08x}, {sec.size} B) is in no known RAM or flash range")
            skipped.append({"section": sec.name, "addr": sec.addr, "size": sec.size, "why": "UNPLACED"})

    # 3. fixed heap regions the firmware registers by constant (not in the ELF)
    for s in segs:
        if s.kind == "fixed" and s.category == "heap":
            heap_regions.append({"name": s.owner, "start": s.start, "size": s.size, "section": "", "symbol": "",
                                 "source": s.src})

    # 4. the stack
    stack = None
    sv = elf.symbol_values(["_stack_start", "_stack_end"])
    if sv["_stack_start"] is not None and sv["_stack_end"] is not None:
        top, bot = sv["_stack_start"], sv["_stack_end"]
        stack = {"start": bot, "end": top, "size": top - bot}
        ssec = elf.section_named(".stack")
        if ssec is not None and (ssec.addr != bot or ssec.size != top - bot):
            warnings.append(f".stack section (0x{ssec.addr:08x}+{ssec.size}) disagrees with _stack_end/_stack_start")

    # 5. classify unclaimed bytes
    quality = _classify(painter, segs)
    rtc_quality = _classify(rtc_painter, chip.rtc)

    heap_regions.sort(key=lambda r: r["start"])
    sections_out.sort(key=lambda x: x["addr"])
    return Ledger(chip, elf_path, sha, painter, rtc_painter, warnings, notes, heap_regions, skipped, blob, stack,
                  quality, rtc_quality, sections_out)


def _name_of(raw: str, blob: Optional[BlobIndex]) -> Named:
    base = raw
    prefix = ""
    for pre in (".Lswitch.table.", ".Lanon.", ".L__unnamed", ".L"):
        if raw.startswith(pre):
            prefix = pre
            base = raw[len(pre):]
            break
    if prefix == ".Lswitch.table.":
        inner = rust_name(base)
        if inner:
            return Named(f"switch table of {inner.pretty}", inner.owner, True)
        return Named(raw, "(anonymous constants)", True)
    if prefix:
        return Named(raw, "(anonymous constants)", True)
    n = rust_name(raw)
    if n:
        return n
    if blob is not None and raw in blob.defs:
        lib, member = blob.defs[raw]
        return Named(raw, f"blob:{lib}", False)
    return Named(raw, "(unmangled Rust/asm/C, not in the blob archives)", False)


def _claim_for(y: Symbol, sec: Section, cat: str, lo: int, n: Named, blob: Optional[BlobIndex], rtc: bool) -> Claim:
    source = ""
    family = ""
    owner = n.owner
    if owner.startswith("blob:") and blob is not None:
        lib, member = blob.defs[y.name]
        source = f"{lib}({member})"
        family = BLOB_FAMILY.get(lib, "other radio blob")
    if cat == "radio_code" and not family:
        family = "Rust glue / not in the blob archives"
    return Claim(cat, owner, sec.name, n.pretty, y.size, lo, source, family, "symbol")


QUALITY_ORDER = [
    ("symbol", "named symbol"),
    ("inferred", "inferred: a gap between two symbols of one owner"),
    ("section", "section only: inside a section no sized symbol covers"),
    ("fixed", "fixed by the map (ROM, cache, JIT region, registered spans, stack)"),
    ("pad", "alignment padding"),
    ("idle", "idle tail of a spare window"),
    ("unknown", "unknown"),
]


def _classify(p: Painter, segs: List[Seg]) -> Dict[str, int]:
    """Give every byte a category and return the attribution-quality tally.

    Pass 1 settles the bytes no section claimed: short runs between claims are
    alignment, the tail of a window the map calls spare is idle, anything else
    is UNKNOWN. Pass 2 revisits the bytes a section claimed but no sized symbol
    did: a short gap is alignment (4 B in code, 16 B in data), a longer one
    between two symbols of one owner is inferred to be that owner's, the rest
    stays "section only" with its neighbours named.
    """
    pad_id = p.intern(Claim("padding", "alignment padding", "", "", 0, 0, kind="pad"))
    runs = list(p.runs())
    for idx, (a, b, cid) in enumerate(runs):
        if cid != 0:
            continue
        seg = next(s for s in segs if s.start <= a < s.end)
        prev_painted = idx > 0 and runs[idx - 1][2] != 0 and runs[idx - 1][1] == a
        next_painted = idx + 1 < len(runs) and runs[idx + 1][2] != 0 and runs[idx + 1][0] == b
        if b - a < PAD_MAX and prev_painted and next_painted:
            p.paint(a, b, pad_id)
        elif seg.fill == "idle" and (b == seg.end or not next_painted):
            p.paint(a, b, p.intern(Claim("idle", f"idle in {seg.label}", seg.label, "", b - a, a, kind="idle")))
        else:
            p.paint(a, b, p.intern(Claim("unknown", f"0x{a:08x}..0x{b:08x}", seg.label, "", b - a, a, kind="unknown")))

    runs = list(p.runs())
    for idx, (a, b, cid) in enumerate(runs):
        c = p.claims[cid]
        if c is None or c.kind != "section":
            continue
        prv = p.claims[runs[idx - 1][2]] if idx > 0 and runs[idx - 1][1] == a else None
        nxt = p.claims[runs[idx + 1][2]] if idx + 1 < len(runs) and runs[idx + 1][0] == b else None
        sym_kinds = ("symbol", "inferred")
        pad_max = 4 if c.category in ("code", "radio_code") else PAD_MAX
        if prv and nxt and prv.kind in sym_kinds and nxt.kind in sym_kinds:
            if b - a < pad_max:
                p.paint(a, b, pad_id)
            elif prv.owner == nxt.owner and prv.category == nxt.category:
                p.paint(a, b, p.intern(Claim(c.category, prv.owner, c.section, "", b - a, a, "", prv.family,
                                             "inferred")))
            else:
                between = f"{trunc(prv.name or prv.owner, 40)} .. {trunc(nxt.name or nxt.owner, 40)}"
                p.paint(a, b, p.intern(Claim(c.category, c.owner, c.section, "", b - a, a, "", "", "section",
                                             between)))
    q = {label: 0 for _, label in QUALITY_ORDER}
    names = dict(QUALITY_ORDER)
    for a, b, cid in p.runs():
        c = p.claims[cid]
        if c is not None:
            q[names[c.kind]] += b - a
    return q


# --------------------------------------------------------------------------
# Summaries


def tally(p: Painter) -> Dict[int, int]:
    t: Dict[int, int] = {}
    for a, b, cid in p.runs():
        t[cid] = t.get(cid, 0) + (b - a)
    return t


def summarize(led: Ledger) -> dict:
    """Per category: bytes, owner -> bytes, families, and (claim, painted bytes) for each symbol."""
    t = tally(led.painter)
    cats: Dict[str, dict] = {}
    for cid, n in t.items():
        c = led.painter.claims[cid]
        if c is None:
            cats.setdefault("unknown", {"bytes": 0, "owners": {}, "syms": [], "families": {}})["bytes"] += n
            continue
        e = cats.setdefault(c.category, {"bytes": 0, "owners": {}, "syms": [], "families": {}})
        e["bytes"] += n
        e["owners"][c.owner] = e["owners"].get(c.owner, 0) + n
        if c.family and c.category == "radio_code":
            e["families"][c.family] = e["families"].get(c.family, 0) + n
        if c.kind == "symbol" and c.name:
            e["syms"].append((c, n))
    return cats


def gaps(led: Ledger) -> List[Tuple[int, int, Claim]]:
    """Section-only runs (no sized symbol), biggest first."""
    out = [(a, b, led.painter.claims[cid]) for a, b, cid in led.painter.runs()
           if led.painter.claims[cid] and led.painter.claims[cid].kind == "section"]
    out.sort(key=lambda x: -(x[1] - x[0]))
    return out


def fmt_n(n: int) -> str:
    return f"{n:,}"


def pct(n: int, total: int) -> str:
    return f"{100.0 * n / total:.2f}%"


def trunc(s: str, w: int = 100) -> str:
    return s if len(s) <= w else s[: w - 1] + "…"


def md_escape(s: str) -> str:
    return s.replace("|", "\\|")


def render_markdown(led: Ledger, top: int) -> str:
    chip, total = led.chip, led.chip.ram_bytes
    cats = summarize(led)
    out: List[str] = []
    w = out.append
    w(f"# RAM ledger: {chip.name}\n")
    w(f"- **Chip:** {chip.title}")
    w(f"- **Image:** `{os.path.basename(led.elf_path)}` (sha256 `{led.sha256[:16]}`)")
    w(f"- **Chip SRAM accounted:** {fmt_n(total)} B ({total / KIB:.0f} KiB)")
    w("- **Tool:** `scripts/ram-ledger.py` (windows and fixed spans are cited under *Memory map and sources*)")
    if led.blob and led.blob.archives:
        w(f"- **Blob archives joined:** {', '.join(a for a, _, _ in led.blob.archives)} "
          f"(`{os.path.basename(os.path.dirname(led.blob.dirs[0]))}`)")
    w("")

    # Whole-RAM table
    w("## Whole-RAM table\n")
    w("| category | bytes | KiB | % of RAM |")
    w("|---|---:|---:|---:|")
    grand = 0
    for cid_, label in CATEGORIES:
        e = cats.get(cid_)
        if not e or e["bytes"] == 0:
            continue
        if cid_ == "heap":
            for owner, n in sorted(e["owners"].items(), key=lambda kv: -kv[1]):
                w(f"| **Heap: {md_escape(owner)}** | **{fmt_n(n)}** | {n / KIB:.1f} | {pct(n, total)} |")
                grand += n
            continue
        w(f"| **{label}** | **{fmt_n(e['bytes'])}** | {e['bytes'] / KIB:.1f} | {pct(e['bytes'], total)} |")
        grand += e["bytes"]
        if cid_ == "radio_code":
            for fam, n in sorted(e["families"].items(), key=lambda kv: -kv[1]):
                w(f"| &nbsp;&nbsp;&nbsp;↳ of which {fam} | {fmt_n(n)} | {n / KIB:.1f} | {pct(n, total)} |")
    w(f"| **TOTAL** | **{fmt_n(grand)}** | {grand / KIB:.1f} | {pct(grand, total)} |")
    ok = grand == total
    w("")
    w(f"Check: rows sum to {fmt_n(grand)} B; the chip is {fmt_n(total)} B — **{'OK' if ok else 'MISMATCH'}**.")
    unk = cats.get("unknown", {"bytes": 0})["bytes"]
    w(f"Unknown: **{fmt_n(unk)} B = {pct(unk, total)}** of RAM (bar: under 1.00%).\n")
    if not ok:
        w("**The ledger does not sum to the chip: this is a bug in the memory map or the tool.**\n")

    # Attribution quality
    w("## How well each byte is attributed\n")
    w("| attribution | bytes | % of RAM |")
    w("|---|---:|---:|")
    for k, n in led.quality.items():
        w(f"| {k} | {fmt_n(n)} | {pct(n, total)} |")
    w("")
    gp = [g for g in gaps(led) if g[1] - g[0] >= 64]
    if gp:
        w("Largest section-only gaps (bytes inside a section that no sized symbol covers; on the Xtensa images "
          "these are mostly anonymous constants whose local labels are not in the symbol table. Naming them "
          "needs a link map or DWARF, which these images do not carry):\n")
        w("| bytes | address | section | between |")
        w("|---:|---|---|---|")
        for a, b, c in gp[:8]:
            w(f"| {fmt_n(b - a)} | `0x{a:08x}` | {c.section} | {md_escape(c.between)} |")
        w("")

    # Section layout
    w("## Section layout (image-owned RAM)\n")
    w("| section | address | bytes | counted as |")
    w("|---|---|---:|---|")
    for s in led.sections:
        w(f"| `{s['section']}` | `0x{s['addr']:08x}` | {fmt_n(s['size'])} | {CAT_LABEL.get(s['category'], s['category'])} |")
    w("")

    # Heap table
    if led.heap_regions:
        w("## Heap regions\n")
        w("| region | address | bytes | from |")
        w("|---|---|---:|---|")
        hsum = 0
        for r in led.heap_regions:
            hsum += r["size"]
            frm = f"ELF `{r['section']}` symbol `{trunc(r['symbol'], 50)}`" if r["section"] \
                else "firmware constant (not in the ELF)"
            w(f"| {md_escape(r['name'])} | `0x{r['start']:08x}` | {fmt_n(r['size'])} | {frm} |")
        w(f"| **total heap the firmware can reach** | | **{fmt_n(hsum)}** | |")
        w("")

    # Stack
    if led.stack:
        w("## Stack\n")
        w(f"`_stack_end`..`_stack_start` = `0x{led.stack['start']:08x}`..`0x{led.stack['end']:08x}` = "
          f"**{fmt_n(led.stack['size'])} B**. It is the residual of the data window after the statics "
          f"(esp-hal `stack.x`), so it moves one byte for every static byte added. "
          f"The ELF cannot say how much of it a workload uses.\n")

    # Per-category detail
    w("## Category detail\n")
    for cid_, label in CATEGORIES:
        e = cats.get(cid_)
        if not e or e["bytes"] == 0 or cid_ in ("padding", "rom", "cache"):
            continue
        w(f"### {label}: {fmt_n(e['bytes'])} B ({pct(e['bytes'], total)})\n")
        owners = sorted(e["owners"].items(), key=lambda kv: -kv[1])
        shown = owners[:14]
        w("| owner | bytes | % of category |")
        w("|---|---:|---:|")
        for o, n in shown:
            w(f"| {md_escape(trunc(o, 70))} | {fmt_n(n)} | {pct(n, e['bytes'])} |")
        if len(owners) > len(shown):
            rest = sum(n for _, n in owners[len(shown):])
            w(f"| *{len(owners) - len(shown)} more owners* | {fmt_n(rest)} | {pct(rest, e['bytes'])} |")
        w("")
        syms = sorted(e["syms"], key=lambda cn: -cn[1])[:top]
        if syms:
            w(f"Top {len(syms)} symbols (bytes the symbol itself holds in the image):\n")
            w("| bytes | symbol | owner | address |")
            w("|---:|---|---|---|")
            for c, n in syms:
                src = f" `{c.source}`" if c.source else ""
                w(f"| {fmt_n(n)} | `{md_escape(trunc(c.name, 90))}` | {md_escape(trunc(c.owner, 40))}{src} | `0x{c.addr:08x}` |")
            w("")

    unknown_runs = [(a, b) for a, b, cid in led.painter.runs()
                    if led.painter.claims[cid] and led.painter.claims[cid].category == "unknown"]
    if unknown_runs:
        w("### Unknown ranges\n")
        for a, b in unknown_runs:
            w(f"- `0x{a:08x}..0x{b:08x}` ({fmt_n(b - a)} B)")
        w("")

    # RTC
    w("## LP/RTC RAM (separate from the table above)\n")
    rtc_total = sum(s.size for s in led.chip.rtc)
    w("| region | bytes | used | idle |")
    w("|---|---:|---:|---:|")
    for s in led.chip.rtc:
        used = idle = 0
        names: List[str] = []
        for a, b, cid in led.rtc_painter.runs():
            if not (s.start <= a < s.end):
                continue
            c = led.rtc_painter.claims[cid]
            if c and c.category == "rtc":
                used += b - a
                if c.section not in names:
                    names.append(c.section)
            else:
                idle += b - a
        extra = f" ({', '.join(names)})" if names else ""
        w(f"| {s.label} `0x{s.start:08x}` | {fmt_n(s.size)} | {fmt_n(used)}{extra} | {fmt_n(idle)} |")
    w(f"| **total** | **{fmt_n(rtc_total)}** | | |")
    w("")

    # Idle summary
    idle_rows = []
    for cid_ in ("idle", "jit", "cache"):
        e = cats.get(cid_)
        if e:
            for o, n in sorted(e["owners"].items(), key=lambda kv: -kv[1]):
                idle_rows.append((cid_, o, n))
    if idle_rows:
        w("## Idle, reserved and cache bytes\n")
        w("| kind | what | bytes |")
        w("|---|---|---:|")
        for k, o, n in idle_rows:
            w(f"| {k} | {md_escape(o)} | {fmt_n(n)} |")
        w("")

    # Memory map with citations
    w("## Memory map and sources\n")
    w("| range | bytes | what | how it is known |")
    w("|---|---:|---|---|")
    for s in sorted(led.chip.segs, key=lambda s: s.start):
        how = s.src if s.kind == "elf" else f"{s.note}. {s.src}"
        w(f"| `0x{s.start:08x}..0x{s.end:08x}` | {fmt_n(s.size)} | {md_escape(s.label)} | {md_escape(how)} |")
    w("")
    if led.skipped:
        w("Sections not counted as RAM: " + ", ".join(
            f"`{s['section']}` ({s['why']})" for s in led.skipped) + ".\n")
    if led.warnings:
        w("## Warnings\n")
        for x in led.warnings:
            w(f"- {x}")
        w("")
    if led.blob and led.blob.ambiguous:
        w(f"Blob join: {len(led.blob.ambiguous)} symbol names are defined in more than one archive "
          f"(the first archive in name order wins).\n")
    return "\n".join(out)


def render_json(led: Ledger, top: int, min_symbol: int) -> dict:
    chip = led.chip
    cats = summarize(led)
    total = chip.ram_bytes
    cat_json = []
    for cid_, label in CATEGORIES:
        e = cats.get(cid_)
        if not e:
            continue
        item = {
            "id": cid_,
            "label": label,
            "bytes": e["bytes"],
            "owners": [{"owner": o, "bytes": n} for o, n in sorted(e["owners"].items(), key=lambda kv: -kv[1])],
            "top_symbols": [
                {"name": c.name, "bytes": n, "owner": c.owner, "addr": c.addr, "source": c.source,
                 "section": c.section}
                for c, n in sorted(e["syms"], key=lambda cn: -cn[1])[:top]
            ],
        }
        if cid_ == "radio_code":
            item["families"] = [{"family": f, "bytes": n}
                                for f, n in sorted(e["families"].items(), key=lambda kv: -kv[1])]
        cat_json.append(item)
    sym_json = []
    for cid_ in ("code", "radio_code", "data", "bss", "heap"):
        for c, n in cats.get(cid_, {}).get("syms", []):
            if n >= min_symbol:
                sym_json.append({"category": cid_, "name": c.name, "bytes": n, "own_size": c.size, "owner": c.owner,
                                 "addr": c.addr, "section": c.section, "source": c.source, "family": c.family})
    sym_json.sort(key=lambda s: -s["bytes"])
    extents: List[dict] = []
    for a, b, cid in led.painter.runs():
        c = led.painter.claims[cid]
        if c is None:
            extents.append({"start": a, "end": b, "category": "unknown", "owner": "unclaimed", "kind": "unknown"})
            continue
        if extents and extents[-1]["category"] == c.category and extents[-1]["owner"] == c.owner \
                and extents[-1]["end"] == a and extents[-1]["kind"] == c.kind:
            extents[-1]["end"] = b
        else:
            e = {"start": a, "end": b, "category": c.category, "owner": c.owner, "section": c.section, "kind": c.kind}
            if c.between:
                e["between"] = c.between
            extents.append(e)
    grand = sum(c["bytes"] for c in cat_json)
    rtc_rows = []
    for s in chip.rtc:
        used = sum(b - a for a, b, cid in led.rtc_painter.runs()
                   if s.start <= a < s.end and led.rtc_painter.claims[cid]
                   and led.rtc_painter.claims[cid].category == "rtc")
        rtc_rows.append({"label": s.label, "start": s.start, "bytes": s.size, "used": used, "idle": s.size - used})
    unk = cats.get("unknown", {"bytes": 0})["bytes"]
    return {
        "chip": chip.name,
        "image": os.path.basename(led.elf_path),
        "sha256": led.sha256,
        "ram_bytes": total,
        "accounted_bytes": grand,
        "sums_to_ram": grand == total,
        "unknown_bytes": unk,
        "unknown_percent": round(100.0 * unk / total, 4),
        "attribution": led.quality,
        "categories": cat_json,
        "heap_regions": led.heap_regions,
        "stack": led.stack,
        "rtc": {"total_bytes": sum(s.size for s in chip.rtc), "regions": rtc_rows},
        "sections": led.sections,
        "memory_map": [{"start": s.start, "end": s.end, "label": s.label, "kind": s.kind,
                        "category": s.category, "note": s.note, "source": s.src} for s in chip.segs],
        "skipped_sections": led.skipped,
        "warnings": led.warnings,
        "symbols_over_min": sym_json,
        "extents": extents,
    }


# --------------------------------------------------------------------------


def main() -> None:
    ap = argparse.ArgumentParser(description="Whole-SRAM ledger for a firmware ELF.")
    ap.add_argument("elf")
    ap.add_argument("--chip", choices=sorted(CHIPS), help="default: detected from the ELF")
    ap.add_argument("--json", metavar="PATH", help="also write the full ledger as JSON")
    ap.add_argument("--top", type=int, default=8, help="top symbols per category (default 8)")
    ap.add_argument("--min-symbol", type=int, default=256, help="JSON lists symbols at least this big (default 256)")
    ap.add_argument("--blobs", action="append", default=[], metavar="DIR",
                    help="a directory of lib*.a radio blob archives (repeatable; default: the registry copy Cargo.lock pins)")
    ap.add_argument("--no-blobs", action="store_true", help="skip the blob archive join")
    ap.add_argument("--max-unknown-pct", type=float, default=1.0,
                    help="exit 3 when more of the RAM than this is UNKNOWN (default 1.0; an image this map does "
                         "not describe, such as the C6 loader, trips it)")
    args = ap.parse_args()

    try:
        elf = Elf32.open(args.elf)
    except (ElfError, OSError) as e:
        raise SystemExit(str(e))
    chip = CHIPS[args.chip] if args.chip else detect_chip(elf)

    blob = None
    if chip.blob_crate and not args.no_blobs:
        blob = BlobIndex()
        repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        dirs = args.blobs or find_blob_dirs(chip.blob_crate, repo_root)
        for d in dirs:
            blob.add_dir(d)
        if not blob.archives:
            print(f"ram-ledger: WARNING no blob archives found for {chip.blob_crate} "
                  f"(looked under the cargo registry); radio C symbols will be unattributed. "
                  f"Run a cargo fetch, or pass --blobs DIR.", file=sys.stderr)
            blob = None

    led = build_ledger(elf, args.elf, chip, blob)
    sys.stdout.write(render_markdown(led, args.top) + "\n")
    if args.json:
        with open(args.json, "w") as f:
            json.dump(render_json(led, args.top, args.min_symbol), f, indent=1)
            f.write("\n")
    cats = summarize(led)
    total = sum(c["bytes"] for c in cats.values())
    if total != chip.ram_bytes:
        print(f"ram-ledger: ERROR ledger sums to {total}, chip is {chip.ram_bytes}", file=sys.stderr)
        raise SystemExit(2)
    unknown = cats.get("unknown", {"bytes": 0})["bytes"]
    if 100.0 * unknown / chip.ram_bytes > args.max_unknown_pct:
        print(f"ram-ledger: ERROR {unknown} B ({100.0 * unknown / chip.ram_bytes:.2f}%) of RAM is UNKNOWN, over "
              f"--max-unknown-pct {args.max_unknown_pct}: this image is not one the chip map describes",
              file=sys.stderr)
        raise SystemExit(3)


if __name__ == "__main__":
    main()
