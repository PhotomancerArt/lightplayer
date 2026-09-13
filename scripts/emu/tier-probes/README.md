# Tier probes — the fidelity tier, sized as a tier

**These patches are not the product.** Nothing here is committed to the
emulator's behaviour, nothing is behind a flag, nothing is default-off in the
product path. They exist so that future-us can re-take the rows in
[`docs/emulator-perf-ledger.md`](../../../docs/emulator-perf-ledger.md)
without re-deriving the hacks.

Written for the emu-loop-redesign milestone's phase **P1b** (gate
`G-LOOP0b`), 2026-09-12.

## Why rungs and not a tier

DD26 registered a "fidelity tier" — frames read from the guest's LED buffer,
console read from the guest's buffer, real output turned off in the firmware
under emulation. DD27 declined it on the ground that *all peripheral models
together are 3.4 % of the run*. That number is the models' **self time**, and
it is not what the tier removes. The tier removes the **per-word world**: 96 %
of the run's 1.53 M slices are bounded by the RMT's per-word event, and behind
each word are the pin fabric, the pin log, the WS281x decoder, the guest's own
refill ISR and its 1.68 M RMT RAM stores. Nobody had measured that.

A rung is a *temporary hack that approximates one tier and reads the wall*. It
is not a design, it is a bound. Four of them bracket the tier from "the guest
is untouched and only the output path is cheaper" (R1) to "the transmitter
swallows the RAM whole and the guest's ISR never runs" (R3).

## Base

`R1`–`R3` apply to **`38b12c848d38ad1337ccd0c210f9d32587d2547f`**
(`origin/main` at P1b's dispatch — the P1 merge, PR #731).

`R5`, `R5a` and `R5c` are **P1d**'s and apply to
**`0c622e12a5e9324fad9b91fc6a0b0e61dfafd5a3`** (`origin/main` at P1d's
dispatch — the P1c merge, PR #736).

```sh
git apply --check scripts/emu/tier-probes/R1-pins-off.patch   # and so on
```

They are `git diff` output against those trees and are **mutually exclusive**
— apply one, take the rows, `git apply -R` it, apply the next.

## The rungs

| patch | rung | what the hack does |
|---|---|---|
| — | **R0** | the base sha, untouched — the reference row |
| `R1-pins-off.patch` | **R1** | the RMT still fetches every word, schedules every `EV_WORD` and raises every interrupt on schedule; the pin **fabric** (`drive` → `settle` → `resolve` → `apply`), the pin log, `Gpio::observe_edges` and `Rmt::observe_edges` are skipped. Frames are derived from the RMT's own words instead, through the same `Ws281xDecoder` |
| `R2-coalesced-words.patch` | **R2** | the per-word `EV_WORD` is replaced by one event at the threshold word and one at end-of-transmission; the words up to the threshold are fetched in bulk and their pulses emitted with their true cycles; the threshold interrupt is raised at the cycle it would have been |
| `R3-instant-rmt.patch` | **R3** | the transmitter consumes the whole RAM window at the first start and raises only end-of-transmission. The threshold interrupt never fires, so the guest's refill ISR never runs. No pins |
| — | **R4** | **not taken.** See "R4" below |
| `R5-write-watermark.patch` | **R5** (P1d) | the per-word `EV_WORD` becomes one event per **run** of words: `fetch` consumes words in a loop, each pulse still stamped with its own true cycle in word order, and schedules one `EV_WORD` at the first word the run did not take. Four bounds — before the threshold word (so `INT_TX_THR` keeps its cycle), before the window's last word, and **at the guest's write watermark**: a new `Rmt::written` bit per RAM word, set by the guest's store and cleared when the transmitter consumes it. This is the shape G-LOOP0b named as the one unexplored candidate |
| `R5a-watermark-plus-guard.patch` | **R5a** (P1d) | R5 plus a fourth bound: the run also stops before any word that would **end** the transmission (`dur1 == 0`). R5's first failure is that the driver's STOP guard is a word the guest writes precisely so it can overwrite it again; R5a removes that failure so the *second*, deeper one can be seen on its own |
| `R5c-shadow-census.patch` | **R5c** (P1d) | **no behaviour change at all.** The transmitter fetches word by word exactly as it does today; alongside it the same four bounds are evaluated and the words a bulk *would* have absorbed are counted. It exists because R5 and R5a do not run the guest, so their slice censuses are a broken program's — R5c sizes the lever on the healthy cadence |

## R4 — not taken, and the census is the reason

R4 was to be an always-empty UART TX FIFO, on the strength of M4's finding
that the harness image's MMIO was 86 % TX-FIFO poll. On `render-basic` t2 it
is not:

```
jit: mmio census (translated code only): 5881883 operation(s)
jit: mmio by peripheral: SYSTIMER   2767705 (47.05 %)
jit: mmio by peripheral: RMT        1811925 (30.81 %)
jit: mmio by peripheral: UART0         8284 ( 0.14 %)
```

**UART0 is 0.14 % of crossings on the render image** — a hundredth of the
brief's 2 % floor. The rung is skipped with the census as the reason, and the
finding is that DD26's "second cord" is a *harness-image* lever, not a
product-image one.

## R3′ — the firmware's own switch does not exist

The brief asked whether `lp-fw/fw-esp32c6/` already has a build or config
switch that turns real output off under emulation, because the firmware's own
switch would be worth more than R3's emulator-side hack. It does not:
`bench/render_loop.rs` mentions emulation only to pick a shorter run, and the
RMT driver is unconditional. **A firmware-side "no real output under
emulation" switch is a lever the ledger registers and nobody has built.**

## Taking the rows

The desk is shared and its load average moves by ten between invocations, so
**rows from two invocations are not comparable**. `rung-rows.mjs` compiles
every rung's `emu.wasm` up front and interleaves the repeats in one
invocation. Stage each rung into its own directory:

```sh
# per rung: apply the patch, stage, un-apply
export LP_EMU_C6_REF_HARNESS=$HOME/.photomancer/emu-lab/images/099ba032448b.elf
export LP_EMU_C6_REF_BOOT_IDLE_MEMFS=$HOME/.photomancer/emu-lab/images/61027da9eabb.elf
export LP_EMU_C6_REF_RENDER_BASIC=$HOME/.photomancer/emu-lab/images/0d3643e03bad.elf
export LP_EMU_C6_REF_RENDER_ROCAILLE=$HOME/.photomancer/emu-lab/images/971db4b2f895.elf

git apply scripts/emu/tier-probes/R1-pins-off.patch
scripts/emu/bench-web.sh --no-serve
cp -R target/emu-bench-web target/emu-bench-web-R1
git apply -R scripts/emu/tier-probes/R1-pins-off.patch
```

then, in one invocation per engine:

```sh
caffeinate -dims /opt/homebrew/bin/node scripts/emu/tier-probes/rung-rows.mjs \
  --stage R0=target/emu-bench-web-R0 --stage R1=target/emu-bench-web-R1 \
  --stage R2=target/emu-bench-web-R2 --stage R3=target/emu-bench-web-R3 \
  --image render-basic --grade t2 --mode jit --fn-blocks 16 \
  --timeout 5500ms --repeats 5 --json rows-v8.json

caffeinate -dims bun scripts/emu/tier-probes/rung-rows.mjs \
  --stage R0=target/emu-bench-web-R0 … --fn-blocks 8 --json rows-jsc.json
```

`/opt/homebrew/bin/node` explicitly: in the agent harness `node` is an nvm
shim. `caffeinate -dims` because the desk sleeps after a minute idle.

**The R0 row's shas are the check that the baseline is the baseline**: UART0
`2407828f80684331` and trap `51ddaf56c96b77d3`, which are P1's browser-row
shas (`G-LOOP0-gate.md`). An R0 that does not reproduce them is not R0.

## Lab build ids

Rungs staged into the perf lab (`~/.photomancer/emu-lab`) get an id of
`<short sha>-dirty-<6 hex>`, where the six hex are `sha256(emu.wasm)[:6]`.
Two rungs at the same HEAD therefore get distinct ids only when their wasm
actually differs — a collision with R0's id means the rung changed no code.

| lab build id | rung | patch | `emu.wasm` sha256[:12] |
|---|---|---|---|
| `f8c44c2-dirty-d65f8c` | R0 | — (base) | `d65f8c6b9594` |
| `f8c44c2-dirty-15a80d` | R1 | `R1-pins-off.patch` | `15a80dce7b6d` |
| not staged | R2 | `R2-coalesced-words.patch` | `3fdbbbd6aa80` |
| not staged | R3 | `R3-instant-rmt.patch` | `3dfc03bd94d2` |

`f8c44c2` is this branch's first commit, which is what both stages' manifests
carried; the six hex that follow are the wasm's own sha, so R0 and R1 are
distinguishable by id. Only R0 and R1 were staged into the lab: R2 and R3 do
not run the guest (see below), so a phone row for them would compare two
different programs.

## What the rungs measured

`render-basic` t2, 5,500 ms, best of 5, interleaved, one invocation per
engine. Base `38b12c848`.

| rung | V8 16/fn | × vs R0 | JSC 8/fn | × vs R0 | retired instr | frames | uart | frames sha | trap |
|---|---:|---:|---:|---:|---:|---:|---|---|---|
| R0 | 5.24 s | 1.000 | 6.35 s | 1.000 | 542,906,355 | 256 × 241 LED | ref | ref | ref |
| **R1** | **5.10 s** | **1.027** | **6.28 s** | **1.010** | 542,906,355 | 256 × 241 LED | **same** | **same** | **same** |
| R2 | 3.51 s | 1.491 | 4.38 s | 1.447 | 337,590,617 | 256 × **4** LED | CHANGED | CHANGED | CHANGED |
| R3 | 3.53 s | 1.483 | 4.38 s | 1.447 | 336,797,583 | **none** | CHANGED | CHANGED | CHANGED |

**R1 is exact.** Same retired instructions, same frames byte for byte, same
trap log, same UART. The only surface it loses is the **pin log** (127,249
lines → 1). It is the one tier lever that leaves the guest's own numbers —
fps, cycles, memory — untouched.

**R2 and R3 do not run the guest**, and their wall times are therefore not
speed numbers: per retired instruction the emulator was *slower* under both.
Their value is the slice count, which is exact:

| | slices | bound by the RMT's per-word event | pin edges drained |
|---|---:|---:|---:|
| R0 / R1 | 1,531,923 | 1,471,630 (96.06 %) | 2,961,409 / **1** |
| R2 | 61,168 | 265 (0.43 %) | 49,153 |
| R3 | 61,155 | 256 (0.42 %) | 1 |

1,531,923 − 61,168 = **1,470,755 slices**, at P1's measured 531 ns a slice =
**781 ms of a 5,500 ms run**. That, and not R2's stopwatch, is what the
per-word cadence is worth.

### Why R2 raced — the finding

`vision.md` §2 expected the obstacle to coalescing to be the pin log's
`(at, seq)` ordering. It is not. **A bulk fetch reads RMT RAM ahead of the
guest that writes it.** The transmitter runs at the leading edge of the
schedule, so every word it takes in bulk has a cycle the CPU has not reached
— and the refill ISR that fills that word runs in the cycles in between. R2
deliberately stopped its bulk *before* the threshold word so the interrupt's
cycle could not move, and it raced anyway: the refill took 47 words to be
answered where the product path takes 15, refills fell from 2,651 to 18 on
the 500 ms cell, and the frames came out 4 LEDs long instead of 241.

The one shape that might be correct is a **write-watermark bulk** — the RMT
already tracks the guest's last RAM write (`refill_wrote`,
`RefillProbe::Filling { last_write }`), so a transmitter could bulk only as
far as the guest has actually written. **NEVER MEASURED.**

## P1d — the write-watermark bulk, measured

The paragraph above is answered. P1d built the shape and it **cannot hold**;
the ledger's cadence lever is closed on the evidence below.

### First, a correction to the paragraph above

`RefillProbe::Filling { last_write }` is **not** a position watermark. It
holds `words_consumed` as of the guest's last RAM write — a *timing* reading
for the fill measurement, not "the highest word the guest has written". The
information does exist at the call site (`refill_wrote(word_index)` is called
from `write_word` with the index), so R5 adds a real watermark: one `written`
bit per RAM word, set by the guest's store and cleared by the transmitter's
consume. Anyone reading the G-LOOP0b paragraph should read it as "the RMT
already has the *hook* for a watermark", not "the RMT already tracks it".

### What the guest's ISR reads (500 ms `render-basic` t2, `--trace RMT`)

| register | reads | writes | when |
|---|---:|---:|---|
| `ch0_tx_status` (`+0x028`) — carries `mem_raddr_ex` | **5,302** | — | **twice per threshold**, over 2,651 refills |
| `ch0_tx_lim` (`+0x058`) | 2,662 | 2,662 | once each per threshold |
| `int_st` (`+0x03c`) | 2,662 | — | once per threshold |
| `int_clr` (`+0x044`) | — | 2,673 | once per threshold |
| `ch0_tx_conf0` (`+0x010`) | 71 | 70 | per frame (start/stop) |
| `ref_cnt_rst` (`+0x070`) | — | 22 | per frame |
| the RAM (`+0x400…`) | — | ~1.68 M | the fill loop |

The bulk changes exactly one of them, and it is the busiest: **`ch0_tx_status`
`mem_raddr_ex`**. Nothing else in the block is a function of how far ahead the
transmitter has run.

### Why the bulk breaks the guest — two independent failures

**1 — the STOP guard is a write the watermark cannot read (R5).**
`lp-ws281x/src/driver.rs::refill` plants a STOP word in the half it just left
and then overwrites it as part of refilling that half. The watermark says
"the guest wrote it", so the bulk takes it. From R5's trace of the first
frame:

```
cyc=36194030 pc=0x40800bda W4 RMT+0x460 = 0x00000000   ← guard planted at word 24
cyc=36203221 RMT ch0 end words=73 idle=0               ← the frame ends on it
```

Every frame truncates at 73 words. The guard exists precisely to be raced;
"written" is a property the driver deliberately gives a word it means to
withdraw.

**2 — `mem_raddr_ex` chooses the half (R5a = R5 + a bound that stops before
any word that would end the transmission, so failure 1 is gone).** From
R5a's trace of the same frame, second threshold, against the base's:

```
base   cyc=36198620 R4 RMT+0x028 ch0_tx_status = 0x00000201   ← pos_before = 1
R5a    cyc=36198620 R4 RMT+0x028 ch0_tx_status = 0x00000218   ← pos_before = 24
```

`in_second_half = pos_before >= half(24)`. The base takes the `false` branch —
guard at word 0, refill the second half, next threshold 24. R5a takes `true` —
guard at word 24 (skipped, because `pos_before == guard_slot`), refill the
**first** half, next threshold 48. The ISR refills the half the transmitter is
standing in, and never re-arms the half threshold:

```
base   cyc=36198802 W4 RMT+0x058 ch0_tx_lim = 0x00000018   ← flipped to 24
R5a    cyc=36198767 W4 RMT+0x058 ch0_tx_lim = 0x00000030   ← unchanged at 48
```

The read pointer is not a diagnostic here. It is the driver's *only* input for
deciding what to do, and a bulk moves it to the half's end at the half's first
cycle. **No bound on how far the bulk reads can fix this**, because the
problem is not what the bulk reads — it is when the pointer moves.

### The identity table (`loop-identity.sh`, 500 ms `render-basic` t2, `--interpreter`, all binaries `--features jit`)

| rung | uart | frames | pin log | trap log | trace | retired instr | frames produced | pin edges | traps |
|---|---|---|---|---|---|---:|---:|---:|---:|
| base `0c622e12a` | `0ccda7f466879e84` | `ea2745af0a1e3c23` | `f23626cda3afd779` | `f06a3513191d759e` | `6c21134c626f2af1` | 43,249,511 | 11 × 241 LED | 127,249 | 2,757 |
| **R5** | **DIFF** `e3ecd70b062ad0ed` | **DIFF** `5123bc1cd6636861` | **DIFF** `bdb6e8d6b964cde3` | **DIFF** `c7e89bbc2459ec2f` | same | 42,881,967 | 18 truncated | 2,689 | 161 |
| **R5a** | **DIFF** `e3ecd70b062ad0ed` | **DIFF** `c417206e1dc739d3` | **DIFF** `66f6a3d22d4c67a1` | **DIFF** `7c438ccb0d4ec2be` | same | 42,878,495 | 18 truncated | 2,593 | 177 |
| **R5c** | **same** | **same** | **same** | **same** | **same** | **43,249,511** | **11 × 241 LED** | **127,249** | **2,757** |

The base row reproduces P1's pair row exactly, which is the check that the
baseline is the baseline.

**The `trace` column is `same` on every row and is not evidence here.**
`loop-identity.sh`'s trace leg is a 20 ms window, and on `render-basic` the
WS281x transmitter has not started a frame by 20 ms — the first `RMT ch0
start` is at cycle 36,188,821, which is 452 ms in. Anyone reading a `trace
same` on an RMT change is reading a run that never touched the RMT.

### R5c — the size of the prize, on the healthy cadence

R5 and R5a do not run the guest, so their slice censuses are a broken
program's, exactly as R2's and R3's were. R5c changes **no** behaviour (the
identity row above is byte-identical on all five readings) and counts the
words a watermark+guard bulk *would* have absorbed:

```
render-basic  t2 500 ms:  63,646 word fetches → 2,673 runs, 60,973 absorbed (95.80 %),
                          longest run 24 words;
                          run ends: thr 2,651 · state 11 · guard 11 · wrap 0 · unwritten 0
render-rocaille t2 500 ms: 17,358 word fetches →  729 runs, 16,629 absorbed (95.80 %),
                          longest run 24 words;
                          run ends: thr 723 · state 3 · guard 3 · wrap 0 · unwritten 0
harness       t2 500 ms:  0 word fetches — the harness image drives no strip
```

**95.80 %, and the run length is exactly the half-window.** Scaled to the
5,500 ms cell P1b measured (1,471,630 slices bound by the per-word event):
1,409,821 slices removed × P1's 531 ns = **749 ms of 5,500 ms ≈ 13.6 %** — very
nearly the whole 14 % the lever was ever worth. If a correct shape existed it
would collect almost all of it.

**And the watermark never binds.** `unwritten 0` on both images: the run is
ended by the *threshold* every single time (2,651 of 2,673 on `render-basic`).
The guest is always far enough ahead that the write watermark stops nothing.
The bound this whole phase was built to test is inert on both product images —
which is another way of saying that the read-ahead race R2 hit was never about
how far the guest had written, and always about the STOP guard and the read
pointer.

### The one shape left, and it is not a rung

The read pointer would have to be **virtualised to the observation cycle**: the
transmitter emits a run's pulses eagerly with their true cycles, but the
bookkeeping (`raddr`, `words_consumed`, the threshold equality, the wrap, the
refill probe) is applied lazily — every RMT register access catches the engine
up to `cx.now` first, a `ch_tx_lim` write catches up before it lands so the
equality is evaluated with the right value at the right position, and the
threshold is a scheduled event at the predicted word's cycle, re-predicted when
`tx_lim` moves. That is `tick`-style exactness applied to one peripheral, and
it is a redesign of the RMT engine, not a patch. **Designed at P1d, NOT BUILT.**

One thing in its favour, measured: under R5 the ISR's entry cycle did not move
(`cyc=36193821` for the `pos_before` read on both the base and R5), so the
longer slices a bulk creates do **not** delay interrupt delivery. Whatever
kills the bulk, it is not the slice boundary.

### Two channels, and why the pin-log ordering trap did not fire

`vision.md` §2's `(at, seq)` trap — two channels' edges interleaving
differently once each channel emits a run at once — could not be tested here:
**`render-basic` and `render-rocaille` both drive exactly one TX channel**
(`rmt refill ch0` only; ch1 is configured and never started). `Fabric::push`
appends in call order with no sort and `Edge` carries no sequence number, so
the trap is real for a two-channel image and remains **NEVER MEASURED**.
