#!/usr/bin/env python3
"""Aggregate `flash-tears` transcripts into tear-shape histograms.

    scripts/emu/flash-tears-analyze.py                      # every committed transcript
    scripts/emu/flash-tears-analyze.py <transcript.txt>...  # just these
    scripts/emu/flash-tears-analyze.py --write-report docs/reports/2026-10-08-c6-nor-tear-calibration.md
    scripts/emu/flash-tears-analyze.py --json               # one JSON object per cut, for a notebook-free pipe

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
DEFAULT_GLOB = "lp-emu/transcripts/esp32c6/flash-tears/*.txt"
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


def read_transcript(path: str) -> Transcript:
    meta_path = path + ".meta.json"
    meta = {}
    if os.path.exists(meta_path):
        with open(meta_path) as f:
            meta = json.load(f)
    boots: list[Boot] = []
    with open(path, "rb") as f:
        for raw in f:
            line = raw.decode("utf-8", errors="replace").rstrip("\r\n")
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
    return Transcript(path, meta, boots)


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
PHASE_TEXT = {k: t for k, _, t in PHASE_ORDER}
PHASE_OF = {k: p for k, p, _ in PHASE_ORDER}


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


def classify(r: dict) -> Phase:
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
    if v == "torn-program":
        p = r["program"] or {}
        shape = p.get("shape")
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
    by_config: dict[str, list[Transcript]] = defaultdict(list)
    for t in transcripts:
        by_config[t.meta.get("configuration", "?")].append(t)
    out: list[str] = []
    rows: list[dict] = []
    w = out.append
    w(f"_Generated by `scripts/emu/flash-tears-analyze.py` over {len(paths)} transcript(s). "
      "Do not edit by hand; re-run it._")
    w("")
    for config in sorted(by_config, key=lambda c: (not c.startswith("silicon"), c)):
        # In the order they were taken: one board's cycle count only grows.
        ts = sorted(by_config[config], key=lambda t: (
            t.meta.get("date", ""),
            next((b.boot.get("latest") or 0 for b in t.boots), 0),
            t.path,
        ))
        w(f"### `{config}`")
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
                ph = classify(f) if f else None
                if is_cut(b) and f is not None:
                    n_cut += 1
                    cuts.append((b, ph))
                    loop_from = prev.scan_done_next if prev else None
                    rows.append({
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
        for cls, phase, text in PHASE_ORDER:
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
        for cls, _, _ in PHASE_ORDER:
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

        if config.startswith("silicon"):
            w(f"**`lp-nor-sim`'s assumptions against these {total} cuts** (the models as `lp-emu/lp-nor-sim` "
              "has them; the verdict is mechanical, from the counts above):")
            w("")
            for line in assumptions(cuts, reads_ff, reads_ff_weak, prog, zr, er, erase_cuts, lifted_to_zero,
                                    unexpected, settled_weak, journal_bad):
                w(line)
            w("")
    return "\n".join(out) + "\n", rows


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
    print("self-test ok")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("transcripts", nargs="*", help=f"default: {DEFAULT_GLOB}")
    ap.add_argument("--write-report", metavar="PATH",
                    help=f"replace the block between {BEGIN} and {END} in PATH")
    ap.add_argument("--json", action="store_true", help="print one JSON object per cut instead")
    ap.add_argument("--self-test", action="store_true", help="check the zero-run reconstruction, then exit")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    paths = args.transcripts
    if not paths:
        root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
        paths = sorted(glob.glob(os.path.join(root, DEFAULT_GLOB)))
    if not paths:
        print("no transcripts found", file=sys.stderr)
        return 1
    text, rows = analyze(paths)
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
