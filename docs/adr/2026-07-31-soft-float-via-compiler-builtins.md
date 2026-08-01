# Soft float calls the platform library directly, with no LightPlayer wrapper

- **Status:** accepted
- **Date:** 2026-07-31
- **Deciders:** Yona, f32 roadmap M9
- **Supersedes:** nothing
- **Related:** `docs/design/float.md`, `docs/adr/2026-07-28-esp32c6-flash-budget.md`,
  `docs/adr/2026-07-30-isa-parameterized-host-emu-engine.md`

## Context

The f32 roadmap adds IEEE-754 binary32 as a second numeric mode beside Q16.16.
Some targets have a single-precision FPU (ESP32-S3's Xtensa LX7, the announced
ESP32-S31 and the ESP32-P4 with RV32IMAFC). Many do not: the **ESP32-C6**
(RV32IMAC) is the reference device today, and the RP2350's Hazard3 cores, every
Cortex-M0+, and any future value part are the same shape.

Those parts must still be able to *execute f32 semantics*, for two reasons:

1. It is the only rv32 **hardware** oracle for f32 available before F-bearing
   silicon reaches the desk.
2. A shader authored in Float mode should not simply fail to run on a board
   without an FPU. Slow is a product decision; "does not exist" is a support
   burden.

So float ops on a non-FPU target lower to library calls. The question this ADR
answers is **which library, reached how**.

Three options were on the table:

- **A. LightPlayer wrapper.** Add `__lp_lpir_fadd_f32` and friends to
  `lps-builtins`, lower to those, and have them call the platform routines.
  This is the shape the Q32 path already has (`__lp_lpir_fdiv_recip_q32`).
- **B. Direct calls to the platform soft-float ABI.** Lower straight to
  `__addsf3`, `__ltsf2`, `__floatsisf` — the symbol names every C toolchain,
  `compiler_builtins`, and Espressif's ROM already publish.
- **C. Our own soft-float implementation** in `lps-builtins`, so one
  implementation runs everywhere including the host emulator.

## Decision

**B: lower to the platform soft-float ABI symbols directly.** No wrapper layer.

`IsaTarget::f32_lowering` names the strategy per target
(`Unsupported` / `SoftFloatCalls` / `HardwareFpu`), and the `SoftFloatCalls` arm
emits a plain `Call` VInst at the ABI symbol name — mechanically the same thing
today's Q32 lowering does, one string different.

Ops the soft-float ABI **does not define** (`sqrt`, `floor`/`ceil`/`trunc`/
`nearest`, `min`/`max`, the unorm lane conversions) call the native-f32 builtin
family instead. That is not a wrapper: those routines have no ABI symbol, so the
builtin *is* the implementation.

**Float→int is a deliberate exception.** `__fixsfsi`/`__fixunssfsi` exist in the
ABI, and we do not call them — see Consequences.

## Rationale

**The symbols are already in every image, for free.** Verified before any
lowering was written:

| Image | Where `__addsf3` resolves |
|---|---|
| `fw-esp32c6` (`riscv32imac-unknown-none-elf`) | **ROM**, `0x400009f8`, via `esp-rom-sys`'s `ld/esp32c6/rom/esp32c6.rom.rvfp.ld` |
| `lps-builtins-emu-app` (host emulator's guest image) | Rust `compiler_builtins`, linked in |

On the C6 the implementation lives in mask ROM, so the call costs an
`auipc`+`jalr` and **zero bytes of the 3 MB app partition** — which matters,
because that partition is the repo's tightest resource. A wrapper layer would
have added a second call frame per float op *and* real flash, to accomplish
nothing.

**Option A's cost is per-op, forever.** Yona, on first hearing the design:
*"it's a bit annoying if we have to double-call, first to lp-builtins, then to
rust-builtins. So we might want to see if we can directly call the rust
builtins."* We can. Soft float is already the slowest numeric path in the
system; doubling its call overhead to gain a naming convention is the wrong
trade.

**Option C loses more than it gains.** Owning a soft-float library would buy
bit-identical behavior between emulator and silicon — genuinely valuable — at
the cost of maintaining correctly-rounded add/sub/mul/div forever, and of
*giving up the ROM implementation*, which is both free and faster than anything
we would write. The correct place to spend effort on emulator-vs-silicon
agreement is a conformance harness, not a reimplementation.

**The ABI's return convention is a feature, not an obstacle.** `__ltsf2` and
friends return a signed integer whose sign answers the comparison, biased so
that the unordered (NaN) case makes the natural test false — matching IEEE-754,
where every comparison but `!=` is false on NaN. Lowering therefore emits the
call plus one compare-against-zero, and the NaN semantics come out right without
a special case. It also means each comparison must use **its own** symbol:
`a < b` is not `__gtsf2(b, a) > 0`, because the two are biased in opposite
directions and differ exactly on NaN.

**Values stay in integer registers.** The soft-float ABI passes and returns a
`float` in the integer argument bank, which is what the emitter already does. So
this path needs no `RegClass::Float` pool, no float argument registers, and no
new emitter instructions — the entire f32 backend for a non-FPU target is a
lowering table.

## Consequences

**This is the standing answer for every future non-FPU target.** Hazard3
(RP2350), RP2040, Cortex-M0+ — each needs its `IsaTarget` variant and its
`f32_lowering` arm, and nothing else. That is the point of writing this down.

**The float-capability seam is load-bearing.** `IsaTarget::Rv32imac` names
hardware *without* the F extension, so an F-bearing rv32 part is a **new
variant**, never a flag on this one. Nothing currently answers
`F32Lowering::HardwareFpu`, and a unit test keeps it that way, because emitting
`fadd.s` for a C6 is not a wrong number — it is an illegal-instruction trap on
the first frame.

**The emulator and the silicon run different code.** The host emulator executes
Rust's `compiler_builtins`; the C6 executes Espressif's ROM `rvfplib`. Both
implement the same ABI, but they are separate implementations, so agreement is a
fact to *measure*. The `test_f32_softfloat` harness on `fw-esp32c6` is that
measurement: it probes the ROM routines against IEEE reference bit patterns
computed off-device (computing them on-device would compare the routines to
themselves), and then runs a GLSL shader compiled on the C6 in Float mode
end to end.

**Float→int does not use the ABI symbol.** `__fixsfsi` is documented as
*undefined* for out-of-range and NaN inputs, while `docs/design/float.md` §3
requires finite out-of-range values to saturate. `compiler_builtins` happens to
saturate; the C6 ROM is a different implementation and need not. Rather than let
the emulator and the silicon legally disagree at exactly the edges the corpus
tests, both conversions go through `__lp_lpir_ftoi_sat_{s,u}_f32`, which is one
implementation everywhere and follows the same rule as wasm's
`i32.trunc_sat_f32_s`. The harness still *probes* the ROM's `__fixsfsi` and
prints what it does, so this can be revisited against data.

**Nothing is bit-identical across float modes, and that is expected.** Q32 and
f32 are different numeric systems; the corpus's `run[q32]:` / `run[f32]:`
channels already carry that.

**Cost is gated.** The f32 lowering sits behind `lpvm-native`'s `float-f32`

feature and the builtin family behind `lps-builtins`' `float-f32`, both off by
default (roadmap D2): `FloatMode` is matched on a runtime value, so LTO cannot
drop the arms on its own, and the shipping C6 image runs Fixed-mode shaders
only. `test_f32_softfloat` is the one configuration in `fw-esp32c6` that turns
them on, which is what keeps `just fw-esp32c6-size-check` measuring an unchanged
product image.

## Follow-ups

- **Reconsider `__fixsfsi`/`__fixunssfsi`** once the C6 harness's probe data says
  whether the ROM's out-of-range and NaN behavior matches `compiler_builtins`. If
  it does, the two float→int ops can join the direct-call set and drop a builtin.
- **Xtensa's `f32_lowering` arm** stays `Unsupported` until the hardware-FPU
  emitter and emitter land (roadmap M6/M7). Soft float would be the wrong answer
  for a part that has an FPU, so this is a decision, not a gap to fill by
  default.
- **A soft-float performance number.** Nothing here measures how slow Float mode
  is on a C6. That belongs with the perf surface the roadmap defers (D3's
  "42.5fps, soft-float" line), not with this decision.
