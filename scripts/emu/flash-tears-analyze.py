#!/usr/bin/env python3
"""Aggregate `flash-tears` transcripts into tear-shape histograms.

    scripts/emu/flash-tears-analyze.py                      # every committed transcript
    scripts/emu/flash-tears-analyze.py <transcript.txt>...  # just these
    scripts/emu/flash-tears-analyze.py --write-report docs/reports/2026-10-08-c6-nor-tear-calibration.md
    scripts/emu/flash-tears-analyze.py --json               # one JSON object per cut, for a notebook-free pipe
    scripts/emu/flash-tears-analyze.py --model-table        # lp-nor-sim's calibrated model, as Rust
    scripts/emu/flash-tears-analyze.py --check-model        # does lp-nor-sim hold those numbers?
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


PHASE_ORDER = [
    ("untouched", "erase", "untouched: the old pattern, whole (cut before the erase moved a cell)"),
    ("erase:zeroing", "erase", "torn erase, zeroing: a run of `0x00` from the front, old after it"),
    ("erase:all-zero", "erase", "torn erase, all `0x00`"),
    ("erase:erasing", "erase", "torn erase, erasing: stable zeros spread over old **and** new zero positions"),
    ("erase:old-left", "erase", "torn erase, old data left (only old zeros, more than a residue)"),
    ("erase:reads-ff-weak", "erase", "torn erase that reads all `0xFF` but has weak bits"),
    ("erased", "erase", "erased: all `0xFF`, no weak bits"),
    ("program:command-boundary", "program", "torn program, prefix ending on a 32-byte command boundary"),
    ("program:mid-command", "program", "torn program, prefix ending inside a 32-byte command"),
    ("program:scattered", "program", "torn program, scattered clears (no prefix)"),
    ("complete", "program", "complete: the new pattern, whole"),
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
                ph = classify(f, payload) if f else None
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
        journal_bad = sum(j.get("damaged", 0) for b, _ in cuts for j in b.journals)
        journal_torn = sum(1 for b, _ in cuts for j in b.journals if j.get("torn_next"))
        w("Outside the in-flight sector, over every cut:")
        w("")
        w(f"- settled sectors not holding their last cycle's pattern: **{unexpected}** (of {settled_n} read)")
        w(f"- weak bits in settled sectors: **{settled_weak}**")
        w(f"- damaged journal slots: **{journal_bad}**; torn next slots: **{journal_torn}**")
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
            for cls in ("program:command-boundary", "program:mid-command"):
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
      f"starts 4 or 20 bytes past a 32-byte boundary and `absolute` says on one)")
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
         f"{n_prog} torn programs, all prefixes; {scattered} scattered",
         verdict(scattered == 0, n_prog, ("held", "contradicted", "not yet seen"))),
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
         f"{untouched} of {n_erase} erase cuts left the old pattern whole",
         verdict(untouched == 0, n_erase, ("held: no started erase left the old data", "seen", "not yet seen"))),
        ("A cut damages only the operation in flight",
         f"{unexpected} settled sectors damaged, {settled_weak} weak bits in them, {journal_bad} journal slots damaged",
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
    assert plan(7, 231)[:3] == [(0, 20), (20, 112), (132, 176)], plan(7, 231)[:3]
    print("self-test ok")
    return 0


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
    out = [
        f"// {len(rows)} silicon cuts.",
        "pub const CX1: TearMix = TearMix {",
        f"    erase_zeroing: {n['erase:zeroing']},",
        f"    erase_all_zero: {n['erase:all-zero']},",
        f"    erase_erasing: {n['erase:erasing']},",
        f"    erase_reads_ff_weak: {n['erase:reads-ff-weak']},",
        f"    erase_reads_ff: {n['erased']},",
        f"    program_command_boundary: {n['program:command-boundary']},",
        f"    program_mid_command: {n['program:mid-command']},",
        "};",
        f"pub const CX1_ERASING: [(u32, u32); {len(erasing)}] = [",
        *[f"    ({z}, {w})," for z, w in erasing],
        "];",
        f"pub const CX1_READS_FF_WEAK: [u32; {len(ff_weak)}] = [{', '.join(map(str, ff_weak))}];",
    ]
    for shape in ("untouched", "erase:old-left", "program:scattered", "unknown"):
        if n[shape]:
            out.append(f"// NOT MODELLED: {n[shape]} cut(s) of shape `{shape}`; the model has no such shape")
    return "\n".join(out) + "\n"


def check_model(table: str, rs_path: str) -> int:
    """Does the model file hold the numbers these transcripts give?"""
    import re

    with open(rs_path) as f:
        rs = f.read()

    def numbers(text: str, start: str, end: str) -> list[int]:
        at = text.index(start)
        body = text[at + len(start) : text.index(end, at + len(start))]
        body = re.sub(r"//[^\n]*", "", body)
        return [int(x.replace("_", "")) for x in re.findall(r"\b\d[\d_]*\b", body)]

    ok = True
    for start, end in (
        ("pub const CX1: TearMix = TearMix {", "};"),
        ("pub const CX1_ERASING: [(u32, u32);", "];"),
        ("pub const CX1_READS_FF_WEAK: [u32;", ";"),
    ):
        want, have = numbers(table, start, end), numbers(rs, start, end)
        if want != have:
            ok = False
            print(f"MODEL DIFFERS: `{start.split(':')[0]}` holds {have}, the transcripts give {want}")
    for line in table.splitlines():
        if line.startswith("// NOT MODELLED"):
            ok = False
            print(line)
    if ok:
        print(f"{rs_path} matches the transcripts ({table.splitlines()[0][3:]})")
        return 0
    print(f"re-run with --model-table and update {rs_path} (and its doc comment's counts)")
    return 1


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("transcripts", nargs="*", help=f"default: {DEFAULT_GLOB}")
    ap.add_argument("--write-report", metavar="PATH",
                    help=f"replace the block between {BEGIN} and {END} in PATH")
    ap.add_argument("--json", action="store_true", help="print one JSON object per cut instead")
    ap.add_argument("--self-test", action="store_true", help="check the zero-run reconstruction, then exit")
    ap.add_argument("--model-table", action="store_true",
                    help="print lp-nor-sim's calibrated model as the silicon cuts give it (Rust)")
    ap.add_argument("--check-model", action="store_true",
                    help=f"exit 1 unless {MODEL_RS} holds what --model-table prints")
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
        return check_model(table, os.path.join(root, MODEL_RS))
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
