#!/usr/bin/env python3
"""Aggregate `flash-tears` transcripts into tear-shape histograms.

    scripts/emu/flash-tears-analyze.py                      # every committed transcript
    scripts/emu/flash-tears-analyze.py <transcript.txt>...  # just these
    scripts/emu/flash-tears-analyze.py --write-report docs/reports/2026-10-08-c6-nor-tear-calibration.md
    scripts/emu/flash-tears-analyze.py --json               # one JSON object per cut, for a notebook-free pipe
    scripts/emu/flash-tears-analyze.py --model-table        # lp-nor-sim's calibrated model, as Rust
    scripts/emu/flash-tears-analyze.py --check-model        # are lp-nor-sim's shares confirmed? (DD29, see --help)
    scripts/emu/flash-tears-analyze.py --rom-split <trace>  # how the emulated ROM split the unaligned writes

The `flash-tears` payload (`lp-fw/fw-checks/src/checks/flash_tears/`) prints,
on every boot, one `[fw-check-json]` record per region sector. This reads
those records back — it never edits a transcript — and sorts each power cut's
in-flight sector into a **physical phase**: where in the erase or the program
the power went. The firmware's own `verdict` is reported beside it, because
the verdicts were named before any real cut had been seen and two of them
(`mixed`, and `torn-program` on a sector the program never reached) turn out
to name phases of the *erase* (see the calibration report).

Two things are reconstructed here rather than read:

- **The patterns.** `pattern()` is `fill_pattern` (`pattern.rs`) in Python.
  Every record's `old_zero_bits` and `new_zero_bits` are checked against it,
  so a drift between the two implementations fails loudly instead of
  mis-sorting a cut.
- **Where a zero run from the front stops**, for records written before the
  firmware counted `leading_zero_bytes` (3cfd41265): the per-page count of
  new zeros landed, laid against the pattern, pins the byte the run ended at
  (or a short range of them, where the pattern has `0xFF` bytes).

**Two payloads.** `flash-tears` programs each sector page by page;
`flash-tears-unaligned` programs it as a 20-byte write and then 16-1040-byte
writes starting at `20 + k*4`, never on a 32-byte boundary (`program_plan.rs`, mirrored by `plan()`
here and checked against the `writes` its in-flight records list). Their
transcripts sit in sibling directories and are reported apart: only the
page-aligned silicon cuts give `--model-table` its numbers. For the
unaligned payload a torn program is judged by where its prefix stops against
the write it was in: on a command counted from the write's address inside its
first page and from the page after it (the split the emulated mask ROM makes,
`--rom-split`), on an absolute 32-byte boundary, on a 32-byte step from the
write's address all the way, or none of them.

With `--write-report PATH` the generated tables replace the block between
`<!-- flash-tears-analyze:begin -->` and `<!-- flash-tears-analyze:end -->`
in PATH; the prose around it is left alone. Re-run it after every batch.
"""

from __future__ import annotations

CHECK_MODEL_RULE = """\
--check-model (DD29, corrected by DD32) -- is lp-nor-sim's TearMix::CX1 confirmed by the silicon cuts?

  1. Per payload. Only the page-aligned `flash-tears` silicon cuts feed the
     model's shares. `flash-tears-unaligned` silicon cuts are counted and
     printed apart; they neither confirm nor refute the erase mix.
  2. Per family. Which operation a cut tore is the workload's, so the erase
     shapes are compared among the cuts at an erase and the program shapes
     among the torn programs. The erase family also holds `cut:before-erase`
     (both journal copies name the cycle, the sector is old and whole: the
     cut landed between the journal write and the erase command); the model
     draws it as where an erase cut lands (`cut_before_erase`), not as a
     shape. A cut after the program finished ("complete") is not a torn
     operation and is left out, and so is a cut whose operation in flight was
     the journal's own (`journal:*`): its shape is read only as slot counts.
  3. Confirmed when EVERY shape's observed share lies inside its Wilson score
     interval around the MODEL's share, at the observed n of its family.
     Bonferroni per family (DD32): a family tested in k cells puts each cell's
     interval at the 1 - 0.05/k level, so an honest sample of that family
     misses somewhere about 5 % of the time (k = 1 is the plain 95 %).
     k counts the cells after pooling (rule 4), not the NOT MODELLED ones.
  4. Pooling: a shape expected fewer than 5 times at that n (model share x n)
     is pooled with the other such shapes into one cell. A pool (or a lone thin
     shape) still under 5 joins the thinnest remaining cell, so no cell is
     tested on fewer than 5 expected cuts.
  5. A silicon class the model has no shape for (untouched, erase:old-left,
     program:scattered, ...), or one whose weight is 0, is never pooled away:
     seeing one is a failure.
  6. Otherwise print the re-fit table (what --model-table prints) and exit 1.

Only the shares are judged; the CX1_ERASING and CX1_READS_FF_WEAK residue
tables are not (they are drawn values, not shares).
"""

import argparse
import glob
import json
import os
import statistics
import sys
from collections import Counter, defaultdict
from dataclasses import dataclass, field

# The payload's geometry (`flash_tears/mod.rs`).
SECTOR = 4096
PAGE = 256
PAGES = SECTOR // PAGE
REGION = 16
# One program command on the C6's flash path: the mask ROM issues a 256-byte
# page as eight 32-byte commands (`analysis.rs`, `PROGRAM_CHUNK`).
COMMAND = 32
# A 32-bit word: a candidate for the part's internal program unit.
WORD = 4
SCAN_READS = 8

JSON_TAG = "[fw-check-json] "
SCAN_DONE = "[flash-tears] === SCAN DONE ==="
DEFAULT_GLOB = "lp-emu/transcripts/esp32c6/flash-tears*/*.txt"
PAYLOAD = "flash-tears"
PAYLOAD_UNALIGNED = "flash-tears-unaligned"
BEGIN = "<!-- flash-tears-analyze:begin -->"
END = "<!-- flash-tears-analyze:end -->"

# A sector whose stable zeros are at most this fraction of its bits is an
# erase's tail: too few to be a program's prefix, too spread to be old data.
RESIDUE_FRACTION = 0.01

M64 = (1 << 64) - 1


# --------------------------------------------------------------------------
# The pattern, as `pattern.rs` writes it.


def splitmix(x: int) -> int:
    z = (x + 0x9E37_79B9_7F4A_7C15) & M64
    z = ((z ^ (z >> 30)) * 0xBF58_476D_1CE4_E5B9) & M64
    z = ((z ^ (z >> 27)) * 0x94D0_49BB_1331_11EB) & M64
    return z ^ (z >> 31)


_patterns: dict[tuple[int, bool], bytes] = {}


def pattern(sector: int, cycle: int) -> bytes:
    invert = (cycle // REGION) % 2 == 1
    key = (sector, invert)
    if key not in _patterns:
        state = splitmix(0xF1A5_7EA2_0000_0000 ^ sector)
        out = bytearray()
        for _ in range(SECTOR // 8):
            state = splitmix(state)
            word = (~state & M64) if invert else state
            out += word.to_bytes(8, "little")
        _patterns[key] = bytes(out)
    return _patterns[key]


def zeros(b: int) -> int:
    return 8 - bin(b).count("1")


# The unaligned program plan, as `program_plan.rs` writes it.
UNALIGNED_FIRST = 20
UNALIGNED_STEP = 4
UNALIGNED_MIN_WRITE = 16
UNALIGNED_SHORT_MAX = 272
UNALIGNED_MAX_WRITE = 1040


def plan(sector: int, cycle: int) -> list[tuple[int, int]]:
    """`program_plan::writes(ProgramMode::Unaligned, sector, cycle)`."""
    state = splitmix((0x0A11_6AED_0000_0000 ^ (sector << 32) ^ cycle) & M64)
    out = [(0, UNALIGNED_FIRST)]
    at = UNALIGNED_FIRST
    while at < SECTOR:
        state = splitmix(state)
        cap = UNALIGNED_SHORT_MAX if state & 3 else UNALIGNED_MAX_WRITE
        choices = (cap - UNALIGNED_MIN_WRITE) // UNALIGNED_STEP + 1
        n = UNALIGNED_MIN_WRITE + ((state >> 2) % choices) * UNALIGNED_STEP
        if (at + n) % COMMAND == 0:
            n = n + UNALIGNED_STEP if n + UNALIGNED_STEP <= cap else n - UNALIGNED_STEP
        n = min(n, SECTOR - at)
        out.append((at, n))
        at += n
    return out


# --------------------------------------------------------------------------
# Reading transcripts.


@dataclass
class Boot:
    transcript: str
    index: int
    boot: dict
    journals: list = field(default_factory=list)
    sectors: list = field(default_factory=list)
    summary: dict | None = None
    repair: dict | None = None
    timing: dict | None = None
    scan_done_next: int | None = None

    @property
    def in_flight(self) -> dict | None:
        return next((s for s in self.sectors if s.get("role") == "in-flight"), None)


@dataclass
class Transcript:
    path: str
    meta: dict
    boots: list
    payload: str = PAYLOAD


def read_transcript(path: str) -> Transcript:
    meta_path = path + ".meta.json"
    meta = {}
    if os.path.exists(meta_path):
        with open(meta_path) as f:
            meta = json.load(f)
    boots: list[Boot] = []
    payload = meta.get("payload")
    with open(path, "rb") as f:
        for raw in f:
            line = raw.decode("utf-8", errors="replace").rstrip("\r\n")
            if payload is None and "[fw-checks-header] " in line:
                payload = json.loads(line.split("[fw-checks-header] ", 1)[1]).get("payload")
            if SCAN_DONE in line and boots:
                tail = line.split("next=", 1)
                if len(tail) == 2 and tail[1].strip().isdigit():
                    boots[-1].scan_done_next = int(tail[1].strip())
                continue
            if JSON_TAG not in line:
                continue
            rec = json.loads(line.split(JSON_TAG, 1)[1])
            kind = rec.get("kind")
            if kind == "ft-boot":
                boots.append(Boot(path, len(boots), rec))
            elif not boots:
                continue
            elif kind == "ft-journal":
                boots[-1].journals.append(rec)
            elif kind == "ft-sector":
                boots[-1].sectors.append(rec)
            elif kind == "ft-summary":
                boots[-1].summary = rec
            elif kind == "ft-repair":
                boots[-1].repair = rec
            elif kind == "ft-timing":
                boots[-1].timing = rec
    return Transcript(path, meta, boots, payload or PAYLOAD)


# The journal's geometry (`journal.rs`): 256 slots a copy, cycle `c` in slot
# `(c + copy * 128) % 256`, a copy erased just before its slot 0 is written.
JOURNAL_SLOTS = SECTOR // 16


def slot_of(copy: int, cycle: int) -> int:
    return (cycle + copy * (JOURNAL_SLOTS // 2)) % JOURNAL_SLOTS


def attribute_journal(b: Boot, ph: Phase) -> Phase:
    """Which operation was in flight, when the journal says it was its own.

    A cycle writes copy 0, then copy 1, then erases and programs its sector
    (`runner.rs`'s `run_cycle`). The cycle the journal names is the highest
    whole entry in either copy, so:

    - the in-flight sector reads its last cycle **whole** (`complete`) when
      the cut came before copy 0 named the next cycle: in copy 0's wrap (its
      erase before slot 0, or that slot's entry: copy 0 then no longer holds
      the cycle copy 1 names), or in copy 0's entry program (a torn next
      slot) — or, copy 0 whole and its next slot blank, after the program;
    - it reads its **old** pattern whole when copy 0 named it and the erase
      had not moved a cell: in copy 1's wrap or copy 1's entry program (copy
      1 does not hold the cycle yet), or — both copies holding it — between
      the journal write and the erase command.

    Returns `ph` re-classed, with the damaged journal slots the op in flight
    accounts for in `journal_slots_in_flight`. Damage nothing here explains
    stays outside the operation in flight."""
    if ph.cls not in ("complete", "untouched") or len(b.journals) < 2:
        return ph
    latest = b.boot.get("latest")
    if latest is None:
        return ph
    j = {r["copy"]: r for r in b.journals}
    if ph.cls == "complete":
        copy, cycle = 0, latest + 1
    else:
        copy, cycle = 1, latest
    rec = j[copy]
    holds = rec.get("latest") == latest
    d = dict(ph.detail, journal_copy=copy)
    if ph.cls == "untouched" and holds and j[0].get("latest") == latest and not rec.get("damaged"):
        return Phase("cut:before-erase", "erase", d)
    if slot_of(copy, cycle) == 0 and (not holds or rec.get("damaged", 0)):
        d["journal_slots_in_flight"] = rec.get("damaged", 0)
        return Phase("journal:wrap-erase", "journal", d)
    if rec.get("torn_next") and (holds or ph.cls == "untouched"):
        d["journal_slots_in_flight"] = 1
        return Phase("journal:program", "journal", d)
    if ph.cls == "untouched" and not holds:
        # Copy 1's entry for the cycle had not landed at all (its slot blank).
        d["journal_slots_in_flight"] = 0
        return Phase("journal:program", "journal", d)
    return ph


def is_cut(b: Boot) -> bool:
    """A boot after a power cut in the work loop: a power-on reset into a
    region the journal says was mid-cycle."""
    return b.boot.get("reset") == "poweron" and b.boot.get("state") == "resume"


def not_a_cut_reason(b: Boot) -> str:
    if b.boot.get("state") != "resume":
        return "fresh region (first boot, nothing in flight)"
    return f"reset `{b.boot.get('reset')}`, not a power cut"


# --------------------------------------------------------------------------
# Sorting one in-flight sector into a phase.


@dataclass
class Phase:
    cls: str  # the histogram's row
    phase: str  # erase / program / none
    detail: dict


# A cut whose operation in flight was the journal's own (`journal.rs`), not
# the region sector's: the in-flight sector then reads its last cycle whole
# (complete) or its old pattern whole, and the journal says which op it was.
# Their shape is read only as slot counts, so they feed no model share.
JOURNAL_ROWS = [
    ("journal:wrap-erase", "journal",
     "cut in a journal copy's wrap: its erase before slot 0 (or that slot's first entry); the other copy "
     "kept the cycle"),
    ("journal:program", "journal", "cut in a journal entry's program (between the two copies' writes)"),
]
PHASE_ORDER = [
    ("cut:before-erase", "erase",
     "cut before the erase started: both journal copies name the cycle and the sector holds the old "
     "pattern whole (the cut landed between the journal write and the erase command)"),
    ("untouched", "erase", "untouched: the old pattern, whole, and the journal does not say where the cut landed"),
    ("erase:zeroing", "erase", "torn erase, zeroing: a run of `0x00` from the front, old after it"),
    ("erase:all-zero", "erase", "torn erase, all `0x00`"),
    ("erase:erasing", "erase", "torn erase, erasing: stable zeros spread over old **and** new zero positions"),
    ("erase:old-left", "erase", "torn erase, old data left (only old zeros, more than a residue)"),
    ("erase:reads-ff-weak", "erase", "torn erase that reads all `0xFF` but has weak bits"),
    ("erased", "erase", "erased: all `0xFF`, no weak bits"),
    ("program:command-boundary", "program", "torn program, prefix ending on a 32-byte command boundary"),
    ("program:mid-command", "program", "torn program, prefix ending inside a 32-byte command"),
    ("program:mid-word", "program",
     "torn program, whole words then one word only part-landed (some of its clears, maybe weak bits), "
     "nothing after"),
    ("program:scattered", "program", "torn program, scattered clears (no prefix)"),
    ("complete", "program", "complete: the new pattern, whole"),
    *JOURNAL_ROWS,
    ("unknown", "none", "unknown"),
]
# The unaligned payload's rows: the erase rows are the same; a torn program
# is sorted by where its prefix stops against the write it was in.
PHASE_ORDER_UNALIGNED = [
    *[row for row in PHASE_ORDER if row[1] == "erase"],
    ("program:between-writes", "program",
     "torn program, prefix ending where a write starts (a cut between two ROM calls)"),
    ("program:command-boundary", "program",
     "torn program, prefix ending on a ROM command boundary (32 B from the write's address in its "
     "first page, 32 B from the page after)"),
    ("program:mid-command", "program", "torn program, prefix ending on a 4-byte word inside a command"),
    ("program:mid-word", "program", "torn program, prefix ending inside a 4-byte word (or a partial byte)"),
    ("program:scattered", "program", "torn program, scattered clears (no prefix)"),
    ("complete", "program", "complete: the new pattern, whole"),
    *JOURNAL_ROWS,
    ("unknown", "none", "unknown"),
]
PHASE_TEXT = {k: t for k, _, t in PHASE_ORDER + PHASE_ORDER_UNALIGNED}
PHASE_OF = {k: p for k, p, _ in PHASE_ORDER + PHASE_ORDER_UNALIGNED}


def phase_order(payload: str) -> list:
    return PHASE_ORDER_UNALIGNED if payload == PAYLOAD_UNALIGNED else PHASE_ORDER


def unaligned_end(r: dict, new: bytes, writes: list[tuple[int, int]]) -> dict:
    """Where an unaligned torn program's prefix stops, judged against the
    write it was in and every alignment hypothesis.

    The prefix's last landed clear is at `landed_extent - 1`; bytes after it
    that the pattern leaves at `0xFF` need no clear, so with no partial byte
    the prefix may have run on through them: the end is a range
    `[landed_extent, next byte with a clear]`, and a hypothesis fits when any
    end in the range is one of its boundaries."""
    p = r["program"]
    extent, partial = p["landed_extent"], p["partial_bytes"]
    if partial:
        ends = [extent]  # inside byte extent-1: no boundary fits
    else:
        hi = extent
        while hi < SECTOR and new[hi] == 0xFF:
            hi += 1
        ends = list(range(extent, hi + 1))
    starts = {a for a, _ in writes}

    def write_of(e: int) -> tuple[int, int]:
        # The write the prefix stopped inside (`e` past its start).
        return next((a, n) for a, n in writes if a < e <= a + n)

    def rom(e: int) -> bool:
        a, _ = write_of(e)
        first_page_end = (a // PAGE + 1) * PAGE
        if e <= first_page_end:
            return (e - a) % COMMAND == 0 or e == first_page_end
        return e % COMMAND == 0

    between = [e for e in ends if e in starts or e == SECTOR]
    inside = [e for e in ends if e not in starts and e != SECTOR]
    d = {
        "end": ends[0] if len(ends) == 1 else (ends[0], ends[-1]),
        "partial_bytes": partial,
        "fits_rom": any(rom(e) for e in inside),
        "fits_absolute": any(e % COMMAND == 0 for e in inside),
        "fits_relative": any((e - write_of(e)[0]) % COMMAND == 0 for e in inside),
        "on_word": (not partial) and any(e % WORD == 0 for e in ends),
    }
    if inside:
        a, n = write_of(inside[0])
        d.update(write=(a, n), offset_in_write=inside[0] - a, in_first_page=inside[0] <= (a // PAGE + 1) * PAGE,
                 end_mod_32=inside[0] % COMMAND, rel_mod_32=(inside[0] - a) % COMMAND)
    if between and not inside:
        cls = "program:between-writes"
    elif partial or not d["on_word"]:
        cls = "program:mid-word"
    elif d["fits_rom"]:
        cls = "program:command-boundary"
    else:
        cls = "program:mid-command"
    d["between_writes"] = bool(between)
    return {"class": cls, **d}


def leading_zero_run(r: dict, new: bytes) -> tuple[int, int] | None:
    """Where a zero run from the front stops, as a byte range `[lo, hi]`.

    Exact when the firmware counted it; otherwise from the pages: whole pages
    of new zeros landed, one partial page, then none — and the partial
    page's count laid against the pattern."""
    if "leading_zero_bytes" in r:
        n = r["leading_zero_bytes"]
        return (n, n)
    landed = r["page_landed"]
    full = [sum(zeros(b) for b in new[p * PAGE : (p + 1) * PAGE]) for p in range(PAGES)]
    p = 0
    while p < PAGES and landed[p] == full[p]:
        p += 1
    if p == PAGES:
        return (SECTOR, SECTOR)
    if any(landed[q] != 0 for q in range(p + 1, PAGES)):
        return None
    want = landed[p]
    base = p * PAGE
    acc = 0
    lo = hi = None
    for n in range(0, PAGE + 1):
        if acc == want:
            lo = base + n if lo is None else lo
            hi = base + n
        if n == PAGE:
            break
        acc += zeros(new[base + n])
        if acc > want and lo is not None:
            break
    if lo is None:
        # The run ended inside a byte: some of its bits zeroed, not all.
        acc = 0
        for n in range(PAGE):
            nxt = acc + zeros(new[base + n])
            if acc < want < nxt:
                return (base + n, base + n + 1)
            acc = nxt
        return None
    return (lo, hi)


def mid_word(r: dict, new: bytes) -> dict | None:
    """A page-aligned torn program that stopped INSIDE a 4-byte word: whole
    bytes up to a word, then that word only part-landed (a partial byte, or
    clears missing anywhere in it), and nothing after it. `None` for every
    other torn program.

    The firmware names two such cuts differently: `scattered` when the word
    has more than one partial byte, `byte-prefix` when only its last byte is
    partial. Both are this one shape at bit level: the part's program unit
    was caught part-way."""
    p = r.get("program") or {}
    shape, extent = p.get("shape"), p.get("landed_extent", 0)
    prefix, partial = p.get("prefix_bytes", 0), p.get("partial_bytes", 0)
    if shape not in ("scattered", "byte-prefix") or p.get("stray_bits", 0) or extent == 0:
        return None
    at = prefix - prefix % WORD
    if extent > at + WORD:
        return None  # clears landed past the word: scattered for real
    if shape == "byte-prefix" and not partial:
        return None  # whole bytes: a prefix that ended on a byte (mid-command judges its word)
    intended = sum(zeros(b) for b in new[at : at + WORD])
    landed = r["landed_new_zeros"] - sum(zeros(b) for b in new[:at])
    if not 0 <= landed < intended:
        return None
    return {
        "end": at,
        "word_at": at,
        "word_intended_bits": intended,
        "word_landed_bits": landed,
        "landed_extent": extent,
        "partial_bytes": partial,
        "on_word_boundary": False,
    }


def classify(r: dict, payload: str = PAYLOAD) -> Phase:
    sector, wrote = r["sector"], r["wrote"]
    old = pattern(sector, wrote - REGION)
    new = pattern(sector, wrote)
    old_zero = sum(zeros(b) for b in old)
    new_zero = sum(zeros(b) for b in new)
    if (old_zero, new_zero) != (r["old_zero_bits"], r["new_zero_bits"]):
        raise SystemExit(
            f"PATTERN MISMATCH: sector {sector} cycle {wrote}: the record counts "
            f"{r['old_zero_bits']}/{r['new_zero_bits']} old/new zero bits, this script "
            f"{old_zero}/{new_zero}; pattern() no longer matches pattern.rs"
        )
    v = r["verdict"]
    rem, land, weak = r["remaining_old_zeros"], r["landed_new_zeros"], r["weak_bits"]
    bits = SECTOR * 8
    d = {
        "verdict": v,
        "weak_bits": weak,
        "weak_bytes": r["weak_bytes"],
        "old_zeros_left": rem / old_zero,
        "new_zeros_landed": land / new_zero,
        "ff_bytes": r["ff_bytes"],
    }
    if v == "old":
        return Phase("untouched", "erase", d)
    if v == "erased":
        return Phase("erased", "erase", d)
    if v == "complete":
        return Phase("complete", "program", d)
    if v == "erased-weak":
        return Phase("erase:reads-ff-weak", "erase", d)
    if payload == PAYLOAD_UNALIGNED:
        writes = [tuple(w) for w in r.get("writes") or []]
        if writes != plan(sector, wrote):
            raise SystemExit(
                f"PLAN MISMATCH: sector {sector} cycle {wrote}: the record lists {writes}, this "
                f"script {plan(sector, wrote)}; plan() no longer matches program_plan.rs"
            )
    if v == "torn-program":
        p = r["program"] or {}
        shape = p.get("shape")
        if payload == PAYLOAD_UNALIGNED and shape in ("op-boundary", "byte-prefix"):
            # The firmware judges commands on absolute 32-byte boundaries;
            # an unaligned write's are not, so the end is judged here.
            u = unaligned_end(r, new, writes)
            d.update({k: v for k, v in u.items() if k != "class"})
            return Phase(u["class"], "program", d)
        word = mid_word(r, new)
        if word is not None:
            d.update(shape=shape, **word)
            return Phase("program:mid-word", "program", d)
        if shape in ("op-boundary", "byte-prefix"):
            extent = p["landed_extent"]
            if shape == "op-boundary":
                end = -(-extent // COMMAND) * COMMAND
            else:
                end = extent
            d.update(
                shape=shape,
                end=end,
                landed_extent=extent,
                partial_bytes=p["partial_bytes"],
                on_word_boundary=(end % WORD == 0),
                page_aligned=(shape == "op-boundary" and end % PAGE == 0),
            )
            cls = "program:command-boundary" if shape == "op-boundary" else "program:mid-command"
            return Phase(cls, "program", d)
        if shape == "scattered" and (rem + land) <= RESIDUE_FRACTION * bits:
            # A handful of zeros, all at new-zero positions, none of them a
            # prefix: no program writes a sector that way. An erase's tail
            # whose leftover zeros happened to miss every old-zero position.
            d["residual_zero_bits"] = rem + land
            return Phase("erase:erasing", "erase", d)
        if shape == "scattered":
            d["shape"] = shape
            return Phase("program:scattered", "program", d)
        d["shape"] = shape
        return Phase("unknown", "none", d)
    if v == "torn-erase":
        if rem <= RESIDUE_FRACTION * bits:
            d["residual_zero_bits"] = rem + land
            return Phase("erase:erasing", "erase", d)
        return Phase("erase:old-left", "erase", d)
    if v == "mixed":
        if rem == old_zero and land == new_zero and weak == 0:
            return Phase("erase:all-zero", "erase", d)
        if rem == old_zero and weak == 0:
            run = leading_zero_run(r, new)
            # The reconstruction must also explain the record's `ff_bytes`:
            # after the run the sector is old, so its `0xFF` bytes are the
            # old pattern's.
            if run is not None and sum(1 for b in old[run[1] :] if b == 0xFF) == r["ff_bytes"]:
                lo, hi = run
                ends = range(lo, hi + 1)
                d.update(
                    zero_run=run,
                    exact="leading_zero_bytes" in r,
                    on_word_boundary=any(n % WORD == 0 for n in ends),
                    on_command_boundary=any(n % COMMAND == 0 for n in ends),
                    on_page_boundary=any(n % PAGE == 0 for n in ends),
                )
                return Phase("erase:zeroing", "erase", d)
            return Phase("unknown", "none", d)
        d["residual_zero_bits"] = rem + land
        return Phase("erase:erasing", "erase", d)
    return Phase("unknown", "none", d)


# --------------------------------------------------------------------------
# The report.


def flash_size(jedec: str) -> str:
    try:
        cap = int(jedec, 16) & 0xFF
    except ValueError:
        return "?"
    if 0x10 <= cap <= 0x20:
        n = 1 << cap
        return f"{n // (1 << 20)} MiB" if n >= 1 << 20 else f"{n // 1024} KiB"
    return "?"


def stats(xs: list[float]) -> str:
    if not xs:
        return "—"
    return f"{min(xs):g} / {statistics.median(xs):g} / {max(xs):g}"


def pct(n: int, total: int) -> str:
    return f"{100.0 * n / total:.1f} %" if total else "—"


def fmt_frac(x: float) -> str:
    return f"{100.0 * x:.2f} %"


def analyze(paths: list[str]) -> tuple[str, list[dict]]:
    transcripts = [read_transcript(p) for p in paths]
    by_config: dict[tuple[str, str], list[Transcript]] = defaultdict(list)
    for t in transcripts:
        by_config[(t.payload, t.meta.get("configuration", "?"))].append(t)
    out: list[str] = []
    rows: list[dict] = []
    w = out.append
    w(f"_Generated by `scripts/emu/flash-tears-analyze.py` over {len(paths)} transcript(s). "
      "Do not edit by hand; re-run it._")
    w("")
    for payload, config in sorted(by_config, key=lambda k: (k[0] != PAYLOAD, not k[1].startswith("silicon"), k)):
        # In the order they were taken: one board's cycle count only grows.
        ts = sorted(by_config[(payload, config)], key=lambda t: (
            t.meta.get("date", ""),
            next((b.boot.get("latest") or 0 for b in t.boots), 0),
            t.path,
        ))
        w(f"### `{config}`" if payload == PAYLOAD else f"### `{payload}` on `{config}`")
        w("")
        w("| transcript | date | firmware | cycles | boots | cuts | not cuts |")
        w("|---|---|---|---|---:|---:|---|")
        cuts: list[tuple[Boot, Phase]] = []
        excluded: list[tuple[Boot, Phase | None]] = []
        idents = Counter()
        for t in ts:
            n_cut = 0
            reasons = Counter()
            prev: Boot | None = None
            for b in t.boots:
                idents[(b.boot.get("mac", "—"), b.boot.get("flash_id", "—"), b.boot.get("base", "—"))] += 1
                f = b.in_flight
                ph = attribute_journal(b, classify(f, payload)) if f else None
                if is_cut(b) and f is not None:
                    n_cut += 1
                    cuts.append((b, ph))
                    loop_from = prev.scan_done_next if prev else None
                    rows.append({
                        "payload": payload,
                        "configuration": config,
                        "transcript": os.path.basename(t.path),
                        "boot": b.index,
                        "cycle": f["wrote"],
                        "sector": f["sector"],
                        "cycles_before_cut": (f["wrote"] - loop_from) if loop_from is not None else None,
                        "class": ph.cls,
                        "phase": ph.phase,
                        **{k: v for k, v in ph.detail.items()},
                    })
                else:
                    reasons[not_a_cut_reason(b)] += 1
                    excluded.append((b, ph))
                prev = b
            lat = [b.boot.get("latest") for b in t.boots if b.boot.get("latest") is not None]
            span = f"{min(lat)}–{max(lat)}" if lat else "—"
            w(f"| `{os.path.basename(t.path)}` | {t.meta.get('date', '?')} | "
              f"`{t.meta.get('firmware_commit', '?')}` | {span} | {len(t.boots)} | {n_cut} | "
              f"{'; '.join(f'{n}× {r}' for r, n in reasons.items()) or '—'} |")
        total = len(cuts)
        w(f"| **all** | | | | {sum(len(t.boots) for t in ts)} | **{total}** | |")
        w("")
        w("Who the records say ran it (from every `ft-boot`):")
        w("")
        w("| MAC | flash JEDEC id (manufacturer, type, capacity) | flash size (capacity byte) | lpfs base | boots |")
        w("|---|---|---|---|---:|")
        for (mac, fid, base), n in sorted(idents.items()):
            w(f"| `{mac}` | `{fid}` | {flash_size(fid) if fid != '—' else '—'} | `{base}` | {n} |")
        w("")
        resets = Counter(b.boot.get("reset") for b, _ in cuts)
        w(f"Reset reasons of the cut boots: {', '.join(f'`{r}` {n}' for r, n in sorted(resets.items())) or '—'}.")
        w("")
        if excluded:
            w("Boots that were not power cuts (left out of every count below):")
            w("")
            w("| boot | reset | state | in-flight sector |")
            w("|---|---|---|---|")
            for b, ph in excluded:
                w(f"| `{os.path.basename(b.transcript)}` boot {b.index} | `{b.boot.get('reset')}` | "
                  f"{b.boot.get('state')} | {ph.cls if ph else '—'} |")
            w("")
        if not total:
            continue

        # Collateral damage: anything but the in-flight sector.
        unexpected = sum((b.summary or {}).get("settled_unexpected", 0) for b, _ in cuts)
        settled_n = sum(1 for b, _ in cuts for s in b.sectors if s.get("role") == "settled")
        settled_weak = sum(s.get("weak_bits", 0) for b, _ in cuts for s in b.sectors if s.get("role") == "settled")
        journal_all = sum(j.get("damaged", 0) for b, _ in cuts for j in b.journals)
        journal_in_flight = sum(ph.detail.get("journal_slots_in_flight", 0) for _, ph in cuts)
        journal_bad = journal_all - journal_in_flight
        journal_torn = sum(1 for b, _ in cuts for j in b.journals if j.get("torn_next"))
        journal_ops = Counter(ph.cls for _, ph in cuts if ph.phase == "journal")
        w("Outside the in-flight sector, over every cut:")
        w("")
        w(f"- settled sectors not holding their last cycle's pattern: **{unexpected}** (of {settled_n} read)")
        w(f"- weak bits in settled sectors: **{settled_weak}**")
        w(f"- damaged journal slots outside the operation in flight: **{journal_bad}**; inside it: "
          f"{journal_in_flight} (of {journal_all} damaged); torn next slots: **{journal_torn}**")
        w(f"- cuts whose operation in flight was the journal's own: "
          f"{', '.join(f'`{c}` {n}' for c, n in sorted(journal_ops.items())) or 'none'}")
        gaps = [r["cycles_before_cut"] for r in rows
                if r["configuration"] == config and r["cycles_before_cut"] is not None]
        w(f"- work cycles completed between a scan and its cut (min / median / max): {stats(gaps)}")
        w("")

        # The histogram.
        hist = Counter(ph.cls for _, ph in cuts)
        w(f"**Tear shapes, {total} cuts** (the in-flight sector of each):")
        w("")
        w("| phase | shape | cuts | share |")
        w("|---|---|---:|---:|")
        for cls, phase, text in phase_order(payload):
            w(f"| {phase} | {text} | {hist.get(cls, 0)} | {pct(hist.get(cls, 0), total)} |")
        erase_n = sum(n for c, n in hist.items() if PHASE_OF[c] == "erase")
        prog_n = sum(n for c, n in hist.items() if PHASE_OF[c] == "program")
        w(f"| **erase** | | **{erase_n}** | {pct(erase_n, total)} |")
        w(f"| **program** | | **{prog_n}** | {pct(prog_n, total)} |")
        w("")

        # Firmware verdict against the phase.
        cross = Counter((ph.detail["verdict"], ph.cls) for _, ph in cuts)
        w("The firmware's `verdict` against the phase it was sorted into:")
        w("")
        w("| firmware verdict | phase | cuts |")
        w("|---|---|---:|")
        for (v, c), n in sorted(cross.items()):
            w(f"| `{v}` | {c} | {n} |")
        w("")

        # Weak bits.
        weak = [(ph, ph.detail["weak_bits"]) for _, ph in cuts]
        with_weak = [x for x in weak if x[1] > 0]
        reads_ff = [ph for _, ph in cuts if ph.cls in ("erased", "erase:reads-ff-weak")]
        reads_ff_weak = [ph for ph in reads_ff if ph.detail["weak_bits"] > 0]
        w(f"**Weak bits** (a bit that did not read the same all {SCAN_READS} times):")
        w("")
        w(f"- in-flight sectors with any: **{len(with_weak)}** of {total}")
        w("")
        w("| phase | cuts | with weak bits | weak bits (min / median / max, where any) |")
        w("|---|---:|---:|---|")
        for cls, _, _ in phase_order(payload):
            mine = [n for ph, n in weak if ph.cls == cls]
            if not mine:
                continue
            some = [n for n in mine if n > 0]
            w(f"| {cls} | {len(mine)} | {len(some)} | {stats(some)} |")
        w("")
        w(f"- sectors reading all `0xFF` in every stable bit: {len(reads_ff)}; of those with weak bits: "
          f"**{len(reads_ff_weak)}** "
          f"({', '.join(str(ph.detail['weak_bits']) for ph in reads_ff_weak) or 'no'} weak bits)")
        w("")

        # Program tears.
        prog = [ph for _, ph in cuts if ph.cls.startswith("program:") and "end" in ph.detail]
        if payload == PAYLOAD_UNALIGNED:
            for line in unaligned_section(cuts):
                w(line)
            prog = []
        if prog:
            w("**Torn programs**: where the landed prefix ends (byte offset in the sector):")
            w("")
            w("| shape | cuts | on a 4-B word boundary | on a 256-B page boundary | partial bytes (min / median / max) | end mod 32 |")
            w("|---|---:|---:|---:|---|---|")
            for cls in ("program:command-boundary", "program:mid-command", "program:mid-word"):
                mine = [ph for ph in prog if ph.cls == cls]
                if not mine:
                    continue
                w(f"| {cls} | {len(mine)} | {sum(1 for ph in mine if ph.detail['on_word_boundary'])} | "
                  f"{sum(1 for ph in mine if ph.detail['end'] % PAGE == 0)} | "
                  f"{stats([ph.detail['partial_bytes'] for ph in mine])} | "
                  f"{', '.join(str(ph.detail['end'] % COMMAND) for ph in mine)} |")
            w("")

        # Zeroing runs.
        zr = [(b, ph) for b, ph in cuts if ph.cls == "erase:zeroing"]
        if zr:
            w("**Zeroing runs**: where the `0x00` run from the front stops (byte offset). `exact` is the "
              "firmware's `leading_zero_bytes`; otherwise the end is pinned from per-page counts against the "
              "pattern (a range where the pattern has `0xFF` bytes), and checked against the record's `ff_bytes`.")
            w("")
            w("| cut | zero run ends at | how | mod 32 | on a 4-B word boundary | on a 32-B command boundary | on a 256-B page boundary |")
            w("|---|---|---|---|---|---|---|")
            yn = lambda x: "yes" if x else "no"  # noqa: E731
            for b, ph in zr:
                d = ph.detail
                lo, hi = d["zero_run"]
                rng = f"{lo}" if lo == hi else f"{lo}–{hi}"
                mods = f"{lo % COMMAND}" if lo == hi else f"{lo % COMMAND}–{hi % COMMAND}"
                w(f"| `{os.path.basename(b.transcript)}` boot {b.index} (cycle {b.in_flight['wrote']}) | {rng} | "
                  f"{'exact' if d['exact'] else 'pages'} | {mods} | {yn(d['on_word_boundary'])} | "
                  f"{yn(d['on_command_boundary'])} | {yn(d['on_page_boundary'])} |")
            w(f"| **all {len(zr)}** | | | | **{sum(1 for _, ph in zr if ph.detail['on_word_boundary'])}** | "
              f"**{sum(1 for _, ph in zr if ph.detail['on_command_boundary'])}** | "
              f"**{sum(1 for _, ph in zr if ph.detail['on_page_boundary'])}** |")
            w("")

        # Erasing: is it positional, or every cell at once?
        er = [(b, ph) for b, ph in cuts if ph.cls == "erase:erasing"]
        if er:
            w("**Erasing**: stable zeros left, as a share of the old pattern's zeros and of the new "
              "pattern's zeros (the old pattern's ones). The two shares track each other when the erase "
              "starts from all `0x00` — every bit is a zero to lift, whatever the old data was; an erase "
              "from the old data would leave only old zeros.")
            w("")
            w("| cut | old zeros still 0 | old ones now 0 | weak bits | weak bytes |")
            w("|---|---:|---:|---:|---:|")
            for b, ph in sorted(er, key=lambda x: -(x[1].detail["old_zeros_left"] + x[1].detail["new_zeros_landed"])):
                d = ph.detail
                w(f"| `{os.path.basename(b.transcript)}` boot {b.index} | {fmt_frac(d['old_zeros_left'])} | "
                  f"{fmt_frac(d['new_zeros_landed'])} | {d['weak_bits']} | {d['weak_bytes']} |")
            w("")
        erase_cuts = [ph for _, ph in cuts if ph.phase == "erase"]
        lifted_to_zero = [ph for ph in erase_cuts if ph.detail["new_zeros_landed"] > 0]
        w(f"- erase-phase cuts with a stable `0` where the old data had a `1`: **{len(lifted_to_zero)}** "
          f"of {len(erase_cuts)}")
        w("")

        # Timing.
        times = [b.timing for t in ts for b in t.boots if b.timing]
        if times:
            clock = "" if config.startswith("silicon") else " — emulated time, a model and not a measurement"
            w(f"**Timing of one cycle** (the timed cycle after each scan, {len(times)} boots; "
              f"µs, min / median / max{clock}):")
            w("")
            for k in ("journal_us", "erase_us", "program_us", "page_us_min", "page_us_max"):
                w(f"- `{k}`: {stats([x[k] for x in times])}")
            je = statistics.median(x["journal_us"] for x in times)
            ee = statistics.median(x["erase_us"] for x in times)
            pp = statistics.median(x["program_us"] for x in times)
            cyc = je + ee + pp
            w(f"- share of a cycle (medians): journal {pct(je, cyc)}, erase {pct(ee, cyc)}, "
              f"program {pct(pp, cyc)}; the cuts landed {pct(erase_n, total)} in the erase and "
              f"{pct(prog_n, total)} in the program")
            w("")

        if config.startswith("silicon") and payload == PAYLOAD:
            w(f"**`lp-nor-sim`'s assumptions against these {total} cuts** (the models as `lp-emu/lp-nor-sim` "
              "has them; the verdict is mechanical, from the counts above):")
            w("")
            for line in assumptions(cuts, reads_ff, reads_ff_weak, prog, zr, er, erase_cuts, lifted_to_zero,
                                    unexpected, settled_weak, journal_bad):
                w(line)
            w("")
    return "\n".join(out) + "\n", rows


def unaligned_section(cuts) -> list[str]:
    """Where unaligned torn programs stopped, against each hypothesis."""
    prog = [(b, ph) for b, ph in cuts if ph.cls.startswith("program:") and ph.cls != "program:scattered"
            and "fits_rom" in ph.detail]
    out = []
    w = out.append
    if not prog:
        w("**Unaligned torn programs**: none yet.")
        w("")
        return out
    judged = [(b, ph) for b, ph in prog if not ph.detail["between_writes"]]
    w("**Unaligned torn programs**: where each prefix stopped, against the write it was in. "
      "`rom` = 32 B from the write's address inside its first page, then 32 B from the page boundary "
      "(the split the emulated mask ROM makes, and `lp-nor-sim`'s per-page op); `absolute` = a 32-byte "
      "boundary of the flash; `relative` = 32 B from the write's address all the way.")
    w("")
    w("| cut | write (at, len) | prefix ends at | into the write | first page of it | end mod 32 | "
      "from the write mod 32 | rom | absolute | relative | class |")
    w("|---|---|---|---:|---|---:|---:|---|---|---|---|")
    yn = lambda x: "yes" if x else "no"  # noqa: E731
    for b, ph in prog:
        d = ph.detail
        end = d["end"] if not isinstance(d["end"], tuple) else f"{d['end'][0]}–{d['end'][1]}"
        if d["between_writes"]:
            w(f"| `{os.path.basename(b.transcript)}` boot {b.index} | — | {end} | 0 | — | — | — | — | — | — | "
              f"{ph.cls} |")
            continue
        wa, wl = d["write"]
        w(f"| `{os.path.basename(b.transcript)}` boot {b.index} | ({wa}, {wl}) | {end} | {d['offset_in_write']} | "
          f"{yn(d['in_first_page'])} | {d['end_mod_32']} | {d['rel_mod_32']} | {yn(d['fits_rom'])} | "
          f"{yn(d['fits_absolute'])} | {yn(d['fits_relative'])} | {ph.cls} |")
    w("")
    n = len(judged)
    on_word = [ph for _, ph in judged if ph.detail["on_word"]]
    rom = sum(1 for ph in on_word if ph.detail["fits_rom"])
    ab = sum(1 for ph in on_word if ph.detail["fits_absolute"])
    rel = sum(1 for ph in on_word if ph.detail["fits_relative"])
    # The cuts that tell the hypotheses apart: an end that is a boundary
    # under one and not another.
    tells = [ph for ph in on_word
             if len({ph.detail["fits_rom"], ph.detail["fits_absolute"], ph.detail["fits_relative"]}) > 1]
    first_page = [ph for ph in on_word if ph.detail["in_first_page"]]
    w(f"- prefixes ending inside a write: **{n}** ({len(prog) - n} more ended where a write starts); "
      f"on a 4-byte word: **{len(on_word)}**")
    w(f"- of those on a word, on a boundary under `rom`: **{rom}**, `absolute`: **{ab}**, `relative`: **{rel}**")
    w(f"- ends that tell the three apart (a boundary under one, not under another): **{len(tells)}**; "
      f"of them `rom` {sum(1 for ph in tells if ph.detail['fits_rom'])}, "
      f"`absolute` {sum(1 for ph in tells if ph.detail['fits_absolute'])}, "
      f"`relative` {sum(1 for ph in tells if ph.detail['fits_relative'])}")
    w(f"- ends inside the write's first page: {len(first_page)} (where `rom` and `relative` say a command "
      f"starts a word or more past a 32-byte boundary and `absolute` says on one)")
    mid = [ph for ph in on_word if not ph.detail["fits_rom"]]
    if mid:
        w(f"- mid-command ends (on a word, on no `rom` boundary): {len(mid)}; their offset into the command "
          f"they stopped in, mod 32 from the write in its first page / absolute after: "
          f"{', '.join(str(ph.detail['rel_mod_32'] if ph.detail['in_first_page'] else ph.detail['end_mod_32']) for ph in mid)}")
    w("")
    return out


def rom_split(trace: str) -> int:
    """How the emulated mask ROM split the unaligned plan's writes into
    page-program commands, from an `lp-emu-esp32c6 --trace SPI1` log of the
    `flash-tears-unaligned` image's first boot (the init pass writes sector
    `c` in cycle `c`). Prints each write's commands as offsets in its sector,
    then a count of the command starts by kind."""
    import re

    region = 0x352000  # lpfs + two journal sectors on the C6's table
    addr = dlen = None
    cmds = []
    with open(trace) as f:
        for line in f:
            m = re.search(r"W4 SPI1\+0x(\w+) \w+ = 0x([0-9a-f]+)", line)
            if not m:
                continue
            reg, val = m.group(1), int(m.group(2), 16)
            if reg == "004":
                addr = val
            elif reg == "024":
                dlen = val
            elif reg == "020" and (val & 0xFF) == 0x02:  # user2: page program
                cmds.append((addr, (dlen + 1) // 8))
    kinds = Counter()
    i, shown = 0, 0
    for cycle in range(REGION):
        base = region + cycle * SECTOR
        while i < len(cmds) and not (base <= cmds[i][0] < base + SECTOR):
            i += 1
        for at, n in plan(cycle, cycle):
            got, need = [], n
            while need > 0 and i < len(cmds):
                a, ln = cmds[i]
                got.append((a - base, ln))
                need -= ln
                i += 1
            if need:
                print(f"cycle {cycle}: the trace ends inside write ({at}, {n})")
                break
            first_page_end = (at // PAGE + 1) * PAGE
            for a, _ in got:
                if a == at:
                    kinds["at the write's address"] += 1
                elif a % PAGE == 0:
                    kinds["on a page boundary"] += 1
                elif a < first_page_end and (a - at) % COMMAND == 0:
                    kinds["32 B on from the address, first page"] += 1
                elif a % COMMAND == 0:
                    kinds["absolute 32 B, after the first page"] += 1
                else:
                    kinds["elsewhere"] += 1
            if shown < 8:
                print(f"cycle {cycle} write ({at}, {n}): {got}")
                shown += 1
    print(f"{len(cmds)} program commands in the trace; command starts in the init pass's writes:")
    for k, v in kinds.most_common():
        print(f"  {v:6d}  {k}")
    return 0 if kinds and "elsewhere" not in kinds else 1


def assumptions(cuts, reads_ff, reads_ff_weak, prog, zr, er, erase_cuts, lifted_to_zero,
                unexpected, settled_weak, journal_bad) -> list[str]:
    """One row per model assumption, with its evidence and a verdict."""
    n_prog = len(prog)
    prefix_word = sum(1 for ph in prog if ph.detail["on_word_boundary"])
    partial = sum(ph.detail["partial_bytes"] for ph in prog)
    scattered = sum(1 for _, ph in cuts if ph.cls == "program:scattered")
    at_cmd = sum(1 for ph in prog if ph.cls == "program:command-boundary")
    at_page = sum(1 for ph in prog if ph.cls == "program:command-boundary" and ph.detail["end"] % PAGE == 0)
    old_left = sum(1 for _, ph in cuts if ph.cls == "erase:old-left")
    untouched = sum(1 for _, ph in cuts if ph.cls == "untouched")
    before_erase = sum(1 for _, ph in cuts if ph.cls == "cut:before-erase")
    mid_word = sum(1 for _, ph in cuts if ph.cls == "program:mid-word")
    journal_ops = Counter(ph.cls for _, ph in cuts if ph.phase == "journal")
    zero_states = sum(1 for _, ph in cuts if ph.cls in ("erase:zeroing", "erase:all-zero"))
    weak_cuts = [ph for _, ph in cuts if ph.detail["weak_bits"] > 0]
    weak_in_erase = sum(1 for ph in weak_cuts if ph.phase == "erase")
    n_erase = len(erase_cuts)

    def verdict(held: bool, seen: int, wording: tuple[str, str, str]) -> str:
        if seen == 0:
            return wording[2]
        return wording[0] if held else wording[1]

    rows = [
        ("A torn program lands a prefix of what it was asked to write (`BytePrefix`)",
         f"{n_prog} torn programs: {n_prog - mid_word} prefixes of whole bytes, {mid_word} whole words then "
         f"one word part-landed (a prefix at word level, not at bit level); {scattered} scattered",
         verdict(scattered == 0, n_prog, ("held at word level" if mid_word else "held", "contradicted",
                                          "not yet seen"))),
        ("`BytePrefix`: the byte after the prefix gets a random subset of its clears",
         f"{partial} partial bytes in {n_prog} torn programs; {prefix_word} of {n_prog} prefixes end on a 4-byte word",
         verdict(partial > 0, n_prog, ("seen", "not seen: every prefix ends on a whole byte" + (" — a whole 4-byte word" if prefix_word == n_prog else ""), "not yet seen"))),
        ("`RandomBits`: a torn program lands a random subset of the page's clears",
         f"{scattered} of {n_prog + scattered} torn programs scattered",
         verdict(scattered > 0, n_prog + scattered, ("seen", "not seen (the model is harsher than this part)", "not yet seen"))),
        ("One program op is a 256-byte page; a cut between ops leaves whole pages",
         f"{at_cmd} of {n_prog} prefixes end on a 32-byte command boundary, {at_page} of them on a page boundary; "
         f"{n_prog - at_cmd} end inside a command",
         verdict(at_cmd == at_page, n_prog, ("held", "contradicted: the unit is the 32-byte ROM command", "not yet seen"))),
        ("A torn erase leaves a byte-wise mix of old bytes, `0xFF` and weak bits (shape 0)",
         f"{old_left} of {n_erase} erase cuts left old data beyond a residue",
         verdict(old_left > 0, n_erase, ("seen", "not seen", "not yet seen"))),
        ("A torn erase can read all `0xFF` and carry weak bits (shape 1)",
         f"{len(reads_ff_weak)} of {len(reads_ff)} sectors reading all `0xFF` had weak bits "
         f"({', '.join(str(ph.detail['weak_bits']) for ph in reads_ff_weak) or 'none'}); "
         f"the model sprinkles one weak bit in ~1 of 32 bytes (~128 a sector)",
         verdict(len(reads_ff_weak) > 0, len(reads_ff), ("held, rarely, and far lighter than modelled", "not seen", "not yet seen"))),
        ("A torn erase can be erased up to a point and old after it (shape 2)",
         f"{old_left} with `0xFF` then old; {len(zr)} with `0x00` then old (zeroing)",
         verdict(old_left > 0, n_erase, ("seen", "not seen as `0xFF`-then-old; seen as `0x00`-then-old", "not yet seen"))),
        ("A torn erase only lifts bits: it never leaves a `0` where the old data had a `1`",
         f"{len(lifted_to_zero)} of {n_erase} erase cuts did ({zero_states} of them reading `0x00` from the front or throughout)",
         verdict(len(lifted_to_zero) == 0, n_erase, ("held", "contradicted", "not yet seen"))),
        ("Weak bits come from torn erases",
         f"{len(weak_cuts)} in-flight sectors had weak bits, {weak_in_erase} of them erase-phase; "
         f"{settled_weak} weak bits in settled sectors",
         verdict(weak_in_erase == len(weak_cuts), len(weak_cuts), ("held", "contradicted", "not yet seen"))),
        ("A torn erase that started changes the sector (`Clean` leaves it old)",
         f"{untouched + before_erase} of {n_erase} erase-phase cuts left the old pattern whole: {before_erase} "
         "with both journal copies naming the cycle — a cut between the journal write and the erase command, "
         "which the model draws as where the cut lands (`cut_before_erase`), not as an erase shape; "
         f"{untouched} with nothing to say where the cut landed",
         verdict(untouched == 0, n_erase, ("held: no started erase left the old data", "seen", "not yet seen"))),
        ("A torn program stops on a whole 4-byte word",
         f"{mid_word} of {n_prog + scattered} torn programs stopped inside a word (some of its clears landed, "
         "weak bits possible); the model draws them as `program_mid_word`",
         verdict(mid_word == 0, n_prog + scattered, ("held", "not always: the part's program unit can be caught part-way",
                                                     "not yet seen"))),
        ("A cut damages only the operation in flight",
         f"{unexpected} settled sectors damaged, {settled_weak} weak bits in them, {journal_bad} journal slots "
         f"damaged outside the operation in flight (journal ops in flight: "
         f"{', '.join(f'`{c}` {n}' for c, n in sorted(journal_ops.items())) or 'none'})",
         verdict(unexpected == 0 and settled_weak == 0 and journal_bad == 0, len(cuts),
                 ("held", "contradicted", "not yet seen"))),
    ]
    lines = ["| assumption | evidence | verdict |", "|---|---|---|"]
    for a, e, v in rows:
        lines.append(f"| {a} | {e} | **{v}** |")
    return lines


def self_test() -> int:
    """The zero-run reconstruction against sectors built from the pattern."""
    sector, cycle = 4, 37156
    old, new = pattern(sector, cycle - REGION), pattern(sector, cycle)
    for n in (0, 4, 156, 1300, 2047, 3639, 4092):
        cells = bytes(n * [0]) + old[n:]
        landed = [0] * PAGES
        for i, c in enumerate(cells):
            # A stable zero where the new pattern wants one: "landed".
            landed[i // PAGE] += bin(~c & ~new[i] & 0xFF).count("1")
        r = {"page_landed": landed}
        lo, hi = leading_zero_run(r, new)
        assert lo <= n <= hi, (n, lo, hi)
        assert all(new[i] == 0xFF for i in range(lo, hi)), (n, lo, hi)
        r["leading_zero_bytes"] = n
        assert leading_zero_run(r, new) == (n, n)
    for sector, cycle in ((0, 0), (2, 10), (15, 46_528)):
        ws = plan(sector, cycle)
        assert ws[0] == (0, UNALIGNED_FIRST) and sum(n for _, n in ws) == SECTOR, ws
        assert all(a % COMMAND != 0 and n % WORD == 0 for a, n in ws[1:]), ws
    # The first boot of the committed emulated dry run lists this plan.
    assert plan(6, 246)[:3] == [(0, 20), (20, 944), (964, 56)], plan(6, 246)[:3]
    check_model_self_test()
    mid_word_self_test()
    journal_self_test()
    print("self-test ok")
    return 0


# `TearMix::CX1` as the first 200 cuts fitted it, held here so the test of the
# RULE does not move when a later re-fit changes the model file.
CX1_200_RS = """pub const CX1: TearMix = TearMix {
    erase_zeroing: 10, erase_all_zero: 26, erase_erasing: 28, erase_reads_ff_weak: 1,
    erase_reads_ff: 101, program_command_boundary: 26, program_mid_command: 7,
};"""
CX1_200_CUTS = {
    "erase:zeroing": 10, "erase:all-zero": 26, "erase:erasing": 28, "erase:reads-ff-weak": 1,
    "erased": 101, "program:command-boundary": 26, "program:mid-command": 7, "complete": 1,
}


def program_record(sector: int, cycle: int, cells: bytes, weak_bits: int = 0) -> dict:
    """An in-flight `ft-sector` record for a torn program of `pattern(sector,
    cycle)` that left `cells`, as the firmware's `program_tear` and its
    counters would write it (`analysis.rs`)."""
    old, new = pattern(sector, cycle - REGION), pattern(sector, cycle)
    p = {"shape": None, "intended_bits": 0, "landed_bits": 0, "stray_bits": 0,
         "prefix_bytes": 0, "partial_bytes": 0, "landed_extent": 0}
    in_prefix = True
    for i, (a, w) in enumerate(zip(cells, new)):
        p["intended_bits"] += zeros(w)
        landed = bin(~a & ~w & 0xFF).count("1")
        p["landed_bits"] += landed
        p["stray_bits"] += bin(~a & w & 0xFF).count("1")
        if landed:
            p["landed_extent"] = i + 1
        if in_prefix and a == w:
            p["prefix_bytes"] += 1
        else:
            in_prefix = False
        if a != w and a != 0xFF:
            p["partial_bytes"] += 1
    extent = p["landed_extent"]
    end = -(-extent // COMMAND) * COMMAND
    if p["stray_bits"]:
        p["shape"] = "stray"
    elif cells[:extent] == new[:extent] and all(b == 0xFF for b in new[extent:end]):
        p["shape"] = "op-boundary"
    elif cells[: extent - 1] == new[: extent - 1]:
        p["shape"] = "byte-prefix"
    else:
        p["shape"] = "scattered"
    return {
        "sector": sector, "wrote": cycle, "verdict": "torn-program",
        "old_zero_bits": sum(zeros(b) for b in old), "new_zero_bits": sum(zeros(b) for b in new),
        "remaining_old_zeros": 0, "landed_new_zeros": p["landed_bits"], "weak_bits": weak_bits,
        "weak_bytes": min(weak_bits, 1), "ff_bytes": sum(1 for b in cells if b == 0xFF), "program": p,
    }


def mid_word_self_test() -> None:
    """A torn program caught inside a word is `program:mid-word`, however
    the firmware named it; a prefix on a whole word is not."""
    sector, cycle = 0, 1328
    new = pattern(sector, cycle)

    def torn(n_whole: int, word: bytes | None) -> bytes:
        tail = bytes(word or b"")
        return new[:n_whole] + tail + bytes([0xFF] * (SECTOR - n_whole - len(tail)))

    # CX1's first one: 3,872 whole bytes, then 3 of the next word's 17 clears
    # in two bytes (the firmware: `scattered`).
    word = bytearray([0xFF] * WORD)
    for i, bit in ((1, 1), (3, 1), (3, 2)):
        assert not new[3872 + i] >> bit & 1, (i, bit)
        word[i] &= ~(1 << bit) & 0xFF
    r = program_record(sector, cycle, torn(3872, bytes(word)), weak_bits=1)
    assert r["program"]["shape"] == "scattered", r["program"]
    ph = classify(r)
    assert ph.cls == "program:mid-word", ph
    assert (ph.detail["word_at"], ph.detail["word_landed_bits"], ph.detail["word_intended_bits"]) == (3872, 3, 17), ph
    # The second: three whole bytes of a word and one byte part-landed (the
    # firmware: `byte-prefix`, ending on the word).
    at = next(a for a in range(3256, SECTOR, WORD) if zeros(new[a + 3]) >= 2)
    last = new[at + 3]
    partial = last | (1 << next(b for b in range(8) if not last >> b & 1))
    r = program_record(sector, cycle, torn(at, new[at : at + 3] + bytes([partial])))
    assert r["program"]["shape"] == "byte-prefix" and r["program"]["partial_bytes"] == 1, r["program"]
    assert classify(r).cls == "program:mid-word", classify(r)
    # Whole words: a mid-command prefix, or a command one; never mid-word.
    assert classify(program_record(sector, cycle, torn(3256, None))).cls == "program:mid-command"
    assert classify(program_record(sector, cycle, torn(3264, None))).cls == "program:command-boundary"
    # Clears past the word are scattered for real.
    far = bytearray(torn(3872, bytes(word)))
    i = next(i for i in range(3880, SECTOR) if new[i] != 0xFF)
    far[i] &= new[i]
    assert classify(program_record(sector, cycle, bytes(far))).cls == "program:scattered"


def journal_self_test() -> None:
    """Which operation was in flight, read off the two journal copies."""

    def boot(latest: int, cls: str, copies: list[tuple[int, int, int | None, bool]]) -> tuple[str, int]:
        b = Boot("t", 0, {"latest": latest, "reset": "poweron", "state": "resume"})
        b.journals = [{"copy": k, "valid": v, "damaged": d, "latest": l, "torn_next": {"shape": "x"} if t else None}
                      for k, (v, d, l, t) in enumerate(copies)]
        ph = attribute_journal(b, Phase(cls, PHASE_OF[cls], {}))
        return ph.cls, ph.detail.get("journal_slots_in_flight", 0)

    # CX1 `-r2` boot 39: copy 0's wrap erase (cycle 5376 is its slot 0); copy
    # 1 kept 5375. All 256 damaged slots are the operation in flight.
    assert boot(5375, "complete", [(0, 256, None, False), (128, 0, 5375, False)]) == ("journal:wrap-erase", 256)
    # The same wrap reading erased (two of the 500 did): no damage, still the op in flight.
    assert boot(22783, "complete", [(0, 0, None, False), (128, 0, 22783, False)]) == ("journal:wrap-erase", 0)
    # Copy 1's wrap (cycle 128 mod 256 is its slot 0): the sector reads old.
    assert boot(1152, "untouched", [(129, 0, 1152, False), (0, 200, None, False)]) == ("journal:wrap-erase", 200)
    # CX1 `-r4` boot 24: both copies name the cycle, the sector is old.
    assert boot(10716, "untouched", [(221, 0, 10716, False), (93, 0, 10716, False)]) == ("cut:before-erase", 0)
    # The emulated dry run's boot 5: copy 1 has not named it yet.
    assert boot(2895, "untouched", [(80, 0, 2895, False), (207, 0, 2894, False)]) == ("journal:program", 0)
    # ... or its entry is torn: that slot is the op in flight.
    assert boot(2895, "untouched", [(80, 0, 2895, False), (207, 1, 2894, True)]) == ("journal:program", 1)
    # Copy 0's next entry torn after a finished program.
    assert boot(1000, "complete", [(233, 1, 1000, True), (105, 0, 1000, False)]) == ("journal:program", 1)
    # A finished program and a whole journal: complete, no journal op.
    assert boot(1000, "complete", [(233, 0, 1000, False), (105, 0, 1000, False)]) == ("complete", 0)
    # Damage no op explains stays outside it: a non-wrap copy that lost the cycle.
    assert boot(1000, "complete", [(0, 50, None, False), (105, 0, 1000, False)]) == ("complete", 0)
    # Other classes are left alone.
    assert boot(1000, "erased", [(0, 9, None, False), (105, 0, 1000, False)]) == ("erased", 0)
    assert slot_of(0, 5376) == 0 and slot_of(1, 1152) == 0 and slot_of(1, 10716) == 92


def synthetic_rows(counts: dict[str, int], payload: str = PAYLOAD, config: str = "silicon:esp32c6") -> list[dict]:
    return [{"configuration": config, "payload": payload, "class": c} for c, k in counts.items() for _ in range(k)]


def check_model_self_test() -> None:
    """DD29's rule, on synthetic transcripts."""
    # Today's 200 cuts against today's model confirm. (The model's shares ARE
    # these cuts' shares, so this also pins the arithmetic: no cell can fail.)
    ok, lines = judge_model(synthetic_rows(CX1_200_CUTS), CX1_200_RS)
    assert ok, "\n".join(lines)
    # The thin shape (1 weak-bit cut, expected 1.0) is pooled, not tested alone.
    assert any("pooled" in l and "erase:reads-ff-weak" in l for l in lines), lines
    # 500 cuts drawn the way the model draws them (a share each is off by well
    # under noise) still confirm; the same 500 with every count nudged by 2.
    near = {k: round(v * 2.5) + d for (k, v), d in zip(CX1_200_CUTS.items(), (2, -2, 3, 0, -3, 2, -1, 0))}
    assert judge_model(synthetic_rows(near), CX1_200_RS)[0], near
    # One shape doubled fails: all-zero erases at 52 against the same others.
    doubled = dict(CX1_200_CUTS, **{"erase:all-zero": 52})
    ok, lines = judge_model(synthetic_rows(doubled), CX1_200_RS)
    assert not ok and any("OUTSIDE" in l and "erase:all-zero" in l for l in lines), lines
    # ... and so does a program shape at three times (n = 33 is thin: doubling 7 to 14 is inside noise).
    ok, lines = judge_model(synthetic_rows(dict(CX1_200_CUTS, **{"program:mid-command": 21})), CX1_200_RS)
    assert not ok and any("OUTSIDE" in l and "program:mid-command" in l for l in lines), lines
    # A shape the model has no weight for is a failure, never pooled away.
    ok, lines = judge_model(synthetic_rows(dict(CX1_200_CUTS, **{"erase:old-left": 1})), CX1_200_RS)
    assert not ok and any("NOT MODELLED" in l for l in lines), lines
    # The 200-cut model (no sub-word tear, no cut before an erase) against
    # the 500 the sitting took: exactly those two, NOT MODELLED.
    cx1_500 = {"erase:zeroing": 17, "erase:all-zero": 63, "erase:erasing": 69, "erase:reads-ff-weak": 4,
               "erased": 264, "cut:before-erase": 1, "program:command-boundary": 58,
               "program:mid-command": 19, "program:mid-word": 2, "journal:wrap-erase": 3}
    ok, lines = judge_model(synthetic_rows(cx1_500), CX1_200_RS)
    unmodelled = [l for l in lines if "NOT MODELLED" in l]
    assert not ok and len(unmodelled) == 2, lines
    assert any("cut:before-erase" in l for l in unmodelled) and any("mid-word" in l for l in unmodelled), lines
    # ... a weight of 0 is no weight ...
    zero = CX1_200_RS.replace("program_mid_command: 7,", "program_mid_command: 7, program_mid_word: 0,")
    assert not judge_model(synthetic_rows(cx1_500), zero)[0]
    # ... and the model with both weighed confirms; the journal's own wrap
    # erases feed neither family.
    cx1_500_rs = """pub const CX1: TearMix = TearMix {
    erase_zeroing: 17, erase_all_zero: 63, erase_erasing: 69, erase_reads_ff_weak: 4, erase_reads_ff: 264,
    cut_before_erase: 1, program_command_boundary: 58, program_mid_command: 19, program_mid_word: 2,
};"""
    ok, lines = judge_model(synthetic_rows(cx1_500), cx1_500_rs)
    assert ok and any("torn erases: 418 cut(s)" in l for l in lines) and any("torn programs: 79" in l for l in lines), lines
    # Per payload: unaligned silicon cuts (any shape) and emulated cuts feed nothing.
    extra = synthetic_rows({"erase:all-zero": 90, "program:scattered": 9}, payload=PAYLOAD_UNALIGNED)
    extra += synthetic_rows({"erase:all-zero": 90}, config="lp-emu:esp32c6:t1")
    ok, lines = judge_model(synthetic_rows(CX1_200_CUTS) + extra, CX1_200_RS)
    assert ok and any("flash-tears-unaligned" in l and "99 silicon" in l for l in lines), lines
    # Pooling: c and d are each expected 1 time in 200; alone, "c seen 0" would
    # fall outside its own Wilson interval, pooled it does not.
    weights = {"a": 900, "b": 90, "c": 5, "d": 5}
    assert wilson(0.005, 200)[0] > 0
    cells = judge_family({"a": 180, "b": 18, "c": 0, "d": 2}, weights)
    assert all(c["ok"] for c in cells), cells
    assert sorted(len(c["classes"]) for c in cells) == [1, 3], cells  # {a}, {b, c, d}
    # A cell that is not thin is never pooled, and a plain miss still fails.
    cells = judge_family({"a": 100, "b": 100}, {"a": 1, "b": 1})
    assert [c["classes"] for c in cells] == [["a"], ["b"]] and all(c["ok"] for c in cells), cells
    assert not any(c["ok"] for c in judge_family({"a": 190, "b": 10}, {"a": 1, "b": 1}))
    # The interval itself: Wilson at p = 0.5, n = 100 is 40.4 % - 59.6 %.
    lo, hi = wilson(0.5, 100)
    assert abs(lo - 0.4038) < 1e-3 and abs(hi - 0.5962) < 1e-3, (lo, hi)
    bonferroni_self_test()


def bonferroni_self_test() -> None:
    """DD32: each of a family's k cells at 1 - 0.05/k, k after pooling."""
    # The quantiles: k = 1 is the plain 95 %; k = 5 puts each cell at 99 %.
    assert abs(bonferroni_z(1) - Z95) < 1e-5, bonferroni_z(1)
    assert abs(bonferroni_z(5) - 2.575829) < 1e-5, bonferroni_z(5)
    # k is counted after pooling: CX1's 200 erases pool reads-ff-weak away, so
    # the erase family is 4 cells at 98.75 %, the program family 2 at 97.5 %.
    ok, lines = judge_model(synthetic_rows(CX1_200_CUTS), CX1_200_RS)
    assert any("torn erases" in l and "4 tested cell(s)" in l and "98.75%" in l for l in lines), lines
    assert any("torn programs" in l and "2 tested cell(s)" in l and "97.50%" in l for l in lines), lines
    # A NOT MODELLED cell does not raise k (it is not a test at a level) and
    # still fails when seen.
    cells = judge_family({"a": 50, "b": 49, "x": 1}, {"a": 1, "b": 1})
    assert {c["k"] for c in cells if not c.get("unmodelled")} == {2}, cells
    assert not all(c["ok"] for c in cells), cells
    # An honest-looking miss: five equal shapes at n = 500 (100 expected each,
    # 95 % band 83 - 118); one at 120 is outside 95 % but inside 99 %. Without
    # the correction it fails; with it (k = 5) the family is confirmed.
    weights = {s: 1 for s in "abcde"}
    seen = {"a": 120, "b": 95, "c": 95, "d": 95, "e": 95}
    assert not all(c["ok"] for c in judge_family(seen, weights, alpha=None))
    assert all(c["ok"] for c in judge_family(seen, weights))
    # ... and a real difference still fails corrected: the same shape at 135.
    seen = {"a": 135, "b": 92, "c": 91, "d": 91, "e": 91}
    assert not all(c["ok"] for c in judge_family(seen, weights))
    # The point of DD32, measured: honest samples of 500 cuts (415 torn
    # erases, 82 torn programs) drawn from the model itself. Uncorrected,
    # about one in five misses somewhere (DD32's premise: ~1 in 4 at 200);
    # corrected, each family misses about one time in twenty. Wilson around a
    # discrete count is approximate, so the bar is 7.5 %, not 5 %.
    import random

    rng = random.Random(32)
    fields = read_weights(CX1_200_RS)
    trials = 2000
    any_plain = [False] * trials
    for shapes in MODEL_SHAPES.values():
        w = model_weights(fields, shapes)
        n = round(sum(CX1_200_CUTS[c] for c in w) * 2.5)
        miss_dd32 = 0
        for t in range(trials):
            draw = Counter(rng.choices(list(w), weights=list(w.values()), k=n))
            obs = {c: draw.get(c, 0) for c in w}
            any_plain[t] = any_plain[t] or not all(c["ok"] for c in judge_family(obs, w, alpha=None))
            miss_dd32 += not all(c["ok"] for c in judge_family(obs, w))
        assert miss_dd32 / trials < 0.075, (n, miss_dd32 / trials)
    assert sum(any_plain) / trials > 0.15, sum(any_plain) / trials


MODEL_RS = "lp-emu/lp-nor-sim/src/calibrated_tear.rs"


def model_table(rows: list[dict]) -> str:
    """`lp-nor-sim`'s calibrated model (`TearMix::CX1`, `CX1_ERASING`,
    `CX1_READS_FF_WEAK`) as these silicon cuts give it, in the Rust the
    model file holds."""
    rows = [r for r in rows if r["configuration"].startswith("silicon") and r["payload"] == PAYLOAD]
    n = Counter(r["class"] for r in rows)
    erasing = sorted(
        ((r["residual_zero_bits"], r["weak_bits"]) for r in rows if r["class"] == "erase:erasing"),
        key=lambda t: t,
    )
    ff_weak = sorted(r["weak_bits"] for r in rows if r["class"] == "erase:reads-ff-weak")
    mid_word = sorted(
        ((r["word_landed_bits"], r["word_intended_bits"], r["weak_bits"]) for r in rows
         if r["class"] == "program:mid-word"),
        key=lambda t: (t[0] / t[1], t),
    )
    out = [
        f"// {len(rows)} silicon cuts.",
        "pub const CX1: TearMix = TearMix {",
        f"    erase_zeroing: {n['erase:zeroing']},",
        f"    erase_all_zero: {n['erase:all-zero']},",
        f"    erase_erasing: {n['erase:erasing']},",
        f"    erase_reads_ff_weak: {n['erase:reads-ff-weak']},",
        f"    erase_reads_ff: {n['erased']},",
        f"    cut_before_erase: {n['cut:before-erase']},",
        f"    program_command_boundary: {n['program:command-boundary']},",
        f"    program_mid_command: {n['program:mid-command']},",
        f"    program_mid_word: {n['program:mid-word']},",
        "};",
        f"pub const CX1_ERASING: [(u32, u32); {len(erasing)}] = [",
        *[f"    ({z}, {w})," for z, w in erasing],
        "];",
        f"pub const CX1_READS_FF_WEAK: [u32; {len(ff_weak)}] = [{', '.join(map(str, ff_weak))}];",
        f"pub const CX1_MID_WORD: [(u32, u32, u32); {len(mid_word)}] = [",
        *[f"    ({a}, {b}, {c})," for a, b, c in mid_word],
        "];",
    ]
    for shape in ("untouched", "erase:old-left", "program:scattered", "unknown"):
        if n[shape]:
            out.append(f"// NOT MODELLED: {n[shape]} cut(s) of shape `{shape}`; the model has no such shape")
    return "\n".join(out) + "\n"


# The model's seven shapes, by the class this script sorts a cut into, and the
# `TearMix` field that weighs each. Which operation a cut tears is the
# workload's business, so the check compares shapes WITHIN a family: the erase
# shapes among the torn erases, the program shapes among the torn programs.
MODEL_SHAPES = {
    "erase": [
        ("erase:zeroing", "erase_zeroing"),
        ("erase:all-zero", "erase_all_zero"),
        ("erase:erasing", "erase_erasing"),
        ("erase:reads-ff-weak", "erase_reads_ff_weak"),
        ("erased", "erase_reads_ff"),
        # Not a shape: where the cut landed (before the erase started). The
        # model draws it among the cuts at an erase, so it is judged there.
        ("cut:before-erase", "cut_before_erase"),
    ],
    "program": [
        ("program:command-boundary", "program_command_boundary"),
        ("program:mid-command", "program_mid_command"),
        ("program:mid-word", "program_mid_word"),
    ],
}


def model_weights(fields: dict[str, int], shapes: list[tuple[str, str]]) -> dict[str, int]:
    """A family's weights by class. A shape the model file does not weigh
    (no field, or 0) is not modelled: it is left out, so seeing it fails."""
    return {cls: fields[f] for cls, f in shapes if fields.get(f, 0) > 0}
# A class that is not a torn operation: a cut after the program finished.
NOT_TORN = {"complete"}
# DD29: a shape expected fewer times than this at the observed n is pooled.
POOL_BELOW = 5.0
Z95 = 1.959964
# DD32: the family-wise miss rate an honest sample may have. Each of a
# family's k tested cells is judged at the 1 - FAMILY_ALPHA/k level.
FAMILY_ALPHA = 0.05


def bonferroni_z(k: int, alpha: float = FAMILY_ALPHA) -> float:
    """The two-sided normal quantile for one of `k` cells judged together at
    family-wise level `alpha`: each cell at 1 - alpha/k (DD32)."""
    from statistics import NormalDist

    return NormalDist().inv_cdf(1 - alpha / (2 * max(1, k)))


def wilson(p: float, n: int, z: float = Z95) -> tuple[float, float]:
    """The Wilson score interval for a binomial share `p` at `n` trials, at
    the level `z` gives (95 % by default; `bonferroni_z` for a family's cells).

    Here `p` is the MODEL's share, not an observed one: the interval is the
    band an honest sample of `n` cuts from the model lands in."""
    if n <= 0:
        return 0.0, 1.0
    d = 1 + z * z / n
    centre = (p + z * z / (2 * n)) / d
    half = z * (p * (1 - p) / n + z * z / (4 * n * n)) ** 0.5 / d
    return max(0.0, centre - half), min(1.0, centre + half)


def judge_family(observed: dict[str, int], weights: dict[str, int],
                 alpha: float | None = FAMILY_ALPHA) -> list[dict]:
    """DD29, for one family of shapes: pool the thin shapes, test each cell,
    each at the 1 - 0.05/k level for the family's k tested cells (DD32).

    `alpha` is the family-wise level; `alpha=None` judges every cell at a
    plain 95 % (no correction) — only the self-test uses that, to show what
    the correction changes. `observed` maps a class to its cut count (unmodelled classes too);
    `weights` maps each MODELLED class to its model weight. Every cell has
    `ok` and the `classes` it stands for. A class the model has no weight for
    is a cell of its own that fails whenever it was seen: a shape the model
    cannot draw cannot be pooled away.
    """
    n = sum(observed.values())
    total = sum(weights.values())
    cells = [
        {"classes": [cls], "model": (w / total if total else 0.0), "seen": observed.get(cls, 0)}
        for cls, w in weights.items()
    ]
    # Every shape expected fewer than POOL_BELOW times at this n goes into one cell.
    thin = [c for c in cells if c["model"] * n < POOL_BELOW]
    if len(thin) > 1:
        cells = [c for c in cells if c not in thin]
        cells.append({
            "classes": [cls for c in thin for cls in c["classes"]],
            "model": sum(c["model"] for c in thin),
            "seen": sum(c["seen"] for c in thin),
        })
    # A pool that is itself still thin (or one thin shape left alone) joins the
    # thinnest of the others, so no cell is tested on fewer than POOL_BELOW.
    thin = [c for c in cells if c["model"] * n < POOL_BELOW]
    if len(cells) > 1 and len(thin) == 1:
        rest = min((c for c in cells if c is not thin[0]), key=lambda c: c["model"])
        rest["classes"] = rest["classes"] + thin[0]["classes"]
        rest["model"] += thin[0]["model"]
        rest["seen"] += thin[0]["seen"]
        cells.remove(thin[0])
    # DD32: k is the number of cells tested here, after pooling. A NOT
    # MODELLED cell (below) is not a test at a level; it fails when seen.
    k = len(cells)
    z = Z95 if alpha is None else bonferroni_z(k, alpha)
    level = 0.95 if alpha is None else 1 - alpha / k
    for c in cells:
        c["n"] = n
        c["k"] = k
        c["level"] = level
        c["expected"] = c["model"] * n
        c["share"] = c["seen"] / n if n else 0.0
        c["lo"], c["hi"] = wilson(c["model"], n, z)
        c["ok"] = n == 0 or c["lo"] <= c["share"] <= c["hi"]
    for cls, k in sorted(observed.items()):
        if cls not in weights and k:
            cells.append({
                "classes": [cls], "model": 0.0, "seen": k, "n": n, "expected": 0.0,
                "share": k / n, "lo": 0.0, "hi": 0.0, "ok": False, "unmodelled": True,
            })
    cells.sort(key=lambda c: -c["model"])
    return cells


def family_counts(rows: list[dict], family: str, payload: str = PAYLOAD) -> dict[str, int]:
    """The silicon cuts of `payload` that tore an operation of `family`."""
    return dict(Counter(
        r["class"] for r in rows
        if r["configuration"].startswith("silicon") and r["payload"] == payload
        and PHASE_OF.get(r["class"], "none") == family and r["class"] not in NOT_TORN
    ))


def read_weights(rs: str) -> dict[str, int]:
    """`TearMix::CX1`'s fields out of the model file."""
    import re

    at = rs.index("pub const CX1: TearMix = TearMix {")
    body = rs[at : rs.index("};", at)]
    return {k: int(v.replace("_", "")) for k, v in re.findall(r"(\w+):\s*(\d[\d_]*)", body)}


def judge_model(rows: list[dict], rs: str) -> tuple[bool, list[str]]:
    """The check: the page-aligned silicon cuts against `TearMix::CX1`.

    Returns (confirmed, lines). Only `flash-tears` (page-aligned) cuts feed
    the model's shares; `flash-tears-unaligned` is reported apart, below."""
    fields = read_weights(rs)
    out = []
    confirmed = True
    for family, shapes in MODEL_SHAPES.items():
        observed = family_counts(rows, family)
        n = sum(observed.values())
        weights = model_weights(fields, shapes)
        cells = judge_family(observed, weights)
        tested = [c for c in cells if not c.get("unmodelled")]
        k = len(tested)
        level = tested[0]["level"] if tested else 1 - FAMILY_ALPHA
        out += [
            f"torn {family}s: {n} cut(s) (page-aligned `{PAYLOAD}`, silicon); {k} tested cell(s), "
            f"each at the {level:.2%} level (Bonferroni: 1 - 0.05/{k}, DD32)",
            "",
            f"| shape | cuts | share | model share | {level:.2%} Wilson interval at n | expected | |",
            "|---|---:|---:|---:|---|---:|---|",
        ]
        for c in cells:
            name = " + ".join(f"`{x}`" for x in c["classes"])
            if len(c["classes"]) > 1:
                name += " (pooled: a shape expected < 5)"
            if c.get("unmodelled"):
                out.append(f"| {name} | {c['seen']} | {c['share']:.1%} | 0 (no such shape) | - | 0 | **NOT MODELLED** |")
            else:
                out.append(
                    f"| {name} | {c['seen']} | {c['share']:.1%} | {c['model']:.1%} | "
                    f"{c['lo']:.1%} - {c['hi']:.1%} | {c['expected']:.1f} | {'ok' if c['ok'] else '**OUTSIDE**'} |"
                )
            confirmed = confirmed and c["ok"]
        out.append("")
    out += unaligned_report(rows)
    return confirmed, out


def unaligned_report(rows: list[dict]) -> list[str]:
    """`flash-tears-unaligned` silicon cuts: counted, never judged. They
    neither confirm nor refute the erase mix (the program is not what tore
    it); section 8 of the report is where their program shapes are read."""
    counts = Counter(
        r["class"] for r in rows
        if r["configuration"].startswith("silicon") and r["payload"] == PAYLOAD_UNALIGNED
    )
    if not counts:
        return ["`flash-tears-unaligned`: no silicon cuts yet (reported apart; never part of this verdict)", ""]
    return [
        f"`flash-tears-unaligned`: {sum(counts.values())} silicon cut(s), reported apart, NOT part of the verdict "
        "(they neither confirm nor refute the erase mix)",
        "",
        *[f"- `{cls}`: {k}" for cls, k in sorted(counts.items())],
        "",
    ]


def check_model(rows: list[dict], table: str, rs_path: str) -> int:
    """DD29: is `TearMix::CX1` confirmed by these transcripts?"""
    with open(rs_path) as f:
        rs = f.read()
    confirmed, lines = judge_model(rows, rs)
    print("\n".join(lines))
    if confirmed:
        print(f"CONFIRMED: every shape's share lies inside its Bonferroni-corrected Wilson interval "
              f"(each family at 5 % overall, DD32) around {rs_path} "
              f"({table.splitlines()[0][3:].rstrip(".")})")
        return 0
    print(f"MODEL DIFFERS: a shape lies outside its Bonferroni-corrected Wilson interval (DD32), or the "
          f"silicon shows a shape the model has none of. Re-fit {rs_path} from this table "
          "(and its doc comment's counts), then re-run the t1/f2/f3 sweeps under it:\n")
    sys.stdout.write(table)
    return 1


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], epilog=CHECK_MODEL_RULE,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("transcripts", nargs="*", help=f"default: {DEFAULT_GLOB}")
    ap.add_argument("--write-report", metavar="PATH",
                    help=f"replace the block between {BEGIN} and {END} in PATH")
    ap.add_argument("--json", action="store_true", help="print one JSON object per cut instead")
    ap.add_argument("--self-test", action="store_true", help="check the zero-run reconstruction, then exit")
    ap.add_argument("--model-table", action="store_true",
                    help="print lp-nor-sim's calibrated model as the silicon cuts give it (Rust)")
    ap.add_argument("--check-model", action="store_true",
                    help=f"is {MODEL_RS} confirmed by the silicon cuts? (rule below); exit 1 with the re-fit table if not")
    ap.add_argument("--rom-split", metavar="TRACE",
                    help="read an `lp-emu-esp32c6 --trace SPI1` log of the unaligned image and print how "
                         "the ROM split each write into program commands")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if args.rom_split:
        return rom_split(args.rom_split)
    paths = args.transcripts
    if not paths:
        root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
        paths = sorted(glob.glob(os.path.join(root, DEFAULT_GLOB)))
    if not paths:
        print("no transcripts found", file=sys.stderr)
        return 1
    text, rows = analyze(paths)
    if args.model_table or args.check_model:
        table = model_table(rows)
        if args.model_table:
            sys.stdout.write(table)
            return 0
        root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
        return check_model(rows, table, os.path.join(root, MODEL_RS))
    if args.json:
        for r in rows:
            print(json.dumps(r, sort_keys=True))
        return 0
    if args.write_report:
        with open(args.write_report) as f:
            doc = f.read()
        if BEGIN not in doc or END not in doc:
            print(f"{args.write_report} has no {BEGIN} … {END} block", file=sys.stderr)
            return 1
        head, rest = doc.split(BEGIN, 1)
        _, tail = rest.split(END, 1)
        with open(args.write_report, "w") as f:
            f.write(head + BEGIN + "\n\n" + text + "\n" + END + tail)
        print(f"wrote {args.write_report}")
        return 0
    sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
