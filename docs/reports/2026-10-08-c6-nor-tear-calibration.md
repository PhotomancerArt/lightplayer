# CX1's NOR flash, cut 200 times: what real tears look like

**Date** 2026-10-08 · **Plan** `lp2025/2026-10-08-1017-tree-store-device-round`
(M4, scope item 4) · **Board** CX1 (`c6-expendable`, tag `sacrificial`, MAC
`14:C1:9F:E6:54:90`; a generic ESP32-C6 dev board, not a XIAO) · **Flash
part** JEDEC id `0x46 0x40 0x16` (manufacturer `0x46`, not identified here;
capacity byte `0x16` = 4 MiB) · **Configuration** `silicon:esp32c6` ·
**Firmware** the `test_flash_tears` image, built from `f5f629536`,
`e8dfa2bf5`, `a53bdecba` and `c322c8489` (one source tree: the commits
between them only add transcripts) · **Payload** `flash-tears`
(`lp-fw/fw-checks/src/checks/flash_tears/`) · **Transcripts**
`lp-emu/transcripts/esp32c6/flash-tears/` · **Analysis** `just
flash-tears-analyze` (`scripts/emu/flash-tears-analyze.py`)

> **STATUS: PARTIAL — 200 of 500 cuts. The sitting resumes when CX1 is
> back** (it is travelling with Yona). Every count, share and verdict below
> is "so far". Section 7 is generated: re-running `just flash-tears-analyze`
> after each new batch rewrites it from every committed transcript, and the
> prose around it says which of its numbers it leans on. The model
> correction (scope item 5, section 5) is in `lp-nor-sim` as
> `TearModel::Calibrated`: **calibrated on these 200 cuts, to be re-checked
> at 500** (`just flash-tears-analyze --check-model` says whether its shares are
> still within noise of every committed cut; the rule is in section 9).
>
> **2026-10-08 night: the owed sitting did not run.** CX1 was attached at
> Yona's other house, but that hub cannot switch power (Yona), so no cut
> there is a power cut; the hardware half stopped after one `board
> power-cycle`, which printed success and changed nothing anyone could see
> (section 9). What did land: the **unaligned-program payload**
> (`flash-tears-unaligned`), its analysis, its emulator dry run and what the
> mask ROM does with an unaligned write (section 8), and the sitting as one
> command for next week (section 9). Sections 8 and 9 follow the generated
> section.

## 1. What was measured

The payload owns 18 sectors at the start of CX1's `lpfs` partition
(`0x350000`): two journal sectors, then 16 region sectors. Forever, with
nothing printed, it writes cycle `c` to both journals, erases region sector
`c % 16` and programs it page by page with a pattern that is the bitwise
**complement** of the one it erased. So every stable `0` in a torn sector
says which operation left it: a `0` where the old pattern had one is an old
zero the erase has not lifted; a `0` where the old pattern had a `1` is
something the part did since — a program, or (it turns out) the erase
itself. On every boot it reads each region sector **8 times** (a bit that
does not read the same all eight times is **weak**), classifies it, and
repairs it.

The cuts: `scripts/emu/flash-tears-cuts.py` reads a boot's scan to its
`SCAN DONE` line, waits a random 50–2000 ms, and runs `board power-cycle`,
which switches VBUS off at the board's USB hub. Four batches of 50, each a
`lp-cli validate record flash-tears --config silicon:esp32c6`, each
reflashed first. The cut lands 10–97 work cycles into the loop (median 50),
so where in a cycle it lands is, as far as anything here can tell, uniform.

Two things belong to *this setup*, not to the part alone: the cut is VBUS
going away, so the board's 3.3 V rail decays at whatever rate its
capacitance allows, and the C6 stops when it stops. All 200 cut boots
reported reset `poweron` (none `brownout`). Every shape below is "this part,
on this board's power path".

The four boots that start each batch (espflash's reset after the flash,
`usb-uart`) are not power cuts and are left out of every count; section 7
lists what they found.

## 2. What a real tear looks like

**Nothing outside the operation in flight was ever touched.** 3,000 settled
sectors read across 200 cuts: none damaged, no weak bit in any of them; no
journal slot damaged.

**The erase is where the power goes (166 of 200 cuts, 83 %), and it passes
through four states, three of which `lp-nor-sim` cannot produce:**

1. **Zeroing** (10 cuts). A run of `0x00` from offset 0, and the old pattern
   whole after it. Every run ended on a **4-byte word boundary** — 10 of 10,
   about a two-in-a-million coincidence if the end were random — and **none
   on a 32-byte command or a 256-byte page boundary**.
2. **All `0x00`** (26 cuts). Every bit of the sector is 0, stable, no weak
   bits.
3. **Erasing** (28 cuts). Zeros spread over the whole sector, and the share
   of the old pattern's zeros still 0 tracks the share of its *ones* now 0
   (78.7 % / 77.2 %, 41.0 % / 40.1 %, 15.4 % / 14.8 %, … down to a handful
   of bits). The erase is lifting a sector that is already all zero, every cell
   at once, not position by position. 26 of the 28 carry weak bits (median
   91, max 2,166).
4. **Reads erased** (102 cuts). All `0xFF` in every stable bit — and **101
   of them with no weak bit at all** across 8 reads. One carried 2 weak
   bits.

Read in order, that is the textbook NOR erase: the part first programs the
whole sector to `0` (front to back, a word at a time), then pulses it back
to `1`, then spends most of the operation on a sector that already reads
erased (verify, and whatever correction the part does). If the cut instant
is uniform, the shares put the four at roughly 1.5 ms, 4 ms, 4 ms and 15 ms
of the 25 ms median erase. That timing is an inference from shares, not a
measurement, and the part's datasheet has not been read: the manufacturer
byte is not identified here.

**Why the zero runs are the erase and not a program.** The obvious
alternative — the erase did nothing, and the program then landed the new
pattern on unerased cells (which also reads `old & new = 0x00`) — predicts
runs that stop where programs stop. Real torn programs stop on a 32-byte
command boundary 26 times in 33; the zero runs did 0 times in 10. And the
"erasing" states start from all-zero: their old-zero and old-one shares are
equal at every depth, which only a sector that was zeroed first can show.

**The program (34 cuts, 17 %) is gentle.** Every torn program left a prefix
of exactly what was asked, then nothing: 33 of 33 prefixes, **0 partial
bytes**, **0 scattered**. 26 ended on a 32-byte command boundary (the mask
ROM's own program command; 5 of those also on a 256-byte page), and the 7
that ended inside a command ended on a 4-byte word boundary. No torn program
had a weak bit. One cut landed after the program finished.

The phase shares match the clock: the timed cycle spends 81.3 % of its time
erasing and 18.4 % programming (medians, 204 boots: 24,960 µs erase, 5,658
µs for 16 pages); 83.0 % of cuts landed in the erase and 17.0 % in the
program. (Counting the zero runs as program would have made it 22 %, and the
all-zero sectors too, 35 %.)

The firmware's own verdicts need a gloss: `mixed` (59 cuts) was named for
"neither phase" and is in fact every zeroing, all-zero and most erasing
state; three `torn-program` and two `torn-erase` verdicts are the last few
residual zeros of an erase, which happened to sit only on new-zero or only
on old-zero positions. Section 7 has the full cross-table. The firmware is
right about the bits; the host is where they are named.

## 3. What it means for a store

- **A torn erase can leave a sector that reads `0x00`**: 36 of 200 cuts so
  far (18 %), from the front or throughout. A store whose format gives
  `0x00` a meaning — a zero length, a zero sequence number, a flag that is
  "set" when cleared — reads a torn erase as data. `lp-nor-sim`'s guessed
  models never make this sector, so until the calibrated model (section 5)
  no store in the testbed had met it.
- **A torn erase usually reads perfectly erased** (101 of 166 erase cuts),
  with nothing a re-read could catch. `lp-nor-sim`'s README rule — trust only
  a sector you finished erasing *and then marked* — is the right rule, and
  this is the evidence for it; the simulator's own "reads erased" shape is
  easier to catch than the real one (it carries ~128 weak bits).
- **Torn programs are tamer than the simulator's**: word-granular prefixes,
  never a stray or a scatter. `BytePrefix` and `RandomBits` are harsher, so
  a store that survives them survives this — they stay useful as a bound.
- **A cut never reached a neighbour**: the "one operation in flight" model
  held on every cut.

## 4. `lp-nor-sim`'s assumptions, so far

The mechanical verdicts are the last table of section 7. Read with the
models in `lp-emu/lp-nor-sim/src/nor_flash_sim.rs` (on main at the time of
writing):

| model assumption | so far |
|---|---|
| A torn program lands a prefix (`BytePrefix`) | **held** (33 of 33) |
| …with a partial byte after it | **not seen** — prefixes end on whole 4-byte words |
| A torn program may scatter (`RandomBits`) | **not seen** — harsher than this part |
| The program op is a 256-byte page | **contradicted** — the unit on the C6 is the ROM's 32-byte command, and inside one, a 4-byte word |
| Torn erase shape 0: byte-wise old / `0xFF` / weak | **not seen** — no erase cut left old bytes beside erased ones |
| Torn erase shape 1: reads `0xFF`, weak bits | **held, rarely, and much lighter** — 1 of 102 such sectors, 2 weak bits, not ~128 |
| Torn erase shape 2: `0xFF` up to a point, old after | **not seen as modelled** — seen as `0x00` up to a point (the zeroing) |
| A torn erase only lifts bits | **contradicted** — 62 of 166 erase cuts had a stable `0` where the old data had a `1` |
| Weak bits come from torn erases | **held** — 27 of 27 sectors with weak bits were erase-phase |
| A started erase changes the sector | **held** — none left the old data whole |
| A cut damages only the operation in flight | **held** — 0 settled sectors or journal slots damaged |

## 5. The correction: `TearModel::Calibrated` (scope item 5)

> **2026-10-14: re-fitted on 500 (section 11).** The table below is the
> 200-cut fit. The 500 added a program torn inside a word and a cut that
> landed before an erase started, and `TearMix::CX1` now holds the 500's
> counts.

**Calibrated on 200 cuts; re-checked at 500.** Additive: `clean`,
`byte_prefix` and `random_bits` are unchanged and stay the drivers' default
list (`TearModel::ALL`); the new models run only when named. Code:
`lp-emu/lp-nor-sim/src/calibrated_tear.rs`.

**What it does.** Only the shape *inside* a torn operation is drawn; which
operation is torn stays the workload's (the op counter).

| torn op | shape | weight (= cuts seen) | what the model leaves |
|---|---|---:|---|
| erase | zeroing | 10 of 166 | `0x00` from offset 0 to a uniform 4-byte word boundary inside the sector, the old data after it, no weak bit |
| erase | all zero | 26 | every bit 0, no weak bit |
| erase | erasing | 28 | all `0xFF` except exactly *k* stable zeros and *w* weak bits at distinct uniform positions, (*k*, *w*) drawn from the 28 observed rows, linearly interpolated between neighbours |
| erase | reads `0xFF`, weak | 1 | all `0xFF`, 2 weak bits (the one observed sector) |
| erase | reads `0xFF` | 101 | all `0xFF`, no weak bit — the erase was cut, nothing shows it |
| program | command boundary | 26 of 33 | a prefix of exactly what was asked, ending on a 32-byte command (offset 0, nothing landed, included) |
| program | mid-command | 7 | a prefix ending on a 4-byte word inside a command |

The command and word are counted from the start of the page operation:
every silicon program was page-aligned, so absolute and relative alignment
were never told apart. The weights live in `TearMix::CX1`
(`NorFlashSim::set_tear_mix` replaces them); `--model-table` prints them
from the transcripts. `calibrated_zeroing`, `calibrated_all_zero`,
`calibrated_erasing`, `calibrated_reads_ff_weak` and `calibrated_reads_ff`
force every torn erase into one state (programs keep the mix), because under
the mix a `0x00` sector meets only about one erase cut in five.

**Does it reproduce the part?** `just flash-tears-sim` runs the payload's
own boot flow (`fw-checks/examples/flash_tears_on_nor_sim.rs`: the same
scan, repair, timed cycle and work loop the firmware runs) on `lp-nor-sim`,
cutting 10–97 cycles after the scan at an op weighted by CX1's median
timings, and this report's own classifier sorts the result. Simulator
numbers, 200 cuts a seed (`lp-nor-sim` at `8d9b99491`):

| shape | CX1 | calibrated, seed 1 | seed 2 | byte_prefix / random_bits (seeds 1, 2) | clean (seeds 1, 2) |
|---|---:|---:|---:|---|---|
| erase: untouched | 0 | 0 | 0 | 0, 0 | 164, 169 |
| erase: zeroing | 10 | 12 | 16 | 0, 0 | 0, 0 |
| erase: all `0x00` | 26 | 24 | 24 | 0, 0 | 0, 0 |
| erase: erasing | 28 | 33 | 32 | 94, 112 | 0, 0 |
| erase: old data left | 0 | 0 | 0 | 37, 31 | 0, 0 |
| erase: reads `0xFF`, weak | 1 | 2 | 3 | 33, 26 | 0, 0 |
| erase: reads `0xFF` | 101 | 93 | 94 | 0, 0 | 4, 2 |
| program: command boundary | 26 | 29 | 23 | 0, 0 | 31, 28 |
| program: mid-command | 7 | 6 | 7 | 35, 30 (`byte_prefix`, partial bytes) | 0, 0 |
| program: scattered | 0 | 0 | 0 | 30–35 (`random_bits`) | 0, 0 |
| complete | 1 | 1 | 1 | 1, 1 | 1, 1 |

Weak bits in erasing sectors: CX1 1 / 91 / 2,166 (min / median / max), the
model 1 / 80 / 1,446 and 1 / 65 / 2,059. Erase-phase cuts with a stable `0`
where the old data had a `1`: CX1 62 of 166, the model 67 of 164 and 69 of
169. The guessed models never make a `0x00` sector and never a silent
reads-erased one; the calibrated one lands in CX1's rows. With 200 cuts the
sampling noise is a few cuts a row; the one row the model runs high on,
reads-`0xFF`-with-weak-bits (5 of 333 against 1 of 166), rests on a single
observation.

**What the stores made of it** (`lp-store-bench sweep`, exhaustive single
cut, 128 sectors, seeds 1 and 2, every cut point of each swept step;
simulator numbers):

| candidate | workload | cases | of them torn erases | failures under calibrated | under each forced erase shape | under the guessed three |
|---|---|---:|---:|---:|---|---|
| t1 | save:c40 | 852 | 26 | 0 | 0 | 0 |
| t1 | panel:c40 | 1,074 | 14 | 0 | 0 | 0 |
| t1 | push:c40 | 1,042 | 42 | 0 | 0 | 0 |
| f2 | save:c40 | 14,360 | 960 | 0 | 0 | 0 |
| f2 | panel:c40 | 800 | 200 | 0 | 0 | 0 |
| f2 | push:c40 | 838 | 94 | 0 (80 non-atomic) | 0 (80) | 0 (80) |
| f3 | save:c40 | 2,018 | 260 | 0 (591 non-atomic) | 0 (591) | 0 (591–592) |
| f3 | panel:c40 | 826 | 204 | 0 | 0 | 0 |
| f3 | push:c40 | 2,436 | 452 | 0 (2,374 non-atomic) | 0 (2,374) | 0 (2,374) |
| f1 | push:c40 | 84 | 30 | 84 | 84 | 84 |
| f1 | save:c40, panel:c40 | 0 | — | — (never reached: f1's c40 push is `NoSpace` fault-free at 128 sectors) | | |
| f1 | push:c20 | 1,968 | 494 | 1,328 | 1,328 | 1,326 / 1,322 / 1,326 |
| f1 | save:c20 | 822 | 134 | 0 (412 non-atomic) | 0 (412) | 0 (412–415) |
| f1 | panel:c20 | 800 | 200 | 0 | 0 | 0 |

Every f1 failure is one of two kinds, and neither is the tear model's: the
same cases fail under `clean`. `doc_not_old_or_new` (`/hardware.json: 0 B,
neither old nor new`) is f1's write path — littlefs as the firmware ships it
creates a file before writing it, so a cut between leaves it empty (store;
already in the testbed's 2026-10-07 overnight report, push and repush failing
under every tear). `next_step_failed: NoSpace` is the harness asking f1 to run
a step it cannot fit (harness: c40 does not fit f1 at 128 sectors; the c20
rows are the meaningful ones). The two extra f1 push failures under
`calibrated` (1,328 against `clean`'s 1,326) are the same kind, from
calibrated program prefixes. Non-atomic counts are scored, not failures:
littlefs's push and f3's save are not transactions.

So far, then: **no store in the race reads a torn erase as data.** Each
forced shape — a `0x00` run, a `0x00` sector, a residue with weak bits, a
sector that reads erased — gives exactly the same outcome per candidate as
the guessed models do, which says these stores never trust a sector whose
erase they did not finish (the rule section 3 draws). t1 meets few erase
cuts (14–42 cases a workload), f2 and f3 hundreds.

## 6. What this does not cover yet

- **300 more cuts.** `scripts/emu/flash-tears-soak.sh --batches 6` when CX1
  is back, then `just flash-tears-analyze`.
- **One part, one board.** The tear shapes belong to JEDEC `0x464016` on
  CX1's power path. Another C6 board with another flash part is another
  calibration.
- **The zero-run ends here are reconstructed.** These 200 records predate
  `leading_zero_bytes` (`3cfd41265`), so the analysis pins each run's end
  from the per-page counts against the pattern and checks it against the
  record's `0xFF` byte count. The rest of the sitting records it directly.
- **Weak bits are 8 reads at boot.** How often a weak bit flips, and whether
  it drifts, is not measured.
- **Retention is not measured.** Every torn sector is rewritten on the boot
  that finds it, so nothing here says whether a sector that "reads erased"
  after a torn erase still does a day later.
- **No cut landed in a journal write** (0.3 % of a cycle). The journal has
  never been seen torn.
- **Wear is light.** By the end of batch 4 the loop had run 46,528 cycles
  over 16 sectors — about 2,900 erases each, plus whatever came before the
  journal started counting. Tears on a worn part may differ (M7 wears one
  sector out).
- **The first four silicon sidecars carry no board metadata** (`board`,
  `mac`): the board is named only by the `ft-boot` records' MAC. They stay
  as they are (a transcript is never edited); from the next batch on the
  soak driver states the board's mark, slug, MAC and the expected flash
  JEDEC id (`lp-cli validate record --board/--mac/--note`) and checks the id
  against every boot's own record (`scripts/emu/flash-tears-check-part.py`).
- **The model's program unit is relative to the page op.** CX1's programs
  were all page-aligned; whether an unaligned program's 32-byte commands
  start at its address or at a 32-byte boundary is not measured on the part.
  The ROM's half is known (section 8): it counts from the address up to the
  end of the write's first page. The payload that measures the part's half
  is ready (section 8); the cuts are section 9's.

## 7. Generated: every transcript, sorted

<!-- flash-tears-analyze:begin -->

_Generated by `scripts/emu/flash-tears-analyze.py` over 16 transcript(s). Do not edit by hand; re-run it._

### `silicon:esp32c6`

| transcript | date | firmware | cycles | boots | cuts | not cuts |
|---|---|---|---|---:|---:|---|
| `silicon-esp32c6-2026-10-08-f5f629536.txt` | 2026-10-08 | `f5f6295369c0` | 20589–23200 | 51 | 50 | 1× reset `usb-uart`, not a power cut |
| `silicon-esp32c6-2026-10-08-e8dfa2bf5.txt` | 2026-10-08 | `e8dfa2bf5904` | 28159–30724 | 51 | 50 | 1× reset `usb-uart`, not a power cut |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` | 2026-10-08 | `a53bdecba0d5` | 35441–38061 | 51 | 50 | 1× reset `usb-uart`, not a power cut |
| `silicon-esp32c6-2026-10-08-c322c8489.txt` | 2026-10-08 | `c322c8489713` | 44089–46528 | 51 | 50 | 1× reset `usb-uart`, not a power cut |
| `silicon-esp32c6-2026-10-14-41e96eead.txt` | 2026-10-14 | `41e96eead15c` | 42–2102 | 51 | 50 | 1× fresh region (first boot, nothing in flight) |
| `silicon-esp32c6-2026-10-14-41e96eead-r2.txt` | 2026-10-14 | `41e96eead15c` | 3664–5900 | 51 | 50 | 1× reset `usb-uart`, not a power cut |
| `silicon-esp32c6-2026-10-14-41e96eead-r3.txt` | 2026-10-14 | `41e96eead15c` | 6859–8802 | 51 | 50 | 1× reset `usb-uart`, not a power cut |
| `silicon-esp32c6-2026-10-14-41e96eead-r4.txt` | 2026-10-14 | `41e96eead15c` | 9747–11782 | 51 | 50 | 1× reset `usb-uart`, not a power cut |
| `silicon-esp32c6-2026-10-14-41e96eead-r5.txt` | 2026-10-14 | `41e96eead15c` | 12812–14847 | 51 | 50 | 1× reset `usb-uart`, not a power cut |
| `silicon-esp32c6-2026-10-14-41e96eead-r6.txt` | 2026-10-14 | `41e96eead15c` | 16663–18971 | 51 | 50 | 1× reset `usb-uart`, not a power cut |
| **all** | | | | 510 | **500** | |

Who the records say ran it (from every `ft-boot`):

| MAC | flash JEDEC id (manufacturer, type, capacity) | flash size (capacity byte) | lpfs base | boots |
|---|---|---|---|---:|
| `14:c1:9f:e6:54:90` | `0x464016` | 4 MiB | `0x350000` | 510 |

Reset reasons of the cut boots: `poweron` 500.

Boots that were not power cuts (left out of every count below):

| boot | reset | state | in-flight sector |
|---|---|---|---|
| `silicon-esp32c6-2026-10-08-f5f629536.txt` boot 0 | `usb-uart` | resume | erased |
| `silicon-esp32c6-2026-10-08-e8dfa2bf5.txt` boot 0 | `usb-uart` | resume | journal:wrap-erase |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 0 | `usb-uart` | resume | program:command-boundary |
| `silicon-esp32c6-2026-10-08-c322c8489.txt` boot 0 | `usb-uart` | resume | erased |
| `silicon-esp32c6-2026-10-14-41e96eead.txt` boot 0 | `usb-uart` | fresh | — |
| `silicon-esp32c6-2026-10-14-41e96eead-r2.txt` boot 0 | `usb-uart` | resume | erased |
| `silicon-esp32c6-2026-10-14-41e96eead-r3.txt` boot 0 | `usb-uart` | resume | erased |
| `silicon-esp32c6-2026-10-14-41e96eead-r4.txt` boot 0 | `usb-uart` | resume | erased |
| `silicon-esp32c6-2026-10-14-41e96eead-r5.txt` boot 0 | `usb-uart` | resume | erased |
| `silicon-esp32c6-2026-10-14-41e96eead-r6.txt` boot 0 | `usb-uart` | resume | erased |

Outside the in-flight sector, over every cut:

- settled sectors not holding their last cycle's pattern: **0** (of 7500 read)
- weak bits in settled sectors: **0**
- damaged journal slots outside the operation in flight: **0**; inside it: 256 (of 256 damaged); torn next slots: **0**
- cuts whose operation in flight was the journal's own: `journal:wrap-erase` 3
- work cycles completed between a scan and its cut (min / median / max): 9 / 43 / 97

**Tear shapes, 500 cuts** (the in-flight sector of each):

| phase | shape | cuts | share |
|---|---|---:|---:|
| erase | cut before the erase started: both journal copies name the cycle and the sector holds the old pattern whole (the cut landed between the journal write and the erase command) | 1 | 0.2 % |
| erase | untouched: the old pattern, whole, and the journal does not say where the cut landed | 0 | 0.0 % |
| erase | torn erase, zeroing: a run of `0x00` from the front, old after it | 16 | 3.2 % |
| erase | torn erase, zeroing caught inside a word: whole `0x00` words from the front, then one word with only part of its pre-program clears landed (maybe weak bits on the others), old after it | 1 | 0.2 % |
| erase | torn erase, all `0x00` | 63 | 12.6 % |
| erase | torn erase, first lift: all `0x00` but a handful of bits (at most 32), each a stable 1 or weak | 0 | 0.0 % |
| erase | torn erase, erasing: stable zeros spread over old **and** new zero positions | 69 | 13.8 % |
| erase | torn erase, old data left (only old zeros, more than a residue) | 0 | 0.0 % |
| erase | torn erase that reads all `0xFF` but has weak bits | 4 | 0.8 % |
| erase | erased: all `0xFF`, no weak bits | 264 | 52.8 % |
| program | torn program, prefix ending on a 32-byte command boundary | 58 | 11.6 % |
| program | torn program, prefix ending inside a 32-byte command | 19 | 3.8 % |
| program | torn program, whole words then one word only part-landed (some of its clears, maybe weak bits), nothing after | 2 | 0.4 % |
| program | torn program, scattered clears (no prefix) | 0 | 0.0 % |
| program | complete: the new pattern, whole | 0 | 0.0 % |
| journal | cut in a journal copy's wrap: its erase before slot 0 (or that slot's first entry); the other copy kept the cycle | 3 | 0.6 % |
| journal | cut in a journal entry's program (between the two copies' writes) | 0 | 0.0 % |
| none | unknown | 0 | 0.0 % |
| **erase** | | **418** | 83.6 % |
| **program** | | **79** | 15.8 % |

The firmware's `verdict` against the phase it was sorted into:

| firmware verdict | phase | cuts |
|---|---|---:|
| `complete` | journal:wrap-erase | 3 |
| `erased` | erased | 264 |
| `erased-weak` | erase:reads-ff-weak | 4 |
| `mixed` | erase:all-zero | 63 |
| `mixed` | erase:erasing | 58 |
| `mixed` | erase:zeroing | 16 |
| `mixed` | erase:zeroing-mid-word | 1 |
| `old` | cut:before-erase | 1 |
| `torn-erase` | erase:erasing | 5 |
| `torn-program` | erase:erasing | 6 |
| `torn-program` | program:command-boundary | 58 |
| `torn-program` | program:mid-command | 19 |
| `torn-program` | program:mid-word | 2 |

**Weak bits** (a bit that did not read the same all 8 times):

- in-flight sectors with any: **71** of 500

| phase | cuts | with weak bits | weak bits (min / median / max, where any) |
|---|---:|---:|---|
| cut:before-erase | 1 | 0 | — |
| erase:zeroing | 16 | 0 | — |
| erase:zeroing-mid-word | 1 | 0 | — |
| erase:all-zero | 63 | 0 | — |
| erase:erasing | 69 | 65 | 1 / 83 / 2343 |
| erase:reads-ff-weak | 4 | 4 | 1 / 2.5 / 8 |
| erased | 264 | 0 | — |
| program:command-boundary | 58 | 0 | — |
| program:mid-command | 19 | 0 | — |
| program:mid-word | 2 | 2 | 1 / 1 / 1 |
| journal:wrap-erase | 3 | 0 | — |

- sectors reading all `0xFF` in every stable bit: 268; of those with weak bits: **4** (2, 3, 1, 8 weak bits)

**Torn programs**: where the landed prefix ends (byte offset in the sector):

| shape | cuts | on a 4-B word boundary | on a 256-B page boundary | partial bytes (min / median / max) | end mod 32 |
|---|---:|---:|---:|---|---|
| program:command-boundary | 58 | 58 | 9 | 0 / 0 / 0 | 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 |
| program:mid-command | 19 | 19 | 0 | 0 / 0 / 0 | 20, 20, 20, 4, 8, 4, 28, 24, 4, 4, 16, 4, 12, 12, 28, 24, 8, 8, 8 |
| program:mid-word | 2 | 0 | 0 | 1 / 1.5 / 2 | 24, 0 |

**Zeroing runs**: where the `0x00` run from the front stops (byte offset). `exact` is the firmware's `leading_zero_bytes`; otherwise the end is pinned from per-page counts against the pattern (a range where the pattern has `0xFF` bytes), and checked against the record's `ff_bytes`.

| cut | zero run ends at | how | mod 32 | on a 4-B word boundary | on a 32-B command boundary | on a 256-B page boundary |
|---|---|---|---|---|---|---|
| `silicon-esp32c6-2026-10-08-f5f629536.txt` boot 12 (cycle 21200) | 1300 | pages | 20 | yes | no | no |
| `silicon-esp32c6-2026-10-08-f5f629536.txt` boot 29 (cycle 22054) | 3639–3640 | pages | 23–24 | yes | no | no |
| `silicon-esp32c6-2026-10-08-f5f629536.txt` boot 44 (cycle 22859) | 3800 | pages | 24 | yes | no | no |
| `silicon-esp32c6-2026-10-08-e8dfa2bf5.txt` boot 11 (cycle 28612) | 156 | pages | 28 | yes | no | no |
| `silicon-esp32c6-2026-10-08-e8dfa2bf5.txt` boot 18 (cycle 28957) | 3576 | pages | 24 | yes | no | no |
| `silicon-esp32c6-2026-10-08-e8dfa2bf5.txt` boot 22 (cycle 29137) | 3432 | pages | 8 | yes | no | no |
| `silicon-esp32c6-2026-10-08-e8dfa2bf5.txt` boot 37 (cycle 30050) | 3504 | pages | 16 | yes | no | no |
| `silicon-esp32c6-2026-10-08-e8dfa2bf5.txt` boot 42 (cycle 30288) | 3852 | pages | 12 | yes | no | no |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 32 (cycle 37156) | 3964 | pages | 28 | yes | no | no |
| `silicon-esp32c6-2026-10-08-c322c8489.txt` boot 45 (cycle 46299) | 2164 | pages | 20 | yes | no | no |
| `silicon-esp32c6-2026-10-14-41e96eead-r2.txt` boot 15 (cycle 4228) | 2300 | exact | 28 | yes | no | no |
| `silicon-esp32c6-2026-10-14-41e96eead-r2.txt` boot 28 (cycle 4838) | 2360 | exact | 24 | yes | no | no |
| `silicon-esp32c6-2026-10-14-41e96eead-r4.txt` boot 3 (cycle 9874) | 1264 | exact | 16 | yes | no | no |
| `silicon-esp32c6-2026-10-14-41e96eead-r5.txt` boot 23 (cycle 13658) | 788 | exact | 20 | yes | no | no |
| `silicon-esp32c6-2026-10-14-41e96eead-r6.txt` boot 42 (cycle 18631) | 3616 | exact | 0 | yes | yes | no |
| `silicon-esp32c6-2026-10-14-41e96eead-r6.txt` boot 47 (cycle 18891) | 3072 | exact | 0 | yes | yes | yes |
| **all 16** | | | | **16** | **2** | **1** |

**Zeroing caught inside a word**: the word the run stopped in, and how many of its clears (the old pattern's ones in it) had landed. The record places the landed clears only to their page (`page_landed`); that they sit in the word the run had reached is the reading.

| cut | word at | clears landed | of | weak bits |
|---|---:|---:|---:|---:|
| `silicon-esp32c6-2026-10-14-41e96eead-r6.txt` boot 32 (cycle 18268) | 3092 | 3 | 14 | 0 |

**Erasing**: stable zeros left, as a share of the old pattern's zeros and of the new pattern's zeros (the old pattern's ones). The two shares track each other when the erase starts from all `0x00` — every bit is a zero to lift, whatever the old data was; an erase from the old data would leave only old zeros.

| cut | old zeros still 0 | old ones now 0 | weak bits | weak bytes |
|---|---:|---:|---:|---:|
| `silicon-esp32c6-2026-10-14-41e96eead-r6.txt` boot 14 | 98.56 % | 97.96 % | 181 | 179 |
| `silicon-esp32c6-2026-10-14-41e96eead-r4.txt` boot 18 | 88.55 % | 86.40 % | 1030 | 907 |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 27 | 78.71 % | 77.21 % | 1486 | 1245 |
| `silicon-esp32c6-2026-10-14-41e96eead.txt` boot 50 | 54.15 % | 51.80 % | 2249 | 1772 |
| `silicon-esp32c6-2026-10-08-f5f629536.txt` boot 36 | 40.95 % | 40.06 % | 2166 | 1691 |
| `silicon-esp32c6-2026-10-14-41e96eead-r5.txt` boot 22 | 35.26 % | 35.27 % | 2343 | 1838 |
| `silicon-esp32c6-2026-10-08-f5f629536.txt` boot 34 | 15.39 % | 14.83 % | 1350 | 1185 |
| `silicon-esp32c6-2026-10-08-c322c8489.txt` boot 15 | 13.12 % | 12.99 % | 1423 | 1215 |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 37 | 10.24 % | 9.37 % | 1117 | 994 |
| `silicon-esp32c6-2026-10-14-41e96eead-r3.txt` boot 13 | 4.93 % | 5.48 % | 761 | 706 |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 30 | 4.21 % | 3.98 % | 603 | 572 |
| `silicon-esp32c6-2026-10-08-e8dfa2bf5.txt` boot 32 | 3.56 % | 3.59 % | 555 | 515 |
| `silicon-esp32c6-2026-10-14-41e96eead-r3.txt` boot 19 | 2.90 % | 2.71 % | 545 | 515 |
| `silicon-esp32c6-2026-10-14-41e96eead.txt` boot 41 | 2.30 % | 2.13 % | 440 | 426 |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 12 | 2.28 % | 2.01 % | 342 | 329 |
| `silicon-esp32c6-2026-10-14-41e96eead-r4.txt` boot 44 | 2.18 % | 1.91 % | 364 | 340 |
| `silicon-esp32c6-2026-10-14-41e96eead-r2.txt` boot 41 | 1.63 % | 1.88 % | 360 | 347 |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 50 | 1.85 % | 1.55 % | 289 | 283 |
| `silicon-esp32c6-2026-10-14-41e96eead-r4.txt` boot 32 | 1.72 % | 1.51 % | 301 | 295 |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 44 | 1.50 % | 1.54 % | 257 | 251 |
| `silicon-esp32c6-2026-10-14-41e96eead-r4.txt` boot 22 | 1.43 % | 1.55 % | 284 | 276 |
| `silicon-esp32c6-2026-10-14-41e96eead-r2.txt` boot 21 | 1.16 % | 0.89 % | 201 | 195 |
| `silicon-esp32c6-2026-10-14-41e96eead-r3.txt` boot 50 | 0.98 % | 1.00 % | 261 | 253 |
| `silicon-esp32c6-2026-10-14-41e96eead-r5.txt` boot 38 | 0.87 % | 1.07 % | 223 | 213 |
| `silicon-esp32c6-2026-10-14-41e96eead-r3.txt` boot 39 | 0.83 % | 1.06 % | 197 | 192 |
| `silicon-esp32c6-2026-10-08-c322c8489.txt` boot 10 | 0.61 % | 0.76 % | 158 | 151 |
| `silicon-esp32c6-2026-10-14-41e96eead-r6.txt` boot 39 | 0.69 % | 0.65 % | 183 | 180 |
| `silicon-esp32c6-2026-10-08-c322c8489.txt` boot 39 | 0.70 % | 0.61 % | 140 | 138 |
| `silicon-esp32c6-2026-10-14-41e96eead-r5.txt` boot 20 | 0.68 % | 0.61 % | 164 | 164 |
| `silicon-esp32c6-2026-10-14-41e96eead-r3.txt` boot 36 | 0.55 % | 0.56 % | 140 | 140 |
| `silicon-esp32c6-2026-10-14-41e96eead-r5.txt` boot 21 | 0.48 % | 0.42 % | 123 | 119 |
| `silicon-esp32c6-2026-10-08-e8dfa2bf5.txt` boot 33 | 0.37 % | 0.44 % | 83 | 83 |
| `silicon-esp32c6-2026-10-08-f5f629536.txt` boot 33 | 0.36 % | 0.41 % | 99 | 98 |
| `silicon-esp32c6-2026-10-08-e8dfa2bf5.txt` boot 45 | 0.38 % | 0.35 % | 73 | 71 |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 48 | 0.36 % | 0.34 % | 83 | 82 |
| `silicon-esp32c6-2026-10-14-41e96eead.txt` boot 43 | 0.39 % | 0.21 % | 76 | 76 |
| `silicon-esp32c6-2026-10-14-41e96eead-r3.txt` boot 44 | 0.24 % | 0.28 % | 65 | 65 |
| `silicon-esp32c6-2026-10-14-41e96eead-r6.txt` boot 3 | 0.23 % | 0.27 % | 60 | 60 |
| `silicon-esp32c6-2026-10-14-41e96eead-r5.txt` boot 17 | 0.24 % | 0.22 % | 65 | 64 |
| `silicon-esp32c6-2026-10-14-41e96eead-r6.txt` boot 12 | 0.20 % | 0.26 % | 71 | 71 |
| `silicon-esp32c6-2026-10-08-c322c8489.txt` boot 29 | 0.16 % | 0.19 % | 57 | 57 |
| `silicon-esp32c6-2026-10-14-41e96eead.txt` boot 16 | 0.18 % | 0.15 % | 59 | 58 |
| `silicon-esp32c6-2026-10-14-41e96eead-r4.txt` boot 37 | 0.10 % | 0.16 % | 43 | 43 |
| `silicon-esp32c6-2026-10-14-41e96eead-r3.txt` boot 21 | 0.13 % | 0.12 % | 49 | 48 |
| `silicon-esp32c6-2026-10-08-f5f629536.txt` boot 22 | 0.12 % | 0.12 % | 32 | 32 |
| `silicon-esp32c6-2026-10-14-41e96eead-r3.txt` boot 9 | 0.10 % | 0.11 % | 39 | 39 |
| `silicon-esp32c6-2026-10-08-f5f629536.txt` boot 7 | 0.12 % | 0.08 % | 48 | 48 |
| `silicon-esp32c6-2026-10-14-41e96eead-r5.txt` boot 24 | 0.08 % | 0.09 % | 27 | 27 |
| `silicon-esp32c6-2026-10-14-41e96eead-r6.txt` boot 37 | 0.07 % | 0.06 % | 20 | 20 |
| `silicon-esp32c6-2026-10-08-f5f629536.txt` boot 50 | 0.05 % | 0.05 % | 14 | 14 |
| `silicon-esp32c6-2026-10-14-41e96eead-r3.txt` boot 14 | 0.04 % | 0.06 % | 34 | 34 |
| `silicon-esp32c6-2026-10-14-41e96eead-r3.txt` boot 10 | 0.02 % | 0.04 % | 15 | 15 |
| `silicon-esp32c6-2026-10-08-c322c8489.txt` boot 17 | 0.01 % | 0.02 % | 8 | 8 |
| `silicon-esp32c6-2026-10-14-41e96eead-r4.txt` boot 47 | 0.02 % | 0.02 % | 13 | 13 |
| `silicon-esp32c6-2026-10-14-41e96eead-r2.txt` boot 40 | 0.01 % | 0.02 % | 4 | 4 |
| `silicon-esp32c6-2026-10-08-c322c8489.txt` boot 36 | 0.01 % | 0.01 % | 2 | 2 |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 34 | 0.00 % | 0.02 % | 0 | 0 |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 21 | 0.01 % | 0.01 % | 3 | 3 |
| `silicon-esp32c6-2026-10-14-41e96eead-r2.txt` boot 30 | 0.01 % | 0.01 % | 2 | 2 |
| `silicon-esp32c6-2026-10-08-e8dfa2bf5.txt` boot 6 | 0.01 % | 0.00 % | 6 | 6 |
| `silicon-esp32c6-2026-10-14-41e96eead-r2.txt` boot 37 | 0.00 % | 0.01 % | 5 | 5 |
| `silicon-esp32c6-2026-10-08-e8dfa2bf5.txt` boot 27 | 0.00 % | 0.01 % | 1 | 1 |
| `silicon-esp32c6-2026-10-08-f5f629536.txt` boot 2 | 0.00 % | 0.01 % | 0 | 0 |
| `silicon-esp32c6-2026-10-14-41e96eead-r4.txt` boot 28 | 0.00 % | 0.01 % | 0 | 0 |
| `silicon-esp32c6-2026-10-14-41e96eead-r5.txt` boot 46 | 0.01 % | 0.00 % | 0 | 0 |
| `silicon-esp32c6-2026-10-08-c322c8489.txt` boot 43 | 0.01 % | 0.00 % | 2 | 2 |
| `silicon-esp32c6-2026-10-14-41e96eead.txt` boot 20 | 0.00 % | 0.01 % | 6 | 6 |
| `silicon-esp32c6-2026-10-14-41e96eead-r6.txt` boot 19 | 0.01 % | 0.00 % | 1 | 1 |
| `silicon-esp32c6-2026-10-14-41e96eead-r6.txt` boot 30 | 0.01 % | 0.00 % | 3 | 3 |

- erase-phase cuts with a stable `0` where the old data had a `1`: **144** of 418

**Timing of one cycle** (the timed cycle after each scan, 510 boots; µs, min / median / max):

- `journal_us`: 78 / 95 / 23114
- `erase_us`: 21092 / 26682 / 32550
- `program_us`: 5529 / 5898 / 6089
- `page_us_min`: 304 / 318 / 335
- `page_us_max`: 322 / 353 / 369
- share of a cycle (medians): journal 0.3 %, erase 81.7 %, program 18.1 %; the cuts landed 83.6 % in the erase and 15.8 % in the program

**`lp-nor-sim`'s assumptions against these 500 cuts** (the models as `lp-emu/lp-nor-sim` has them; the verdict is mechanical, from the counts above):

| assumption | evidence | verdict |
|---|---|---|
| A torn program lands a prefix of what it was asked to write (`BytePrefix`) | 79 torn programs: 77 prefixes of whole bytes, 2 whole words then one word part-landed (a prefix at word level, not at bit level); 0 scattered | **held at word level** |
| `BytePrefix`: the byte after the prefix gets a random subset of its clears | 3 partial bytes in 79 torn programs; 77 of 79 prefixes end on a 4-byte word | **seen** |
| `RandomBits`: a torn program lands a random subset of the page's clears | 0 of 79 torn programs scattered | **not seen (the model is harsher than this part)** |
| One program op is a 256-byte page; a cut between ops leaves whole pages | 58 of 79 prefixes end on a 32-byte command boundary, 9 of them on a page boundary; 21 end inside a command | **contradicted: the unit is the 32-byte ROM command** |
| A torn erase leaves a byte-wise mix of old bytes, `0xFF` and weak bits (shape 0) | 0 of 418 erase cuts left old data beyond a residue | **not seen** |
| A torn erase can read all `0xFF` and carry weak bits (shape 1) | 4 of 268 sectors reading all `0xFF` had weak bits (2, 3, 1, 8); the model sprinkles one weak bit in ~1 of 32 bytes (~128 a sector) | **held, rarely, and far lighter than modelled** |
| A torn erase can be erased up to a point and old after it (shape 2) | 0 with `0xFF` then old; 16 with `0x00` then old (zeroing) | **not seen as `0xFF`-then-old; seen as `0x00`-then-old** |
| A torn erase only lifts bits: it never leaves a `0` where the old data had a `1` | 144 of 418 erase cuts did (80 of them reading `0x00` from the front or throughout) | **contradicted** |
| Weak bits come from torn erases | 71 in-flight sectors had weak bits, 69 of them erase-phase; 0 weak bits in settled sectors | **contradicted** |
| A torn erase that started changes the sector (`Clean` leaves it old) | 1 of 418 erase-phase cuts left the old pattern whole: 1 with both journal copies naming the cycle — a cut between the journal write and the erase command, which the model draws as where the cut lands (`cut_before_erase`), not as an erase shape; 0 with nothing to say where the cut landed | **held: no started erase left the old data** |
| A torn program stops on a whole 4-byte word | 2 of 79 torn programs stopped inside a word (some of its clears landed, weak bits possible); the model draws them as `program_mid_word` | **not always: the part's program unit can be caught part-way** |
| An erase's pre-program stops on a whole 4-byte word, and its lift moves many cells at once | 1 of 418 erase cuts caught the pre-program inside a word (some of its clears landed); 0 caught the sector all `0x00` but a handful of bits; the model draws them as `erase_zeroing_mid_word` and `erase_first_lift` | **not always: the pre-program word, and the lift's first cells, can be caught** |
| A cut damages only the operation in flight | 0 settled sectors damaged, 0 weak bits in them, 0 journal slots damaged outside the operation in flight (journal ops in flight: `journal:wrap-erase` 3) | **held** |

### `lp-emu:esp32c6:t1`

| transcript | date | firmware | cycles | boots | cuts | not cuts |
|---|---|---|---|---:|---:|---|
| `lp-emu-esp32c6-t1-2026-10-08-51e2c5fed.txt` | 2026-10-08 | `51e2c5fed6d0` | 321–3427 | 6 | 5 | 1× fresh region (first boot, nothing in flight) |
| **all** | | | | 6 | **5** | |

Who the records say ran it (from every `ft-boot`):

| MAC | flash JEDEC id (manufacturer, type, capacity) | flash size (capacity byte) | lpfs base | boots |
|---|---|---|---|---:|
| `—` | `—` | — | `0x350000` | 6 |

Reset reasons of the cut boots: `poweron` 5.

Boots that were not power cuts (left out of every count below):

| boot | reset | state | in-flight sector |
|---|---|---|---|
| `lp-emu-esp32c6-t1-2026-10-08-51e2c5fed.txt` boot 0 | `poweron` | fresh | — |

Outside the in-flight sector, over every cut:

- settled sectors not holding their last cycle's pattern: **0** (of 75 read)
- weak bits in settled sectors: **0**
- damaged journal slots outside the operation in flight: **0**; inside it: 0 (of 0 damaged); torn next slots: **0**
- cuts whose operation in flight was the journal's own: none
- work cycles completed between a scan and its cut (min / median / max): 257 / 484 / 1494

**Tear shapes, 5 cuts** (the in-flight sector of each):

| phase | shape | cuts | share |
|---|---|---:|---:|
| erase | cut before the erase started: both journal copies name the cycle and the sector holds the old pattern whole (the cut landed between the journal write and the erase command) | 0 | 0.0 % |
| erase | untouched: the old pattern, whole, and the journal does not say where the cut landed | 0 | 0.0 % |
| erase | torn erase, zeroing: a run of `0x00` from the front, old after it | 0 | 0.0 % |
| erase | torn erase, zeroing caught inside a word: whole `0x00` words from the front, then one word with only part of its pre-program clears landed (maybe weak bits on the others), old after it | 0 | 0.0 % |
| erase | torn erase, all `0x00` | 0 | 0.0 % |
| erase | torn erase, first lift: all `0x00` but a handful of bits (at most 32), each a stable 1 or weak | 0 | 0.0 % |
| erase | torn erase, erasing: stable zeros spread over old **and** new zero positions | 0 | 0.0 % |
| erase | torn erase, old data left (only old zeros, more than a residue) | 0 | 0.0 % |
| erase | torn erase that reads all `0xFF` but has weak bits | 0 | 0.0 % |
| erase | erased: all `0xFF`, no weak bits | 4 | 80.0 % |
| program | torn program, prefix ending on a 32-byte command boundary | 1 | 20.0 % |
| program | torn program, prefix ending inside a 32-byte command | 0 | 0.0 % |
| program | torn program, whole words then one word only part-landed (some of its clears, maybe weak bits), nothing after | 0 | 0.0 % |
| program | torn program, scattered clears (no prefix) | 0 | 0.0 % |
| program | complete: the new pattern, whole | 0 | 0.0 % |
| journal | cut in a journal copy's wrap: its erase before slot 0 (or that slot's first entry); the other copy kept the cycle | 0 | 0.0 % |
| journal | cut in a journal entry's program (between the two copies' writes) | 0 | 0.0 % |
| none | unknown | 0 | 0.0 % |
| **erase** | | **4** | 80.0 % |
| **program** | | **1** | 20.0 % |

The firmware's `verdict` against the phase it was sorted into:

| firmware verdict | phase | cuts |
|---|---|---:|
| `erased` | erased | 4 |
| `torn-program` | program:command-boundary | 1 |

**Weak bits** (a bit that did not read the same all 8 times):

- in-flight sectors with any: **0** of 5

| phase | cuts | with weak bits | weak bits (min / median / max, where any) |
|---|---:|---:|---|
| erased | 4 | 0 | — |
| program:command-boundary | 1 | 0 | — |

- sectors reading all `0xFF` in every stable bit: 4; of those with weak bits: **0** (no weak bits)

**Torn programs**: where the landed prefix ends (byte offset in the sector):

| shape | cuts | on a 4-B word boundary | on a 256-B page boundary | partial bytes (min / median / max) | end mod 32 |
|---|---:|---:|---:|---|---|
| program:command-boundary | 1 | 1 | 0 | 0 / 0 / 0 | 0 |

- erase-phase cuts with a stable `0` where the old data had a `1`: **0** of 4

**Timing of one cycle** (the timed cycle after each scan, 6 boots; µs, min / median / max — emulated time, a model and not a measurement):

- `journal_us`: 17 / 17.5 / 18
- `erase_us`: 4 / 4.5 / 5
- `program_us`: 782 / 782 / 783
- `page_us_min`: 25 / 26 / 26
- `page_us_max`: 26 / 26 / 26
- share of a cycle (medians): journal 2.2 %, erase 0.6 %, program 97.3 %; the cuts landed 80.0 % in the erase and 20.0 % in the program

### `flash-tears-unaligned` on `silicon:esp32c6`

| transcript | date | firmware | cycles | boots | cuts | not cuts |
|---|---|---|---|---:|---:|---|
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` | 2026-10-14 | `609f0e4c9b80` | 57–2179 | 51 | 50 | 1× fresh region (first boot, nothing in flight) |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r2.txt` | 2026-10-14 | `609f0e4c9b80` | 3022–5289 | 51 | 50 | 1× reset `usb-uart`, not a power cut |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r3.txt` | 2026-10-14 | `609f0e4c9b80` | 6457–8385 | 51 | 50 | 1× reset `usb-uart`, not a power cut |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r4.txt` | 2026-10-14 | `609f0e4c9b80` | 9511–11592 | 51 | 50 | 1× reset `usb-uart`, not a power cut |
| **all** | | | | 204 | **200** | |

Who the records say ran it (from every `ft-boot`):

| MAC | flash JEDEC id (manufacturer, type, capacity) | flash size (capacity byte) | lpfs base | boots |
|---|---|---|---|---:|
| `14:c1:9f:e6:54:90` | `0x464016` | 4 MiB | `0x350000` | 204 |

Reset reasons of the cut boots: `poweron` 200.

Boots that were not power cuts (left out of every count below):

| boot | reset | state | in-flight sector |
|---|---|---|---|
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` boot 0 | `usb-uart` | fresh | — |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r2.txt` boot 0 | `usb-uart` | resume | erased |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r3.txt` boot 0 | `usb-uart` | resume | erased |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r4.txt` boot 0 | `usb-uart` | resume | erased |

Outside the in-flight sector, over every cut:

- settled sectors not holding their last cycle's pattern: **0** (of 3000 read)
- weak bits in settled sectors: **0**
- damaged journal slots outside the operation in flight: **0**; inside it: 0 (of 0 damaged); torn next slots: **0**
- cuts whose operation in flight was the journal's own: `journal:program` 1, `journal:wrap-erase` 1
- work cycles completed between a scan and its cut (min / median / max): 8 / 42 / 97

**Tear shapes, 200 cuts** (the in-flight sector of each):

| phase | shape | cuts | share |
|---|---|---:|---:|
| erase | cut before the erase started: both journal copies name the cycle and the sector holds the old pattern whole (the cut landed between the journal write and the erase command) | 0 | 0.0 % |
| erase | untouched: the old pattern, whole, and the journal does not say where the cut landed | 0 | 0.0 % |
| erase | torn erase, zeroing: a run of `0x00` from the front, old after it | 4 | 2.0 % |
| erase | torn erase, zeroing caught inside a word: whole `0x00` words from the front, then one word with only part of its pre-program clears landed (maybe weak bits on the others), old after it | 2 | 1.0 % |
| erase | torn erase, all `0x00` | 24 | 12.0 % |
| erase | torn erase, first lift: all `0x00` but a handful of bits (at most 32), each a stable 1 or weak | 2 | 1.0 % |
| erase | torn erase, erasing: stable zeros spread over old **and** new zero positions | 32 | 16.0 % |
| erase | torn erase, old data left (only old zeros, more than a residue) | 0 | 0.0 % |
| erase | torn erase that reads all `0xFF` but has weak bits | 3 | 1.5 % |
| erase | erased: all `0xFF`, no weak bits | 106 | 53.0 % |
| program | torn program, prefix ending where a write starts (a cut between two ROM calls) | 0 | 0.0 % |
| program | torn program, prefix ending on a ROM command boundary (32 B from the write's address in its first page, 32 B from the page after) | 16 | 8.0 % |
| program | torn program, prefix ending on a 4-byte word inside a command | 9 | 4.5 % |
| program | torn program, prefix ending inside a 4-byte word (or a partial byte) | 0 | 0.0 % |
| program | torn program, scattered clears (no prefix) | 0 | 0.0 % |
| program | complete: the new pattern, whole | 0 | 0.0 % |
| journal | cut in a journal copy's wrap: its erase before slot 0 (or that slot's first entry); the other copy kept the cycle | 1 | 0.5 % |
| journal | cut in a journal entry's program (between the two copies' writes) | 1 | 0.5 % |
| none | unknown | 0 | 0.0 % |
| **erase** | | **173** | 86.5 % |
| **program** | | **25** | 12.5 % |

The firmware's `verdict` against the phase it was sorted into:

| firmware verdict | phase | cuts |
|---|---|---:|
| `erased` | erased | 106 |
| `erased-weak` | erase:reads-ff-weak | 3 |
| `mixed` | erase:all-zero | 24 |
| `mixed` | erase:erasing | 29 |
| `mixed` | erase:first-lift | 2 |
| `mixed` | erase:zeroing | 4 |
| `mixed` | erase:zeroing-mid-word | 2 |
| `old` | journal:program | 1 |
| `old` | journal:wrap-erase | 1 |
| `torn-erase` | erase:erasing | 2 |
| `torn-program` | erase:erasing | 1 |
| `torn-program` | program:command-boundary | 16 |
| `torn-program` | program:mid-command | 9 |

**Weak bits** (a bit that did not read the same all 8 times):

- in-flight sectors with any: **36** of 200

| phase | cuts | with weak bits | weak bits (min / median / max, where any) |
|---|---:|---:|---|
| erase:zeroing | 4 | 0 | — |
| erase:zeroing-mid-word | 2 | 1 | 2 / 2 / 2 |
| erase:all-zero | 24 | 0 | — |
| erase:first-lift | 2 | 1 | 1 / 1 / 1 |
| erase:erasing | 32 | 31 | 2 / 171 / 2289 |
| erase:reads-ff-weak | 3 | 3 | 1 / 1 / 2 |
| erased | 106 | 0 | — |
| program:command-boundary | 16 | 0 | — |
| program:mid-command | 9 | 0 | — |
| journal:wrap-erase | 1 | 0 | — |
| journal:program | 1 | 0 | — |

- sectors reading all `0xFF` in every stable bit: 109; of those with weak bits: **3** (2, 1, 1 weak bits)

**Unaligned torn programs**: where each prefix stopped, against the write it was in. `rom` = 32 B from the write's address inside its first page, then 32 B from the page boundary (the split the emulated mask ROM makes, and `lp-nor-sim`'s per-page op); `absolute` = a 32-byte boundary of the flash; `relative` = 32 B from the write's address all the way.

| cut | write (at, len) | prefix ends at | into the write | first page of it | end mod 32 | from the write mod 32 | rom | absolute | relative | class |
|---|---|---|---:|---|---:|---:|---|---|---|---|
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` boot 2 | (752, 252) | 864 | 112 | no | 0 | 16 | yes | yes | no | program:command-boundary |
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` boot 4 | (1508, 132) | 1632 | 124 | no | 0 | 28 | yes | yes | no | program:command-boundary |
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` boot 7 | (3032, 400) | 3088 | 56 | no | 16 | 24 | no | no | no | program:mid-command |
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` boot 8 | (3348, 252) | 3508 | 160 | yes | 20 | 0 | yes | no | yes | program:command-boundary |
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` boot 15 | (996, 864) | 1496 | 500 | no | 24 | 20 | no | no | no | program:mid-command |
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` boot 23 | (3760, 216) | 3776 | 16 | yes | 0 | 16 | no | yes | no | program:mid-command |
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` boot 39 | (316, 928) | 736 | 420 | no | 0 | 4 | yes | yes | no | program:command-boundary |
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` boot 44 | (20, 664) | 148 | 128 | yes | 20 | 0 | yes | no | yes | program:command-boundary |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r2.txt` boot 10 | (1076, 240) | 1140 | 64 | yes | 20 | 0 | yes | no | yes | program:command-boundary |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r2.txt` boot 13 | (1444, 76) | 1500 | 56 | yes | 28 | 24 | no | no | no | program:mid-command |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r2.txt` boot 16 | (172, 772) | 384 | 212 | no | 0 | 20 | yes | yes | no | program:command-boundary |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r2.txt` boot 26 | (3052, 140) | 3168 | 116 | no | 0 | 20 | yes | yes | no | program:command-boundary |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r2.txt` boot 32 | (3460, 260) | 3704 | 244 | no | 24 | 20 | no | no | no | program:mid-command |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r2.txt` boot 48 | (2760, 180) | 2816 | 56 | yes | 0 | 24 | yes | yes | no | program:command-boundary |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r3.txt` boot 14 | (2596, 728) | 2676 | 80 | yes | 20 | 16 | no | no | no | program:mid-command |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r3.txt` boot 16 | (20, 868) | 452 | 432 | no | 4 | 16 | no | no | no | program:mid-command |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r3.txt` boot 23 | (3032, 116) | 3136 | 104 | no | 0 | 8 | yes | yes | no | program:command-boundary |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r3.txt` boot 26 | (2936, 228) | 3100 | 164 | no | 28 | 4 | no | no | no | program:mid-command |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r3.txt` boot 47 | (3576, 520) | 3712 | 136 | no | 0 | 8 | yes | yes | no | program:command-boundary |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r4.txt` boot 3 | (1752, 732) | 2020 | 268 | no | 4 | 12 | no | no | no | program:mid-command |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r4.txt` boot 4 | (3064, 192) | 3136 | 72 | no | 0 | 8 | yes | yes | no | program:command-boundary |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r4.txt` boot 16 | (1800, 232) | 1864 | 64 | yes | 8 | 0 | yes | no | yes | program:command-boundary |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r4.txt` boot 22 | (644, 968) | 1184 | 540 | no | 0 | 28 | yes | yes | no | program:command-boundary |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r4.txt` boot 28 | (1336, 212) | 1496–1497 | 160 | yes | 24 | 0 | yes | no | yes | program:command-boundary |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r4.txt` boot 38 | (3416, 256) | 3544 | 128 | yes | 24 | 0 | yes | no | yes | program:command-boundary |

- prefixes ending inside a write: **25** (0 more ended where a write starts); on a 4-byte word: **25**
- of those on a word, on a boundary under `rom`: **16**, `absolute`: **11**, `relative`: **6**
- ends that tell the three apart (a boundary under one, not under another): **17**; of them `rom` 16, `absolute` 11, `relative` 6
- ends inside the write's first page: 10 (where `rom` and `relative` say a command starts a word or more past a 32-byte boundary and `absolute` says on one)
- mid-command ends (on a word, on no `rom` boundary): 9; their offset into the command they stopped in, mod 32 from the write in its first page / absolute after: 16, 24, 16, 24, 24, 16, 4, 28, 4

**Zeroing runs**: where the `0x00` run from the front stops (byte offset). `exact` is the firmware's `leading_zero_bytes`; otherwise the end is pinned from per-page counts against the pattern (a range where the pattern has `0xFF` bytes), and checked against the record's `ff_bytes`.

| cut | zero run ends at | how | mod 32 | on a 4-B word boundary | on a 32-B command boundary | on a 256-B page boundary |
|---|---|---|---|---|---|---|
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` boot 43 (cycle 1852) | 1624 | exact | 24 | yes | no | no |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r3.txt` boot 9 (cycle 6783) | 3260 | exact | 28 | yes | no | no |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r4.txt` boot 15 (cycle 10097) | 484 | exact | 4 | yes | no | no |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r4.txt` boot 18 (cycle 10231) | 220 | exact | 28 | yes | no | no |
| **all 4** | | | | **4** | **0** | **0** |

**Zeroing caught inside a word**: the word the run stopped in, and how many of its clears (the old pattern's ones in it) had landed. The record places the landed clears only to their page (`page_landed`); that they sit in the word the run had reached is the reading.

| cut | word at | clears landed | of | weak bits |
|---|---:|---:|---:|---:|
| `silicon-esp32c6-2026-10-14-609f0e4c9-r3.txt` boot 10 (cycle 6837) | 764 | 17 | 21 | 2 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r3.txt` boot 38 (cycle 7915) | 2364 | 2 | 16 | 0 |

**First lift**: the bits that were not a stable `0`.

| cut | stable 1s | weak bits | leading `0x00` bytes |
|---|---:|---:|---:|
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` boot 50 (cycle 2179) | 0 | 1 | 2154 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r4.txt` boot 17 (cycle 10184) | 1 | 0 | 2530 |

**Erasing**: stable zeros left, as a share of the old pattern's zeros and of the new pattern's zeros (the old pattern's ones). The two shares track each other when the erase starts from all `0x00` — every bit is a zero to lift, whatever the old data was; an erase from the old data would leave only old zeros.

| cut | old zeros still 0 | old ones now 0 | weak bits | weak bytes |
|---|---:|---:|---:|---:|
| `silicon-esp32c6-2026-10-14-609f0e4c9-r3.txt` boot 45 | 34.63 % | 33.66 % | 2289 | 1803 |
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` boot 37 | 27.00 % | 26.74 % | 2066 | 1645 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r2.txt` boot 29 | 20.59 % | 19.16 % | 1877 | 1540 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r4.txt` boot 7 | 12.80 % | 11.96 % | 1527 | 1311 |
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` boot 40 | 10.34 % | 10.32 % | 1352 | 1170 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r3.txt` boot 11 | 5.87 % | 6.16 % | 854 | 770 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r4.txt` boot 46 | 3.01 % | 3.32 % | 531 | 504 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r2.txt` boot 50 | 2.88 % | 3.00 % | 509 | 478 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r4.txt` boot 1 | 2.77 % | 3.11 % | 581 | 550 |
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` boot 14 | 2.59 % | 2.61 % | 451 | 430 |
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` boot 31 | 2.12 % | 1.88 % | 424 | 402 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r4.txt` boot 21 | 1.64 % | 1.67 % | 357 | 338 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r2.txt` boot 28 | 1.30 % | 1.19 % | 273 | 262 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r2.txt` boot 2 | 0.94 % | 1.21 % | 234 | 226 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r2.txt` boot 19 | 0.92 % | 0.96 % | 211 | 202 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r3.txt` boot 29 | 0.66 % | 0.64 % | 171 | 168 |
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` boot 5 | 0.57 % | 0.51 % | 140 | 137 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r3.txt` boot 18 | 0.38 % | 0.44 % | 114 | 113 |
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` boot 20 | 0.42 % | 0.39 % | 123 | 120 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r4.txt` boot 10 | 0.30 % | 0.25 % | 77 | 74 |
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` boot 26 | 0.25 % | 0.23 % | 56 | 56 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r4.txt` boot 9 | 0.12 % | 0.12 % | 37 | 37 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r3.txt` boot 44 | 0.06 % | 0.06 % | 14 | 14 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r4.txt` boot 13 | 0.06 % | 0.02 % | 10 | 10 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r2.txt` boot 6 | 0.02 % | 0.03 % | 12 | 12 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r3.txt` boot 4 | 0.01 % | 0.02 % | 5 | 5 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r3.txt` boot 27 | 0.02 % | 0.01 % | 5 | 5 |
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` boot 19 | 0.01 % | 0.01 % | 5 | 5 |
| `silicon-esp32c6-2026-10-14-609f0e4c9.txt` boot 35 | 0.01 % | 0.01 % | 3 | 3 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r2.txt` boot 35 | 0.00 % | 0.01 % | 4 | 4 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r3.txt` boot 43 | 0.01 % | 0.00 % | 2 | 2 |
| `silicon-esp32c6-2026-10-14-609f0e4c9-r3.txt` boot 50 | 0.01 % | 0.00 % | 0 | 0 |

- erase-phase cuts with a stable `0` where the old data had a `1`: **62** of 173

**Timing of one cycle** (the timed cycle after each scan, 204 boots; µs, min / median / max):

- `journal_us`: 79 / 96 / 22022
- `erase_us`: 25805 / 27879 / 33696
- `program_us`: 6157 / 6458 / 6749
- `page_us_min`: 33 / 43 / 48
- `page_us_max`: 409 / 1166.5 / 1474
- share of a cycle (medians): journal 0.3 %, erase 81.0 %, program 18.8 %; the cuts landed 86.5 % in the erase and 12.5 % in the program

### `flash-tears-unaligned` on `lp-emu:esp32c6:t1`

| transcript | date | firmware | cycles | boots | cuts | not cuts |
|---|---|---|---|---:|---:|---|
| `lp-emu-esp32c6-t1-2026-10-08-f8f2be0e6.txt` | 2026-10-08 | `f8f2be0e6` | 246–2895 | 6 | 5 | 1× fresh region (first boot, nothing in flight) |
| **all** | | | | 6 | **5** | |

Who the records say ran it (from every `ft-boot`):

| MAC | flash JEDEC id (manufacturer, type, capacity) | flash size (capacity byte) | lpfs base | boots |
|---|---|---|---|---:|
| `a0:f2:62:87:b4:8c` | `0xef4016` | 4 MiB | `0x350000` | 6 |

Reset reasons of the cut boots: `poweron` 5.

Boots that were not power cuts (left out of every count below):

| boot | reset | state | in-flight sector |
|---|---|---|---|
| `lp-emu-esp32c6-t1-2026-10-08-f8f2be0e6.txt` boot 0 | `poweron` | fresh | — |

Outside the in-flight sector, over every cut:

- settled sectors not holding their last cycle's pattern: **0** (of 75 read)
- weak bits in settled sectors: **0**
- damaged journal slots outside the operation in flight: **0**; inside it: 0 (of 0 damaged); torn next slots: **0**
- cuts whose operation in flight was the journal's own: `journal:program` 1
- work cycles completed between a scan and its cut (min / median / max): 174 / 433.5 / 1494

**Tear shapes, 5 cuts** (the in-flight sector of each):

| phase | shape | cuts | share |
|---|---|---:|---:|
| erase | cut before the erase started: both journal copies name the cycle and the sector holds the old pattern whole (the cut landed between the journal write and the erase command) | 1 | 20.0 % |
| erase | untouched: the old pattern, whole, and the journal does not say where the cut landed | 0 | 0.0 % |
| erase | torn erase, zeroing: a run of `0x00` from the front, old after it | 0 | 0.0 % |
| erase | torn erase, zeroing caught inside a word: whole `0x00` words from the front, then one word with only part of its pre-program clears landed (maybe weak bits on the others), old after it | 0 | 0.0 % |
| erase | torn erase, all `0x00` | 0 | 0.0 % |
| erase | torn erase, first lift: all `0x00` but a handful of bits (at most 32), each a stable 1 or weak | 0 | 0.0 % |
| erase | torn erase, erasing: stable zeros spread over old **and** new zero positions | 0 | 0.0 % |
| erase | torn erase, old data left (only old zeros, more than a residue) | 0 | 0.0 % |
| erase | torn erase that reads all `0xFF` but has weak bits | 0 | 0.0 % |
| erase | erased: all `0xFF`, no weak bits | 2 | 40.0 % |
| program | torn program, prefix ending where a write starts (a cut between two ROM calls) | 1 | 20.0 % |
| program | torn program, prefix ending on a ROM command boundary (32 B from the write's address in its first page, 32 B from the page after) | 0 | 0.0 % |
| program | torn program, prefix ending on a 4-byte word inside a command | 0 | 0.0 % |
| program | torn program, prefix ending inside a 4-byte word (or a partial byte) | 0 | 0.0 % |
| program | torn program, scattered clears (no prefix) | 0 | 0.0 % |
| program | complete: the new pattern, whole | 0 | 0.0 % |
| journal | cut in a journal copy's wrap: its erase before slot 0 (or that slot's first entry); the other copy kept the cycle | 0 | 0.0 % |
| journal | cut in a journal entry's program (between the two copies' writes) | 1 | 20.0 % |
| none | unknown | 0 | 0.0 % |
| **erase** | | **3** | 60.0 % |
| **program** | | **1** | 20.0 % |

The firmware's `verdict` against the phase it was sorted into:

| firmware verdict | phase | cuts |
|---|---|---:|
| `erased` | erased | 2 |
| `old` | cut:before-erase | 1 |
| `old` | journal:program | 1 |
| `torn-program` | program:between-writes | 1 |

**Weak bits** (a bit that did not read the same all 8 times):

- in-flight sectors with any: **0** of 5

| phase | cuts | with weak bits | weak bits (min / median / max, where any) |
|---|---:|---:|---|
| cut:before-erase | 1 | 0 | — |
| erased | 2 | 0 | — |
| program:between-writes | 1 | 0 | — |
| journal:program | 1 | 0 | — |

- sectors reading all `0xFF` in every stable bit: 2; of those with weak bits: **0** (no weak bits)

**Unaligned torn programs**: where each prefix stopped, against the write it was in. `rom` = 32 B from the write's address inside its first page, then 32 B from the page boundary (the split the emulated mask ROM makes, and `lp-nor-sim`'s per-page op); `absolute` = a 32-byte boundary of the flash; `relative` = 32 B from the write's address all the way.

| cut | write (at, len) | prefix ends at | into the write | first page of it | end mod 32 | from the write mod 32 | rom | absolute | relative | class |
|---|---|---|---:|---|---:|---:|---|---|---|---|
| `lp-emu-esp32c6-t1-2026-10-08-f8f2be0e6.txt` boot 1 | — | 1116 | 0 | — | — | — | — | — | — | program:between-writes |

- prefixes ending inside a write: **0** (1 more ended where a write starts); on a 4-byte word: **0**
- of those on a word, on a boundary under `rom`: **0**, `absolute`: **0**, `relative`: **0**
- ends that tell the three apart (a boundary under one, not under another): **0**; of them `rom` 0, `absolute` 0, `relative` 0
- ends inside the write's first page: 0 (where `rom` and `relative` say a command starts a word or more past a 32-byte boundary and `absolute` says on one)

- erase-phase cuts with a stable `0` where the old data had a `1`: **0** of 3

**Timing of one cycle** (the timed cycle after each scan, 6 boots; µs, min / median / max — emulated time, a model and not a measurement):

- `journal_us`: 16 / 16 / 16
- `erase_us`: 4 / 5 / 5
- `program_us`: 774 / 824.5 / 866
- `page_us_min`: 5 / 5 / 5
- `page_us_max`: 40 / 76 / 90
- share of a cycle (medians): journal 1.9 %, erase 0.6 %, program 97.5 %; the cuts landed 60.0 % in the erase and 20.0 % in the program


<!-- flash-tears-analyze:end -->

## 8. The unaligned program — prepared, not yet cut

**The question** (tree-store M2 P10). Every silicon program so far was
page-aligned: a torn prefix ending on a 32-byte boundary could not say
whether the 32-byte commands count from the write's address or from absolute
32-byte boundaries, because on a page-aligned write those are the same
offsets. The store's records start anywhere, and `TearModel::Calibrated`
counts commands from the start of each page operation.

**The payload.** `flash-tears-unaligned` is the `flash-tears` harness built
with `test_flash_tears_unaligned`: same region, journal, patterns, scan,
gates and cut driver; only the program plan differs
(`lp-fw/fw-checks/src/checks/flash_tears/program_plan.rs`). Each cycle
programs its sector as a 20-byte write at offset 0, then writes of 16–1,040
bytes (multiples of 4, drawn from sector and cycle) back to back, so every
write after the first starts at `20 + k·4` — at a word, never on a 32-byte
boundary (a length that would put the next write on one is moved by a
word). That is the store's own case: its `Flash` goes through esp-storage,
which takes only 4-byte-aligned offsets and lengths, so its programs reach
the ROM word-aligned at any word inside a page. Byte-unaligned offsets are
not measured: esp-storage cannot produce them. Each write is one
esp-storage write, so one call into the mask ROM. The in-flight record lists the cycle's plan
(`"writes":[[at,len],…]`); the analysis recomputes it and refuses a
mismatch. Its transcripts go to
`lp-emu/transcripts/esp32c6/flash-tears-unaligned/` and are reported apart:
they never feed `--model-table`.

**What the ROM does with such a write** — measured on the emulator, which
runs the real mask ROM (`lp-emu-esp32c6 --trace SPI1` on the
`flash-tears-unaligned` image at `f8f2be0e6`, read by
`scripts/emu/flash-tears-analyze.py --rom-split`). Over the init pass's 16
sectors (2,273 command starts inside the plan's writes):

| where a page-program command starts | commands |
|---|---:|
| at the write's address | 286 |
| 32 B on from the address, inside the write's first page | 740 |
| on a page boundary (a short command before it ends the first page) | 240 |
| on an absolute 32-byte boundary, after the first page | 1,007 |
| anywhere else | 0 |

For example, a 340-byte write at offset 1,092 is sent as `1092+32` …
`1220+32`, `1252+28`, then `1280+32` … `1376+32`, `1408+24`; a 116-byte
write at 976 as `976+32`, `1008+16`, `1024+32`, `1056+32`, `1088+4`. (The
same split held on the first image of this payload, `104c1a547`, with
16-byte-stepped writes.) So the ROM counts 32-byte commands from
the write's address **inside its first page** and from the page boundary
after it — which is exactly how `lp-nor-sim` tears an unaligned program (it
splits a write at pages, one operation each, and the calibrated model counts
commands from each operation's start). This is the ROM's code, which a C6
runs byte for byte; what the part does with a cut *inside* one of those
commands is what the sitting measures. The analysis judges every unaligned
torn prefix three ways — `rom` (the split above), `absolute` (32-byte
boundaries of the flash) and `relative` (32 bytes from the write's address
all the way) — and the cuts that end inside a write's first page are the
ones that tell `rom` from `absolute`.

**Why three lengths in four are short.** Only a write's first-page
commands are unaligned; after the first page boundary the ROM is
page-aligned again, and a cut there is the old experiment. Lengths uniform
over 16–1,040 put about 27 % of the programmed bytes in unaligned commands;
drawing three in four from 16–272 and one from the whole range puts about
45 % there and keeps the range (about 60 % of the writes still cross a
page; an estimate over 3,000 sector plans). Lengths of at most 272 would put
67 % there, at the cost of the long writes.

**The emulator dry run** (`lp-emu:esp32c6:t1`, `f8f2be0e6`, five cuts):
75 settled sectors read, every one whole, no weak bit — so the ROM put every
unaligned, page-crossing write where it belonged. In flight: two erased, two
old, and one torn program ending exactly where a write starts (offset
1,116). An emulated cut lands between two flash commands by construction,
so this proves the payload and the parser, not the part. (The first dry run,
on `060dd91ea`'s 16-byte-stepped plan, had two cuts on the ROM's command
boundaries, one inside a write's first page; it was removed from this
unmerged branch when the plan changed, because the analysis refuses a
record whose plan is not today's.)

**What the models predict** (`just flash-tears-sim-unaligned`, seed 1, 300
cuts each, simulator numbers):

| model | torn programs | between writes | on a `rom` boundary (of them also `absolute`) | mid-command, on a word | mid-word / partial byte |
|---|---:|---:|---|---:|---:|
| `calibrated` | 51 | 5 | 35 (25) | 11 | 0 |
| `clean` | 51 | 24 | 27 (27) | 0 | 0 |
| `byte_prefix` | 51 | 2 | 0 (0) | 0 | 49 |

**What would change the model.** If the part's torn prefixes land on `rom`
boundaries that are not `absolute` ones (inside a write's first page, a
word or more past a 32-byte boundary), the model is confirmed as it is. If they land on `absolute`
boundaries the ROM never sent a command boundary to, the part aligns inside
a command on its own: an additive program shape in `lp-nor-sim`, and the
t1/f2/f3 sweeps re-run under it.

**How many cuts.** 17 % of CX1's cuts landed in the program. At that share,
100 unaligned cuts give about 17 torn programs, about 12 on command
boundaries, and — at the simulator's 10 in 35 — **about 3 that tell `rom`
from `absolute`**. Two batches meet the brief's ≥ 100 but give a thin
answer; four batches (200 cuts, about 7) are what section 9's command runs.

## 9. The owed sitting, in one command

**Needs a hub that switches VBUS** — the desk's VIA hub pair, which the
first 200 cuts used. Not the hub at Yona's other house.

```bash
scripts/emu/flash-tears-soak.sh --batches 6 \
  && scripts/emu/flash-tears-soak.sh --unaligned --batches 4 \
  && just flash-tears-analyze && just flash-tears-analyze --check-model
```

Each soak run checks the registry (tag `sacrificial`, MAC
`14:C1:9F:E6:54:90`), leases CX1 as `direct: tree-store M4`, refuses a dirty
`lp-fw/`, and per batch builds and flashes the image, cuts 50 times, and
checks every boot's JEDEC id. **The first cut of each batch proves itself:**
`flash-tears-cuts.py` stops (exit 5) if the boot after a power cycle does not
say reset `poweron`, or never comes. An agent running it splits it into one
`--batches 1` run per foreground call and commits each transcript before the
next. The aligned batches add cuts 201–500; the unaligned ones (200; two
batches if time is short, which meets the brief's ≥ 100) go to section 8's
table.

**What `--check-model` decides (DD29, corrected by DD32).** Shares within sampling noise, not
raw counts: 500 cuts have other counts than 200, so a count comparison would
say "differs" whatever the part did. The rule, which `--help` repeats:

1. **Per payload.** Only the page-aligned `flash-tears` silicon cuts feed the
   model's shares. `flash-tears-unaligned` silicon cuts are counted and
   printed apart; they neither confirm nor refute the erase mix (section 8's
   table is where their program shapes are read).
2. **Per family.** Which operation a cut tore is the workload's, so erase
   shapes are compared among the torn erases and program shapes among the
   torn programs, each at its own observed n. A cut after the program
   finished is not a torn operation and is left out.
3. **Confirmed** when every shape's observed share lies inside its Wilson
   score interval around the *model's* share at that n. **Bonferroni per
   family (DD32):** a family tested in k cells (counted after pooling) puts
   each cell's interval at the 1 − 0.05/k level, so an honest sample misses
   somewhere in that family about one time in twenty. On the first 200 cuts
   the erase family is 4 cells, each at 98.75 %, and the program family 2,
   each at 97.5 %.
4. **Pooling.** A shape expected fewer than 5 times at that n (model share
   times n) is pooled with the other such shapes; a pool, or a lone thin
   shape, still under 5 joins the thinnest remaining cell. On the first 200
   cuts the one weak-bit cut (expected 1.0) pools with zeroing.
5. **A class the model has no shape for** (untouched, old data left,
   scattered, ...) is never pooled away: seeing one fails the check. It is
   not a test at a level, so it does not count towards k.
6. **Otherwise** it prints the re-fit table (`--model-table`) and exits 1:
   re-fit `TearMix::CX1`, re-run the t1/f2/f3 sweeps under it, and say so.
   Exit 0 prints `CONFIRMED` and is "confirmed at 500".

On today's 200 cuts it prints `CONFIRMED` (the model *is* these cuts, so no
cell can fail; that pins the arithmetic, not the part). `--self-test` also
checks that a doubled shape fails and that pooling works. Only shares are
judged: the `CX1_ERASING` and `CX1_READS_FF_WEAK` residue tables are drawn
values, so a re-fit refreshes them from `--model-table` but the check never
fails on them.

**Why the correction (DD32, before any data).** DD29 judged each of the six
cells at a plain 95 %, so roughly one honest sample in four missed somewhere
and "MODEL DIFFERS" would have said more about the test than the part.
`--self-test` measures it: 2,000 honest samples of 500 cuts drawn from the
200-cut model (415 torn erases, 82 torn programs, fixed seed) miss somewhere
more than 15 % of the time uncorrected, and each family misses about 6 % (erase)
and 3 % (program) of the time corrected. Wilson around a small discrete count
is approximate, which is why the erase family sits a little over 5 %; the
self-test's bar is 7.5 % per family. It also checks that a share outside 95 %
but inside 99 % confirms a five-cell family and fails without the correction,
and that a real difference (the same shape further out, or a doubled shape)
still fails. The rule was changed before the sitting's first cut, not to fit
a result. `just lint-flash-tears-analyze` runs the self-test in `check-lint`.

**What happened on 2026-10-08 night** (the sitting that did not run). CX1
was leased (`direct: tree-store M4`) on `/dev/cu.usbmodem1301`, hub `0-1`
port 3. `board verify` said `esp32c6 rev v0.2, 4MB, 14:C1:9F:E6:54:90`. A
first capture, before any power cycle, got **0 bytes in 30 s** (whatever
image CX1 held, it did not answer the payload's handshake — after `board
verify`'s espflash reset it may have been in the ROM's download mode). Then
one `board power-cycle c6-expendable` printed `CX1 c6-expendable: power
cycle on 0-1 port 3` and exited 0; the capture after it also got **0 bytes
in 30 s**, so **no boot after it reported a reset reason** — the power cut
was not proved, and Yona then said the hub cannot switch power. Nothing was
flashed; no transcript was recorded; the lease was dropped. The cut driver's
new exit-5 check is that night's lesson: `board power-cycle` exiting 0 is not
evidence that the power went.

## 10. The sitting of 2026-10-14: cuts 201–500 (aligned); unaligned not finished

CX1 (`c6-expendable`, `14:C1:9F:E6:54:90`) was on the desk's VIA hub
`1-1.2` port 2, twin `1-2.2` port 2. It was leased as `direct: tree-store M4`
and resolved by MAC, with no probing. Power was cut only through
`board power-cycle`. The firmware was `41e96eead`; `lp-fw/` is unchanged from
`35e8825a3`. `--check-model` was Bonferroni-corrected (DD32) before the first
cut.

**Aligned: 300 cuts in six batches of 50, all real.** All 300 boots after a
power cycle reported reset `poweron`. The only other reset was each batch's
post-flash boot (`usb-uart`), which is left out as before. Every boot read
JEDEC `0x464016`. Section 7's tables now cover the 500.

**`--check-model` at 500: MODEL DIFFERS, but only through rule 5.** Every
modelled shape's share lies inside its corrected interval: the erase family at
98.75 % (4 cells, n = 418) and the program family at 97.5 % (2 cells, n = 79).
Two cuts are of shapes `TearMix::CX1` has no weight for. Seeing either one
fails the check. The re-fit table is what `--model-table` prints. **This
report does not re-fit.** A shape the model cannot draw is a model addition,
a director's call. Nothing was edited to make the check pass.

| shape | cuts | 98.75 % / 97.5 % interval | verdict |
|---|---:|---|---|
| `erased` | 264 (63.2 %, model 60.8 %) | 54.8 % – 66.6 % | ok |
| `erase:erasing` | 69 (16.5 %, model 16.9 %) | 12.8 % – 21.9 % | ok |
| `erase:all-zero` | 63 (15.1 %, model 15.7 %) | 11.7 % – 20.6 % | ok |
| `erase:zeroing` + `erase:reads-ff-weak` | 21 (5.0 %, model 6.6 %) | 4.2 % – 10.3 % | ok |
| `untouched` | 1 | none: not modelled | **NOT MODELLED** |
| `program:command-boundary` | 58 (73.4 %, model 78.8 %) | 66.9 % – 87.2 % | ok |
| `program:mid-command` | 20 (25.3 %, model 21.2 %) | 12.8 % – 33.1 % | ok |
| `program:scattered` | 1 | none: not modelled | **NOT MODELLED** |

**Three findings to triage** (findings, not reasons to edit a golden):

1. **A program caught inside a word.** This is `program:scattered`, in
   `silicon-esp32c6-2026-10-14-41e96eead.txt` boot 34, sector 0, cycle 1328.
   It is almost a prefix. The first 3,872 bytes landed whole; 3,872 is page
   15 plus one 32-byte command. Then two bytes of the next 4-byte word landed
   only some of their bits, and one bit there is weak. Nothing landed past
   byte 3,876. The analyzer calls it "scattered" because it ends in more than
   one partial byte. Every one of the first 200 torn programs ended on a whole
   word. This is the first cut that caught the part's program unit
   part-way, at bit level. `TearModel::Calibrated` cannot produce it: its
   torn programs end on whole words.
2. **An erase-phase cut that left the old data whole.** This is `untouched`,
   in `-r4.txt` boot 24, sector 12, cycle 10716. The journal names the
   cycle, so the cut came after the journal write. The sector reads its old
   pattern whole. The likeliest reading is a cut between the journal write
   and the erase command, which is a timing window, not an erase shape. The
   emulator's unaligned dry run shows the same class (2 of 5). The model has
   no weight for it, because it draws a shape only for a torn erase. Before
   adding a shape, check whether the erase had been issued: the payload does
   not record that.
3. **A torn journal erase, which the analyzer counts as collateral.** This is
   in `-r2.txt` boot 39. Journal copy 0 reads 0 valid and 256 damaged slots.
   Copy 1 holds cycle 5375, and the in-flight data sector reads complete.
   Copy 0's slot 0 belongs to cycle 5376. By the journal's design, copy 0 is
   erased just before that slot is written, so the cut tore the journal's own
   erase. That *was* the operation in flight. Copy 1 kept the number, as the
   two-copy design intends. Section 7's "a cut damages only the operation in
   flight: contradicted" comes from these 256 slots. It is an analyzer gap:
   the analyzer does not recognize a journal-wrap erase. It is not damage
   outside the operation. No settled sector was damaged and none had a weak
   bit (0 of 7,500 read).

**Unaligned: not finished. No transcript.** The `--unaligned` soak built and
flashed the `flash-tears-unaligned` image. Its first batch took 24 cuts, and
all 24 boots after them reported `poweron`. On cut 25, `board power-cycle`
never returned. Its child `uhubctl -l 1-1.2 -p 2 -a on -e` sat for 47 minutes
in `libusb_exit`, where `darwin_exit` waits on the hotplug thread and that
thread waits on a mutex in `darwin_devices_detached`. CX1 had re-enumerated,
so the "on" had been sent. `board` has no timeout around it. The sitting was
stopped instead of worked around. The cut driver was stopped first, then the
hung processes. `lp-cli validate record` writes a transcript only for a
finished batch, so none was written. The partial capture (25 boots) was kept
outside the repo, and it is not a transcript. Section 8's 200 unaligned cuts
are still owed. (They were taken later the same day: section 12.)

## 11. The three findings, triaged; the model re-fitted on 500 (2026-10-14)

No board was touched for this section. It reads the committed transcripts,
`lp-nor-sim`, and the emulator's dry run. Section 7 is regenerated with the
analyzer's new classes, so its tables no longer show `untouched`,
`program:scattered` or `complete` for these cuts.

**1. The sub-word tear is a real disagreement, and there were two.** Boot
34 (sector 0) is as section 10 says: 3,872 whole bytes, then the next word
with 3 of its 17 clears landed in two bytes and one weak bit. Looking inside
every torn program's last word found a second one. `41e96eead.txt` boot 24
(sector 5, cycle 949) ended on a word at byte 3,260. That word's last byte
had only 14 of the word's 15 clears landed, plus one weak bit. The firmware
called it `byte-prefix` and the analyzer had filed it `mid-command`,
because it judged where the landed bytes ended, not whether the last word
was whole. Both are now `program:mid-word`, 2 of 79 torn programs. The
model draws them as `ProgramShape::MidWord`: whole words, then one word
with at least one of its clears landed and at least one not, weak bits on
clears that did not land, then nothing. `CX1_MID_WORD` = (3 of 17, 1 weak),
(14 of 15, 1 weak), and the landed share and weak count interpolate between
them. The fidelity defect is
`docs/defects/2026-10-14-the-calibrated-nor-model-never-tears-a-program-inside-a-word.md`
(fixed in this change).

In the second word, the weak bit is most likely the one clear that did not
land. If so, the record holding that word **reads exactly as written** on
any read where the bit reads 0. The tree store's sweeps below show what
that costs.

**2. The `untouched` erase cut is a cut before the erase started.** The
payload does not record whether the erase command had gone out, so this
is decided from the journal, the timing and the emulator.

- *The journal.* Both copies hold cycle 10716 with no damaged slot (copy
  0: 221 valid; copy 1: 93 valid). So both journal programs had finished
  before the cut. A cut in copy 1's write would have left copy 1 a cycle
  behind, or with a torn slot.
- *The timing.* The cut driver waits a uniform 50–2,000 ms after the scan,
  about 60 cycles of 33 ms, so where a cut lands in a cycle is uniform in
  time. 17 of 500 cuts caught a `0x00` run still growing (`erase:zeroing`),
  3.4 % of a 33 ms cycle. That is 1.1 ms for 1,024 words, so the part
  pre-programs a 4-byte word about every µs. An erase that had been issued
  would move its first cell within a few µs, which predicts about 0.05 cuts
  in 500 that find it issued with nothing moved. The window between copy 1's program finishing and the
  erase's first cell moving is mostly software: the program call's return,
  the erase call, `Wait_SPI_Idle`, write enable, then the command. That is
  tens of µs. The timing records show the same order between calls: the 16
  page programs take 5,898 µs (median), 30–50 µs a page more than the page
  calls themselves (318–353 µs median, fastest and slowest). A 10–50 µs window
  predicts 0.15–0.75 cuts in 500. One was seen.
- *The emulator.* Its flash completes every operation inside the register
  write (`lp-emu-esp32c6/src/periph/spi1.rs`), so an emulated cut can land
  only *between* operations. Its 2 of 5 unaligned dry-run cuts that read old
  split once the journal is read. Boot 2: both copies name cycle 422, the
  same gap before the erase. Boot 5: copy 1 still holds 2894 against copy
  0's 2895, the gap between the two journal writes.

So it is modelled as where the cut landed, not as an erase shape.
`TearMix::cut_before_erase` (1 of 418 cuts at an erase) is drawn in
`NorFlashSim::erase_sector` before any shape. Power is gone, the sector is
exactly as it was and not tainted, and the cut is counted in
`NorStats::erase_cuts_before_start`, not in `torn_erases`. A program needs
no such weight: a prefix ending at offset 0 is the same gap, and it is
already inside `program_command_boundary`. The analyzer classes the cut
`cut:before-erase` only when both journal copies name the cycle. With copy
1 behind, it is `journal:program`.

**3. The journal-wrap erase was the operation in flight, three times.** The
analyzer now reads the two copies against the journal's geometry (`slot_of`,
copy erased before its slot 0). A cut in a copy's wrap, or in an entry's
program, is `journal:wrap-erase` or `journal:program`, and the slots it
damaged count inside the operation in flight. **All three silicon
`complete` cuts were copy 0's wrap** (cycles 5375, 7167 and 22783, each
`≡ 255 mod 256`). Copy 0 read 256 damaged slots once and blank twice, and
copy 1 kept the cycle each time. None was a cut after a finished program.
Recomputed, "a cut damages only the operation in flight" **holds**: 0
settled sectors damaged, 0 weak bits in them, 0 journal slots damaged
outside the operation in flight. Their shape is read only as slot counts,
so `--check-model` leaves them out of both families. Self-tests for each
reading are in `just lint-flash-tears-analyze`.

**The re-fit.** A shape the model could not draw needed a weight, and the
weights are counts from the fitting set, so the whole mix is now fitted on
the 500 aligned cuts (`--model-table`, unchanged estimator):

| family | shape | 200-cut weight | 500-cut weight |
|---|---|---:|---:|
| cut at an erase | zeroing | 10 | 17 |
| | all zero | 26 | 63 |
| | erasing | 28 | 69 |
| | reads `0xFF`, weak | 1 | 4 |
| | reads `0xFF` | 101 | 264 |
| | **before the erase started** | — | **1** |
| torn program | command boundary | 26 | 58 |
| | mid-command (on a word) | 7 | 19 |
| | **mid-word** | — | **2** |

`CX1_ERASING` grows from 28 rows to 69 (the largest residue is now 32,197
zeros with 181 weak bits). `CX1_READS_FF_WEAK` is `[1, 2, 3, 8]`.
`calibrated_before_erase` and `calibrated_mid_word` are new forced models.
Each makes every cut of its family meet the new state.

**`--check-model`: CONFIRMED.** Erase family: 5 cells at 99 %, with
`erase:reads-ff-weak` + `cut:before-erase` pooled. Program family: 2 cells
at 97.5 %, with `mid-command` + `mid-word` pooled. This is **in-sample**:
the model is now fitted on the cuts it is checked against, so the shares
match by construction. The check at 500 against the 200-cut fit was the
out-of-sample test, and it passed on every share that fit had. The next
aligned sitting is the next out-of-sample test. The 200 unaligned cuts
still owed do not feed it (rule 1).

**The tree store under the re-fitted model.** Simulator numbers
(`lp-nor-sim`), release builds, every cut point. The tree store is main's
(`6fbafce6e`, merged into this branch). fs-tree's emulated cut walk is not
on this branch's base, so it was not run.

*In-crate sweeps* (`LP_TREE_STORE_SWEEP_CUTS=1000000
LP_TREE_STORE_SWEEP_STEPS=12 LP_TREE_STORE_SWEEP_TEARS=<model> cargo test
--release -p lp-tree-store -- cut_sweep a_torn_kill_erase_or_header_program_never_reads_as_newer`,
at the sweeps' own geometry and with `LP_TREE_STORE_SWEEP_SECTORS` 128 and
176). Each test stops at its first failure, so the result is pass or fail:

| model | per-call | transaction | append 4 KiB | deflated push | one-slot deflated push | through GC | torn header |
|---|---|---|---|---|---|---|---|
| `calibrated` (re-fitted) | pass | pass | pass | pass | pass | pass | pass |
| `calibrated_before_erase` | pass | pass | pass | pass | pass | pass | pass |
| `calibrated_mid_word` | **fail** | pass | **fail** | **fail** | pass | **fail** | pass |

The four failures are the same at all three geometries: `step 3 cut 2/7:
remount after re-run` (per-call), `get: Corrupt("record CRC")` (append),
`step 0 cut 15/24: remount after re-run` (deflated push), `step 10 cut
114/124: remount after re-run` (GC).

*`lp-store-bench`*: `sweep` (every cut point, seeds 1 and 2) and `double`
(seed 1), each candidate × workload × model. The 128- and 176-sector runs
gave identical counts, so one table holds both. Smoke under each model at
both sizes: 0 failures (25 random-walk cuts a codec).

| driver | candidate | workload | cases (each model) | torn erases | `calibrated` | `calibrated_before_erase` | `calibrated_mid_word` |
|---|---|---|---:|---:|---:|---:|---|
| double-cut | t1 | save:c40 | 1,280 | 16 | 0 | 0 | **10** (remount_failed 2, unmountable 8) |
| double-cut | t1 | panel:c40 | 6,256 | 88 | 0 | 0 | **7** (remount_wrong_state 3, unmountable 4) |
| double-cut | t1 | push:c40 | 128 | 0 | 0 | 0 | 0 |
| double-cut | t1@codec=host_deflate | save:c40 | 1,280 | 0 | 0 | 0 | **21** (remount_failed 12, remount_wrong_state 1, unmountable 8) |
| double-cut | t1@codec=host_deflate | panel:c40 | 6,256 | 88 | 0 | 0 | **7** (remount_wrong_state 3, unmountable 4) |
| double-cut | t1@codec=host_deflate | push:c40 | 128 | 0 | 0 | 0 | **1** (remount_failed 1) |
| exhaustive | t1 | save:c40 | 1,364 | 30 | 0 | 0 | **2** (read_error 1, unmountable 1) |
| exhaustive | t1 | panel:c40 | 1,830 | 22 | 0 | 0 | 0 |
| exhaustive | t1 | push:c40 | 3,288 | 106 | 0 | 0 | 0 |
| exhaustive | t1 | repush:c40 | 266 | 4 | 0 | 0 | 0 |
| exhaustive | t1 | switch:c13,c40reuse | 1,506 | 54 | 0 | 0 | 0 |
| exhaustive | t1@codec=host_deflate | save:c40 | 1,344 | 26 | 0 | 0 | **1** (unmountable 1) |
| exhaustive | t1@codec=host_deflate | panel:c40 | 1,830 | 22 | 0 | 0 | 0 |
| exhaustive | t1@codec=host_deflate | push:c40 | 1,890 | 42 | 0 | 0 | 0 |
| exhaustive | t1@codec=host_deflate | repush:c40 | 362 | 6 | 0 | 0 | 0 |
| exhaustive | t1@codec=host_deflate | switch:c13,c40reuse | 690 | 12 | 0 | 0 | 0 |

Each model ran 30,296 cases at each geometry. `calibrated` had 0 failures
and `calibrated_before_erase` had 0. **`calibrated_mid_word` had 49**,
the same 49 at 128 and at 176 sectors.

**A finding: the tree store fails under `calibrated_mid_word`.** The mix
(`calibrated`) and `calibrated_before_erase` are clean everywhere. Under
`calibrated_mid_word`, a record torn so that its only missing clear is weak
reads whole on one mount. The scan accepts it, the head resumes after it,
and the next commit lands behind it. A later mount reads the weak bit the
other way, closes the sector at the torn record, and loses what came
after: a committed write rolls back, or no complete root is left
(`unmountable`). Filed open, not fixed:
`docs/defects/2026-10-14-tree-store-trusts-a-torn-record-that-reads-whole-once.md`.
The cut before an erase changes nothing for the store, which never trusts a
sector it did not erase and read back this session.

## 12. The sitting of 2026-10-14, later: 200 unaligned cuts

CX1 (`c6-expendable`, `14:C1:9F:E6:54:90`) was on hub `1-1.2` port 2, twin
`1-2.2` port 2, leased as `direct: tree-store M4` and resolved by MAC, with
no probing. Power was cut only through `board power-cycle`. The image was
`flash-tears-unaligned` built from `609f0e4c9` (main after #1067). This is
the first sitting since `board` got its 20 s guard on a hung `uhubctl`, and
the guard never fired. The command was section 9's
`scripts/emu/flash-tears-soak.sh --unaligned --batches 4`.

**200 cuts in four batches of 50, all real.** All 200 boots after a power
cycle reported reset `poweron`. The only other boots were each batch's
post-flash boot (`usb-uart`), left out as before. Every boot read JEDEC
`0x464016`. No settled sector was damaged and none had a weak bit (0 of
3,000 read). Section 7 now holds these four transcripts under
`flash-tears-unaligned` on `silicon:esp32c6`.

**The model was not re-fitted.** These cuts are out of sample:
`TearMix::CX1` is fitted on the 500 aligned cuts only, and `--check-model`
does not judge unaligned cuts (rule 1). Its verdict is unchanged,
**CONFIRMED**, and it counts the 200 apart. The tables below run the same
judge (`judge_family`, same pooling, same Bonferroni) on the unaligned cuts
by themselves and on all 700 together. They are a reading, not a gate.

| cuts judged | family | n | cells | verdict |
|---|---|---:|---|---|
| unaligned 200 | erase | 172 | `erased` 61.6 % (model 63.2 %, 53.7–71.7 %), `erasing` 19.8 % (16.5 %, 10.6–24.7 %), `all-zero` 14.0 % (15.1 %, 9.5–23.1 %), `zeroing` + `reads-ff-weak` + `before-erase` 4.7 % (5.3 %, 2.4–11.3 %) | every share inside |
| unaligned 200 | program | 25 | `command-boundary` 64.0 % (73.4 %, 51.0–88.0 %), `mid-command` + `mid-word` 36.0 % (26.6 %, 12.0–49.0 %) | every share inside |
| all 700 | erase | 590 | 5 cells, each inside | every share inside |
| all 700 | program | 104 | `command-boundary` 71.2 % (73.4 %, 62.8–81.9 %), the rest 28.8 % (26.6 %, 18.1–37.2 %) | every share inside |

These are the analyzer's own classes. Two of the cuts are filed under
classes that do not describe them (below). Read correctly, both are shapes
the model cannot draw, and rule 5 would fail on them.

**Where an unaligned torn program stops: on the mask ROM's commands.** This
is the question the payload was built to answer (section 8). All 25 torn
programs stopped on a whole 4-byte word inside a write. None stopped where a
write starts. Of the 17 ends that tell the three readings apart, `rom` fits
16, `absolute` fits 11 and `relative` fits 6. Inside a write's first page,
6 ends fit only `relative` and 1 fits only `absolute`: the commands start at
the write's address there, as the emulated ROM splits them. The one
`absolute`-only end (`609f0e4c9.txt` boot 23, 16 bytes into its write) sits
16 bytes into a `rom` command, an ordinary mid-command end. Only `rom` keeps
the shares inside the model's intervals. Under `absolute`, 11 of 25 would be
command boundaries (44 %, below 51.0 %). Under `relative`, 6 would be (24 %).
`lp-nor-sim` already tears each page op from its own start, which is the
`rom` reading, so nothing there needs to change. None of the 25 stopped
inside a word (the model expects 0.6 at this n).

**Two erase states the model cannot draw.** The erase is the same in both
payloads, so these 172 are also the model's first out-of-sample erase cuts.
In none of the 418 aligned erase cuts did either state appear.

1. **A pre-program caught inside a word.** `-r3.txt` boot 10 (sector 5,
   cycle 6837) is a `0x00` run from the front: bytes 0–764 whole (exact,
   `leading_zero_bytes` 765). Then word 764–767 has 17 of its 21 clears
   landed and 2 weak bits on page 2. The old data is whole after that
   (the 10 `0x00` and 12 `0xFF` bytes after it are the old pattern's own).
   This is the erase-side twin of `program:mid-word`. `EraseShape::Zeroing`
   always stops on a whole word with no weak bits. The analyzer files it
   `erase:erasing`, because a `0x00` run with a weak bit falls through to
   that class.
2. **An erase caught as its first cell lifts.** Two sectors read all `0x00`
   but one bit. In `609f0e4c9.txt` boot 50 (sector 3, cycle 2179) that bit
   is weak (byte 2154). The analyzer files it `erase:erasing`. In `-r4.txt`
   boot 17 (sector 8, cycle 10184) the bit reads a stable 1 (byte 2530) and
   there are no weak bits. The analyzer files it
   `unknown`: the bit sits where the old data had a 1, so no old zero was
   lost, and the zero-run path does not fit. Both lie between `AllZero` and
   the least-lifted `CX1_ERASING` row (32,197 zeros, 181 weak). The model
   draws `Erasing` only inside that table, so it can draw neither. The
   likeliest reading is the first moments of the erase pulse.

All three would fail rule 5 if they were in the fitting set. Three of 172
here against none of 418 aligned is a little surprising (about 1 in 40 if
both sets come from the same part behaving the same way). The erase itself
is the same command. Filed open, not fixed, and the model not re-fitted:
`docs/defects/2026-10-14-the-calibrated-nor-model-never-tears-an-erase-inside-a-word-or-at-its-first-lift.md`.

## 13. Does programming a torn word again settle it? (2026-10-14, 600 cuts)

**Yes, on CX1, every time it was tried.** Every weak bit that was
programmed again with what it was meant to hold read solid and correct
straight away. It still did after the next power cut. This held for 5
torn program words (9 weak bits) and for 49 torn-erase sectors (26,198
weak bits in 64 reads), all programmed to `0x00`. The bits that were
left alone kept flickering. So the assumption behind PR #1136's settle
step holds on this part. That assumption is `lp-nor-sim`'s model:
a weak bit programmed to 0 becomes a solid 0, `nor_flash_sim.rs`
`program`. No defect is filed against the tree store's settle step or
against `lp-nor-sim`.

**The payload.** `flash-reprogram`
(`lp-fw/fw-checks/src/checks/flash_reprogram/`, harness
`lp-fw/fw-esp32c6/src/tests/flash_reprogram.rs`) owns all of CX1's
`lpfs`. Each boot works in five steps:

1. It re-reads the words or sectors the previous boot re-programmed. The
   cut that started this boot is their "further power cycle".
2. It finds what this cut tore.
3. It reads each torn word 4,096 times left alone: 8 chunks of 512,
   20 ms apart. This is the control.
4. It programs the word's write again with the intended bytes, from the
   write's start through its last dirty word, in one program call, as
   the settle step does.
5. It reads the word 4,096 times again.

A bit is **weak** if it reads both ways, whether in the 64 reads around
the frontier or in the control's 4,096. It is a sibling of `flash-tears`
with its own name, so `flash-tears-analyze.py` never reads its
transcripts. Read them with `scripts/emu/flash-reprogram-analyze.py`.

**How the cuts were biased toward programs.** In `flash-tears`, 83 % of
cuts landed in an erase, because erasing is 81 % of a work cycle. Here
every erase runs *before* the scan-done line, so the work loop does
nothing but program: 2,784 writes of 256 bytes, filling the region in
about 0.91 s. The cut window was then made short enough that every cut
lands inside that fill. `board power-cycle` cuts 455–736 ms after the cut
driver's own wait (measured on the first 50 cuts). With the wait at
0–120 ms, the cut lands 334–911 ms in (median 536 ms). **All 425
program-loop cuts landed in a program**, and none found the region
already full.

That is still not many torn words. The part stops on a whole word almost
every time. Of the 425 program-loop cuts, 103 stopped on a 256-byte
write boundary, 225 on a 32-byte command boundary and 91 inside a command
on a word. Only **6 stopped inside a word (1.4 %)**. That matches
`flash-tears` (2 of 79). After 250 cuts it was clear that 600 cuts would
give a handful of torn words. So the image built from `89674b8c5`
alternates. Odd generations still program. Even generations **erase**
the 64 sectors their boot programmed: 64 × 24 ms, the cut 15–40 sectors
in. A torn erase leaves cells part-lifted, which is the same question for
far more cells. Such a sector is read 64 times, 512 times left alone
(8 chunks of 64, 20 ms apart), programmed to `0x00`, and read 512 times
again. `0x00` is what the erase's own pre-program writes, the brief's
"torn erase pre-program". That gave 175 erase-loop cuts and 175 more
program-loop cuts.

**The sitting.** CX1 (`c6-expendable`, `14:C1:9F:E6:54:90`) was on hub
`1-1.2` port 2, leased as `direct: tree-store M7`, resolved by MAC, with
no probing. Power was cut only through `board power-cycle`. There were
**600 cuts in 12 batches of 50**, each batch its own transcript in
`lp-emu/transcripts/esp32c6/flash-reprogram/`. **All 600 boots after a
power cycle reported reset `poweron`**. The 12 post-flash boots
(`usb-uart`) are left out. Every boot read JEDEC `0x464016`. Batch 1 was
cut with a 0–300 ms wait, and the rest with 0–120 ms. Batches 1–5 ran the
program-only image (`lp-fw/` at `812d12f4c`). Batches 6–12 ran the
alternating image (`89674b8c5`). No cut ever touched anything outside the
operation in flight. Every re-programmed write's tail still read erased.
Every sector the boots erased read back blank.

**Torn program words** (the direct question):

| transcript, boot | byte in write | clears landed | weak bits; how often each read 0 left alone (4,096 reads) | after re-program (4,096 reads) | after the next cut (4,096 reads) |
|---|---:|---:|---|---|---|
| `812d12f4c` 12 | 224 | 15 of 17 | bit 19: 1 | stable, correct | stable, correct |
| `843127cd4` 37 | 40 | 10 of 13 | bit 23: 38 | stable, correct | stable, correct |
| `843127cd4` 38 | 44 | 13 of 15 | bit 6: 3,926; bit 22: 512 | stable, correct | stable, correct |
| `e2a83a6bb` 27 | 224 | 0 of 14 | bit 24: 3,946 | stable, correct | stable, correct |
| `e382cd382` 4 | 148 | 1 of 19 | bit 10: 55; bit 23: 658; bit 24: 9; bit 29: 3,069 | stable, correct | stable, correct |
| `e382cd382` 20 | 4 | 1 of 13 | none (12 clears stably not landed) | stable, correct | stable, correct |

The other bits of each torn word read stably, and the clears that never
landed read a stable 1. The weak bits do not settle on their own. Every
weak word was still weak after 4,096 reads, at about the same rate in
each 512-read chunk (bit 6: 94.5–97.1 % zeros a chunk; bit 23: 14.1–18.6
%). Some are rare: bit 19 read 0 once in 4,096, and 64 reads missed
three of the five weak words entirely. Each was re-programmed with the
other bits of its write. Then none read any other way in 4,096 reads,
and none did after the next power cut.

**Torn-erase sectors** (175 erase-loop cuts):

| shape | sectors | weak bits (64 reads) |
|---|---:|---:|
| erasing (zeros spread over the sector, part-lifted) | 35 | 26,152 |
| reads erased, with weak bits | 8 | 35 |
| zeroing (a `0x00` run from the front, old data after) | 9 | 4 |
| all zero, with weak bits | 4 | 7 |
| all zero | 26 | 0 |

In the other 93 cuts, the sector in flight still read its old pattern and
the one before it read erased whole, with no weak bit.

| sectors with weak bits | weak, 64 reads | weak, 512 reads left alone | 64-read weak bits that held still for all 512 | zeroed and stable after re-program (512 reads) | still weak, or a stable 1, after it | zeroed and stable after the next cut |
|---:|---:|---:|---:|---:|---:|---:|
| 49 | 26,198 | 34,823 | 758 (2.9 %) | 49 of 49 | 0 bits | 48 of 48 |

Left alone, the weak cells stay weak. Reading more finds more of them:
34,823 in 512 reads against 26,198 in 64. Only 2.9 % of the bits that
flickered in the first 64 reads held still for the next 512. After one
program to `0x00`, **not one of the 34,823 bits read 1 even once** in 512
reads, and none did after the next cut. One sector, the last cut of batch
6, has no re-check: the next boot was a reflash. The two zeroing sectors
with weak bits (`9409cd6d7` boots 23 and 39, 2 weak bits each) are most
likely the torn pre-program; both read all zero after it.

**What this does not show.**

- *"Stable" has a resolution: 4,096 reads a word, 512 a sector.* The
  control shows bits as rare as 1 in 4,096. A cell left that marginal by
  the re-program would be missed. But the erase sectors put 34,823 cells
  through the same step, and not one flicker came back in 512 reads.
- *The time scale is seconds.* The re-check is the next boot, one power
  cut and a few seconds later. Retention over days, heat, or many power
  cycles is not measured.
- *One board, one part* (JEDEC `0x464016`, manufacturer not identified).
  Another part's program algorithm could skip a cell that already
  verifies. Nothing here says it would not.
- *The erase sectors are a proxy.* Their cells were part-way through an
  erase, not a program. They test the same thing (a cell between states,
  programmed to 0) and are reported apart from the program words.
- *The settle step programs the bytes it read, not the intended bytes.*
  For a record whose CRC checked, those are the same: every weak bit read
  as meant on that read. A weak bit that read the wrong way fails the CRC.
  That record takes the other path: its header is programmed to zeros,
  which is the case the erase sectors test.

Batches 1–5 and 6–12 ran different images. The program-loop cuts are one
population: the program loop did not change, only the generations it runs
on.
