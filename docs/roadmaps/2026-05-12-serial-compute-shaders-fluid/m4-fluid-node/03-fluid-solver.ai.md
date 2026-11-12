# Phase 3: Fluid Solver

## Scope Of Phase

Move/adapt the firmware prototype solver into engine-owned fluid runtime modules.

In scope:

- Copy/adapt RGB Q32 MSAFluid solver.
- Add emitter stamping helpers.
- Add sampler helpers.
- Add focused solver tests in `lpc-engine`.

Out of scope:

- Runtime node integration.
- Project loader integration.
- Wgpu/GPU solver.

## Code Organization Reminders

- Use `lp-core/lpc-engine/src/nodes/fluid/`.
- Keep solver, emitter stamping, and sampling separate:
  - `solver.rs`
  - `emit.rs`
  - `sampler.rs`
- Avoid importing firmware test modules from engine.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report changed files, validation, and deviations.

## Implementation Details

Reference files:

- `lp-fw/fw-esp32/src/tests/msafluid_solver.rs`
- `lp-fw/fw-esp32/src/tests/fluid_demo/emitters.rs`
- `lp-fw/fw-esp32/src/tests/fluid_demo/sampler.rs`
- `lp-fw/fw-esp32/src/tests/fluid_demo/runner.rs`

Implement or adapt:

- `MsaFluidSolver`
- `FluidSolverConfig`
- `stamp_emitter`
- nearest sampling from normalized/q16 coordinates to `rgba_unorm16`

Important constraints:

- Keep solver `no_std + alloc` compatible.
- Use Q32 internally as the prototype does.
- Avoid hot-path `f32` where reasonable, especially in sampling loops.
- Do not bring demo pulser logic into engine.

Emitter interpretation:

- `pos`: normalized coordinates.
- `dir`: direction vector.
- `radius`: normalized radius.
- `color`: RGB.
- `velocity`: force magnitude.
- `intensity`: color/force gain.
- Clamp to grid bounds.
- Treat invalid/zero direction as no force but still allow color stamping.

Add tests:

- Solver allocates expected channel buffers.
- Stamping one emitter changes nearby color/velocity.
- Update fades/diffuses nonzero color without panicking.
- Sampling nonzero cell returns nonzero `rgba_unorm16`.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-engine fluid
```

