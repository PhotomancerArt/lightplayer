# Phase 5: Port Fixture Demand-Root Sampling Node

## Scope of Phase

Port fixture behavior into a first-class core `Node` that acts as a demand root,
resolves shader/pattern render products, samples them, and pushes color/channel
data into output sinks.

In scope:

- Add/complete `nodes::core::FixtureNode`.
- Reuse legacy fixture mapping/accumulation internals where appropriate.
- Resolve render products from shader/pattern producers.
- Sample render products through the render-product API.
- Write non-visual output/fixture data through runtime services/runtime buffers.
- Add focused tests for demand-root sampling.

Out of scope:

- Output provider flushing.
- Server wiring.
- Full buffer sync.
- New fixture model features beyond MVP behavior.

## Code Organization Reminders

- Keep fixture node code separate from legacy runtime code.
- Prefer extracting reusable pure helpers from legacy mapping modules only when
  necessary.
- Tests should be concise and use builders/helpers.
- Record tactical shortcuts in `future.md`.

## Sub-agent Reminders

- Do not commit.
- Stay within phase scope.
- Do not suppress warnings or weaken tests.
- If fixture sampling cannot be expressed through the current render-product API,
  stop and report instead of reading raw texture internals.
- Report changed files, validation results, and deviations.

## Implementation Details

Read first:

- `lp-core/lpc-engine/src/legacy/nodes/fixture/runtime.rs`.
- `lp-core/lpc-engine/src/legacy/nodes/fixture/mapping/`.
- `lp-core/lpc-engine/src/nodes/core/` from previous phases.
- `lp-core/lpc-engine/src/render_product/`.
- `lp-core/lpc-engine/src/project_runtime/runtime_services.rs`.
- `lp-core/lpc-engine/src/engine/engine.rs` demand-root behavior.

Behavior:

- Fixture nodes are demand roots.
- A fixture resolves the render products it needs.
- The fixture samples render products and writes output/color data into the
  output sink side of runtime services.
- Outputs remain pushed sinks; do not make outputs pull demand roots.
- Preserve room for future many-to-many fixture -> output mapping.

Tests:

- Fixture demand root resolves a shader/render product once per frame.
- Fixture samples a deterministic render product and writes expected channel or
  color data.
- Same-frame cache prevents duplicate producer work where applicable.

## Validate

Run:

```bash
cargo test -p lpc-engine fixture
cargo test -p lpc-engine engine
```
