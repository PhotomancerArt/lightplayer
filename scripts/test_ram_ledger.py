#!/usr/bin/env python3
"""Tests for scripts/ram-ledger.py and scripts/elf32.py.

    python3 scripts/test_ram_ledger.py        # or: just test-ram-ledger

Two layers, and neither needs a firmware build:

1. Synthetic ELFs built in memory (there is no checked-in ELF fixture small
   enough to reuse): the reader, the Rust v0 name decoder, the `ar` reader, and
   the ledger's own invariants — rows sum to the chip, gaps are classified, a
   heap arena is found.
2. A smoke run of the real script on the newest fetched CI image
   (`just fetch-ci-images`), when there is one. When there is none this says
   SKIPPED on stderr, loudly, and the exit code is still 0: a test that
   could not run has not run, and the output says so.
"""

import glob
import importlib.util
import os
import struct
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
sys.path.insert(0, HERE)

from elf32 import Elf32, SHF_ALLOC, SHF_WRITE, SHT_NOBITS, SHT_SYMTAB  # noqa: E402

_spec = importlib.util.spec_from_file_location("ram_ledger", os.path.join(HERE, "ram-ledger.py"))
rl = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rl)  # type: ignore[union-attr]

STT_OBJECT, STT_FUNC = 1, 2


def make_elf(sections, symbols, machine=0x5E):
    """sections: [(name, type, flags, addr, size)]; symbols: [(name, value, size, kind, bind, section name)]."""
    names = [""] + [s[0] for s in sections] + [".symtab", ".strtab", ".shstrtab"]
    shstr = b"\0"
    name_off = {"": 0}
    for n in names[1:]:
        name_off[n] = len(shstr)
        shstr += n.encode() + b"\0"
    strtab = b"\0"
    stroff = {}
    for sy in symbols:
        stroff[sy[0]] = len(strtab)
        strtab += sy[0].encode() + b"\0"
    sec_index = {s[0]: i + 1 for i, s in enumerate(sections)}
    symtab = b"\0" * 16
    for name, value, size, kind, bind, sec in symbols:
        symtab += struct.pack("<IIIBBH", stroff[name], value, size, (bind << 4) | kind, 0, sec_index[sec])
    body = b""
    off = 52
    # symtab, strtab, shstrtab data follow the header
    symtab_off = off
    body += symtab
    strtab_off = symtab_off + len(symtab)
    body += strtab
    shstr_off = strtab_off + len(strtab)
    body += shstr
    shoff = shstr_off + len(shstr)
    shdrs = [struct.pack("<10I", 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)]
    for name, typ, flags, addr, size in sections:
        shdrs.append(struct.pack("<10I", name_off[name], typ, flags, addr, 0, size, 0, 0, 4, 0))
    n_user = len(sections)
    shdrs.append(struct.pack("<10I", name_off[".symtab"], SHT_SYMTAB, 0, 0, symtab_off, len(symtab),
                             n_user + 2, 0, 4, 16))
    shdrs.append(struct.pack("<10I", name_off[".strtab"], 3, 0, 0, strtab_off, len(strtab), 0, 0, 1, 0))
    shdrs.append(struct.pack("<10I", name_off[".shstrtab"], 3, 0, 0, shstr_off, len(shstr), 0, 0, 1, 0))
    ehdr = b"\x7fELF" + bytes([1, 1, 1]) + b"\0" * 9
    ehdr += struct.pack("<HHIIIIIHHHHHH", 1, machine, 1, 0, 0, shoff, 0, 52, 0, 0, 40, len(shdrs), len(shdrs) - 1)
    return ehdr + body + b"".join(shdrs)


def synthetic_chip():
    return rl.Chip(
        name="synthetic",
        title="synthetic",
        ram_bytes=0x10000,
        segs=[
            rl.Seg(0x10000, 0x1F000, "app", "elf", fill="idle", src="test"),
            rl.Seg(0x1F000, 0x20000, "rom", "fixed", "rom", "ROM", "test", "test"),
        ],
        rtc=[rl.Seg(0x50000000, 0x50000400, "rtc", "elf", fill="idle", src="test")],
        flash_ranges=[(0x42000000, 0x43000000)],
    )


def synthetic_elf_bytes():
    rw = SHF_ALLOC | SHF_WRITE
    sections = [
        (".data", 1, rw, 0x10000, 0x100),
        (".bss", SHT_NOBITS, rw, 0x10100, 0x2000),
        (".stack", SHT_NOBITS, rw, 0x12100, 0x100),
        (".rtc_fast.persistent", SHT_NOBITS, rw, 0x50000000, 0x40),
        (".text", 1, SHF_ALLOC | 4, 0x42000000, 0x1000),
    ]
    symbols = [
        ("_RNvCs1_4mycr3FOO", 0x10000, 0x40, STT_OBJECT, 1, ".data"),
        ("_RNvCs1_4mycr3BAR", 0x10050, 0x40, STT_OBJECT, 1, ".data"),  # 16 B gap: inferred
        ("_RNvCs1_4mycr3BAZ", 0x10094, 0x40, STT_OBJECT, 1, ".data"),  # 4 B gap: padding
        ("_RNvCs1_4mycr4HEAP", 0x10100, 0x1000, STT_OBJECT, 1, ".bss"),
        ("blobfn", 0x11200, 0x20, STT_OBJECT, 1, ".bss"),  # after the arena
    ]
    return make_elf(sections, symbols)


class ReaderTests(unittest.TestCase):
    def test_sections_and_symbols(self):
        elf = Elf32(synthetic_elf_bytes())
        names = [s.name for s in elf.sections]
        self.assertIn(".data", names)
        self.assertEqual(elf.section_named(".bss").size, 0x2000)
        self.assertTrue(elf.section_named(".bss").nobits)
        syms = {s.name: s for s in elf.symbols()}
        self.assertEqual(syms["_RNvCs1_4mycr4HEAP"].size, 0x1000)
        self.assertEqual(elf.symbol_values(["blobfn", "nope"]), {"blobfn": 0x11200, "nope": None})

    def test_rejects_non_elf(self):
        with self.assertRaises(Exception):
            Elf32(b"not an elf at all")


class NameTests(unittest.TestCase):
    def test_v0_inherent_impl(self):
        n = rl.rust_name("_RNvMCs6foylQ03DXg_15lps_builtin_idsNtB2_9BuiltinId4name")
        self.assertEqual(n.owner, "lps_builtin_ids")
        self.assertEqual(n.pretty, "<lps_builtin_ids::BuiltinId>::name")

    def test_v0_plain_static(self):
        n = rl.rust_name("_RNvNtNtNtCsguvEde6iiQr_10fw_esp32c65board7esp32c64init9HEAP_MAIN")
        self.assertEqual(n.pretty, "fw_esp32c6::board::esp32c6::init::HEAP_MAIN")
        self.assertEqual(n.owner, "fw_esp32c6")

    def test_legacy(self):
        n = rl.rust_name("_ZN4core3fmt5write17h0123456789abcdefE")
        self.assertEqual((n.pretty, n.owner), ("core::fmt::write", "core"))

    def test_c_symbol_is_not_rust(self):
        self.assertIsNone(rl.rust_name("ieee80211_output_process"))


class ArchiveTests(unittest.TestCase):
    def test_join_by_symbol_name(self):
        obj = make_elf([(".text", 1, SHF_ALLOC | 4, 0, 0x20)], [("blobfn", 0, 0x20, STT_FUNC, 1, ".text")], machine=0xF3)
        hdr = b"obj.o/".ljust(16) + b"0".ljust(12) + b"0".ljust(6) + b"0".ljust(6) + b"644".ljust(8) \
            + str(len(obj)).encode().ljust(10) + b"`\n"
        ar = b"!<arch>\n" + hdr + obj + (b"\n" if len(obj) & 1 else b"")
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "libtest.a")
            with open(p, "wb") as f:
                f.write(ar)
            idx = rl.BlobIndex()
            idx.add_archive(p)
        self.assertEqual(idx.defs["blobfn"], ("libtest.a", "obj.o"))


class LedgerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.path = os.path.join(self.tmp.name, "synthetic.elf")
        with open(self.path, "wb") as f:
            f.write(synthetic_elf_bytes())
        self.blob = rl.BlobIndex()
        self.blob.defs["blobfn"] = ("libtest.a", "obj.o")
        self.chip = synthetic_chip()
        self.led = rl.build_ledger(Elf32.open(self.path), self.path, self.chip, self.blob)
        self.cats = rl.summarize(self.led)

    def tearDown(self):
        self.tmp.cleanup()

    def test_rows_sum_to_the_chip(self):
        self.assertEqual(sum(c["bytes"] for c in self.cats.values()), self.chip.ram_bytes)

    def test_expected_categories(self):
        b = {k: v["bytes"] for k, v in self.cats.items()}
        self.assertEqual(b["rom"], 0x1000)
        self.assertEqual(b["heap"], 0x1000)
        self.assertEqual(b["padding"], 4)
        self.assertEqual(b["data"], 0x100 - 4)
        self.assertEqual(b["stack"], 0x100)
        self.assertEqual(b["bss"], 0x2000 - 0x1000)
        self.assertEqual(b["idle"], 0x1F000 - 0x12200)
        self.assertNotIn("unknown", b)

    def test_gap_between_one_owners_symbols_is_inferred(self):
        self.assertEqual(self.led.quality[dict(rl.QUALITY_ORDER)["inferred"]], 16)

    def test_heap_arena_found_by_symbol(self):
        self.assertEqual([r["size"] for r in self.led.heap_regions], [0x1000])

    def test_blob_symbol_owned_by_its_archive(self):
        self.assertIn("blob:libtest.a", self.cats["bss"]["owners"])

    def test_markdown_and_json_render(self):
        md = rl.render_markdown(self.led, 5)
        self.assertIn("**OK**", md)
        j = rl.render_json(self.led, 5, 16)
        self.assertTrue(j["sums_to_ram"])
        self.assertEqual(j["unknown_bytes"], 0)
        self.assertEqual(j["rtc"]["regions"][0]["used"], 0x40)

    def test_unclaimed_bytes_outside_idle_windows_are_unknown(self):
        chip = synthetic_chip()
        chip.segs[0].fill = "unknown"
        led = rl.build_ledger(Elf32.open(self.path), self.path, chip, self.blob)
        unknown = rl.summarize(led)["unknown"]["bytes"]
        self.assertEqual(unknown, 0x1F000 - 0x12200)


class SmokeOnCiImage(unittest.TestCase):
    """The real script on the newest fetched CI images, when present."""

    def newest(self, pattern):
        root = os.environ.get("LP_CI_IMAGES")
        pats = [os.path.join(root, pattern)] if root else []
        pats.append(os.path.join(REPO, "target", "ci-images", "*", pattern))
        found = [p for pat in pats for p in glob.glob(pat)]
        return max(found, key=os.path.getmtime) if found else None

    def run_script(self, pattern, chip):
        elf = self.newest(pattern)
        if elf is None:
            sys.stderr.write(f"\n*** SKIPPED: no CI image for {chip} (looked for {pattern}); "
                             f"run `just fetch-ci-images` — this smoke test has NOT RUN ***\n")
            self.skipTest("no CI image fetched")
        r = subprocess.run([sys.executable, os.path.join(HERE, "ram-ledger.py"), elf],
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("**OK**", r.stdout)
        self.assertIn(f"# RAM ledger: {chip}", r.stdout)

    def test_c6(self):
        self.run_script("esp32c6/tree/ESP32C6_SERVER_RADIO_SPLIT/p2.elf", "esp32c6")

    def test_classic(self):
        self.run_script("esp32v3/fw-esp32v3-shipped.elf", "esp32v3")

    def test_s3(self):
        self.run_script("esp32s3/fw-esp32s3-shipped.elf", "esp32s3")


if __name__ == "__main__":
    unittest.main(verbosity=2)
