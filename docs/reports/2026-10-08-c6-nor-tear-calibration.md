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
> at 500** (`just flash-tears-analyze --check-model` says whether its numbers
> still match every committed cut).
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

_Generated by `scripts/emu/flash-tears-analyze.py` over 6 transcript(s). Do not edit by hand; re-run it._

### `silicon:esp32c6`

| transcript | date | firmware | cycles | boots | cuts | not cuts |
|---|---|---|---|---:|---:|---|
| `silicon-esp32c6-2026-10-08-f5f629536.txt` | 2026-10-08 | `f5f6295369c0` | 20589–23200 | 51 | 50 | 1× reset `usb-uart`, not a power cut |
| `silicon-esp32c6-2026-10-08-e8dfa2bf5.txt` | 2026-10-08 | `e8dfa2bf5904` | 28159–30724 | 51 | 50 | 1× reset `usb-uart`, not a power cut |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` | 2026-10-08 | `a53bdecba0d5` | 35441–38061 | 51 | 50 | 1× reset `usb-uart`, not a power cut |
| `silicon-esp32c6-2026-10-08-c322c8489.txt` | 2026-10-08 | `c322c8489713` | 44089–46528 | 51 | 50 | 1× reset `usb-uart`, not a power cut |
| **all** | | | | 204 | **200** | |

Who the records say ran it (from every `ft-boot`):

| MAC | flash JEDEC id (manufacturer, type, capacity) | flash size (capacity byte) | lpfs base | boots |
|---|---|---|---|---:|
| `14:c1:9f:e6:54:90` | `0x464016` | 4 MiB | `0x350000` | 204 |

Reset reasons of the cut boots: `poweron` 200.

Boots that were not power cuts (left out of every count below):

| boot | reset | state | in-flight sector |
|---|---|---|---|
| `silicon-esp32c6-2026-10-08-f5f629536.txt` boot 0 | `usb-uart` | resume | erased |
| `silicon-esp32c6-2026-10-08-e8dfa2bf5.txt` boot 0 | `usb-uart` | resume | complete |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 0 | `usb-uart` | resume | program:command-boundary |
| `silicon-esp32c6-2026-10-08-c322c8489.txt` boot 0 | `usb-uart` | resume | erased |

Outside the in-flight sector, over every cut:

- settled sectors not holding their last cycle's pattern: **0** (of 3000 read)
- weak bits in settled sectors: **0**
- damaged journal slots: **0**; torn next slots: **0**
- work cycles completed between a scan and its cut (min / median / max): 10 / 50 / 97

**Tear shapes, 200 cuts** (the in-flight sector of each):

| phase | shape | cuts | share |
|---|---|---:|---:|
| erase | untouched: the old pattern, whole (cut before the erase moved a cell) | 0 | 0.0 % |
| erase | torn erase, zeroing: a run of `0x00` from the front, old after it | 10 | 5.0 % |
| erase | torn erase, all `0x00` | 26 | 13.0 % |
| erase | torn erase, erasing: stable zeros spread over old **and** new zero positions | 28 | 14.0 % |
| erase | torn erase, old data left (only old zeros, more than a residue) | 0 | 0.0 % |
| erase | torn erase that reads all `0xFF` but has weak bits | 1 | 0.5 % |
| erase | erased: all `0xFF`, no weak bits | 101 | 50.5 % |
| program | torn program, prefix ending on a 32-byte command boundary | 26 | 13.0 % |
| program | torn program, prefix ending inside a 32-byte command | 7 | 3.5 % |
| program | torn program, scattered clears (no prefix) | 0 | 0.0 % |
| program | complete: the new pattern, whole | 1 | 0.5 % |
| none | unknown | 0 | 0.0 % |
| **erase** | | **166** | 83.0 % |
| **program** | | **34** | 17.0 % |

The firmware's `verdict` against the phase it was sorted into:

| firmware verdict | phase | cuts |
|---|---|---:|
| `complete` | complete | 1 |
| `erased` | erased | 101 |
| `erased-weak` | erase:reads-ff-weak | 1 |
| `mixed` | erase:all-zero | 26 |
| `mixed` | erase:erasing | 23 |
| `mixed` | erase:zeroing | 10 |
| `torn-erase` | erase:erasing | 2 |
| `torn-program` | erase:erasing | 3 |
| `torn-program` | program:command-boundary | 26 |
| `torn-program` | program:mid-command | 7 |

**Weak bits** (a bit that did not read the same all 8 times):

- in-flight sectors with any: **27** of 200

| phase | cuts | with weak bits | weak bits (min / median / max, where any) |
|---|---:|---:|---|
| erase:zeroing | 10 | 0 | — |
| erase:all-zero | 26 | 0 | — |
| erase:erasing | 28 | 26 | 1 / 91 / 2166 |
| erase:reads-ff-weak | 1 | 1 | 2 / 2 / 2 |
| erased | 101 | 0 | — |
| program:command-boundary | 26 | 0 | — |
| program:mid-command | 7 | 0 | — |
| complete | 1 | 0 | — |

- sectors reading all `0xFF` in every stable bit: 102; of those with weak bits: **1** (2 weak bits)

**Torn programs**: where the landed prefix ends (byte offset in the sector):

| shape | cuts | on a 4-B word boundary | on a 256-B page boundary | partial bytes (min / median / max) | end mod 32 |
|---|---:|---:|---:|---|---|
| program:command-boundary | 26 | 26 | 5 | 0 / 0 / 0 | 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 |
| program:mid-command | 7 | 7 | 0 | 0 / 0 / 0 | 20, 20, 20, 4, 8, 4, 28 |

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
| **all 10** | | | | **10** | **0** | **0** |

**Erasing**: stable zeros left, as a share of the old pattern's zeros and of the new pattern's zeros (the old pattern's ones). The two shares track each other when the erase starts from all `0x00` — every bit is a zero to lift, whatever the old data was; an erase from the old data would leave only old zeros.

| cut | old zeros still 0 | old ones now 0 | weak bits | weak bytes |
|---|---:|---:|---:|---:|
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 27 | 78.71 % | 77.21 % | 1486 | 1245 |
| `silicon-esp32c6-2026-10-08-f5f629536.txt` boot 36 | 40.95 % | 40.06 % | 2166 | 1691 |
| `silicon-esp32c6-2026-10-08-f5f629536.txt` boot 34 | 15.39 % | 14.83 % | 1350 | 1185 |
| `silicon-esp32c6-2026-10-08-c322c8489.txt` boot 15 | 13.12 % | 12.99 % | 1423 | 1215 |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 37 | 10.24 % | 9.37 % | 1117 | 994 |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 30 | 4.21 % | 3.98 % | 603 | 572 |
| `silicon-esp32c6-2026-10-08-e8dfa2bf5.txt` boot 32 | 3.56 % | 3.59 % | 555 | 515 |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 12 | 2.28 % | 2.01 % | 342 | 329 |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 50 | 1.85 % | 1.55 % | 289 | 283 |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 44 | 1.50 % | 1.54 % | 257 | 251 |
| `silicon-esp32c6-2026-10-08-c322c8489.txt` boot 10 | 0.61 % | 0.76 % | 158 | 151 |
| `silicon-esp32c6-2026-10-08-c322c8489.txt` boot 39 | 0.70 % | 0.61 % | 140 | 138 |
| `silicon-esp32c6-2026-10-08-e8dfa2bf5.txt` boot 33 | 0.37 % | 0.44 % | 83 | 83 |
| `silicon-esp32c6-2026-10-08-f5f629536.txt` boot 33 | 0.36 % | 0.41 % | 99 | 98 |
| `silicon-esp32c6-2026-10-08-e8dfa2bf5.txt` boot 45 | 0.38 % | 0.35 % | 73 | 71 |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 48 | 0.36 % | 0.34 % | 83 | 82 |
| `silicon-esp32c6-2026-10-08-c322c8489.txt` boot 29 | 0.16 % | 0.19 % | 57 | 57 |
| `silicon-esp32c6-2026-10-08-f5f629536.txt` boot 22 | 0.12 % | 0.12 % | 32 | 32 |
| `silicon-esp32c6-2026-10-08-f5f629536.txt` boot 7 | 0.12 % | 0.08 % | 48 | 48 |
| `silicon-esp32c6-2026-10-08-f5f629536.txt` boot 50 | 0.05 % | 0.05 % | 14 | 14 |
| `silicon-esp32c6-2026-10-08-c322c8489.txt` boot 17 | 0.01 % | 0.02 % | 8 | 8 |
| `silicon-esp32c6-2026-10-08-c322c8489.txt` boot 36 | 0.01 % | 0.01 % | 2 | 2 |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 34 | 0.00 % | 0.02 % | 0 | 0 |
| `silicon-esp32c6-2026-10-08-a53bdecba.txt` boot 21 | 0.01 % | 0.01 % | 3 | 3 |
| `silicon-esp32c6-2026-10-08-e8dfa2bf5.txt` boot 6 | 0.01 % | 0.00 % | 6 | 6 |
| `silicon-esp32c6-2026-10-08-e8dfa2bf5.txt` boot 27 | 0.00 % | 0.01 % | 1 | 1 |
| `silicon-esp32c6-2026-10-08-f5f629536.txt` boot 2 | 0.00 % | 0.01 % | 0 | 0 |
| `silicon-esp32c6-2026-10-08-c322c8489.txt` boot 43 | 0.01 % | 0.00 % | 2 | 2 |

- erase-phase cuts with a stable `0` where the old data had a `1`: **62** of 166

**Timing of one cycle** (the timed cycle after each scan, 204 boots; µs, min / median / max):

- `journal_us`: 93 / 95 / 18547
- `erase_us`: 21092 / 24959.5 / 28892
- `program_us`: 5529 / 5658 / 5773
- `page_us_min`: 304 / 311 / 321
- `page_us_max`: 322 / 337 / 349
- share of a cycle (medians): journal 0.3 %, erase 81.3 %, program 18.4 %; the cuts landed 83.0 % in the erase and 17.0 % in the program

**`lp-nor-sim`'s assumptions against these 200 cuts** (the models as `lp-emu/lp-nor-sim` has them; the verdict is mechanical, from the counts above):

| assumption | evidence | verdict |
|---|---|---|
| A torn program lands a prefix of what it was asked to write (`BytePrefix`) | 33 torn programs, all prefixes; 0 scattered | **held** |
| `BytePrefix`: the byte after the prefix gets a random subset of its clears | 0 partial bytes in 33 torn programs; 33 of 33 prefixes end on a 4-byte word | **not seen: every prefix ends on a whole byte — a whole 4-byte word** |
| `RandomBits`: a torn program lands a random subset of the page's clears | 0 of 33 torn programs scattered | **not seen (the model is harsher than this part)** |
| One program op is a 256-byte page; a cut between ops leaves whole pages | 26 of 33 prefixes end on a 32-byte command boundary, 5 of them on a page boundary; 7 end inside a command | **contradicted: the unit is the 32-byte ROM command** |
| A torn erase leaves a byte-wise mix of old bytes, `0xFF` and weak bits (shape 0) | 0 of 166 erase cuts left old data beyond a residue | **not seen** |
| A torn erase can read all `0xFF` and carry weak bits (shape 1) | 1 of 102 sectors reading all `0xFF` had weak bits (2); the model sprinkles one weak bit in ~1 of 32 bytes (~128 a sector) | **held, rarely, and far lighter than modelled** |
| A torn erase can be erased up to a point and old after it (shape 2) | 0 with `0xFF` then old; 10 with `0x00` then old (zeroing) | **not seen as `0xFF`-then-old; seen as `0x00`-then-old** |
| A torn erase only lifts bits: it never leaves a `0` where the old data had a `1` | 62 of 166 erase cuts did (36 of them reading `0x00` from the front or throughout) | **contradicted** |
| Weak bits come from torn erases | 27 in-flight sectors had weak bits, 27 of them erase-phase; 0 weak bits in settled sectors | **held** |
| A torn erase that started changes the sector (`Clean` leaves it old) | 0 of 166 erase cuts left the old pattern whole | **held: no started erase left the old data** |
| A cut damages only the operation in flight | 0 settled sectors damaged, 0 weak bits in them, 0 journal slots damaged | **held** |

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
- damaged journal slots: **0**; torn next slots: **0**
- work cycles completed between a scan and its cut (min / median / max): 257 / 484 / 1494

**Tear shapes, 5 cuts** (the in-flight sector of each):

| phase | shape | cuts | share |
|---|---|---:|---:|
| erase | untouched: the old pattern, whole (cut before the erase moved a cell) | 0 | 0.0 % |
| erase | torn erase, zeroing: a run of `0x00` from the front, old after it | 0 | 0.0 % |
| erase | torn erase, all `0x00` | 0 | 0.0 % |
| erase | torn erase, erasing: stable zeros spread over old **and** new zero positions | 0 | 0.0 % |
| erase | torn erase, old data left (only old zeros, more than a residue) | 0 | 0.0 % |
| erase | torn erase that reads all `0xFF` but has weak bits | 0 | 0.0 % |
| erase | erased: all `0xFF`, no weak bits | 4 | 80.0 % |
| program | torn program, prefix ending on a 32-byte command boundary | 1 | 20.0 % |
| program | torn program, prefix ending inside a 32-byte command | 0 | 0.0 % |
| program | torn program, scattered clears (no prefix) | 0 | 0.0 % |
| program | complete: the new pattern, whole | 0 | 0.0 % |
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
- damaged journal slots: **0**; torn next slots: **0**
- work cycles completed between a scan and its cut (min / median / max): 174 / 433.5 / 1494

**Tear shapes, 5 cuts** (the in-flight sector of each):

| phase | shape | cuts | share |
|---|---|---:|---:|
| erase | untouched: the old pattern, whole (cut before the erase moved a cell) | 2 | 40.0 % |
| erase | torn erase, zeroing: a run of `0x00` from the front, old after it | 0 | 0.0 % |
| erase | torn erase, all `0x00` | 0 | 0.0 % |
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
| none | unknown | 0 | 0.0 % |
| **erase** | | **4** | 80.0 % |
| **program** | | **1** | 20.0 % |

The firmware's `verdict` against the phase it was sorted into:

| firmware verdict | phase | cuts |
|---|---|---:|
| `erased` | erased | 2 |
| `old` | untouched | 2 |
| `torn-program` | program:between-writes | 1 |

**Weak bits** (a bit that did not read the same all 8 times):

- in-flight sectors with any: **0** of 5

| phase | cuts | with weak bits | weak bits (min / median / max, where any) |
|---|---:|---:|---|
| untouched | 2 | 0 | — |
| erased | 2 | 0 | — |
| program:between-writes | 1 | 0 | — |

- sectors reading all `0xFF` in every stable bit: 2; of those with weak bits: **0** (no weak bits)

**Unaligned torn programs**: where each prefix stopped, against the write it was in. `rom` = 32 B from the write's address inside its first page, then 32 B from the page boundary (the split the emulated mask ROM makes, and `lp-nor-sim`'s per-page op); `absolute` = a 32-byte boundary of the flash; `relative` = 32 B from the write's address all the way.

| cut | write (at, len) | prefix ends at | into the write | first page of it | end mod 32 | from the write mod 32 | rom | absolute | relative | class |
|---|---|---|---:|---|---:|---:|---|---|---|---|
| `lp-emu-esp32c6-t1-2026-10-08-f8f2be0e6.txt` boot 1 | — | 1116 | 0 | — | — | — | — | — | — | program:between-writes |

- prefixes ending inside a write: **0** (1 more ended where a write starts); on a 4-byte word: **0**
- of those on a word, on a boundary under `rom`: **0**, `absolute`: **0**, `relative`: **0**
- ends that tell the three apart (a boundary under one, not under another): **0**; of them `rom` 0, `absolute` 0, `relative` 0
- ends inside the write's first page: 0 (where `rom` and `relative` say a command starts a word or more past a 32-byte boundary and `absolute` says on one)

- erase-phase cuts with a stable `0` where the old data had a `1`: **0** of 4

**Timing of one cycle** (the timed cycle after each scan, 6 boots; µs, min / median / max — emulated time, a model and not a measurement):

- `journal_us`: 16 / 16 / 16
- `erase_us`: 4 / 5 / 5
- `program_us`: 774 / 824.5 / 866
- `page_us_min`: 5 / 5 / 5
- `page_us_max`: 40 / 76 / 90
- share of a cycle (medians): journal 1.9 %, erase 0.6 %, program 97.5 %; the cuts landed 80.0 % in the erase and 20.0 % in the program


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

**`--check-model` will say MODEL DIFFERS at 500 whatever the part did**: it
compares the weights in `calibrated_tear.rs` with the transcripts' raw
counts, and 500 cuts have other counts than 200. Whether the 500 *agree*
with the 200 is a question of shares within sampling noise, which the
check does not ask; that call — re-fit (paste `--model-table`) or
"confirmed at 500" — is left to whoever reads the 500.

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
