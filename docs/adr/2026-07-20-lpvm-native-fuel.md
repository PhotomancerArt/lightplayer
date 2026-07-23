# ADR: Fuel metering in `lpvm-native` (per-invocation counter fuel)

- **Status:** Accepted
- **Date:** 2026-07-20
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The on-device GLSL JIT (`lpvm-native`) executes user shader code as raw RV32
machine code. Before this decision an infinite loop in applied shader code
hung the render loop until the ~8 s RWDT rebooted the board — the crash
recovery model (`2026-07-04-crash-recovery-model.md`) then attributed the
watchdog reset and blocked the path, but at the cost of a reboot and with
poor diagnostics. The shader auto-apply ADR
(`2026-07-14-shader-auto-apply.md`, "Fuel posture") raised in-JIT fuel to top
priority: with auto-apply, mid-edit shader states execute continuously, and
`while(true)` is one keystroke away.

Constraints that shaped the design:

- Everything runs on ESP32-C6 (`no_std + alloc`); the compiler *is* the
  product, so the mechanism must be cheap in code size and hot-path cycles.
- Filetests must be able to pin the semantics deterministically (a wall
  clock cannot).
- JIT'd code has no unwinder; aborting a deep call stack needs an explicit
  mechanism.
- A plain returned `Err` is invisible to the lp-recovery blame ledger — and
  the recovery frame's clean completion on the error path actively *heals*
  a yellow entry. Blame requires a caught panic.

## Decision

**Counter-based, per-invocation (per-pixel) fuel, always on, with a typed
trap threaded to the engine and converted to a caught panic for blame.**

### Fuel unit and tanks

- Unit = **loop back-edge executions** (≈ iterations). Deterministic and
  filetest-able; straight-line code consumes nothing.
- **Per-invocation tanks**: the LPIR-level synthesised render wrappers
  (`lp-shader/src/synth/render_texture.rs`, `render_samples.rs`) re-arm the
  tank at the top of each pixel/sample loop body with
  `DEFAULT_INVOCATION_FUEL = 100_000` and write the linear invocation index.
  The wrapper is synthesised below the GLSL frontend, so user code cannot
  reach the reset. A trapping pixel is genuinely the expensive one, and the
  budget is a small constant independent of texture size / LED count.

### Why back-edges are a sound unit

The claim behind the design: bounding back-edge traversals bounds every
loop-shaped divergence. The argument is structural, in two pigeonhole
steps:

1. An infinite execution path through a finite CFG visits finitely many
   nodes, so some node is visited infinitely often; each pair of
   consecutive visits closes a cycle — the path traverses cycles
   infinitely often.
2. LPIR's structured control flow (`LoopStart`/`End`/`Break`/`Continue`,
   no goto) lowers to a **reducible** CFG by construction: deleting
   back-edges leaves a DAG, so every cycle contains a back-edge. Lowering
   materialises exactly one back-edge per natural loop (`Continue` routes
   through the continuing block into the same `Br`), identified
   syntactically — no dominance analysis anywhere.

Hence any infinite path crosses back-edges infinitely often and hits the
decrementing check. The only divergence class outside the argument is
unbounded recursion, which diverges in stack rather than cycles (the
frontends do not emit it; entry checks deliberately do not meter it — a
1-per-activation decrement would overflow the stack long before denting
the tank).

Deliberate consequences of defining the unit at IR-structure level
(rather than counting instructions, as wasmtime fuel does):

- **Not a cost model.** Straight-line code is free and the unit is blind
  to loop-body size. The job is *termination detection*, not time
  accounting; the wall-clock bound is `budget × max-body-cost`, bounded
  in practice by compile/JIT-chunk budgets.
- **Deterministic and compiler-stable.** The count is a function of
  source-level loop structure, invariant under regalloc, peephole, and
  emission changes — filetests can assert exact remainders
  (`1_000_000 − N`) that survive backend evolution.
- **Inlining-invariant.** Inlining a loop-free callee removes only a
  non-decrementing entry check; a loopy callee's back-edges carry over
  unchanged.

**Randomness footnote** (soundness vs completeness): the structural
argument quantifies over individual paths, so randomized branch decisions
cannot escape it — any actually-infinite run traps regardless of how its
branches were chosen. What randomness breaks is *completeness*: an
almost-surely-terminating randomized loop (rejection sampling is the
graphics-shaped example; MTG's "Four Horsemen" loop is the folklore one)
terminates with probability 1 but has no finite iteration bound, so every
finite budget falsely traps a measure-tiny set of unlucky runs. Fuel
respects bounded termination, not almost-sure termination — deciding the
latter at runtime is not tractable (probabilistic-termination proofs need
ranking supermartingales, nothing a runtime checks cheaply). In this
codebase the case is theoretical: shader "randomness" (`lpfn_random`,
`lpfn_hash`, the noise family) is seeded-hash — a pure function of
coordinates/time/seed, with no entropy source in the sans-IO core — so
every execution is deterministic and every terminating shader has a
concrete per-input iteration count. A hash-based rejection loop with an
absurdly unlucky pixel traps with that pixel's coordinates in the
message, which is the designed outcome.

### VmContext header contract (16-byte header unchanged)

| Offset | Word | Meaning |
|---|---|---|
| 0 | fuel low u32 | remaining fuel counter (all arithmetic u32) |
| 4 | fuel high u32 | current invocation index; `0xFFFF_FFFF` = host-armed, outside any per-invocation wrapper |
| 8 | `trap` (renamed from never-read `trap_handler`) | trap code: 0 = none, 1 = out-of-fuel (`TRAP_CODE_OUT_OF_FUEL`) |
| 12 | `metadata` | untouched — reserved as the future home of probe trace state (`docs/future/2026-05-19-shader-probes.md`) |

### Check placement and abort

- **Loop back-edge**: check-then-decrement (7 emitted words per site, 5 on
  the hot path: `lw` / `bne` / `addi` / `sw` + the branch). The trap fires
  when a check *observes* 0; reaching exactly 0 on the final decrement and
  returning is not a trap.
- **Function entry**: check-only (no decrement). Loop-free functions consume
  nothing (preserves `__lp_get_fuel()` semantics pinned by
  `filetests/vmcontext/fuel-read.glsl`) and the entry check is the cascade
  cutoff.
- **Abort = epilogue cascade**: there is no unwinder in JIT'd code. On trap
  the check sequence writes `TRAP_CODE_OUT_OF_FUEL` to vmctx+8 and branches
  to the function's existing epilogue (callee-saved registers restore from
  the frame, so register state at the trap site is irrelevant). Fuel stays
  0, so every caller's next check fails too and the abort cascades up the
  stack naturally. Return values on this path are garbage; the trap code is
  the authoritative signal and the host discards outputs.

### Host arm / detect

`rt_jit` and `rt_emu` arm the header before **every** guest entry (fuel low
= `DEFAULT_VMCTX_FUEL as u32` = 1M for flat/init/compute entries — render
wrappers immediately re-arm per pixel; fuel high = armed sentinel; trap =
0; metadata untouched) and read the trap slot after return. Nonzero →
`NativeError::Trap { code, invocation }` and outputs are discarded.

### Typed marker → panic → blame

The trap flows to the engine as **data, never substring matching**:

- `lpvm::GuestTrapError` (accessor trait bound on `LpvmInstance::Error`)
  exposes `GuestTrap { code, invocation }`; only `NativeError` returns
  `Some`.
- `lp-shader`'s `px_shader` — the layer that knows the entry kind and the
  texture width — converts it to `LpsError::FuelExhausted(ShaderFuelTrap)`
  with derived coordinates (`(x, y) = (i % w, i / w)` for textures, sample
  index for LED batches) and the budget; `GfxError::FuelExhausted` carries
  it to the engine.
- `ShaderNode` (inside `catch_node_panic_framed`, `FrameKind::NodeRender`)
  converts the marker to a **panic** with the legible message, e.g.
  `shader fuel exhausted: render_texture pixel (34, 12) exceeded 100000
  iterations`. This is deliberate, limited panic-as-control-flow: only a
  caught panic records blame in lp-recovery (yellow → red-gate on repeat =
  the retry latch + the existing sticky blocked UX), while a returned `Err`
  would record nothing and its clean frame completion would heal an
  existing yellow. The panic message becomes the node error status.
  Non-fuel render errors keep returning plain `Err`.

### Posture

- **Always on** for device codegen (`NativeCompileOptions::default()` has
  `fuel: true`); the flag exists for tests/perf comparison only.
- **On for rv32 filetest targets** (rv32n/rv32lpn): tests exercise what
  ships. `filetests/fuel/` pins consumption semantics (1 unit per
  back-edge, entry checks free) and trap behavior (direct + through a
  callee cascade).
- **RWDT layering preserved**: fuel is layer 1 (in-frame, late-but-bounded
  abort, no reboot); the 8 s RWDT remains the layer-2 backstop for
  non-shader hangs and codegen bugs, as does the host emulator's
  instruction limit for fuel-off compiles.

### Accepted divergences

- **wasmtime keeps its own per-call store fuel** (~64× tank). Guest memory
  writes cannot reset wasmtime's fuel, so per-invocation tanks are
  impossible there; host builds get a per-call bound with a plain error (no
  blame route — the panic conversion only triggers on the native marker).
- **interp** has no loop bound at all (opt-in oracle target; recorded
  follow-up).
- Compute-tick and `__shader_init` traps surface as plain typed-message
  errors (bounded abort, no coordinates, no panic/blame route) — only the
  per-invocation render/sample paths get the panic conversion today.

## Consequences

- An infinite-loop pixel now costs one late-but-bounded frame: draining a
  100k tank ≈ 600k guest instructions (≈ 4 ms at 160 MHz) — three orders of
  magnitude under the 8 s RWDT. No reboot, and the node error names the
  exact pixel.
- Repeat offenses red-gate the node path via the existing ledger; the rest
  of the project keeps rendering (proven end-to-end in
  `fw-tests/tests/recovery_emu.rs::fuel_exhausted_shader_gates_without_reboot`).
- Measured cost (emulated rv32n, Q32, release; fuel-on vs fuel-off):
  - Runtime: **+7.1%** guest instructions on a user-GLSL fbm kernel
    (16 px × 5 octaves), **+7.7%** on a worley kernel (16 px × 3×3 cell
    search), **+19.5%** on a control-torture microkernel (tiny loop bodies
    — the worst-case shape), **+5 instructions total** on a loop-free flat
    call (entry checks only). Note: `lpfn_*` builtins are precompiled Rust
    (not lpvm-native-emitted), so their internal loops are unmetered — the
    overhead applies to user-GLSL loops.
  - Code size: +28 bytes per loop back-edge, +20 bytes per function entry;
    on the measured kernels +12–17% of emitted module bytes (fbm 1656 vs
    1472 B). Relevant to the ESP32 16 KB JIT chunk budget but small in
    absolute terms.
  - Compile time: within noise (< ~2% on ms-scale kernels; the ~194 ms
    device compile budget is unaffected beyond that).
  - fw-esp32 flash: the whole feature (codegen + runtime + error
    threading) costs **+2,576 bytes** of text (+0.08%; linked
    release-esp32 build, 3,096,048 vs 3,093,472 at the main merge-base).
- The per-site inline trap sequence trades bytes for simplicity; a
  per-function shared trap stub would save ~2 words per back-edge site
  (recorded follow-up).
- `DEFAULT_INVOCATION_FUEL = 100_000` is the initial calibrated budget
  (worst case well under RWDT; generous for legitimate per-pixel work —
  the whole existing corpus passes untouched). Final sign-off rides the
  hardware-smoke gate.

## Alternatives Considered

- **Wall-clock deadlines** (check a cycle counter / time source): matches
  human intuition ("too slow") but is nondeterministic, unfiletestable,
  and needs a clock in the sans-IO core. Rejected.
- **Per-frame tanks** (one budget per render call): simpler (host-armed
  only) but the budget would have to scale with texture size / LED count,
  and diagnostics degrade to "the frame died" instead of naming the
  offending pixel. Rejected — per-invocation is the user-preferred
  diagnostics-first shape and its budget is a small constant.
- **Plain-Err blame API** (thread the trap as a returned error and teach
  lp-recovery an explicit "record blame without panic" entry point): avoids
  panic-as-control-flow but invents a second blame path through every
  engine layer, and the FrameGuard clean-completion semantics would still
  heal yellow on the error path unless that is special-cased too. The
  caught panic reuses the one battle-tested route. Rejected.
- **Builtin-based reset** (a `__lp_reset_fuel()` builtin called by the
  wrappers instead of raw header stores): needs a builtin slot and a call
  per pixel; plain LPIR stores through the vmctx pointer compile to two
  `sw` instructions and work identically on every backend. Rejected.

## Follow-ups

- **Fuel heatmap / probe synergy**: the trap report (entry kind + pixel) is
  the natural entry point to the GLSL probe workflow — the trapping pixel
  IS the probe selection; per-pixel fuel-consumed also gives a cost
  heatmap. `metadata` (vmctx+12) stays reserved for probe trace state.
- **interp loop cap**: the interpreter still has unbounded loops (opt-in
  target only; guarded by `@unsupported(interp)` on trap filetests).
- **Per-function trap stub**: share one trap-write-and-jump stub per
  function to shrink back-edge sites from 7 to ~5 words if the JIT chunk
  budget gets tight.
- **Compute/init blame route**: compute-tick and shader-init traps abort
  bounded but bypass the panic/blame ledger today; wire them through if
  runaway compute shaders show up in practice.
- **wasm per-invocation parity**: revisit only if host-side blame for
  infinite shaders becomes a product need (wasmtime fuel cannot be reset
  from guest code — would need epoch interruption or a different scheme).
