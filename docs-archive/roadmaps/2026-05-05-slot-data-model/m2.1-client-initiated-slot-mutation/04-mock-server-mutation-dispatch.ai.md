# Phase 4: Mock Server Mutation Dispatch

## Scope Of Phase

Add mock server-side mutation application and optimistic-lock validation.

In scope:

- `MockRuntime::apply_slot_mutation`.
- Generic validation for root, path, shape version, data version, and value type.
- Explicit owner dispatch for the first supported mutation targets.
- Tests for accepted and rejected mutation responses.

Out of scope:

- Broad generic mutation traits.
- Structural mutation operations beyond the test slice.
- Real server/message transport wiring.

## Code Organization Reminders

- Mutate owning Rust objects, not server-side `SlotData` projections.
- Keep supported target dispatch explicit and readable.
- Add helper functions below public runtime methods.
- Do not add temporary TODOs.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-mockup/src/engine/runtime.rs`
- `lp-core/lpc-slot-mockup/src/source/shader_def.rs`
- `lp-core/lpc-slot-mockup/src/tests/fixture.rs`
- new or updated mutation test file under
  `lp-core/lpc-slot-mockup/src/tests/`

Accepted mutation targets:

- `engine.shader_node`, path `params.exposure`, `SetValue(F32)`.
- `source.shader`, path `param_defs.exposure.label`, `SetValue(String)`.

Rejection cases:

- unknown root,
- unknown path,
- stale root shape version,
- stale target data version,
- wrong value type,
- unsupported valid target.

## Validate

```bash
cargo test -p lpc-slot-mockup mutation -- --nocapture --test-threads=1
```
