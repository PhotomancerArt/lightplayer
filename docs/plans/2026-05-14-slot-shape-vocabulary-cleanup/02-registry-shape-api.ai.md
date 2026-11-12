# Phase 2: Registry Shape API

## Scope Of Phase

Add shape-named registry APIs and migrate internal call sites away from
root-named registry methods.

In scope:

- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`
- Call sites of:
  - `register_root*`
  - `ensure_root*`
  - `replace_root*`
  - `unregister_root*`
- Tests in `slot_shape_registry.rs`.

Out of scope:

- Renaming `SlotAccessor` fields.
- Changing registry serialization shape.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Add preferred APIs:

- `register_shape`
- `register_shape_named`
- `register_shape_with_version`
- `register_shape_named_with_version`
- `ensure_shape`
- `ensure_shape_named`
- `ensure_shape_with_version`
- `ensure_shape_named_with_version`
- `replace_shape`
- `replace_shape_named`
- `replace_shape_with_version`
- `replace_shape_named_with_version`
- `unregister_shape`
- `unregister_shape_with_version`

Then migrate internal call sites in:

- `lp-core/lpc-model/src/slot/slot_access.rs`
- `lp-core/lpc-slot-mockup/src/engine/runtime.rs`
- generated code expectations in `lp-core/lpc-slot-codegen/src/lib.rs`
- tests under `lp-core/lpc-model/src/slot`
- any other `rg` hits.

Compatibility:

- Prefer migrating all call sites and removing old root-named registry methods.
- Keep compatibility wrappers only if removal creates unexpected scope.

Searches:

```bash
rg -n "register_root|ensure_root|replace_root|unregister_root" lp-core docs/design/slots
```

## Validate

```bash
cargo test -p lpc-model slot_shape_registry
cargo test -p lpc-model slot_accessor
cargo test -p lpc-slot-codegen
cargo test -p lpc-slot-mockup shape_codegen
```
