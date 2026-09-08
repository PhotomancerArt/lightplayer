# The ESP32-C6 hardware walk, run on an emulator

**Date** 2026-09-08 · **Plan** `2026-09-06-1001-esp-emulator` (M8, acceptance
item 8) · **Command** `just walk-esp32c6-emu` ·
**Script** `scripts/emu/m4-walk.sh` · **Twin of**
`scripts/m4-hardware-walk.sh --chip esp32c6`

This is the record of the first time LightPlayer's hardware walk was answered
without hardware. Read the two sections at the end — **What this does not
cover** and **The constants nobody has named** — before quoting anything from
the middle.

---

## 1. What the walk asks, and what it got

The hardware walk asks a device one question: *does the shader you compiled
and executed on your own JIT render the same bytes a host render produces?*
`projects/test/shader-oracle` is clock-free precisely so the comparison needs
no time synchronisation, and `lp-app/lpa-server/tests/shader_oracle_frame.rs`
renders it twice on the host — once through wasmtime, once through
`lpvm-native`'s rv32 code generator under `rt_emu` — so which engine a device
byte agrees with is the whole triage.

On the two Xtensa chips the rv32 engine is that code generator one ISA over.
**On the C6 it is the same code generator on the same ISA**, which changes
what a disagreement means and is written into both scripts' triage text.

The emulator answers the question **twice**, where a board answers once:

| reading | what it is | what it proves |
|---|---|---|
| `[OUT] dump … rgb=` | the firmware's own record of the bytes it handed the WS281x driver, printed over the serial link | the render produced these bytes |
| the pad | the WS281x waveform the RMT model actually drove onto gpio18, decoded back at the datasheet's ±150 ns by a decoder that never spoke to the firmware | these bytes left the chip |

The first is what a board gives you (`frame-dump`, ported to this chip in M8
for exactly this purpose — `lp-fw/fw-esp32c6/src/output/rmt/frame_dump.rs`,
code identical to the S3's and the classic's, guarded by
`lp-fw/fw-tests/tests/frame_dump_parity.rs`). The second is what only an
emulator gives you. **The walk's claim rests on the two agreeing with each
other and with the oracle.**

### The run

`just walk-esp32c6-emu`, 2026-09-08, on the tree at `194344c33`, ROM-up from
a merged image with sha256 `0f7c4523…6eac6` (espflash 3.3.0):

```text
===== COMPARISON =====
  pad 18: 1110 frame(s) decoded, 1109 lit, 1 distinct lit frame(s)
PASS: the frame is byte-identical on all three readings (384 hex chars).
  [OUT] dump == pad 18 == [ORACLE] rgb

emu: esp32c6 rom-up boot, grade lp-emu:esp32c6:t1, usb-serial-jtag on 127.0.0.1:5597
emu: reached its deadline — 8000000 us emulated, 857789067 instructions
emu: no unmapped accesses
```

The three readings, in full:

| reading | value |
|---|---|
| `[OUT] dump frame=31` | `crc=0x55772254`, `rgb=324a0208376a1c28…4c2d05` |
| pad 18, first lit frame (`n=1`, at 1.973 s of emulated time) | 64 LEDs, 1536 bits, 0 errors, signal `RMT_SIG_0`, `rgb=324a0208376a1c28…4c2d05` |
| `[ORACLE] rgb` (wasmtime) | `crc=0x55772254`, `rgb=324a0208376a1c28…4c2d05` |
| `[ORACLE-RV32] rgb` (`lpvm-native` rv32 under `rt_emu`) | identical — `[ORACLE-DIFF] 0 differing bytes of 192` |

Four things in that block are worth more than the PASS.

**`1 distinct lit frame`** of 1,109. The project is clock-free, so a render
that is a function of the project alone must produce one frame forever; the
walk asserts that rather than assuming it, because comparing a single frame
to the oracle when the frames differ would be luck rather than evidence.

**The two readings are of different things.** `[OUT] dump frame=31` is the
firmware's record of what it handed the driver, taken 30 frames after the
first lit one (the deferral is on purpose — a dump printed into the
post-compile log burst is dropped end to end). The pad's frame is `n=1` at
1.97 s, decoded from a waveform. They agree, so what the render produced is
what left the chip.

**`no unmapped accesses`** over the whole boot, upload, compile and render.
An address no peripheral claims reads zero and the guest believes it; a run
that ends well with a hundred of those has told you less than it looks like.

**`[ORACLE-DIFF] 0 differing bytes of 192`.** The two host engines share
nothing below the project file, and one of them is the same code generator
the C6 itself JITs, on the same ISA.


## 2. Every gate, beside silicon's figure

| # | gate | result |
|---|---|---|
| **G8-1** | `just walk-esp32c6-emu` green end to end | **MET** — PASS above, from a clean tree, ROM-up |
| **G8-2** | the frame dump equals the pin decoder's frame | **MET** — 192 of 192 bytes, and both equal the oracle |
| **G8-3** | heap figures equal the last walk's for the same commit | **MET** — see below |
| **G8-4** | `just heap-budget-check` passes with the emulator as the C6 source | **MET** — `heap-budget-check-chips` green; the required job's `check` prints a named SKIP with no image |

G8-3 in figures. `just heap-budget-check-chips` on this tree, beside the
committed silicon capture (`boot-idle-flash`, `735af98ae`) and beside what
M6 P5 measured on the emulator at that commit:

| figure | this tree (emulator) | M6 P5 (emulator, `735af98ae`) | silicon (`735af98ae`) |
|---|---:|---:|---:|
| `totalBytes` | 325,536 | 325,536 | 325,536 |
| `usedBytes` | 60,432 | 60,432 | 60,440 |
| `freeBytes` | 265,104 | 265,104 | 265,096 |
| `largestFreeBlock` | 198,876 | 198,876 | 198,886 |
| `[stack]` high-water | 11,908 B | 11,908 B | 11,908 B |
| of a stack of | 71,152 B | 71,512 B | 71,512 B |

The first four are byte-identical between this tree's image and the one the
transcripts were recorded against, which is what G8-3 asks. The stack TOTAL
moved 360 B with the tree — a linker fact, gated exactly on today's value —
and the high-water did not move at all. The gap to silicon is §7.1's eight
bytes, unchanged.


## 3. Timing: what the numbers are, and why none of them is a gate

Plan decision PD9 and vision D13: **no host gate runs on emulated
microseconds.** M8 moved heap gates only. This section is why.

### What the `timing` class actually carries

`slice_cycles` is the guest's own `mcycle` delta across a compile slice.
`slice_us` is that divided by a **fixed 160 MHz** — on both sides, and by the
firmware, not by the harness. So `slice_us` is not a wall clock and never was;
it is `slice_cycles` in different units. Both transcripts' 92 points confirm
it: the implied MHz is 160.0 on silicon (median, range 160.0–161.0) and 160.0
on both emulated grades.

That matters because the plan carried a puzzle about it. G3 sitting 1 (PR
#564) measured silicon's `mcycle` on UART-drain ticks implying ~27.6 MHz
effective while `slice_us` reported the 7 ms floor. Both are true and they are
about different things: `mcycle` on a *drain tick* counts a core that is
mostly waiting, and `slice_us` on a *compile slice* counts one that is not.
Neither is a promise about the other, and no grade-3 claim can be built on
either until someone measures a slice against an external clock.

### The 92-point curve, silicon against the emulator

`lp-emu/transcripts/esp32c6/shader-compile-stress/`, same commit
(`735af98ae`), same image, 92 paired compile slices:

| | silicon | `lp-emu:esp32c6:t1` | `lp-emu:esp32c6:t2` |
|---|---:|---:|---:|
| total `slice_cycles` | 21,212,389 | 88,153,417 | 89,451,035 |
| total `slice_us` | 132,530 | 550,896 | 559,028 |
| worst single slice | 10,008 µs | 10,676 µs | 11,345 µs |

Per-slice ratio, silicon ÷ emulator:

| | min | p25 | median | p75 | max |
|---|---:|---:|---:|---:|---:|
| `t1` `slice_cycles` | 0.03 | 0.04 | 0.16 | 0.50 | **4.97** |
| `t2` `slice_cycles` | 0.03 | 0.04 | 0.16 | 0.50 | **3.57** |

**Read that spread, not the total.** In aggregate the emulator spends about
4.2× silicon's cycles, which sounds like a usable constant. Per slice it
ranges over more than two orders of magnitude and *changes sign*: on the
median slice the emulator is ~6× slower, and on at least one slice silicon is
5× slower than the emulator. There is no scale factor. A budget that passes
here can fail on the board and vice versa — which is exactly what the
2026-09-06 spike found of esp-emu, byte-equal on memory and 2.4× wrong on
time in the same run, with the compile harness's own 5 ms slice budget passing
under the emulator and failing on the board.

### And yet the memory columns are identical

The same 92 rows carry `mem_before` and `mem_after`. **All 92 pairs match on
both grades, byte for byte, on both sides.** One transcript, one pair of
columns: the memory is silicon's and the clock is not. That is the whole
argument for where the gates were moved and where they were not, in one file.

## 4. The heap gates now read from the emulator

`scripts/heap-budget-record.json` grew a `chips` section, and
`scripts/heap-budget-check.sh` grew the reader for it
(`just heap-budget-check-chips`). See `docs/heap-budget-gate.md`, "The second
source", for the full rules; the short version:

- The **shipped** image (`esp32c6,server,radio` — the bytes a board is flashed
  with) boots on `lp-emu:esp32c6:t1` to its first heartbeat, and the gate reads
  the allocator figures the firmware itself reports.
- `totalBytes` and `stackTotal` are **exact**; `usedBytes` ratchets on growth;
  `freeBytes` and `largestFreeBlock` ratchet on shrinking; `stackHighWater` is
  a **band**, for the reason in §6.
- The record also carries `silicon_reference` — the same figures from a
  committed silicon transcript, with its commit — **never gated**, printed on
  every run, so the gap in §6 cannot widen unseen.
- It runs in CI's path-gated `emu-c6` job, which builds firmware anyway, and
  not in the required job, which must not start a cross-target build. The
  required job prints a named SKIP.
- It uses the **direct load**, not the ROM-up boot: M7's G7-4 measured the two
  paths' idle heap byte-identical, so the bootloader would add seconds of wall
  clock and nothing to the answer.

```text
heap-budget: booting esp32c6 (esp32c6,server,radio) on lp-emu:esp32c6:t1
  ok: totalBytes: 325536          ok: usedBytes: 60432
  ok: freeBytes: 265104           ok: largestFreeBlock: 198876
  ok: stackTotal: 71152           ok: stackHighWater: 11908 B (band 11400..12500)
  silicon reference (735af98ae, boot-idle-flash):
    freeBytes: silicon 265096 / this tree 265104
    usedBytes: silicon 60440  / this tree 60432
    totalBytes: silicon 325536 / this tree 325536
    largestFreeBlock: silicon 198886 / this tree 198876
```


## 5. ROM-up or direct load: which path the walk runs, and why

Both exist and M7 proved them equivalent where it matters — 2,412,746 B of app
segments byte-equal at app entry, and an idle heap identical to the byte.

**The walk boots ROM-up**, from a merged 4 MiB image: the hart starts at
`0x4000_0000`, the real mask ROM detects the chip and reads the ESP-IDF
second-stage bootloader out of flash, and the bootloader reads the partition
table, hashes the image and loads the app. That is the closer twin of a script
whose first act is to flash and reset a board — the walk's whole premise is
that as little as possible differs from the hardware run, and "the app was
already in memory" is not nothing. It also means the walk exercises the boot
chain on every run, which is the only routine exercise that chain gets.

**The per-tick gates use the direct load**, and so does the heap gate: they
run on every emulator PR, the bootloader adds seconds of wall clock, and M7
measured that it adds nothing else. `LP_WALK_BOOT=direct` takes the same path
in the walk for a fast iteration.

This answers the ADR's own open question
(`docs/adr/2026-09-06-esp-soc-emulator-architecture.md`, now Accepted): ROM-up
for the walk, direct load for the gates.

## 6. What this does not cover

A harness that overstates its fidelity is worse than none. The emulator does
**not** model:

| not covered | consequence |
|---|---|
| **The Chromium USB stack** | Studio talks to a board through Web Serial in a browser. The walk's link is a TCP socket carrying the same bytes; every fault that lives in the browser's serial implementation, in permission grants, or in a replug is invisible here. `docs/debt/studio-no-reconnect-after-replug.md` is that category. |
| **Radio** | ESP-NOW is an accept-and-remember stub. The C6's known frame truncation under a WiFi scan (`docs/debt/c6-scan-truncation-accepted.md`) cannot reproduce here — there is no scan. |
| **Anything analog** | No PHY, no PLL settling, no brownout, no temperature. `regi2c` answers from one shared data byte (§7). |
| **RX pins and input** | The RMT has TX channels only; GPIO is a routing view. A button, an encoder, an incoming DMX universe: none of it. |
| **Wall-clock time** | §3. Time grades 1 and 2 are event schedulers, not clocks. Grades 3 and 4 are out of plan one's scope by design. |
| **The board** | Power, wiring, connectors, the strip itself, and every fault that is really a loose wire. |

**What the walk replaces is the routine C6 walk** — the one run to check that
a render still renders after a change to the engine, the compiler or the
driver. It does not replace a desk sitting, and a change touching any row of
that table still needs one.

## 7. The constants nobody has named

Three, each with a filed home. None blocks anything; all three are stated here
so a reader of a green walk knows what green does not mean.

### 7.1 Eight bytes of heap

Every capture, every commit, both links, both boot paths: the emulator's idle
heap reports `freeBytes` **+8** and `usedBytes` **−8** against silicon.
Everything else agrees — `totalBytes`, `largestFreeBlock`, the stack
high-water, and the filesystem backing's cost (1,296 B on both machines to the
byte). Three explanations have been tested and refuted: sampling (M6 P4 keyed
the series on the 5 s tick), the board's boot history (M6 P5 sampled boot 3 and
boot 10), and the loader (M7 booted ROM-up and got a byte-identical ledger).

Silicon has one live 8-byte block we do not, and it is there before the first
heartbeat. Naming it needs a **power-on** capture — every silicon transcript
in the tree was taken after a reset, and a reset is not a power-on. That needs
a hand on a cable.
`docs/debt/emulator-heap-ledger-differs-from-silicon-by-eight-bytes.md`.

### 7.2 The reference image is reproducible on one host, not between two

L4 (PR #591) pinned everything a build can be told: `SOURCE_DATE_EPOCH=0` for
`esp_app_desc!()`'s timestamp, `--remap-path-prefix` for the absolute paths in
`.debug_str`, and a rebuild-once guard for a cold-tree race that links the
stock `rodata.x`. Two builds on one host now produce identical sha256s.

Between hosts they differ by ~4.7 KB, and the residual is the **rustc binary's
own host build** — same nightly hash, different `.text`. The code lands at
different addresses. M7 found the same drift in *layout*: this tree's ELF at
the transcript's commit links `.rodata` 0x20 higher, so espflash splits the app
into six segments where silicon's had five.

Every **memory-class** figure survives that and stays exact. Every figure that
depends on where the code is does not, and is asserted as a **band with a
digest**: the `[stack]` high-water is the deepest point an interrupt ever
landed on the main task, and a tick landing on a different instruction of a
differently laid-out image has a different deepest point (11,432 B here,
11,560 B on a GitHub runner). Wherever this report quotes a stack or
instruction figure, that is the host it came from and not a portable number.

Closing the gap means one build environment for the reference images — a
pinned container — which is an infrastructure decision with a per-PR cost, and
it was raised with this milestone's report rather than taken inside it.
`docs/debt/reference-images-are-not-reproducible-across-hosts.md`.

### 7.3 `regi2c` is one data register, not a register file

The mask ROM's `wait_rfpll_cal_end` polls an analog register that the
`I2C_ANA_MST` accept block answers with a single shared `data` byte, so a
ROM-up boot prints three `pll_cal exceeds 2ms!!!` lines that neither silicon
nor the direct load prints. Filed, not fixed: the fix changes what every
`regi2c` read in every configuration answers, and it is the same lab task as
the accept-block reset-value sweep.
`docs/defects/2026-09-08-regi2c-is-one-data-register-not-a-register-file.md`
and `docs/defects/2026-09-07-accept-blocks-carry-only-the-reset-values-a-boot-needed.md`.

## 8. What every milestone deviated on, in one place

The walk stands on eight milestones; each left something the walk's reader
should know. Full detail is in each `m*-_DONE.md` in the plan directory.

| milestone | the deviation a walk reader needs |
|---|---|
| M1 | the fence is a crate **allowlist**, not a directory blacklist; the ISA/ELF crates stay AGPL, so the MIT unit is not externally self-contained (Yona's G1 ruling). |
| M2 | transcript sidecars are **required and authoritative**; `--commit` is stated by the operator, never sniffed from HEAD — an image on a board is often older than the checkout. |
| M3 | the memfs idle heartbeat's 104 B against esp-emu was a documented deferral, later explained as sampling (M6 P4). The `[stack]` gate became a band (DD45). |
| M4 | the SPI1 accept block's reset values were wrong in a way no boot checks — an accept block's reset values are facts, and the sweep for the rest is still open. |
| M5 | the **pin grade stays `modeled`** even with the oracle equality: the RMT model produces the waveform and our decoder reads it, so both readings of the pad are ours. `measured` needs an instrument. The `LedChannel` harness double-swaps colour order; the shipped path does not. |
| M6 | `usb-serial-jtag` stays `modeled` with the silicon transcript landed — byte-equality is evidence in the reason, not a promotion. |
| M7 | `--reboot-on-reset` is **off** by default; `Saved PC:` is subtracted from silicon's side of the boot-log diff; three `pll_cal` lines (§7.3); the six-vs-five segment split (§7.2). |
| M8 | this report. The walk is one run where the hardware walk is two flashes — espflash's `--monitor` holds the port and a socket does not — so the "project survives a second boot" half of the hardware walk's round 2 is `tests/flash_persistence.rs`'s gate, not this walk's. |

## 9. Not run

- **The esp-emu loopback differential** (the plan's G4-3). esp-emu 0.42.0 is
  not installed on this host and fetching the release asset is an action the
  agent lane could not authorize on its own; the recipe is in the plan's
  `notes.md`. It would be a *second* oracle for the frame, not the gate — the
  gate is the host oracle, and it is met.
- **A silicon `rmt-chase` capture** and a **silicon pin transcript**. The
  second is the `measured` step for the pin class (§8, M5). Both are cheap
  while the C6 is on the bench and neither gates anything.
- **The `bootCount 1` power-on capture** (§7.1).

## 10. Reproducing this

```bash
just walk-esp32c6-emu            # the whole thing, from a clean tree
just walk-esp32c6-emu --keep     # …and leave the artefacts in target/lp-emu-c6-walk/
LP_WALK_BOOT=direct just walk-esp32c6-emu    # the fast boot path
```

The artefacts are `walk.console.txt` (everything the device said),
`walk.frames.jsonl` (every frame decoded off a pad, `wire` and `rgb` both),
`merged.bin` with its `.sha256` and `.provenance`, and the ELF the walk built.
The hardware twin is `scripts/m4-hardware-walk.sh --chip esp32c6`, which wants
a board — resolve it by MAC (`LP_BOARD_MAC=…`, or
`scripts/emu/board-port.py --list`) rather than letting a probe pick, because
two C6s on one bus are indistinguishable by port name.
