# Phase 4: Remove `#[slot(skip)]` From The Simple Path

## Scope Of Phase

Make `SlotRecord` stricter and simpler.

In scope:

- Remove `#[slot(skip)]` from docs and macro support.
- Fix current records that use `#[slot(skip)]`.
- Convert discriminator or derived fields into explicit slot-modeled fields, wrapper types, or non-slot outer structs.
- Ensure slot records have public, slot-participating fields.

Out of scope:

- Designing future `#[slot(transient)]`.
- Preserving hidden fields inside slot records.
- Solving every domain-model awkwardness perfectly.

## Code Organization Reminders

- Keep records simple.
- If a type needs private runtime state, split it into an outer runtime object and an inner public slot model.
- Do not add special hidden field machinery to `SlotRecord`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Current known uses:

```bash
rg "#\\[slot\\(skip\\)\\]" lp-core/lpc-model lp-core/lpc-slot-mockup
```

Known areas:

- `lp-core/lpc-model/src/nodes/project/project_def.rs`
- `lp-core/lpc-model/src/nodes/fixture/fixture_def.rs`
- `lp-core/lpc-slot-mockup/src/source/project_def.rs`
- `lp-core/lpc-slot-mockup/src/source/fixture_def.rs`
- `lp-core/lpc-slot-mockup/src/source/output_def.rs`
- `lp-core/lpc-slot-mockup/src/source/shader_def.rs`
- `lp-core/lpc-slot-mockup/src/source/texture_def.rs`

Update:

- `lp-core/lpc-slot-macros/src/lib.rs`
- `lp-core/lpc-slot-macros/src/attr.rs`
- `lp-core/lpc-slot-macros/src/record.rs`

The derive should fail if a field cannot be modeled, rather than silently skipping it.

Suggested patterns:

- If a field is authored/persisted/synced, make it a slot field.
- If a field is runtime-only, move it out of the `SlotRecord` type.
- If a field is a discriminator, model it explicitly or handle it in the enum/wrapper layer.
- If a field is complex, create a slot-data sub-record and delegate.

## Validate

```bash
cargo fmt
cargo test -p lpc-slot-macros
cargo test -p lpc-model slot_record
cargo test -p lpc-slot-mockup
```
