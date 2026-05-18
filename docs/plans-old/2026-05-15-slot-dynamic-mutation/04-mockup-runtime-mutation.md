# Phase 4: Use Generic Mutation In The Mockup Runtime

## Scope Of Phase

Replace the mockup runtime's path-specific mutation apply code with generic slot mutation.

In scope:

- Keep the existing wire mutation request/response shape.
- Keep shape/data conflict behavior.
- Replace hard-coded setter dispatch with `lpc-model` generic mutation.
- Update mockup mutation tests.

Out of scope:

- Changing the wire protocol.
- Removing all domain-specific helper setters.
- Using generic mutation for serialization/deserialization.

## Code Organization Reminders

- Keep runtime orchestration in `runtime.rs`.
- Move reusable mutation logic to `lpc-model`; do not bury it in the mockup.
- Do not add mockup-specific code to `lpc-slot-codegen`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-mockup/src/engine/runtime.rs`
- `lp-core/lpc-slot-mockup/src/engine/shader_node.rs`
- `lp-core/lpc-slot-mockup/src/source/shader_def.rs`
- `lp-core/lpc-slot-mockup/src/tests/mutation.rs`

Current runtime has hard-coded targets:

```rust
MutationTarget::ShaderExposureParam
MutationTarget::ShaderExposureLabel
```

Replace this with:

1. Resolve root name to `&mut dyn SlotMutAccess`.
2. Use generic mutation to set the requested value.
3. Keep conflict checks:
   - shape revision check stays based on registry entry revision.
   - data revision check should be read generically from the target leaf before mutation.

If generic data revision lookup is not yet available, add a small generic helper beside `set_slot_value`:

```rust
pub fn slot_data_revision(
    root: &dyn SlotAccess,
    registry: &SlotShapeRegistry,
    path: &SlotPath,
) -> Result<Revision, SlotMutationError>
```

Tests:

- Existing `client_mutation_accepts_runtime_value_without_optimistic_write` still passes.
- Existing `client_mutation_accepts_source_value` passes through generic mutation.
- Wrong type, unknown path, unsupported target behavior remains clear.
- Add one enum-active-variant mutation test if there is a stable mockup enum path.

## Validate

```bash
cargo fmt -p lpc-model -p lpc-slot-mockup
cargo test -p lpc-slot-mockup mutation
cargo test -p lpc-slot-mockup
```
