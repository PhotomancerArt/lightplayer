# ADR: A spent loop budget on the GPU tier faults the frame

- **Status:** Accepted
- **Date:** 2026-09-07
- **Deciders:** Photomancer
- **Plan:** `lp2025/2026-09-07-0102-gpu-tier-spent-budget-faults`
- **Fixes:** the "bounded, not faulted" gap accepted in
  [2026-09-06-gpu-tier-loop-bounds.md](2026-09-06-gpu-tier-loop-bounds.md) §2
- **Supersedes:** None (amends §2 and the Consequences of the 2026-09-06 ADR)
- **Superseded by:** None
- **Related:** [2026-09-06-gpu-tier-loop-bounds.md](2026-09-06-gpu-tier-loop-bounds.md)
  (the budget this reports), [2026-09-02-fault-is-never-black.md](2026-09-02-fault-is-never-black.md)
  (what a fault means), [2026-07-09-preview-fidelity-tiers.md](2026-07-09-preview-fidelity-tiers.md)
  (the tier contract)

## Context

The 2026-09-06 ADR gave every GPU-compiled loop a per-invocation back-edge
budget equal to the LPVM's fuel tank, and accepted that a spent budget
exits the loop and lets the invocation complete: the GPU cannot trap, so
the tier was *bounded, not faulted*. The LPVM tiers trap
(`GfxError::FuelExhausted`), the shader node turns that into a tick
error, the engine classifies it `Fault`, and after one second every output
of the project paints the never-black fault pattern. On a GPU tier the
same content rendered whatever the bounded loop left behind — a frame that
looks like a program, from a program that had failed.

The alternatives the 2026-09-06 ADR rejected for its first cut were
`discard` (paints black), a NaN sentinel (quantises to 0 on the product
path) and a storage-buffer flag (needs a binding, a read-back and a
per-frame policy). This ADR takes the third.

## Decision

### 1. The module counts the invocations that ran dry

`loop_bound_pass::bound_loop_iterations` also gives a module with loops
one `@group(0)` storage global `lp_gfx_loop_fault: atomic<u32>`
(read_write), on the next free binding after the authored uniforms and the
assigned textures, and in every loop's `continuing` block — beside the
budget charge — adds

```wgsl
if (spent == BUDGET + 1u) { atomicAdd(&lp_gfx_loop_fault, 1u); }
```

The budget keeps counting past the cap, so only the first loop to cross
sees `spent == BUDGET + 1`; every later loop in the same invocation breaks
at once without charging. The flag is therefore the **number of
invocations** of the dispatch that spent their budget, not the number of
loop exits. A loop-free module gets no flag and no binding.

The statement order in `continuing` is part of the decision: the budget
is stored first and **read back** for both the crossing test and the
`break if`. The obvious shape — compute the sum once, test it, store it,
break on it — bounds the loop correctly on Metal but the guarded
`atomicAdd` never executes for a top-level loop
(`docs/defects/2026-09-07-metal-drops-atomic-guarded-by-loop-exit-sum.md`);
every shape whose `break if` reads the variable back after the store
counts. The device test holds the working shape.

The payload is a count, not a coordinate. The LPVM reports the pixel or
sample that trapped because it stops there; a GPU runs every invocation
to its bound and reports afterwards, and a coordinate would need
`gl_FragCoord` plumbed into helper functions and a grid index into the
sample unit. The count is what the pass has in hand at every back-edge and
it reads the same on the render, sample and probe paths.

### 2. The host clears, captures and collects the flag around each dispatch

`fault_flag.rs` owns the bound buffer and a 4-byte `MAP_READ` staging
copy per `GpuShader`. Every dispatch — `render`, the sample pass, the
filetest probe — clears the flag in the command stream before its draw,
copies it into the staging buffer after, and reads the count after the
submit. A non-zero count is returned as `GfxError::FuelExhausted(
ShaderFuelTrap { entry: Invocations { spent }, budget })`, the LPVM's own
error type with a third entry variant, so the shader node's existing arm
routes it and the engine classifies it `Fault`. Nothing in the engine
changes.

How the count leaves the GPU follows the read-back doctrine:

- **native** — the dispatch waits for its own submission (bounded by the
  product read-back wait, 10 s; the probe's 20 s) and reports the fault
  for the frame that ran dry. `render` did not wait before; it does now,
  once per call. The native `render` callers are the harness and the
  filetest probe; the native product path (sampling) already blocked.
- **browser** — a blocking map is forbidden. The dispatch harvests the
  *previous* frame's map before it draws, copies only while no map is
  outstanding, issues the map after the submit, and reports what it
  harvested: **one frame of latency**. Harvesting before the draw rather
  than after keeps a capture in flight every frame, so a runaway faults on
  every frame and the engine's one-second persistence rule trips; a
  harvest-after-draw design would capture every other frame and the fault
  would flicker below the rule.

### 3. The shape is per shader, not per device

The flag is a resource of the compiled shader, in its own bind group
layout, so a fault attributes to the node that compiled the program. The
sample pass reuses the shader's layout and bind group, so the flag rides
into the point-list draw unchanged; the sample unit is checked to bind the
flag on the same slot as the render unit, as its uniforms already are.

## Consequences

- A data-dependent runaway paints the fault pattern on every tier. The
  GPU tiers report one frame late in the browser and same-frame natively.
- The diagnostic differs by tier: the LPVM names the first pixel or
  sample; the GPU names how many invocations ran dry
  (`shader fuel exhausted: 16 invocation(s) exceeded 100000 iterations`).
- Every shader with loops carries one more `@group(0)` binding and one
  4-byte storage buffer; a shader with loops but no uniforms or textures
  now has a bind group where it had none. Loop-free shaders are unchanged.
- The per-back-edge cost grows by one compare, a never-taken branch and
  a read-back of the budget (a register after the driver's own
  promotion); the atomic runs once per invocation that faults, never on
  a healthy frame. The parity envelopes hold (`just test-gfx`).
- The crossing charge is pinned on a device, not only on the IR: the
  Metal miscompile shows a shape can validate, bound the loop and still
  not count. `tests/loop_fault.rs` is the guard; it runs wherever an
  adapter exists (the M2 Max), not in CI.
- Native `render` is synchronous now. The harness's per-tick timings
  include the wait; the browser's frame cadence is untouched.
- WebGPU compatibility mode, where a fragment stage may have no storage
  buffers, would refuse the pipeline. It is not a target.

## Alternatives considered

- **Keep the gap.** The 2026-09-06 decision. Rejected once the fault
  pattern was the product's answer to "is this output lying": a bounded
  GPU frame is exactly the lie the never-black ADR exists to stop.
- **`discard` on the crossing.** Paints black; forbidden by the never-black
  ADR, and a fragment cannot discard from a helper function anyway.
- **A NaN sentinel in the output.** Quantises to 0 on the product path and
  is indistinguishable from authored black; only the raw-float probe could
  see it.
- **A coordinate payload (`atomicMin` of `1 + linear index`).** Faithful to
  the LPVM's message, but needs `gl_FragCoord` in every function with a
  loop and a grid index in the sample unit. Recorded as future work; the
  count is enough to fault.
- **Harvest after the draw on the browser.** Simpler ordering; captures
  every other frame, and the fault flickers below the one-second rule.
- **One flag per device.** Fewer resources; loses attribution to the node,
  and the sample pass would need a second bind group.

## Follow-ups

- A first-runaway coordinate in the payload if a GPU tier ever needs to
  point at a pixel.
