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

| lab build id | rung | patch |
|---|---|---|
| see `G-LOOP0b-gate.md` | | |
