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
> correction (scope item 5) has not started: it waits for M2 to merge.

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
  "set" when cleared — reads a torn erase as data. `lp-nor-sim` never
  makes this sector, so no store in the testbed has met it.
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

## 5. What the correction will add (scope item 5 — not started)

Additive, beside today's models, once M2 has merged and the director says
go: torn-erase shapes **zeroing** (a word-aligned `0x00` prefix, old after),
**all-zero**, **erasing from zero** (every bit lifting at once, the residue
spread over old and new positions alike, weak bits in proportion) and a
**silent reads-erased** shape (all `0xFF`, no weak bits, but never marked);
a torn program that stops on a **4-byte word** inside a **32-byte command**.
The phase shares in section 7 are what a seeded mix would draw from.

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
- **The silicon sidecars carry no board metadata** (`board`, `mac`): the
  board is named only by the `ft-boot` records' MAC. The emulated sidecar
  does carry it.

## 7. Generated: every transcript, sorted

<!-- flash-tears-analyze:begin -->

_Generated by `scripts/emu/flash-tears-analyze.py` over 5 transcript(s). Do not edit by hand; re-run it._

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


<!-- flash-tears-analyze:end -->
