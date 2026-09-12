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

Every patch applies to **`38b12c848d38ad1337ccd0c210f9d32587d2547f`**
(`origin/main` at dispatch — the P1 merge, PR #731).

```sh
git checkout 38b12c848d38ad1337ccd0c210f9d32587d2547f
git apply --check scripts/emu/tier-probes/R1-pins-off.patch   # and so on
```

They are `git diff` output against that tree and are **mutually exclusive** —
apply one, take the rows, `git apply -R` it, apply the next.

## The rungs

| patch | rung | what the hack does |
|---|---|---|
| — | **R0** | the base sha, untouched — the reference row |
| `R1-pins-off.patch` | **R1** | the RMT still fetches every word, schedules every `EV_WORD` and raises every interrupt on schedule; the pin **fabric** (`drive` → `settle` → `resolve` → `apply`), the pin log, `Gpio::observe_edges` and `Rmt::observe_edges` are skipped. Frames are derived from the RMT's own words instead, through the same `Ws281xDecoder` |
| `R2-coalesced-words.patch` | **R2** | the per-word `EV_WORD` is replaced by one event at the threshold word and one at end-of-transmission; the words up to the threshold are fetched in bulk and their pulses emitted with their true cycles; the threshold interrupt is raised at the cycle it would have been |
| `R3-instant-rmt.patch` | **R3** | the transmitter consumes the whole RAM window at the first start and raises only end-of-transmission. The threshold interrupt never fires, so the guest's refill ISR never runs. No pins |
| — | **R4** | **not taken.** See "R4" below |

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
