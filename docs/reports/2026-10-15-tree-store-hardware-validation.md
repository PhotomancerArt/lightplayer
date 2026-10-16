# The tree store on CX1: 3,100 power cuts, silicon perf, and the G2 picture

**Date** 2026-10-15 · **Plan** `lp2025/2026-10-08-1017-tree-store-device-round`,
milestone M7 (hardware validation), gate **G2 (adopt?)** · **PR** #1146

**Board:** CX1 `c6-expendable` (tag `sacrificial`), MAC `14:C1:9F:E6:54:90`.
It is a generic Amazon ESP32-C6, not a XIAO. Chip v0.2, flash JEDEC id
`0x464016` (4 MiB, DIO 40 MHz), on a VIA twin hub, port 2. The hub showed as
bus `1-1.2` for the soak and as `0-1.2` after the laptop was docked again.
`lpfs` is 176 sectors at `0x350000` (`lp-fw/fw-esp32c6/partitions.csv`). The
board has no `/hardware.json` and no saved network, so Wi-Fi did not join and
the radio was on but idle. No other board was touched.

**Firmware:**
- **`fs-tree`:** `main` `55422b0e8` (#1138), clean; the hello says `commit
  55422b0e8683 dirty=false`. A monolithic dev image built in
  `lp-fw/fw-esp32c6` with `--profile release-esp32 --features
  esp32c6,server,radio,heap_peak_diag,fs-tree` plus the defaults. ELF sha256
  `f84a82c3…c79f9d24e`.
- **littlefs control** (perf only): the same features without `fs-tree`,
  built at this branch's `7a95d4d46`, clean. Its `lp-fw`, `lp-base`,
  `lp-core`, `lp-app` and `lp-emu` sources are `55422b0e8`'s. ELF sha256
  `9fd2b34e…52973adc52`.
- Configuration `silicon:esp32c6`.

**Everything in "The soak", "Silicon perf" and "Cut timing" is silicon.**
Simulator and emulator numbers quoted from earlier milestones are labelled
where they appear.

## Where G2 stands

| | the bar (plan, Direction) | measured |
|---|---|---|
| power-cut cycles on CX1 | ≥ 3,000, zero failures | **3,100 cycles, 0 failures** |
| every cut a power-on reset | yes | **3,100 of 3,100.** 3,038 by the board's own `[RECOVERY] boot: cause=power-on` line, 62 by a heartbeat's `resetReason`. The cut interrupted by the undock was checked on re-dock (below). |
| silicon perf: mount, commits, largest read | measured, compared with M5 | **Measured, with a littlefs control.** The relative figures match the emulator. The absolute request and load times are 1.5–3.3x slower than emulated on both filesystems, filed as a fidelity defect. |
| wear-to-death | (scope item 3) | **Not run, by direction (DD45).** The argument is below. |

## The soak (scope item 1)

**Driver:** `lp-cli/tests/board_tree_store_soak.rs`
(`tree_store_power_cut_soak_on_cx1`, `#[ignore]`d). It uses
`support/desk_board.rs` for the board, power and link, and
`support/board_files_model.rs` for the oracle, which has unit tests that run in
CI. It ran from a copy of the test binary outside `target/`, under `caffeinate
-i -s`, in two legs:
- cycles 0–2,955, 06:40–16:43 PDT, ended when the laptop was undocked
  (`pmset`: clamshell sleep, on battery) and the hub went with it;
- cycles 2,956–3,099 after the re-dock, resumed with `LP_SOAK_START=2956`.

**One cycle:**

1. The host does one thing a user does, over USB, on the board's lp-link:
   - a **push**: Studio's conversation, `lpa_client::push_project`. On the tree
     store that is the one-slot batch (`BeginBatch` → clear → deflated writes →
     load → hash → `CommitBatch`), into the running slot: the basic project
     with a new shader marker.
   - a **c40 push**: 132 files, ~216 KB, into a slot that holds none.
   - a **save**: `shader.glsl` written whole.
   - a **panel write** to the `time` channel. The board's own auto-save
     persists it 10 s after boot.
   - a **switch**: `LoadProject` of the other slot. The board rewrites
     `/lightplayer.json` once the load has run its first frames.
2. `board power-cycle` cuts the power at a random point inside the operation's
   flash work. The aim uses the hub's measured latency (see "Cut timing").
3. The cut is checked, and anything wrong **stops the run**: the command must
   exit 0, the port must go and come back, and the boot must be a power-on.
4. **The boot check.** The link opens the moment the port is back, so the
   board's own lines are kept: `[FS] tree store mounted (176 sectors, N free,
   root R)`, `[perf] fs mount`, and the hello's mount verdict.
5. **The files.** Every file the board serves is read back over the wire.
   - Each file must hold its old or its new content.
   - A batch must be old or new as a whole.
   - Anything the board answered must be there: a push whose `CommitBatch` was
     answered, or a save that was answered.
   - Writes the board makes on its own are always "old or new": a panel file
     holds a written value, and `/lightplayer.json` names a loaded project.
6. **The app.** The project `/lightplayer.json` names is the one running. Its
   `HashPackage` on the board equals the host's hash of the files the board
   served.
7. **The flash**, every 25 cycles:
   - The raw `lpfs` is read over the bootloader (`lp-cli hardware lpfs save`)
     and mounted on the host with `lp-tree-store`.
   - It must hold exactly the files the board served, and pass `lp-cli
     hardware tree check` (the store's fsck).

**Results** (`python3 docs/reports/2026-10-15-tree-store-cx1-soak/summarize.py`
reproduces every figure here):

| operation | cycles | answered before the cut | cut in flight | landed old | landed new | failures |
|---|---:|---:|---:|---:|---:|---:|
| push (basic, one batch) | 980 | 503 | 477 | 413 | 567 | **0** |
| push c40 (one batch) | 85 | 14 | 71 | 69 | 16 | **0** |
| save | 755 | 513 | 242 | 190 | 564 | **0** |
| panel write | 634 | 601 | 33 | 384 | 250 | **0** |
| switch | 646 | 528 | 118 | 156 | 490 | **0** |
| **all** | **3,100** | 2,159 | 941 | 1,212 | 1,887 | **0** |

- **941 cuts fell while the operation was in flight**, that is, before its
  answer.
  - 117 of them landed new: the store had committed, and the answer was lost
    with the power. That is 64 pushes, 51 saves and 2 c40 pushes.
  - None was a mix. Every batch read back whole, old or new, including 71 c40
    pushes cut mid-batch.
  - One save, cycle 2,956, the first after the resume, wrote the bytes the
    file already held, so it landed "same". The interrupted cycle 2,956 had
    already committed them (root 1,848, below).
- **2,159 operations were answered before the cut**, and every one was there
  after it. The rest of those cuts landed in what the board does after the
  answer: the panel auto-save, the startup write, the next frames.
- **Mount verdict:** `mounted` 3,100 of 3,100. Never `refused`, never
  `formatted`.
- **Flash checks:** 124. In all 124 the host's mount of the raw image held
  exactly the board's files, and the fsck said `CONSISTENT`.
- **The store under churn:**
  - The root sequence went from 23 to 1,910.
  - Free sectors (the store's own count) moved between 149 and 173 as GC ran.
  - The board held 17 to 142 files: two basic projects, or a basic one and a
    c40, which was on the board in about 110 cycles.
  - After the first leg, the raw image had every one of the 176 sectors
    `valid` (written).
  - Erase counts per sector were 1 / 10 / 11 (min / median / max), 1,789 in
    all. No sector was retired.
- Read-backs retried on a fresh link: 0.
- Median cycle: 10.5 s. The total was 10.3 hours.

**The cut the undock interrupted** (the first leg's cycle 2,956, with no
record) was checked on re-dock, before the second leg:
- The board's heartbeat said `resetReason: power-on`, `bootCount: 1`, and the
  hello said `fs: mounted`.
- Its raw `lpfs` passed `hardware tree check`: CONSISTENT, root 1,848 (the
  last recorded root, 1,847, plus that cycle's commit), 18 files, 8.3 KB live
  and 636 KB garbage.
- The capture and image are in the planning workspace, `results-m7/redock/`.

Not counted:

- **Two trial runs (24 cycles)** and the cut-timing probe (8 cycles) before
  the soak. Their records are in the planning workspace.
- **Trial 2's cycle 11** printed failures. They were the driver's own:
  - The Mac idle-slept for 16 minutes mid-run (`pmset -g log`, 05:09). The
    board kept running, and when the host came back the link reset in the
    middle of the read-back.
  - The read-back came back partial, and the oracle judged an empty file set.
  - That cycle's raw image passes the fsck. Mounted, it holds the pre-push
    state whole: root 22, the c40 slot intact.
  - Fixes: the soak now runs under `caffeinate`, and a read-back the link
    interrupts is read again on a fresh link (0 needed in the soak).

### Finding: on a used partition the mount takes 0.56 s, not 93 ms

The board's `[perf] fs mount` line, at every boot of the soak:

| cycles | mount, median (min–max) | live files on the board |
|---|---|---|
| 0 (start, after the trial runs) | 97 ms | 17 |
| 10–40 (after each cut c40 push: +65–70 ms) | 165 → 240 → 275 ms | 17–18 |
| 250–3,099 (each block of 250) | **553–568 ms** (540–597) | 17–142 |

#1135 measured 92.7 ms for a c40 store written once. The figure above is
different: once churn has written every sector, the mount reads them all
(live records, garbage and GC copies), whatever is live. Here two small
projects mount in the same ~0.56 s as a c40. A cut c40 batch leaves ~200 KB
of uncommitted records, and each one added 65–70 ms until GC took the space.
The re-dock image shows the state: 8.3 KB live in 644 KB of records. The perf
sitting's "full of live data" mount reads 781 ms (below).

It is boot time before the radios, and inside the 30 s RTC-watchdog boot
window. Over 3,100 boots it caused no reset. But it is the figure a board in
use will show, about 6x what #1135 measured on a fresh store. **The adoption
round should quote 0.56 s at 176 sectors on a used board, and 0.78 s at full.**
By proportion that is roughly 0.4 s at 128 sectors, an estimate.

### Cut timing: where `board power-cycle` cuts on this hub

`power_cut_timing_on_cx1` (8 runs) sends requests back to back while
`board power-cycle` runs:

- The board's last answer comes 0.25–0.35 s after the command starts. That is
  the USB 2 half of the twin switching off.
- VBUS drops when the twin switches, one uhubctl run later.
- In the soak, an in-flight operation's last answer came at most 0.62 s after
  the command started.
- The driver aims the cut 0.45 s after the command starts.

macOS keeps a powered-off port's device node until power comes back. The node
goes and returns about 3.2 s in, at power-up, not at the cut (lp-board-bench's
`power.ts` notes the same). So the node's going is never the cut time.

### Why this is not a `lp-cli validate` transcript

The validation system grades payloads, which are `fw-checks` modules
printing records, with transcripts of the port's bytes. The soak runs the
product image with a host client driving it over the wire, and it has no
payload. Its record is the driver's per-cycle JSON:

- `docs/reports/2026-10-15-tree-store-cx1-soak/cycles.jsonl.gz`: all 3,100
  cycles, each with the operation, the aim, the power cycle's times and exit
  code, the boot and `[FS]` lines, the mount verdict and time, how each event
  landed, the flash check and any failures.
- Console excerpts: the first cycles, one flash-check cycle, the last cycles.
- The full console (gzipped), both legs' driver logs and the trial runs are in
  the planning workspace, `results-m7/`.

## Silicon perf (scope item 2)

**Driver:** `lp-cli/tests/board_tree_store_perf.rs`
(`tree_store_silicon_perf_on_cx1`), run once per image on CX1 after the soak,
2026-10-15. Records: `docs/reports/2026-10-15-tree-store-cx1-perf/`. `python3
…/summarize.py` reproduces every figure here.

The method:
- It erases `lpfs`, then runs the same operations as M5's emulated
  `emu_tree_store_perf`, on the same c40 shape.
- A request's figure is its round trip on the host's wall clock: one request
  in flight, over the board's USB lp-link.
- "Less baseline" takes away a same-size read's round trip (a save against a
  read of the same document, an append against a 4 KiB read), or a
  `ListLoadedProjects` (5.5 ms).
- A mount's figure is the board's own `[perf] fs mount` line, read at power-up
  after an idle `board power-cycle`, each checked to be a power-on boot.

| CX1, silicon (medians) | `fs-tree` | littlefs | fs-tree ÷ littlefs | emulated fs-tree / littlefs (M5, `t1`, CPU only) |
|---|---:|---:|---:|---:|
| first boot (format) | **3.884 s** | 8.86 ms | | 68 ms CPU (est. 3.70 s) / 0.69 ms |
| mount, empty (n=3) | **8.16 ms** | 0.75 ms | 10.9x | 3.72 / 0.23 ms |
| mount, c40 (n=3) | **136.0 ms** | 7.50 ms | 18.1x | 277.8 / 3.1 ms |
| mount, full of live data (n=3) | **780.8 ms** (c40 + 127 × 4 KiB, 3 sectors free) | 7.85 ms (c40 + 13 × 4 KiB; littlefs was full) | | 330.6 ms (c40 + 60) / 3.6 ms (c40 + 28) |
| mount, written-out partition (soak) | **560 ms** (n=3,039) | — | | — |
| push c40 and load (n=1) | **3.27 s** | 69.1 s | 0.047 | 2.31 / 13.0 s |
| save, 1.7 KB document (n=10) | **235.8 ms** (−read 185.2) | 326.4 ms (274.0) | 0.72 | 88.0 / 115.3 ms |
| 4 KiB append (n=12) | **199.4 ms** (−read 146.7) | 272.9 ms (220.0) | 0.73 | 87.3 / 89.3 ms |
| read the largest c40 file, 4,365 B (n=10) | **54.7 ms** | 54.3 ms | 1.01 | 35.7 / 34.9 ms |
| whole-project load, stop + LoadProject c40 (n=3) | **233.8 ms** | 250.4 ms | 0.93 | 71.3 / 75.8 ms |
| panel write, the request (n=6) | **116.9 ms** | 118.6 ms | 0.99 | — |
| panel auto-save (the frame that holds it) | **77.5 ms** frame vs 36 ms idle (≈ +40 ms) | 75 ms vs 36 ms | ≈ 1 | — |
| new 4 KiB file, filling to full | **127.6 ms** (n=127; GC near full: 7 writes over 1 s, the worst 3.78 s) | 155.6 ms (n=13) | 0.82 | — |
| with and without the SHA accelerator | not measurable on this build: `fs-tree` hard-wires `hw_sha`, and `fw-esp32c6` has no `soft-sha` path. A software-hash image is a firmware change. | | | |

### Reading the table

- **Commits.** A save costs ~185 ms beyond a read of the same document on the
  tree store, against ~274 ms on littlefs. A 4 KiB append costs ~147 ms
  against ~220 ms.
  - The first save of each run is the cheapest: 88 ms on the tree store
    (the emulated mean, by coincidence) and 167 ms on littlefs.
  - The later saves sit near 230 ms and 325 ms. Each save changes a file of
    the running project, and the round trip carries the server's work on that
    change, on both images.
- **Reads, loads, panel writes.** These are the same on both images. A read is
  ~50 ms against a 5.5 ms request with no file.
- **The panel write's flash work** is its auto-save on a later frame: one
  ~40 ms longer frame on both filesystems.
- **Pushing a c40 is 21x faster on the tree store** (3.3 s against 69 s). The
  emulator called it 5.6x.
- **Mount** is where the tree store pays:
  - 8 ms empty, 136 ms at c40 and 781 ms nearly full, against littlefs's
    0.75–7.9 ms;
  - 560 ms on the soak's used partition.
  - The c40 figure is 136 ms here against #1135's 92.7 ms, which was uploaded
    by `lp-cli upload`. Both are c40-shaped, but the records differ in layout.
- **Near full**, the store's GC made seven writes take over a second, the
  worst 3.78 s, with 3 sectors free. littlefs stopped at 13 files (the
  "cannot hold two c40" limit M5 found).
- **The first-boot format read 3.884 s, against #1135's 3.316 s.** Both were
  timed from an `lpfs` erased with `espflash`, on different firmware commits
  (`dc5164ef1`, `55422b0e8`). The cause is not known. Wear is not it: the
  sectors had 11 erases at most. DD35 (the lazy format) stands either way.

### Against M5's emulated table

**The relative figures match.** fs-tree ÷ littlefs on silicon against the
emulator:

| | silicon | emulated |
|---|---:|---:|
| save | 0.72 | 0.76 |
| read | 1.01 | 1.02 |
| load | 0.93 | 0.94 |
| append | 0.73 | 0.98 |

Every emulated "fs-tree is about as fast as littlefs, or faster, at commit and
read" verdict holds on silicon. M5's Q18 verdict (no directory cache: a load
and an append within 1.5x of littlefs) holds too: 0.93 and 0.73.

**The absolute figures do not match.** Silicon is 2.3–3.3x the emulated round
trip for saves, appends and loads, and 1.5x for reads, **on both filesystems
alike**.

- A save's flash work is an estimated ~5 ms, and a load's is almost nothing,
  so the gap is not the flash time the emulator leaves out. It is CPU time
  `t1` does not charge.
- Filed as
  `docs/defects/2026-10-15-the-emulated-c6-answers-saves-and-loads-two-to-three-times-faster-than-silicon.md`
  (emulator-fidelity, open). Its candidates are flash-cache misses on code
  run from flash, esp-storage's cache-off reads, and the radio.
- The emulated c40 mount stays about 2x over silicon's: 278 ms against 136 ms.
  That is the open mount defect, amended here with M7's figures.

## Wear-to-death (scope item 3): not run, and why that is enough for G2

Yona questioned the need for it (2026-10-08: "I question the need to do that
just for fun"), and the director's position (DD45) is to skip it. A wear-out
run would show two things: how many cycles this part takes, and whether the
store retires the sector. The second is already proven where it can be made
exhaustive.

- **The failure is modelled the way NOR fails.** `lp-nor-sim`'s wear-out
  mode (`lp-emu/lp-nor-sim/src/wear_out.rs`) makes a sector silently wrong
  after N erases. A worn erase leaves bits stuck at 0, and a worn program
  leaves clears undone. The operation still "succeeds", as real NOR
  reports nothing. Only a store that reads back what it wrote can tell.
- **The store retires it, and the retirement survives a remount.**
  `verify_after_write_retires_a_worn_sector_and_the_retirement_survives_remount`
  (`lp-base/lp-tree-store/src/store_tests.rs`):
  - Both modes, 80 rounds of churn, a remount every 7 rounds.
  - Every file reads back correctly throughout.
  - The worn sector is retired exactly once, never chosen as a head again,
    and still retired after the remount.
  - `hardware tree inspect` reports a retired sector (`image_inspect_tests`).
- **Breaking it is caught.** M3's mutants include `open_without_read_back`,
  a store that skips the read-back that finds a worn sector. All 12 mutants
  are caught: 11 from M3, and the twelfth, `trust_tail_once`, added by #1136.
- **The cycle count is a fact about the part, not the store.** NOR is
  specified at ~100k erases per sector (the part's datasheet figure).
  Lifetime is set by how the store spreads its wear:
  - In M3's re-race simulation, 30 days of the c40 workload wore the tree
    store's sectors 0 / 32 / 69 erases (min / median / max), against
    littlefs-F2's 21,617 on its hottest pair.
  - On CX1, the 3,100-cut soak itself left 1 / 10 / 11. At those rates a
    sector of this part is years from its rating.
- **What a silicon run could still find:** a real wear-out signature the
  model does not draw, for example an erase that slows down or a retention
  loss rather than stuck bits.
  - CX1's 1,300 cuts so far (M4) found the model's tear shapes wrong twice
    and corrected them. That argues for calibrating against silicon where
    silicon is cheap.
  - A ~35-minute wear run on one sector is cheap, but it destroys that
    sector, and today it answers no G2 question.
  - The lean: run it only if the adoption round's ADR wants a measured
    endurance figure for this part.

## The rest of the G2 picture

- **M4, calibration: confirmed.**
  - 700 real cuts on CX1 were analysed (500 aligned, 200 unaligned).
  - The calibrated `lp-nor-sim` model was re-fitted on all 700 and gained two
    erase shapes (zeroing caught inside a word, first lift). `--check-model`
    says CONFIRMED, in-sample (Bonferroni per family).
  - Out of sample, the 200 unaligned cuts' shares all fell inside the 500-cut
    model's intervals. The two unmodelled states they showed are now
    modelled.
  - A further 600 cuts (#1139) showed that re-programming a torn word settles
    it on this part, which is the assumption #1136's mount settle rests on.
  - Details: `docs/reports/2026-10-08-c6-nor-tear-calibration.md`, sections
    10–14.
- **The store under the re-fitted model** (simulator, `lp-nor-sim`): 0
  failures in 219,336 `lp-store-bench` cases. All 8 in-crate cut tests pass
  under every forced shape at 128 and 176 sectors.
- **M3, the re-race (simulator):**
  - Every cut sweep for f2, f3, t1 and t1 with host deflate had 0 failures,
    under every tear model, at 128 and 176 sectors.
  - All 11 mutants are caught, and the unmutated store is clean in all seven
    drivers. (There are 12 since #1136 added `trust_tail_once`.)
  - Fuzz: 64,000 cases, 0 failures.
  - The near-full residual M3 found frequent at the reserve was fixed by #1133
    (`docs/defects/2026-10-10-tree-store-gc-cannot-pack-what-the-bound-admits.md`,
    fixed).
  - Wear, 30 days of c40: t1 0 / 32 / 69 erases per sector, against F2's
    21,617 hottest.
  - Results: `results-m3/` in the planning workspace. PR #1069.
- **M5, the emulator walks** (`lp-emu:esp32c6:t1+net=lan+flash-cut`, never
  hardware-validated):
  - fs-tree: 6,663 cuts at #1131, then the long walk under the re-fitted model
    (#1138): **4,961 cuts, 0 failures**.
  - The littlefs control, with the create fix #1134: 7,755 cuts, 0 failures.
  - RAM resident +5.6 KB emulated and +2.8 KB live on CX1 (#1135). At rest,
    with Wi-Fi joined on silicon, the largest block is 39.7 KB, against
    littlefs's 33.3 KB.

## Open items the adoption round inherits

1. **A lazy first-boot format (DD35)**, required before `fs-tree` goes live.
   It measures 3.3–3.9 s on CX1, with interrupts masked one erase at a time.
2. **Mount time: 0.56 s on a used board and 0.78 s near full, at 176 sectors**
   (this run). The 93–136 ms of a fresh c40 is not what a used board shows.
   Decide whether that is acceptable, or whether the mount should read less
   than every written sector.
3. **Near-full writes stall in GC for 1–4 s** (seven of the last twenty
   4 KiB writes before "no space"). That is liveness, not loss.
4. The rest of M5's adoption checklist
   (`docs/reports/2026-10-10-tree-store-emulator-walk.md`):
   - the two-way littlefs / tree-store guard in the product image;
   - the unreachable legacy-layout guard (make it reachable, or drop it with
     the probe);
   - littlefs leaving the image, a nice-to-have.
5. Still-open defects:
   - `2026-10-08-tree-store-root-sequence-does-not-wrap.md` (theoretical: a
     u64 sequence; this soak used 1,887 in 10 hours);
   - `2026-10-14-the-emulated-c6-mounts-a-c40-tree-store-three-times-slower-than-silicon.md`;
   - `2026-10-15-the-emulated-c6-answers-saves-and-loads-two-to-three-times-faster-than-silicon.md`.
6. Wear-to-death: only if the ADR wants an endurance figure for this part.

## G2's questions

1. Adopt the tree store and plan the adoption round (migration, repartition
   to 128, the ADR, the default)?
2. Must anything change first? The candidates:
   - the lazy format (already required);
   - the mount's 0.56–0.78 s on a used or full partition;
   - the near-full GC stalls.
