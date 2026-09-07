# ADR: The GPU tier bounds every loop — static refusal plus an injected back-edge budget

- **Status:** Accepted
- **Date:** 2026-09-06
- **Deciders:** Photomancer
- **Plan:** `lp2025/2026-09-06-2240-gpu-unbounded-shader-guard`
- **Fixes:** `docs/defects/2026-09-06-gpu-tier-executes-unbounded-shaders.md`
- **Supersedes:** None
- **Superseded by:** None
- **Related:** [2026-09-02-fault-is-never-black.md](2026-09-02-fault-is-never-black.md)
  (the fuel trap as a feature), [2026-07-09-preview-fidelity-tiers.md](2026-07-09-preview-fidelity-tiers.md)
  (the tier contract), [2026-09-06-catalog-content-tree.md](2026-09-06-catalog-content-tree.md)
  (the content eviction)

## Context

Every LPVM tier meters fuel: `lpvm::DEFAULT_INVOCATION_FUEL` (100,000)
loop back-edges per invocation, re-armed by the render wrapper at the top
of each pixel or sample, trapping when the tank runs dry. The never-black
ADR turned that trap into a feature — `fault-demo`'s `while (true)` is the
deterministic demonstration of the fault pattern — and so shader content
in this repo is authored on the assumption that an unbounded loop is a
*fault*, not a hang.

The GPU tier (`lp-gfx-wgpu`: the browser WebGPU preview worker and the
native wgpu harness) compiles the same GLSL through naga and dispatches
it with no meter at all. On 2026-09-06 the catalog branch's gallery
previewed `fault-demo` on that tier: Brave hung, Firefox took the host
down, and a native run reset the GPU under every window on the desktop.
The driver watchdog is not a sandbox — it resets the device, and every
other surface in flight goes with it.

## Decision

`lp-gfx-wgpu` closes the gap in the naga IR, between `glsl-in` and
validation (`loop_bound_pass.rs`), in two layers, and bounds its waits.

### 1. A loop that can never exit is a compile error

`refuse_loops_without_exit` walks every function and entry point. A
`Loop` is refused unless it has a `break_if` or its body has, on some
path, a `Break` targeting it, a `Return`, or a `Kill`. `If` conditions
that fold to a constant (`Literal(Bool)`, `!` of one, a `ZeroValue`, a
module constant) prune the dead arm first — glsl-in lowers `while (true)`
to a loop whose body opens with `if (!true) { break; }`, and the constant
evaluator folds `!true` to `false`, so the demo's exit exists in the IR
and is unreachable. `Break`s inside a nested `Loop` or `Switch` target
that construct, not the outer loop.

The refusal is a `GfxError::Compile` naming the function, the
assembled-source line, and its text. The shader node classifies it as an
authoring `Error` (the tier refuses the program; the fix is an edit or a
different tier), the same class as a naga diagnostic. The check only
ever refuses a loop with *no* exit, so it cannot reject a program the
LPVM tiers would run; a data-dependent runaway compiles and is left to
layer 2.

### 2. Every loop charges a per-invocation back-edge budget

`bound_loop_iterations` adds one `var<private> lp_gfx_loop_budget: u32 =
0u` to a module that has loops (a private global is per-invocation in
WGSL and zeroed at invocation start, so no wrapper reset is needed) and,
in every loop's `continuing` block, appends `budget = budget + 1u` and
sets `break if budget > DEFAULT_INVOCATION_FUEL` (OR-ed onto an existing
`break_if`). The `continuing` block runs once per back-edge — including
the ones a `continue` takes — and `break if` is evaluated right after
it, so the unit and the placement are the fuel meter's own; the constant
is the fuel meter's own, re-exported through `lp_shader`. The pass
appends expressions and touches only `continuing` blocks and `break_if`
slots, so no arena is rebuilt and existing `Emit` ranges stay valid.

The GPU cannot trap. An exhausted budget exits the loop and the
invocation completes with whatever value it has; the fault pattern does
not paint for a GPU runaway. This is the accepted gap: bounded, not
faulted. It is strictly better than the watchdog, and the LPVM tiers —
which every device runs — still fault the same content.

### 3. The product read-back waits are bounded

The three native product read-back paths (`read_back`, the raw-float
probe, the sample pass) wait `PRODUCT_READ_BACK_WAIT` (10 s) on their
submission instead of indefinitely. With every compiled shader
loop-bounded, a wait that outlives its bound means a lost or wedged
device, and the poll surfaces `PollError::Timeout` as a `GfxError::
Backend` rather than hanging the host behind a device the driver has
already reset. The filetest probe keeps its own 20 s bound.

## Consequences

- `fault-demo` (and any loop with no exit) does not compile on the GPU
  tier. Content that wants to demonstrate the fault pattern uses an LPVM
  tier, which is where the pattern is defined.
- Every GPU-compiled loop costs an increment, a compare and a branch per
  back-edge — the same work the LPVM tiers already pay. No parity
  envelope moved (`just test-gfx` holds).
- A shader that legitimately needs more than 100,000 back-edges per
  invocation is not a shader this product runs anywhere: the LPVM would
  trap it first.
- The static check rejects nothing the LPVM would run; the budget
  changes no output below the tank size. Above it the tiers differ: the
  LPVM faults the frame, the GPU finishes the invocation.

## Alternatives considered

- **Static refusal only.** Cheapest; misses every data-dependent
  runaway (`while (x < 1.0) { x += 0.0; }`), which is the honest
  mistake the gallery will meet next. Kept as layer 1 because it gives
  the author a line number, which the budget cannot.
- **Per-loop counters (locals) instead of one per-invocation global.**
  Bounds each loop to N, so nested loops compound to N²; the fuel meter
  is a per-invocation total. The private global is the faithful model
  and needs no per-loop local or reset.
- **A trap on the GPU (NaN sentinel, `discard`, a storage-buffer
  flag).** `discard` paints black, which the never-black ADR forbids; a
  NaN sentinel quantises to 0 on the product path and is read only by
  the filetest probe; a storage flag needs a binding, a read-back and a
  per-frame policy. Deferred to a follow-up if the gap ever matters on
  a GPU tier.
- **Timeouts only.** Bounds the host's wait, not the device's work: the
  watchdog has already fired and taken the sibling surfaces by the time
  the timeout reports. Kept as layer 3, not as the fix.
- **Bounding at the GLSL source level.** Naga's IR is where every
  construct has already been lowered to `Loop { body, continuing,
  break_if }`, so one shape covers `for`, `while`, `do … while` and the
  spliced prelude; the source level would need a parser and would miss
  the prelude.

## Follow-ups

- A fault signal on the GPU tier (so a runaway paints the pattern
  there too) if a GPU tier ever runs a wall.
- The `ui_shader_error.rs` diagnostic parser: the refusal's `line N`
  refers to the assembled source, as naga's own diagnostics do; if the
  Studio ever maps assembled lines back to authored ones, this message
  should ride the same mapping.
