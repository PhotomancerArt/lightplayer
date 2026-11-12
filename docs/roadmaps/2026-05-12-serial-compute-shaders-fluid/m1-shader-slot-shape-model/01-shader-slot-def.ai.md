# Phase 01: Shader Slot Def

## Scope Of Phase

Replace the old visual-only `ShaderParamDef` concept with a general
`ShaderSlotDef` model.

In scope:

- Add `shader_slot_def.rs`.
- Add `shader_slot_mapping.rs`.
- Change `ShaderDef.param_defs` to `MapSlot<String, ShaderSlotDef>`.
- Remove `ShaderParamDef` and `ScalarHint` from `lpc-model` exports if no real
  dependency remains.
- Update model, mockup, and wire tests that reference shader param defs.

Out of scope:

- `ComputeShaderDef`.
- `FluidEmitter`.
- Runtime shader param materialization beyond keeping existing tests coherent.

## Code Organization Reminders

- Keep one concept per file.
- Put public types and core impls above helpers and tests.
- Put tests at the bottom.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- Report changed files and validation.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/nodes/shader/shader_def.rs`
- `lp-core/lpc-model/src/nodes/shader/shader_param_def.rs`
- `lp-core/lpc-model/src/nodes/shader/mod.rs`
- `lp-core/lpc-model/src/lib.rs`
- `lp-core/lpc-slot-mockup/src/source/shader_def.rs`
- `lp-core/lpc-wire/tests/source_slot_sync.rs`

Expected model:

- `ShaderSlotDef` is a Rust-authored slot record.
- M1 must support at least:
  - `kind = "value"` with `value = "f32"` or native names later;
  - `kind = "map"` with `key = "u32"` and `value = "lp::fluid::Emitter"` later;
  - optional `mapping`.
- `ShaderSlotMappingDef` supports `kind = "sentinel"`, `len`, `key`, and
  `empty_key`.

Use compact authored TOML:

```toml
[param_defs.exposure]
kind = "value"
value = "f32"
default = 1.0
```

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model shader
cargo test -p lpc-slot-mockup dynamic_param_shape
cargo test -p lpc-wire source_slot_sync
```

