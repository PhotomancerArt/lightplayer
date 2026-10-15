# The tree store on CX1: a 2,956-cycle power-cut soak, and the G2 picture

**Date** 2026-10-15 · **Plan** `lp2025/2026-10-08-1017-tree-store-device-round`,
milestone M7 (hardware validation), gate **G2 (adopt?)** · **PR** #1146

**Board:** CX1 `c6-expendable` (tag `sacrificial`), MAC `14:C1:9F:E6:54:90`.
It is a generic Amazon ESP32-C6, not a XIAO. Chip v0.2, flash JEDEC id
`0x464016` (4 MiB, DIO 40 MHz), on VIA twin hub `1-1.2`/`1-2.2` port 2.
`lpfs` is 176 sectors at `0x350000` (`lp-fw/fw-esp32c6/partitions.csv`). The
board has no `/hardware.json` and no saved network, so Wi-Fi did not join and
the radio was on but idle. No other board was touched.

**Firmware:** `main` `55422b0e8` (#1138), clean. The hello says `commit
55422b0e8683 dirty=false`. It is a monolithic dev image, built in
`lp-fw/fw-esp32c6` with `--profile release-esp32 --features
esp32c6,server,radio,heap_peak_diag,fs-tree` plus the defaults. ELF sha256
`f84a82c3…c79f9d24e`. Configuration `silicon:esp32c6`.

**Everything in "The soak" and "Cut timing" is silicon.** Simulator and
emulator numbers quoted from earlier milestones are labelled where they
appear.

## Where G2 stands

| | the bar (plan, Direction) | measured |
|---|---|---|
| power-cut cycles on CX1 | ≥ 3,000, zero failures | **2,956 cycles, 0 failures.** The run stopped at cycle 2,956 when the laptop left the desk (16:43, clamshell, on battery), and the hub went with it. It is 44 short. |
| every cut a power-on reset | yes | **2,956 of 2,956.** 2,897 by the board's own `[RECOVERY] boot: cause=power-on` line, 59 by a heartbeat's `resetReason`. The cut in progress at the undock (cycle 2,956) is not counted, and its power-on is unverified (see "Owed"). |
| silicon perf: commits, largest read, mount full | measured | **Not measured: the board left before the perf sitting.** The driver is committed (`lp-cli/tests/board_tree_store_perf.rs`), and the sitting is one command (below). The soak itself measured the **mount on a written-out partition: 560 ms**. |
| wear-to-death | (scope item 3) | **Not run, by direction (DD45).** The argument is below. |

## The soak (scope item 1)

**Driver:** `lp-cli/tests/board_tree_store_soak.rs`
(`tree_store_power_cut_soak_on_cx1`, `#[ignore]`d). It uses
`support/desk_board.rs` for the board, power and link, and
`support/board_files_model.rs` for the oracle, which has unit tests that run in
CI. It ran from a copy of the test binary outside `target/` under `caffeinate
-i -s`, from 06:40 to 16:43 PDT.

**One cycle:**

1. The host does one thing a user does, over USB, on the board's lp-link:
   - a **push**: Studio's conversation, `lpa_client::push_project`. On the tree
     store that is the one-slot batch (`BeginBatch` → clear → deflated writes →
     load → hash → `CommitBatch`). It goes into the running slot, which is the
     basic project with a new shader marker. 936 cycles.
   - a **c40 push**: 132 files, ~216 KB, into a slot that holds none. 83
     cycles.
   - a **save**: `shader.glsl` written whole. 718 cycles.
   - a **panel write** to the `time` channel. The board's own auto-save
     persists it 10 s after boot. 603 cycles.
   - a **switch**: `LoadProject` of the other slot. The board rewrites
     `/lightplayer.json` once the load has run its first frames. 616 cycles.
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
| push (basic, one batch) | 936 | 491 | 445 | 383 | 553 | **0** |
| push c40 (one batch) | 83 | 13 | 70 | 68 | 15 | **0** |
| save | 718 | 491 | 227 | 177 | 541 | **0** |
| panel write | 603 | 573 | 30 | 363 | 240 | **0** |
| switch | 616 | 512 | 104 | 140 | 476 | **0** |
| **all** | **2,956** | 2,080 | 876 | 1,131 | 1,825 | **0** |

- **876 cuts fell while the operation was in flight**, that is, before its
  answer.
  - 114 of them landed new: the store had committed, and the answer was lost
    with the power. That is 62 pushes, 50 saves and 2 c40 pushes.
  - The rest landed old.
  - None was a mix. Every batch read back whole, old or new, including 70 c40
    pushes cut mid-batch.
- **2,080 operations were answered before the cut**, and every one was
  there after it. The rest of those cuts landed in what the board does after
  the answer: the panel auto-save, the startup write, the next frames.
- **Mount verdict:** `mounted` 2,956 of 2,956. Never `refused`, never
  `formatted`.
- **Flash checks:** 118. In all 118 the host's mount of the raw image held
  exactly the board's files, and the fsck said `CONSISTENT`.
- **The store under churn:**
  - The root sequence went from 23 to 1,847.
  - Free sectors (the store's own count) moved between 149 and 173 as GC ran.
  - The board held 17 to 142 files: two basic projects, or a basic one and a
    c40, which was on the board in 92 cycles.
- Read-backs retried on a fresh link: 0.
- Median cycle: 10.5 s. The total was 9.8 hours.

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
| 250–2,955 (each block of 250) | **553–568 ms** (540–597) | 17–142 |

#1135 measured 92.7 ms for a c40 store written once. The figure above is
different: once churn has written every sector, the mount reads them all
(live records, garbage and GC copies), whatever is live. Here two small
projects mount in the same ~0.56 s as a c40. A cut c40 batch leaves ~200 KB
of uncommitted records, and each one added 65–70 ms until GC took the space.

It is boot time before the radios, and inside the 30 s RTC-watchdog boot
window. Over 2,956 boots it caused no reset. But it is the figure a board in
use will show, about 6× what #1135 measured on a fresh store. **The adoption
round should quote 0.56 s at 176 sectors.** By proportion that is roughly
0.4 s at 128, an estimate. The checklist item "mount time" stands with this
number.

The emulated "mount, full" (331 ms of CPU, c40 plus 60 × 4 KiB, `lp-emu
535f7d135`) is not a comparison: it is a different state, and the emulator
counts no flash time. This is not a new fidelity defect. The open one
(`2026-10-14-the-emulated-c6-mounts-a-c40-tree-store-three-times-slower-than-silicon.md`)
still applies to the emulated mount.

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

- `docs/reports/2026-10-15-tree-store-cx1-soak/cycles.jsonl.gz`: all 2,956
  cycles, each with the operation, the aim, the power cycle's times and exit
  code, the boot and `[FS]` lines, the mount verdict and time, how each event
  landed, the flash check and any failures.
- Console excerpts: the first cycles, one flash-check cycle, the last cycles.
- The full console (23 MB; 1.2 MB gzipped), the driver log and the trial runs
  are in the planning workspace, `results-m7/`.

## Silicon perf (scope item 2): owed

The perf sitting did not run: CX1 left with the laptop before it. What there
is:

| CX1, silicon, `fs-tree` | figure | source |
|---|---|---|
| first-boot format | 3.316 s (3.290–3.340, n=5) | #1135, `dc5164ef1` |
| mount, empty | 7.63 ms (n=3) | #1135 |
| mount, c40 written once | 92.70 ms (n=20) | #1135 |
| **mount, written-out partition (2 basic projects to c40 live)** | **560 ms median, n=2,897** | this soak |
| commit: save, panel write, 4 KiB append; largest read; mount full of live data | **not measured** | the sitting below |
| with and without the SHA accelerator | **not measurable on this build** | `fs-tree` hard-wires `hw_sha` (no `soft-sha` path in `fw-esp32c6`); a software-hash image is a firmware change |

**The sitting, when CX1 is back on the desk hub** (about 15 minutes per image):

```bash
board take c6-expendable --as "direct: tree-store M7" --for "fs-tree perf"
# fs-tree image (as in the soak), then:
LP_SOAK_DIR=<out> LP_PERF_LABEL=fs-tree BOARD_HOLDER="direct: tree-store M7" \
  cargo test -p lp-cli --test board_tree_store_perf -- --ignored --nocapture
# the littlefs control image (default + heap_peak_diag), the same with LP_PERF_LABEL=littlefs
```

It erases `lpfs`, then measures:
- the first boot and three empty mounts;
- a c40 push and load;
- 10 saves and 10 reads of the 1.7 KB document, so a save is quoted with and
  without the same-size read's round trip;
- 12 4-KiB appends against a 4 KiB read;
- 10 reads of the largest c40 file;
- three whole-project loads;
- six panel writes, 12 s apart, with a 60 s idle control. The auto-save's
  cost comes from the firmware's `[perf] frames … max=` lines.
- three c40 mounts;
- 4 KiB files until "no space", then three mounts of the full store.

Every mount is an idle power cycle, checked to be a power-on. The rows match
M5's emulated table (`docs/reports/2026-10-10-tree-store-emulator-walk.md`,
"Perf") one for one.

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
  a store that skips the read-back that finds a worn sector. All 12 mutants (11 from M3, the twelfth with #1136)
  are caught.
- **The cycle count is not a store question.** It is the part's datasheet
  figure: NOR is specified ~100k erases per sector. The store's wear is what
  decides lifetime. In M3's re-race simulation, 30 days of the c40 workload
  wore the tree store's sectors 0 / 32 / 69 erases (min / median / max),
  against littlefs-F2's 21,617 on its hottest pair. At that rate one sector
  of CX1's part is years from its rating.
- **What a silicon run could still find:** a real wear-out signature the
  model does not draw, for example an erase that slows down or a retention
  loss rather than stuck bits. CX1's 1,300 cuts so far (M4) found the model's
  tear shapes wrong twice and corrected them. That argues for calibrating
  against silicon where silicon is cheap. A ~35-minute wear run on one
  sector is cheap, but it destroys that sector, and today it answers no G2
  question. The lean: run it only if the adoption round's ADR wants a
  measured endurance figure for this part.

## The rest of the G2 picture

- **M4, calibration: confirmed.** 700 real cuts on CX1 were analysed
  (500 aligned, 200 unaligned). The calibrated `lp-nor-sim` model was re-fitted
  on all 700 and gained two erase shapes (zeroing caught inside a word, first
  lift). `--check-model` says CONFIRMED, in-sample (Bonferroni per family).
  Out of sample, the 200 unaligned cuts' shares all fell inside the 500-cut
  model's intervals. The two unmodelled states they showed are now
  modelled. A further 600 cuts (#1139) showed that re-programming a torn word
  settles it on this part, which is the assumption #1136's mount settle rests
  on. Details: `docs/reports/2026-10-08-c6-nor-tear-calibration.md`, sections
  10–14.
- **The store under the re-fitted model** (simulator, `lp-nor-sim`): 0
  failures in 219,336 `lp-store-bench` cases. All 8 in-crate cut tests pass
  under every forced shape at 128 and 176 sectors.
- **M3, the re-race (simulator):**
  - Every cut sweep for f2, f3, t1 and t1 with host deflate had 0 failures,
    under every tear model, at 128 and 176 sectors.
  - All 11 mutants are caught (12 since #1136 added `trust_tail_once`), and the unmutated store is clean in all seven
    drivers.
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
   It measures 3.3 s on CX1 (#1135), with interrupts masked one erase at a
   time.
2. **Mount time on a used board: 0.56 s at 176 sectors** (this soak), not
   the 93 ms of a fresh c40. Decide whether it is acceptable, or whether the
   mount should scan less than every written sector.
3. The rest of M5's adoption checklist
   (`docs/reports/2026-10-10-tree-store-emulator-walk.md`):
   - the two-way littlefs / tree-store guard in the product image;
   - the unreachable legacy-layout guard (make it reachable, or drop it with
     the probe);
   - the at-rest largest block, measured (#1135);
   - littlefs leaving the image, a nice-to-have.
4. The silicon perf sitting above, and the 44 soak cycles to 3,000 (resume
   with `LP_SOAK_START=2956`).
5. Still-open defects:
   - `2026-10-08-tree-store-root-sequence-does-not-wrap.md` (theoretical: a
     u64 sequence; this soak used 1,824 in 10 hours);
   - `2026-10-14-the-emulated-c6-mounts-a-c40-tree-store-three-times-slower-than-silicon.md`.
6. Wear-to-death: only if the ADR wants an endurance figure for this part.

## Owed from this run

- **Cycle 2,956's cut is not counted.** The laptop left mid-cycle, and the
  driver stopped ("CX1's port did not come back") before it wrote the record.
  Whether that cut was followed by a power-on reset is unverified. The board,
  and the port's power state on the hub, were not seen again. On reattach:
  - check `board list`;
  - run `board power-on c6-expendable` if the port reads off;
  - expect the first boot to say `cause=power-on` and `fs=mounted`.
- The perf sitting and the 44 cycles, as above.

## G2's questions

1. Adopt the tree store and plan the adoption round (migration, repartition
   to 128, the ADR, the default)?
2. Must anything change first? The candidates: the lazy format (already
   required), and the mount's 0.56 s on a used partition.
