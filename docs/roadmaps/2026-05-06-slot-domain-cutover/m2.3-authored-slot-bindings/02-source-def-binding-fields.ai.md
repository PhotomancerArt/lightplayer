# Phase 2: Source Def Binding Fields

## Scope Of Phase

Add shared `bindings` fields to real node definitions and begin replacing
bespoke connectivity fields with authored slot bindings.

In scope:

- Add default-empty `bindings: BindingDefs` to relevant node defs.
- Replace `ShaderDef.texture_loc` with a produced `output` binding.
- Replace `TextureDef` connectivity expectations with a consumed `input`
  binding.
- Replace `FixtureDef.texture_loc` with a consumed `input` binding.
- Decide from implementation pressure whether `FixtureDef.output_loc` moves now
  or remains until M2.4 output sink registration cleanup.
- Update source-def tests for new fields and defaults.

Out of scope:

- Full runtime node flow refactor.
- Bus resolution policy.
- Invocation-site binding overrides.
- Removing all old source prop code.

## Code Organization Reminders

- Keep node def files focused on durable model shape.
- Do not add new binding types under `lpc-source`.
- If helpers are needed for common default bindings, place them near the model
  concepts they belong to.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-source/src/node/shader/shader_def.rs`
- `lp-core/lpc-source/src/node/texture/texture_def.rs`
- `lp-core/lpc-source/src/node/fixture/fixture_def.rs`
- `lp-core/lpc-source/src/node/output/output_def.rs`
- `lp-core/lpc-source/src/node/node_invocation.rs`
- `lp-core/lpc-source/src/node/project/mod.rs`

Despite the paths, these node defs are shared semantic model concepts in the
current crate layout. Binding types should be imported from `lpc-model`.

Expected changes:

- `ShaderDef`
  - remove or deprecate `texture_loc`
  - add `bindings: BindingDefs`
  - default should publish `output` to a reasonable bus endpoint only if that is
    a deliberate domain default; otherwise default-empty and examples specify
    the binding explicitly.

- `TextureDef`
  - add `bindings: BindingDefs`
  - examples should bind `input` from `bus#visual.out`.

- `FixtureDef`
  - remove or deprecate `texture_loc`
  - add `bindings: BindingDefs`
  - examples should bind `input` from texture output or bus, depending on the
    target flow selected for M2.3.
  - `output_loc` may remain temporarily if moving it would force output sink
    runtime changes before M2.4.

- `OutputDef`
  - add `bindings` only if there is a real authored slot binding for outputs in
    this milestone.
  - Do not invent output consumer semantics here.

- `NodeInvocation`
  - do not expand `overrides`.
  - if `overrides` still references `SrcBinding`, isolate or mark it
    transitional for Phase 5.

Tests to update/add:

- Each def deserializes with absent `bindings`.
- Each def with `bindings` round-trips through TOML.
- Old connectivity fields removed in this phase no longer appear in canonical
  generated/fixture TOML.

## Validate

Run:

```bash
cargo fmt --package lpc-source
cargo test -p lpc-source
cargo check -p lpc-source --features schema-gen
```
