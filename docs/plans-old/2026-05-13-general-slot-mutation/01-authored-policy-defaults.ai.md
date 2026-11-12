# Phase 1: Authored Policy Defaults

## Scope of phase

In scope:

- Extend `SlotRecord` derive support so authored records can declare a default slot policy for generated fields.
- Add field-level policy override support for explicit opt-out.
- Update authored node-definition records to default to writable persisted policy.
- Add or update derive and shape tests that prove authored defs become writable by default while opt-out remains available.

Out of scope:

- Server-side mutation application.
- Runtime-state mutation.
- UI behavior changes beyond what falls out from shape policy.

## Code organization reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Relevant files and symbols:

- `lp-core/lpc-slot-macros/src/attr.rs`
- `lp-core/lpc-slot-macros/src/record.rs`
- `lp-core/lpc-model/src/slot/slot_policy.rs`
- `lp-core/lpc-model/src/slot/slot_shape.rs`
- `lp-core/lpc-model/tests/slot_record_derive.rs`
- authored defs under `lp-core/lpc-model/src/nodes/**`

Expected changes:

- Add container-level derive attribute parsing for an authored default policy.
- Add field-level derive attribute parsing for explicit policy override.
- Change macro output to build fields with `field_with_semantics_and_policy(...)`.
- Choose an annotation strategy for authored defs that is explicit at the root record level rather than relying on a global `SlotPolicy::default()` change.
- Apply the authored writable-default annotation to root node-def structs such as:
  - `ProjectDef`
  - `ClockDef`
  - `TextureDef`
  - `ShaderDef`
  - `ComputeShaderDef`
  - `FluidDef`
  - `OutputDef`
  - `FixtureDef`
- Keep non-def helper/runtime records unchanged unless they clearly belong in the authored-default domain.

Tests to add or update:

- Derive test proving a record with authored writable-default generates writable policies on ordinary fields.
- Derive test proving explicit read-only field override wins over the container default.
- One authored-def test that confirms a real node-def shape contains writable policy on a representative field such as output brightness or pin/options leaves.

Constraints and edge cases:

- Do not change `SlotPolicy::default()` globally.
- Keep handwritten shape implementations working unchanged.
- Preserve existing semantics generation and merge-policy behavior.

## Validate

```bash
cargo test -p lpc-model slot_record_derive
cargo test -p lpc-model output_def
```

