# Plan: MSAFluid theoretical-upper-bound perf experiment on esp32c6

## Status

In progress. Throwaway perf experiment — *not* a product feature, *not*
shipping code. Goal is one data point: how fast can a Stam-style fluid
solver run on esp32c6 *at all*, when implemented as native Rust + Q32
(skipping the JIT and the entire fixture/texture pipeline)?

## Why

The engine pipeline architecture doc
([`docs/future/2026-04-20-engine-pipeline-architecture.md`](../../future/2026-04-20-engine-pipeline-architecture.md))
calls out fluid as the one esp32 stretch target that the proposed
point-fixture / functional-effect architectural split doesn't help —
fluid is intrinsically stateful and grid-based. Before committing to
that architecture, we want to know whether fluid on esp32 is *real* or
*aspirational*. If the raw native Rust + Q32 solver doesn't fit the
budget, no compiler / pipeline work will save it.

This experiment establishes the **theoretical upper bound**:

- Native Rust compiled with `-O3` for `riscv32imac` → no JIT
  overhead, no interpreter, no shader plumbing. Whatever LLVM can do
  with the algorithm, this gets.
- Q32 fixed-point math → matches what lpvm uses (so it's
  apples-to-apples vs the eventual JIT path) *and* is the right shape
  for an FPU-less RV32 target. f32 on esp32c6 (no FPU) would go
  through soft-float helpers and not represent the platform's actual
  ceiling.
- Solver-only — no fixture, no texture, no strip output. We measure
  the cost of `update()` in isolation.

If the solver alone doesn't fit at e.g. 16×16 / 30 fps with reasonable
headroom, fluid is a wgpu-only feature. That's a real product
decision and worth knowing now.

## Reference

- **Algorithm reference (read this first):**
  `~/dev/personal/lightPlayer/PlayerCore/src/main/java/com/lightatplay/lightplayer/rendering/msafluid/MSAFluidSolver2D.java`
  — the lp2014 port of Memo Akten's MSAFluid, itself based on Jos
  Stam's "Real-Time Fluid Dynamics for Games" GDC 2003 paper. We are
  porting **the monochrome (single-channel `r`) path** for this
  experiment — 1/3 the work of the RGB path, sufficient to establish
  the budget. RGB extension is trivial later if needed.
- **Q32 type and ops:** `lp-shader/lps-q32/src/q32.rs` — `Q32(i32)`
  with `Add` / `Sub` / `Mul` / `Div` / `Neg` traits, `from_f32_wrapping`,
  `from_i32`, `to_f32`, `clamp`, `min`, `max`, `abs`, `ZERO`, `HALF`,
  `ONE`. The `Div` impl is the saturating-divide path — fine for the
  one or two divides per `update()` call (everything else hand-LICMs to
  multiplies-by-precomputed-reciprocal, see "Implementation notes").
- **`test_` feature pattern (template to copy):**
  `lp-fw/fw-esp32/src/tests/test_dither.rs` + the wiring in
  `lp-fw/fw-esp32/src/main.rs` (the `#[cfg(feature = "test_dither")]`
  blocks at lines ~116, ~159, ~177).
- **Cycle counter on esp32c6:** the RISC-V `mcycle` CSR
  (`riscv::register::mcycle::read64()` from the `riscv` crate, or
  inline asm `csrr` if `riscv` isn't already a dep). esp32c6 has Zicntr
  so this works in user mode.

## Phase outline

| # | Title | Sub-agent | Out |
| --- | --- | --- | --- |
| 01 | Port the monochrome MSAFluid solver to no_std Rust + Q32 | yes | `lp-fw/fw-esp32/src/tests/msafluid_solver.rs` (no wiring) |
| 02 | Add `test_msafluid` feature, run at 16/32/48/64, print cycles | yes | `lp-fw/fw-esp32/src/tests/test_msafluid.rs` + Cargo + main wiring |
| 03 | Cleanup, validation, summary | supervised | `summary.md`, plan move |

Each phase commits **at the end** as a single squash-style commit
(this is a throwaway experiment, no need for per-phase commits like
the fixture-render plan had — there's no profile-after-each-phase
loop here). Composer 2 sub-agent does all three.

## Success criteria

- `cargo build -p fw-esp32 --features test_msafluid --release` succeeds.
- `cargo build -p fw-esp32 --release` (default features) still
  succeeds (i.e. the new test mode is properly gated).
- The user can flash + run the firmware and see a log line per
  resolution of the form:
  `[msafluid] N=16 step_cycles=AVG (median=M, min=Lo, max=Hi over K runs)`
  for N ∈ {16, 32, 48, 64}.
- No host-side correctness tests required. We're measuring the
  algorithmic cost on real hardware; correctness of the port is
  checked informally by the sub-agent (output values are non-NaN,
  bounded, and the average density evolves the way Stam fluid evolves
  when given a force impulse).

## Implementation notes

These are the things you would otherwise discover painfully — pull
them out so the sub-agent doesn't have to.

### Hand-LICM the divides

Stam's `linearSolver` does `(stuff) / c` where `c = 1.0 + 4*a` is a
loop-invariant per call. f32 LLVM might LICM this; Q32 with the
`Div` trait will not (the divisor isn't a literal). **Hand-hoist** to
a precomputed reciprocal:

```rust
// at the top of linear_solver(), once per call:
let inv_c = Q32::ONE / c;        // one divide
// in the inner loop:
x[idx] = (a * (...) + x0[idx]) * inv_c;  // multiply, no divide
```

Same for `dt0 = _dt * _NX` in `advect` — already a constant per
call in the Java source, just preserve the pattern.

`project()` has a literal `0.5 / _NX` which composer 2 should also
precompute (call it `inv_2nx`).

### Use raw `i32` indexing, not `Q32`, for grid coordinates

`FLUID_IX(i, j) = i + (NX+2) * j` is integer math on grid
coordinates. Don't wrap that in `Q32`. Q32 is only for the *field*
values (u, v, r, etc.).

### Storage layout

`(NX+2) * (NY+2)` cells; ghost cells for boundary handling. Use
`alloc::vec::Vec<Q32>` (no_std + alloc is already available — see
`extern crate alloc;` in `test_dither.rs`). Allocate once per
resolution, reuse across the timing loop.

For 64×64 the field arrays are `66*66 = 4356` Q32s = 17 KB per array.
With six arrays (`r, rOld, u, v, uOld, vOld`) that's ~104 KB. esp32c6
has 512 KB SRAM, so this is fine, but worth being mindful of.

### Skip RGB

Port only the monochrome path:
- `r`, `rOld` (no `g`/`b`/`gOld`/`bOld`)
- `addSource`, `swapR`, `diffuse(r)`, `advect(r)`, `fadeR`
- `linearSolver` (not `linearSolverRGB` / `linearSolverUV`)
- `setBoundary` (not `setBoundaryRGB`)
- Keep the full UV path (`addSourceUV`, `diffuseUV`, `project`,
  `advect(u)`, `advect(v)`) — that's the dominant cost regardless.

### Inject a force every step (otherwise nothing happens)

To get a representative measurement, inject a constant impulse at
each step before calling `update()`:

```rust
solver.add_force_at_cell(NX/2, NY/2, Q32::from_f32_wrapping(0.5),
                                      Q32::from_f32_wrapping(0.3));
solver.add_color_at_cell(NX/2, NY/2, Q32::from_f32_wrapping(1.0));
```

Otherwise the first `update()` is a no-op (zero everywhere) and not
representative.

### Cycle measurement

Read `mcycle` CSR before and after `update()`:

```rust
use riscv::register::mcycle;
let start = mcycle::read64();
solver.update();
let end = mcycle::read64();
let cycles = end - start;
```

If `riscv` isn't already in `fw-esp32`'s deps, use inline asm:

```rust
#[inline(always)]
fn read_mcycle() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!(
            "csrr {lo}, mcycle",
            "csrr {hi}, mcycleh",
            lo = out(reg) lo,
            hi = out(reg) hi,
        );
    }
    ((hi as u64) << 32) | (lo as u64)
}
```

(Note: mcycle/mcycleh is technically not atomic on a 32-bit core if
the low half rolls over between reads. Standard mitigation is the
read-hi / read-lo / re-read-hi pattern; for a measurement of a
single solver step on a 160 MHz core where mcycle takes ~27 seconds
to wrap the low half, this is overkill. Read once, ignore the
race — sub-agent should not over-engineer this.)

Run K = 30 steps per resolution, drop the first 5 (warmup), report
median + min + max + average of the remaining 25.

### esp32c6 clock

esp32c6 default core clock is 160 MHz. So:

- Budget per frame at 30 fps = 160e6 / 30 ≈ **5.33 million cycles**.
- That's the headroom number to compare `step_cycles` against.

The sub-agent should print this prominently at the end of the run:

```
[msafluid] === SUMMARY ===
[msafluid] esp32c6 @ 160 MHz, frame budget @ 30 fps = 5,333,333 cycles
[msafluid]   N=16: step=  XXX,XXX cycles (XX.X% of frame budget)
[msafluid]   N=32: step=  XXX,XXX cycles (XX.X% of frame budget)
[msafluid]   N=48: step=  XXX,XXX cycles (XX.X% of frame budget)
[msafluid]   N=64: step=  XXX,XXX cycles (XX.X% of frame budget)
```

That makes the "does it fit?" question one glance away.

### Don't shortcut the solver

`_solverIterations = 10` (Jacobi iterations in `linearSolver`) is the
lp2014 default and is what makes the solver actually look fluid.
**Keep it at 10.** Reducing to 4 or 5 would understate cost vs. the
real product.

### Skip "fade" stats unless trivial

The lp2014 `fadeR()` updates `_avgDensity`, `_avgSpeed`, and
`uniformity` — these are diagnostic outputs, not part of the solver.
Port the *fade* (`r[i] *= holdAmount`) but skip the avg / uniformity
accumulation. They'd add per-cell work that the real product
wouldn't pay either.

## Out of scope

- Anything touching the engine pipeline, fixture nodes, scene
  loader, project format, etc. This is a pure perf measurement.
- RGB fluid path. Mono only.
- A separate crate. Drop it inside `fw-esp32` as a no_std module.
- Host-side correctness tests. We don't need bit-exact reproduction
  of lp2014 behavior; we need a representative cost number.
- Wiring the solver output to actual LEDs / strip / display. The
  `r[]` field gets computed and discarded after the cycle measurement.
- Comparing against an f32 baseline. Q32 is the platform-realistic
  baseline.
- Profile-mode integration (`cargo run -p lp-cli -- profile ...`).
  This experiment runs on real hardware via firmware flash, not the
  emulator.

## Open questions for the user (after results land)

These belong in the post-experiment summary, not now:

1. Do the cycle counts justify keeping fluid as an esp32 stretch
   target, or does it become wgpu-only?
2. If yes, which N is the "minimum viable" for the product
   (acknowledging the user's hypothesis: 32×32)?
3. Does the result change anything in the engine pipeline
   architecture doc's recommendation? (Likely not — fluid was already
   noted as not benefiting from the proposed split.)
