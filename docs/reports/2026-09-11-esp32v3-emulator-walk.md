# The classic ESP32 hardware walk, run on an emulator

**Date** 2026-09-11 · **Plan** `2026-09-10-0021-xtensa-emulator` (M5,
acceptance item 3) · **Command** `just walk-esp32v3-emu` ·
**Script** `scripts/emu/m4-walk-esp32v3.sh` · **Twin of**
`scripts/m4-hardware-walk.sh --chip esp32`

This is the record of the second chip whose hardware walk LightPlayer can
answer without hardware, and the first one on Xtensa. Read the two sections at
the end — **What this does not cover** and **The constants nobody has named**
— before quoting anything from the middle.

Its companion is `docs/reports/2026-09-08-esp32c6-emulator-walk.md`, and the
two lists of what is not covered are **not the same list**. The classic is not
wrong about the same things the C6 is.

---

## 1. What the walk asks, and what it got

The hardware walk asks a device one question: *does the shader you compiled
and executed on your own JIT render the same bytes a host render produces?*
`projects/test/shader-oracle` is clock-free precisely so the comparison needs
no time synchronisation, and `lp-app/lpa-server/tests/shader_oracle_frame.rs`
renders it twice on the host — once through wasmtime, once through
`lpvm-native`'s rv32 code generator under `rt_emu`.

On the classic the device's own engine is that rv32 code generator **one ISA
over**: the guest JITs Xtensa, the oracle's second engine emits RV32. So a
disagreement between the device and the oracle would be a code-generator
question, and an agreement is two independent back ends landing on the same
192 bytes.

The emulator answers the question **twice**, where a board answers once:

| reading | what it is | what it proves |
|---|---|---|
| `[OUT] dump … rgb=` | the firmware's own record of the bytes it handed the WS281x driver, printed over UART0 | the render produced these bytes |
| the pad | the WS281x waveform the RMT model actually drove onto IO18, decoded back by a decoder that never spoke to the firmware | these bytes left the chip |

The first is what a board gives you (`frame-dump`, whose code is identical on
all three chips and guarded by `lp-fw/fw-tests/tests/frame_dump_parity.rs`).
The second is what only an emulator gives you. **The walk's claim rests on the
two agreeing with each other and with the oracle.**

### The run

`just walk-esp32v3-emu --keep`, 2026-09-11, on the tree at `095d46b90` — this
branch, with every commit after that one being documentation. The commit id is
linked **into** the firmware, so a run on a later commit is a different
instruction stream and reaches a different frame count in the same 10 s of
emulated time. **The bytes of the frame do not move**: the same 384 hex
characters came off IO18 at `e201a2160` (2,437 lit frames), at `9fcbe87fa`
(3,728) and here (3,772). ROM-up from a merged 4 MiB image with sha256
`a9f9882dbb30f886818e78f16ea63b5ea532761e031d98888838104c42c39b9a`
(espflash 3.3.0), quantum 256. The firmware says which tree it is out of its
own mouth: `commit=095d46b90fe7 dirty=false`.

```text
===== COMPARISON =====
  pad 18: 3774 frame(s) decoded, 3772 lit, 1 distinct lit frame(s)
PASS: pad 18 == [ORACLE] rgb (384 hex chars), 3772 lit frame(s),
      all of them the same bytes.
PASS: the frame is byte-identical on all THREE readings (384 hex chars).
  [OUT] dump == pad 18 == [ORACLE] rgb

run: cycles=2400000000 instructions=1685848322 (core0=1290106945 core1=395741377) idle=2428405 unmapped=0 (reads 0, writes 0, 0 sites) fence=3634 quantum=256
pin gpio18: 3774 frames, 3773 complete, 0 errors, 29 leds, 11592056 edges
```

The three readings, in full:

| reading | value |
|---|---|
| `[OUT] dump frame=31` | `crc=0x55772254`, `rgb=324a0208376a1c28…4c2d05` |
| pad 18, first lit frame (`n=1`, starting at 0.805 s of emulated time) | 64 LEDs, 1536 bits, 0 errors, signal `RMT_SIG_0`, same 384 hex characters |
| `[ORACLE] rgb` (wasmtime) | `crc=0x55772254`, same 384 hex characters |
| `[ORACLE-RV32] rgb` (`lpvm-native` rv32 under `rt_emu`) | identical — `[ORACLE-DIFF] 0 differing bytes of 192` |

Five things in that block are worth more than the PASS.

**`1 distinct lit frame`** of 3,772. The project is clock-free, so a render
that is a function of the project alone must produce one frame forever; the
walk asserts that rather than assuming it, because comparing a single frame to
the oracle when the frames differ would be luck rather than evidence.

**The two readings are of different things, and 30 frames apart.**
`[OUT] dump frame=31` is the firmware's record of what it handed the driver,
deferred on purpose — a dump printed into the post-compile log burst is
dropped end to end — while the pad's frame is `n=1` at 0.805 s, decoded from a
waveform. That they agree across a 30-frame gap is only meaningful *because*
the project is clock-free: on a project whose output moved with time this
comparison would be illegitimate, and the "1 distinct lit frame" assertion is
what establishes that it is not.

**`unmapped=0`** over the whole boot, upload, compile and render. An address
no peripheral claims reads zero and the guest believes it; a run that ends
well with a hundred of those has told you less than it looks like.

**Two cores ran, and the console says so without being asked.**
`[INIT] RMT ISR on APP core` is the line silicon prints; the single-core
fallback prints `[INIT] APP core unavailable; RMT ISR on PRO core (single-core
semantics)` instead. `core1=395741377` instructions is the same fact with a
number on it.

**`3774 frames, 3773 complete`.** The deadline is a cycle and it fell in the
middle of the last wave, so one frame is cut. 3,774 decoded − 1 dark (`n=0`,
the open-time black fallback) − 1 incomplete = the 3,772 lit the PASS counts.
Nothing is dropped; the tail is where a run ends.

### The cable, which the C6 has no equivalent of

The classic's link is UART0 through a CH340K bridge chip on the board, so the
walk drives a *cable*, and it does it from the file rather than from a human's
head. Both halves, quoted from the same run:

```text
===== CABLE =====
attach           ok attach cyc=3360000 us=14000
reset            ok reset cyc=3600000 us=15000
open             ok open cyc=240000 us=1000
state            ok state cyc=480000 us=2000 cable=attached port=open dtr=0 rts=0 en=1 io0=1 strap=app reboots=1

===== UPLOAD =====
Project uploaded and running.
upload: OK

===== CABLE (released) =====
close            ok close cyc=303600000 us=1265000
detach           ok detach cyc=303840000 us=1266000
state            ok state cyc=304080000 us=1267000 cable=absent port=closed dtr=0 rts=0 en=1 io0=1 strap=app reboots=1
PASS: the port is released, the lines are slack, and the cable
      rebooted the chip exactly once.
```

Read the cycles: `reset` lands at 3,600,000 and `open` at 240,000, because the
reset's **release** rebooted the chip and the guest clock went back to zero.
The final `state` is asserted rather than admired — `port=closed`,
`cable=absent`, both modem lines slack, `reboots=1`.

The upload is a real `lp-cli upload` over the socket, not a replay: fifteen
`M!{…}` replies came back over the one connection in this run, ids 0 through
11 in order. That the link keeps answering *after the boot settles* is not
free: until M4 P3b the hart's poll point re-sampled the wrong form of the
interrupt question and zeroed its own asserted mask on every MMIO store, so
esp-rtos's executor lost the doorbell it had just rung and a scripted request
after `boot complete` was never answered.


## 2. Every gate, beside silicon's figure

The plan's acceptance criteria, each with what it actually got.

| # | criterion | result |
|---|---|---|
| **1** | the shipped image boots both paths, prints its `[INIT]` chain over UART0, answers on the socket, and its idle heartbeat matches a committed silicon `boot-idle` capture **on every memory-class field** | **MET except two named fields.** Seven of nine equal to the byte; `[MEM] used` +84 B and `[stack]` high-water −480 B. Both pinned, neither masked. **E2 is open** — see below |
| **2** | core 1 starts through DPORT, binds the RMT ISR, runs the wire pusher; the first lit frame equals the firmware's `frame-dump` line and the host oracle byte for byte, and every later frame is the same bytes | **MET** — #707: 2,437 lit frames, one distinct; and again here, 3,772. Beside it, **five wires over four channel slots**: IO18, IO16, IO14, IO2 and IO13 each carry their own frame, IO18 and IO13 **time-share `RMT_SIG_0`** with a `GPIO_OUT` park between waves, every per-wire checksum matches a line the guest printed, 0 bit errors, and the whole set is byte-identical at quantum 256 and 64 |
| **3** | `just walk-esp32v3-emu` runs ROM-up with no unmapped access and passes the walk's own criteria; the heap gates read from the emulator; `lp-emu:esp32v3:t1` is a configuration with committed transcripts, every class `modeled` with its evidence | **MET** — the run above; §4 below; `lp-cli validate list`. `t2` does not exist on this machine (E1) |
| **5** | every register that answers is graded; the cache-off fetch stop is on for every gate run; no host gate runs on emulated microseconds | **MET** — every peripheral view carries per-register grades (`--strict-grade`); `--cache-off-fetch` defaults to `stop` and no gate run passes `permit`; the walk runs `--strict-bus`; and §3 |

### The memory comparison, field by field

From the committed pair — `lp-emu/transcripts/esp32v3/boot-idle/`, both sides
at commit `c976f17a9`, **both sides the same ELF**
(`670b7bf1…a1f9` stated by both sidecars), both ROM-up, both a **second**
boot, both on the same elicited stimulus (`walks/v3-stop-all.script`). Replayed
by `lp-emu/lp-emu-validate/tests/v3_replays.rs` on every run of the gate.

| field | silicon | `lp-emu:esp32v3:t1` | gap |
|---|---:|---:|---|
| boot heap arithmetic `15072+112640+98304+15536` | 241,552 | 241,552 | **0** |
| `[INIT] main stack` | 45,280 B | 45,280 B | **0** |
| JIT region line (`sram0 ibus 0x40088000..0x40098000`, `rwtext_end=0x40083f78`) | identical string | identical string | **0** |
| `[JIT]` census (all nine fields) | `used=0 peak=0 cap=65536 spans=0 peak_spans=0 allocs=0 frees=0 fails=0 largest_free=65536` | identical string | **0** |
| `[MEM] largest_free` | 108,526 | 108,526 | **0** |
| `[MEM] retry_saves` | 0 | 0 | **0** |
| `[MEM] free` | 223,352 | 223,268 | −84 B |
| **`[MEM] used`** | **18,200** | **18,284** | **+84 B** |
| **`[stack]` high-water** | **16,972** | **16,492** | **−480 B** |
| `[stack]` headroom | 28,308 | 28,788 | +480 B (the same gap, mirrored) |

`free` and `used` move the same 84 bytes in opposite directions, which says
this is **one arena partitioned differently** rather than two arenas of
different sizes.

The replay's own verdict, which is a failure **on purpose**:

```text
  class             compared     equal    differ
  memory                   2         0         2
  wire                     1         1         0
  structural               6         6         0

  REPLAY FAILED (2 problem(s)):
    memory field stack-heartbeat[45280].high_water differs: 16492 vs 16972
    memory field stack-heartbeat[45280].headroom differs: 28788 vs 28308
```

⚠️ Read that "memory 2 compared" carefully: the replay comparator's series for
this payload are the stack pair, so **the +84 B is not the replay's to fail
on**. It is pinned by `v3_replays.rs`'s own field-by-field test, which parses
both `[MEM]` lines and asserts `used` is exactly 18,284 against 18,200 and
`free` moves exactly 84 the other way. Two mechanisms, both exact.

Nothing is masked and no threshold is widened. Both gaps are pinned to their
exact values and **a move in either direction fails**, including a move that
closes one: a gap that closed is a finding to be re-read, not a test that
quietly goes green.

The other three payloads of the same sitting, same-bytes pairs, same replay:

| payload | verdict |
|---|---|
| `shader-compile-stress` | **REPLAY OK** — **372 memory comparisons, all equal**; 190 structural, all equal; 188 timing differ and are not gated |
| `gpio-calibrate` | **REPLAY OK** — header and `CAL READY target=esp32v3` on both |
| `cycle-probe` | **REPLAY OK** — 360 structural, all equal; 160 CCOUNT figures recorded and gated nowhere |

### What this walk's tree measures, beside silicon's capture

`just heap-budget-check-chips-v3` on the tree this record was written from:

```text
heap-budget: booting esp32v3 (esp32,server,float-f32) on lp-emu:esp32v3:t1
  ok: totalBytes: 241552            ok: usedBytes: 17044
  ok: freeBytes: 224508             ok: largestFreeBlock: 108526
  ok: stackTotal: 45280             ok: stackHighWater: 16060 B (band 15500..16600)
  silicon reference (c976f17a9, boot-idle):
    freeBytes: silicon 223352 / this tree 224508
    usedBytes: silicon 18200  / this tree 17044
    totalBytes: silicon 241552 / this tree 241552
    largestFreeBlock: silicon 108526 / this tree 108526
```

⚠️ **Those silicon rows are printed and are not comparable**, and the record
says so on every run rather than letting a reader do the subtraction. Two
confounders sit on them: the gate boots a **direct load with no flash chip**
so the firmware falls back to the memory FS, and it is a **different build**
from the transcript's. The comparable numbers are the table above, where both
sides ran the same ELF over the same flash. `usedBytes` −1,156 B here is **not
E2's +84 B**, and reading it as such is the mistake the `gap_note` exists to
prevent.

### E2, open, and what this record assumes while it is

**This section is written on the director's lean, as a stated assumption, and
Yona rules on it at G-classic.** The lean:

- `[MEM] used` is a **pinned, reported constant** (+84 B) — the classic's
  version of the C6's eight bytes: named, never widened, attributed to
  nothing yet;
- the `[stack]` high-water is graded **`modeled` with a band**, because
  interrupt arrival decides it on silicon too;
- acceptance line 1 then reads *"every memory-transfer field equal; the
  pinned constant and the stack band named"*.

Three explanations have been tested and refuted for the +84 B: **sampling**
(L1 tightened the host's send from a 50 ms poll to a 1 ms one; both sides
repeat deterministically), **the boot path** (direct and ROM-up give the same
+84 B), and **the second core** (M4 P1 brought core 1 up and the gap did not
move, so it is not core 1's). DD48's one carry is spent.

**Nothing here presents the gaps as closed, and the lean is not a decision.**


## 3. Timing: one grade, and why none of it is a gate

Plan decision PD9 and vision D13: **no host gate runs on emulated
microseconds.** M5 moved heap gates only.

### This machine has one time grade, and that is a statement

The C6 has three (`t1` counts instructions, `t2` uses a per-class cost model,
`t3` adds what an access's address costs). **The classic has `t1` and only
`t1`.** `TimeGrade` on `lp-emu-esp32v3` has one arm and the binary's
`--time-grade` accepts one word.

That is an **inconsistency with the C6, stated rather than papered over**
(M5 ruling R1; Yona's answer to E1 on 2026-09-11: *"it doesn't have to be that
perfect, but we should note inconsistency"*). There is no measured LX6
per-class cost model to calibrate a second grade against, and a `t2` that was
`t1` under another name would be exactly the dishonesty the trust table exists
to prevent. **`t2` is named future work**, and its first problem is not the
cost model: it is DD40's gap, where before the app reprograms the PLL the
guest's notion of the clock rate and ours differ — visible in the bootloader's
timings and in nothing after them.

### How wrong it is, in the two places that measured it

`lp-cli validate replay`, emulated against silicon, same commit, same bytes:

| | emulator (`t1`) | silicon | ratio |
|---|---:|---:|---:|
| `shader-compile-stress` total `build_us` | 70,979 | 272,835 | **0.26×** |
| its worst single slice, `worst_slice_us` | 2,304 | 20,626 | **0.11×** |
| `cycle-probe[0].cycles` (the first sample) | 18 | 2,910 | **0.01×** |
| `cycle-probe[1..4].cycles` (steady state) | 18 | 33 | **0.55×** |

Read the sign before the size. **On the classic the emulator reports fewer
microseconds than silicon takes**, which is the opposite direction from the
C6's record — there, `t1` spent about 4.2× silicon's cycles. Two chips, two
signs, one grade name. Anybody building a budget on either number is building
it on the wrong thing.

`cycle-probe` also shows exactly *what* is unmodelled at `t1`: the first
sample costs silicon 2,910 cycles and the next four cost 33, because the first
one pays the flash cache's cold fill. The emulator charges 18 for all five.
And what the desk sitting established about the counter itself is worth
keeping, because it is a silicon fact rather than a model's: **240 cycles per
microsecond**, and `iram_loop == flash_loop` once the loop is hot.

Those 160 CCOUNT figures are **recorded in the transcript and compared by no
gate**. They are the future grade's input, not a claim about milliseconds.

### And the memory columns are identical

`shader-compile-stress` carries both halves in one file: **188 timing
comparisons differ and 372 memory comparisons are equal, all of them** —
`mem_before_free`, `mem_before_used`, `mem_after_free` and `mem_after_used`,
92 samples each, ratio 1.00× on every one. One transcript, one pair of
columns: the memory is silicon's and the clock is not. That is the whole
argument for where the gates were moved and where they were not.


## 4. The heap gates now read from the emulator

`scripts/heap-budget-record.json`'s `chips` section grew an `esp32v3` entry
and `scripts/heap-budget-check.sh` learned a second chip
(`just heap-budget-check-chips-v3`). The rules are the C6's:

- the **shipped** feature set (`esp32,server,float-f32`) boots on
  `lp-emu:esp32v3:t1` and the gate reads the allocator figures the firmware
  itself reports;
- `totalBytes` and `stackTotal` are **exact**; `usedBytes` ratchets on growth;
  `freeBytes` and `largestFreeBlock` ratchet on shrinking; `stackHighWater` is
  a **band**, for the reason in §7.2;
- `silicon_reference` is printed on every run and **gated never**, with the
  `gap_note` above beside it;
- it uses the **direct load**, for the reason in §5.

⚠️ **The classic's first heartbeat has to be asked for**, and this is the one
thing a reader copying the C6's arm gets wrong. `esp32_memory_stats` runs on a
project load, unload, stop-all or a client `runtime_status` — **never on the
five-second server heartbeat**. A classic boot with nobody talking prints no
`[MEM]`, no `[JIT]` and no `[stack] heartbeat:` line at all. The arm sends
`lp-emu/lp-emu-validate/walks/v3-stop-all.script` — the same bytes on the same
trigger as `boot_idle.rs` and as the desk sitting — and stops on the first
`[JIT] used=`.

**Where it runs in CI** is the classic's own job, `Emulator ESP32v3 (x64)`,
and not the C6's heap job (DD71). The reason is structural rather than
wall-clock: the classic's ELF is an Xtensa cross-build and the heap job
installs no Xtensa toolchain, and that job's path filter never fires for
`lp-fw/fw-esp32v3/**` — so a classic firmware change would never have run its
own heap gate. The classic arm costs **24 s** warm on this desk.


## 5. ROM-up or direct load: which path the walk runs, and why

Both exist. **The walk boots ROM-up**, from a merged 4 MiB image: the hart
starts at `0x4000_0000`, the real mask ROM detects the chip and reads the real
ESP-IDF second-stage bootloader (`v5.1-beta1-378-gea5e0ff298`, espflash
3.3.0's bundle, which is the desk board's own) out of flash, and the
bootloader reads the partition table and loads the app. The log is silicon's,
line for line, including the benign
`E boot: Image contains multiple DROM segments. Only the last one will be
mapped.` that the desk board prints too.

That is the closer twin of a script whose first act is to flash and reset a
board, and it means the walk exercises the boot chain on every run — the only
routine exercise that chain gets.

**The per-tick gates and the heap gate use the direct load**: they run on
every emulator PR and the bootloader adds seconds of wall clock.
`LP_WALK_BOOT=direct just walk-esp32v3-emu` takes the same path in the walk
for a fast iteration, and reaches the same three readings.

⚠️ **The two paths are not identical on every figure, and the one that differs
is named.** Every memory-transfer field is byte-equal between them —
`free`, `used`, `largest_free`, `retry_saves`, the whole `[JIT]` census. The
main stack's high-water is not: `boot_idle.rs::PATH_HIGH_WATER_GAP` is **−64**
(direct 16,460, ROM-up 16,396 on this tree), because the high-water is how
deep an interrupt happened to land and the two paths reach the heartbeat at
different points in the pacer's phase. It is pinned, signed, and re-measured
whenever it moves.

⚠️ **A reset is not a second boot.** The cable's reset restores the power-on
snapshot, and that includes the flash chip, so the boot after a cable reset
formats the same blank `lpfs` the first one did. Silicon's captures are all
second boots — espflash hard-resets after *writing* — and the emulated twin
gets there by running the machine twice over a writable copy, which is what
`ChipArm::second_boot` is. Before that was acted on, the twin's
`largest_free` read 106,494 against silicon's 108,526: **2,032 bytes that were
never a model difference at all**, only a live `lpfs` format in the arena.


## 6. What this does not cover

A harness that overstates its fidelity is worse than none.

| not covered | consequence |
|---|---|
| **Memory ordering** | The two cores run a deterministic quantum interleave on one clock, with cross-core stores visible immediately. That is **one schedule**. A store-buffer race, a write-posting window, any ordering silicon permits that this schedule never exercises: none of it can appear here, and none of it is claimed. What *is* shown is that the result does not depend on the schedule — the five-wire gate runs at quantum 256 and 64 and every shared frame is byte-identical on every pad. Those are different claims. |
| **Wall-clock time** | `t1` only (§3). No measured LX6 cycle model exists. **CCOUNT is recorded and gated by nothing.** DD40's pre-PLL crystal-rate gap is the first thing a future `t2` would have to fix. No figure in this record is a claim about milliseconds on a board. |
| **Radio** | The shipped classic image carries none and none is modelled (Q4). Not a stub, not a gap — the image does not use it. |
| **The unmapped aliases** | SRAM1's I-bus mirror and RTC-fast's I-bus view are **unmapped** (DD3, DD24 R1, DD36): `SocBus` cannot alias two windows onto one store, and two independent stores would disagree silently. An access there is a strict stop, which is what would name a user and buy a real alias. Nothing has. |
| **Any instrument on a pad** | The waveform is a modelled RMT's and the decoder that reads it back is ours — **both readings of the pad are ours**. No logic analyser and no scope has been on a classic pad. That is why `pin` is `modeled` and why `records_pins = true` on our configuration is a statement about capability and not about truth. `silicon:esp32v3` records no pins at all. |
| **The download-mode strap** | The real cable's download-mode row was measured on the desk (whole-status `0x4`, 100 ms, `0x2`, 80 ms, `0x0` → `boot:0x3`, `waiting for download`). **The classic binary has no `--strap` flag**, so there is no emulated twin of that capture to compare it against. Named, not counted. |
| **The Chromium USB stack** | Studio talks to a board through Web Serial in a browser. The walk's link is a TCP socket carrying the same bytes; every fault that lives in the browser's serial implementation, in permission grants, or in a replug is invisible here. |
| **Anything analog** | No PHY, no PLL settling, no brownout, no temperature; drive strength, pull-up *values*, open-drain, pad filters and the input synchroniser are all outside the model. |
| **The board** | Power, wiring, connectors, the strip itself, and every fault that is really a loose wire. |
| **The ESP32-S3** | **Nothing in this record is a claim about it.** `lp-emu-esp32s3` is register tables and a vendored ROM (M6 P01); it has no map, no hart, no peripheral and no boot. |
| **The 32-byte DROM inter-segment gap** | A named exclusion from G2's app-entry state comparison between the two boot paths (DD44), where ROM-up is the one that is right. This walk does not compare app-entry state, so it does not touch it — and it is named here so that a reader who goes looking for "the two paths are identical" finds the one place they are not. |
| **An instruction tracer** | **The machine has none.** The window-underflow dig that fixed #711 ran on a throwaway 60-line hook that was never committed. A reader who expects to reproduce a per-instruction trace from this tree cannot; it is an M8 candidate. |
| **The reference image between two hosts** | §7.2. Reproducible on one host, and 500 bytes apart between two — with, this time, the memory figures identical on both. |

**What the walk replaces is the routine classic walk** — the one run to check
that a render still renders after a change to the engine, the compiler or the
driver. It does not replace a desk sitting, and a change touching any row of
that table still needs one.


## 7. The constants nobody has named

Three, each with a filed home. None blocks anything; all three are here so a
reader of a green walk knows what green does not mean.

### 7.1 Eighty-four bytes of heap, and four hundred and eighty of stack

Every capture, every commit, both boot paths, single core and dual: the
emulator's idle heap reports `[MEM] used` **+84 B** and `free` **−84 B**
against silicon. Everything else in the ledger agrees to the byte — the boot
heap arithmetic, `largest_free`, `retry_saves`, the main stack's size, the
whole `[JIT]` census. Sampling, the boot path and the second core have each
been tested and refuted (§2). Silicon partitions one arena 84 bytes
differently from us and nobody has named the block.

Beside it, the `[stack]` high-water is **480 B shallower** than silicon's on
the like-for-like comparison. This one is *not* a memory-transfer figure: it
is the deepest point an interrupt ever landed on the main task, so it moves
with where in the pacer's phase the heartbeat falls — 64 B between this
machine's own two boot paths, and it moved 112 B when M4 P3b changed *when*
the hart takes its interrupts (with the cause attached, toward silicon).

⚠️ **A correction this record makes.** Until M5 P7 the trust row reconciled
the −480 B with `boot_idle.rs`'s −640 B through that file's cross-path gap,
"16332 + 160 = 16492". **The +160 was the pre-#704 value of
`PATH_HIGH_WATER_GAP`, which has been −64 since M4 P3b re-measured it** —
re-measured again for this record and −64 again. The arithmetic is withdrawn;
the two figures are simply not the same measurement (−640 is a direct load of
the HEAD image, −480 a ROM-up boot of `c976f17a9`'s). **No number moved and no
pin was touched.**

Both gaps are pinned in `lp-emu/lp-emu-validate/tests/v3_replays.rs` and in
`lp-emu/esp/lp-emu-esp32v3/tests/boot_idle.rs`. **E2 is open with Yona** and
G-classic is where it is answered.

### 7.2 The reference image is reproducible on one host, not between two

The classic's reference image is built by
`scripts/emu/build-reference-image.sh --chip esp32` with everything a build
can be told pinned, and `--verify` asserts two builds in two directories
produce one sha256. **Between two hosts they differ**: at `111976fc0`, this
desk's ELF is 2,985,952 bytes and a GitHub `ubuntu-24.04` runner's is
2,986,452 — **500 bytes apart**, with different sha256s, at one commit, one
Xtensa toolchain and one host rustc *version*. What differs is the triple the
compiler binary itself was built for.

**And every figure the heap gate reads is identical on both hosts** —
`stackHighWater` 16,060 and `usedBytes` 17,044, plus `freeBytes` 224,508,
`largestFreeBlock` 108,526, `totalBytes` 241,552, `stackTotal` 45,280. That is
the classic's version of "memory-class figures survive the drift", and it is a
**finding, not a licence to drop the band**: two hosts agreeing once is not
proof a third will. Both rows are in `scripts/heap-budget-record.json`'s
`host_band`, and the band stays.

### 7.3 The emulator was wrong about the second core, and the bench said so

Worth recording because of how it was settled rather than what it was. M4 P1's
first model ran the ROM's full reset path on the APP core when DPORT released
it, which re-unpacked `.data_xtos_pro` over heap region 0 and killed the
shipped image a few seconds in — on a machine running the same bytes the desk
board runs happily.

The emulator's claim was treated as **the emulator's to prove**: a canary
firmware (`test_appcore_rom_path`) allocated a block in region 0, started core
1, and reported which bytes came back changed. **Silicon rewrote not one byte**
of `0x3ffe0440..0x3ffe1440` — not the tables, not even ROM `main`'s handler
pairs — in two sittings. The same build on the then-current emulator rewrote
3,800. So the model changed: core 1 begins at `DPORT.appcpu_ctrl_d`'s address
with **no ROM code run**, and a DPORT-trace test asserts no write into that
span after the release. The defect file is the emulator's, not the firmware's,
and the hardware rationale for *why* the pulse does not re-run the ROM is
still owed and stated as owed.


## 8. What every milestone deviated on, in one place

| milestone | the deviation a walk reader needs |
|---|---|
| M0 | the study's "33 % image coverage" was a **name-table artefact**: whole-ELF decode reads 98.86 % of the image and 99.96 % of the ROM. One real decoder bug fell out of it (`lsi` decoded as `ssai`). |
| M1 | the LX6 `f64*` acceleration instructions stay **`Unsupported`** (DD16): 807 of 836 sites are literal-pool artefacts and no soft-double helper exists in the image, so a strict stop at boot is the honest test — and neither boot path hit one. `PS_BOOT` is `0x0006_0020` (CALLINC 2, because the bootloader enters through `callx8`), measured rather than assumed. |
| M2 | the interrupt seam has **two halves** and which one a machine asks is an ISA fact; `engine::sha` was extracted against the stated bar and the crate README names its different justification, so it is not a precedent. |
| M3 | the alias policy (unmapped, DD36), **tight apertures** on the classic so a gap is a stop naming an offset rather than a silent zero (DD39), **no vendored bootloader** (DD25), and OCD/debug external registers that read 0 — "no debugger attached" — because the firmware asks. |
| M4 | the APP-core ROM path was the **emulator's** defect and the bench arbitrated it (§7.3). **Two hart bugs the walk found rather than the fixtures**: the poll point re-sampled the RV32 single-line interrupt question and zeroed the hart's own asserted mask on every MMIO store (P3b, #704), and CALL0/CALLX0 wrote `PS.CALLINC = 0` where the ISA RM gives them no such effect, so an interrupt between a `callN` and its `entry` came back with the wrong increment (P4b, #711). Both fixes are in code the **user-mode oracle shares**, and no user-mode golden moved. Two more a reader needs: the classic console **interleaves records** — the driver's own open line arrives cut in four — so the gates rejoin a cut record before matching; and the guest's frame counter is visible only every sixtieth frame, so "the counts agree" is a claim about where the deadline fell, while the gate's claim is that the pad carried at least every frame the guest counted and under one report period more. |
| M5 | **`t1` only** (E1), the two memory gaps still standing (E2), `records_pins` flipped to `true` on our configuration in P7 with this record's pad sentence as its companion (DD72), and the classic's heap arm riding the `Emulator ESP32v3 (x64)` job rather than the C6's heap job (DD71). |
| L1 / L2 | the bench is the arbiter twice over: L1 measured the download-mode strap (`0x3`) and pinned the first memory gaps; L2 answered the APP-core question with three words of silicon. `read-flash` **fails over the CH340K** — do not build a step on it. |


## 9. Not run

- **A silicon pin capture.** The `measured` step for the `pin` class on this
  chip, and it needs a logic analyser on a classic pad. Nothing has been on
  one.
- **A silicon `rmt-chase` transcript.** The payload's 768-of-768 agreement
  (guest checksum against the decoded pad, 0 bit errors, byte-identical dumps
  across three recordings) is **emulator-only**, on this chip as on the C6.
- **A `t2` run.** There is no `t2` on this machine (§3). Not "not run" so much
  as "does not exist", and it is in this list so nobody goes looking.
- **An emulated twin of the download-mode capture.** No `--strap` on the
  classic binary (§6).
- **A second oracle for the frame.** The host oracle is two independent
  engines and it is met; no third machine was asked.
- **Speed numbers.** `just bench-emu-esp32v3` is M7's, and this plan's rule is
  that speed is measured, never promised.


## 10. Reproducing this

```bash
just walk-esp32v3-emu                        # the whole thing, ROM-up, with the cable
just walk-esp32v3-emu --keep                 # …and leave the artefacts in target/lp-emu-esp32v3-walk/
LP_WALK_BOOT=direct just walk-esp32v3-emu    # the same three readings, direct load
just walk-esp32v3-emu-frame                  # the frame half alone: direct load, no cable
just heap-budget-check-chips-v3              # the heap gate, on its own
just test-emu-esp32v3-gate                   # the gate: the boot suite, the lints, the replays, the image
```

The artefacts are `walk.console.txt` (everything the device said),
`walk.cable.txt` (every control verb and its reply, with the guest cycle),
`walk.frames.jsonl` (every frame decoded off a pad, `wire` and `rgb` both),
`merged.bin` with its `.sha256` and `.provenance`, and the ELF the walk built.

The hardware twin is `scripts/m4-hardware-walk.sh --chip esp32`, which wants
the board: DOM-Z-102, ESP32 **v3.1**, MAC `30:76:f5:ec:f6:34`. Resolve it by
identity rather than by "first port" — it has moved between enumerations
once already — and never probe while a walk holds the port.
