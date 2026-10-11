"""A 32-bit little-endian ELF reader: section headers and the symbol table.

The one reader behind `scripts/heap-budget-stack-layout.py` and
`scripts/ram-ledger.py`. Pure stdlib, no `readelf`/`nm` (neither is on a stock
macOS, and a host `llvm-nm` is not guaranteed on every runner). It reads the
RV32 (ESP32-C6) and Xtensa (ESP32-S3, classic ESP32) images alike: both are
ELFCLASS32/ELFDATA2LSB. Shape after `scripts/emu/elf-section-digest.py`.

    elf = Elf32.open(path)
    for s in elf.sections: ...          # Section(name, type, flags, addr, ...)
    for sym in elf.symbols(): ...       # Symbol(name, value, size, kind, bind, shndx)
"""

import struct
from dataclasses import dataclass
from typing import Dict, Iterator, List, Optional

SHT_SYMTAB = 2
SHT_NOBITS = 8
SHF_WRITE = 0x1
SHF_ALLOC = 0x2
SHF_EXECINSTR = 0x4

STT_NOTYPE = 0
STT_OBJECT = 1
STT_FUNC = 2
STT_SECTION = 3
STT_FILE = 4

STB_LOCAL = 0
STB_GLOBAL = 1
STB_WEAK = 2

SHN_UNDEF = 0
SHN_ABS = 0xFFF1
SHN_COMMON = 0xFFF2


@dataclass(frozen=True)
class Section:
    index: int
    name: str
    type: int
    flags: int
    addr: int
    offset: int
    size: int
    link: int

    @property
    def alloc(self) -> bool:
        return bool(self.flags & SHF_ALLOC)

    @property
    def nobits(self) -> bool:
        return self.type == SHT_NOBITS

    @property
    def end(self) -> int:
        return self.addr + self.size


@dataclass(frozen=True)
class Symbol:
    name: str
    value: int
    size: int
    kind: int  # STT_*
    bind: int  # STB_*
    shndx: int


class ElfError(Exception):
    pass


class Elf32:
    def __init__(self, blob: bytes, label: str = "<elf>") -> None:
        self.blob = blob
        self.label = label
        if blob[:4] != b"\x7fELF":
            raise ElfError(f"{label}: not an ELF")
        if blob[4] != 1 or blob[5] != 1:
            raise ElfError(f"{label}: only 32-bit little-endian ELF is supported")
        self.e_machine = struct.unpack_from("<H", blob, 0x12)[0]
        e_shoff = struct.unpack_from("<I", blob, 0x20)[0]
        e_shentsize, e_shnum, e_shstrndx = struct.unpack_from("<HHH", blob, 0x2E)
        raw = []
        for i in range(e_shnum):
            off = e_shoff + i * e_shentsize
            # name, type, flags, addr, offset, size, link
            raw.append(struct.unpack_from("<IIIIIII", blob, off))
        shstr_off = raw[e_shstrndx][4] if raw else 0
        self.sections: List[Section] = [
            Section(i, self._cstr(shstr_off, h[0]), h[1], h[2], h[3], h[4], h[5], h[6])
            for i, h in enumerate(raw)
        ]

    @classmethod
    def open(cls, path: str) -> "Elf32":
        with open(path, "rb") as f:
            return cls(f.read(), path)

    def _cstr(self, table_off: int, idx: int) -> str:
        end = self.blob.index(b"\0", table_off + idx)
        return self.blob[table_off + idx : end].decode("utf-8", "replace")

    def section_named(self, name: str) -> Optional[Section]:
        for s in self.sections:
            if s.name == name:
                return s
        return None

    def symbols(self) -> Iterator[Symbol]:
        """Every named entry of every `.symtab`, in table order."""
        for h in self.sections:
            if h.type != SHT_SYMTAB:
                continue
            str_off = self.sections[h.link].offset
            for k in range(h.size // 16):
                st_name, st_value, st_size, st_info, _other, st_shndx = struct.unpack_from(
                    "<IIIBBH", self.blob, h.offset + k * 16
                )
                if st_name == 0:
                    continue
                yield Symbol(
                    self._cstr(str_off, st_name),
                    st_value,
                    st_size,
                    st_info & 0xF,
                    st_info >> 4,
                    st_shndx,
                )

    def symbol_values(self, wanted: List[str]) -> Dict[str, Optional[int]]:
        """The value of each named symbol (first definition wins), or None."""
        found: Dict[str, Optional[int]] = {n: None for n in wanted}
        for sym in self.symbols():
            if sym.name in found and found[sym.name] is None:
                found[sym.name] = sym.value
        return found
