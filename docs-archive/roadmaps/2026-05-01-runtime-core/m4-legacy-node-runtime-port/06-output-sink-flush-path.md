# Phase 6: Port Output Sink And Flush Path

## Scope of Phase

Port output behavior into the new runtime as a pushed sink that receives fixture
data and flushes dirty output buffers through `OutputProvider`.

In scope:

- Add/complete `nodes::core::OutputNode` or a runtime service-backed output sink
  type.
- Open/write/close output handles through existing `OutputProvider` APIs.
- Track dirty outputs per frame.
- Flush outputs after fixture demand-root ticks.
- Add focused output sink tests with `MemoryOutputProvider`.

Out of scope:

- Server wiring.
- Full client sync.
- New output protocols.
- Many-to-many mapping beyond keeping the API compatible with that future.

## Code Organization Reminders

- Keep output sink state and output node behavior easy to read.
- Place helpers near the bottom.
- Avoid duplicating large chunks of legacy output runtime if a smaller service
  abstraction works.
- Record temporary shortcuts in `future.md`.

## Sub-agent Reminders

- Do not commit.
- Stay within phase scope.
- Do not suppress warnings or weaken tests.
- If output flushing requires a post-tick hook not planned, stop and report with
  the smallest API needed.
- Report changed files, validation results, and deviations.

## Implementation Details

Read first:

- `lp-core/lpc-engine/src/legacy/nodes/output/runtime.rs`.
- `lp-core/lpc-engine/src/legacy/project.rs` output flush section.
- `lp-core/lpc-engine/src/project_runtime/runtime_services.rs`.
- `lpc_shared::output` provider types.
- Existing tests using `MemoryOutputProvider`.

Behavior:

- Outputs are pushed sinks, not demand roots.
- Fixture nodes push channel/color data to output sinks.
- Runtime services track which output sinks were changed this frame.
- After engine tick completes, `CoreProjectRuntime::tick` flushes dirty outputs
  through `OutputProvider`.

Tests:

- Writing output data marks an output dirty.
- `CoreProjectRuntime::tick` or an explicit flush method writes expected bytes to
  `MemoryOutputProvider`.
- Untouched outputs do not flush.

## Validate

Run:

```bash
cargo test -p lpc-engine output
cargo test -p lpc-engine project_runtime
```
