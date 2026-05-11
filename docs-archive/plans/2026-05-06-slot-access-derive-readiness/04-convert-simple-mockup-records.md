# Phase 4: Convert Simple Mockup Records

## Scope Of Phase

Use the new derive for the simplest mockup records first.

In scope:

- Enable the `derive` feature for `lpc-slot-mockup`'s `lpc-model` dependency.
- Convert simple records:
  - `OutputDef`
  - `TextureDef`
  - `OutputNode`
  - `NodeInvocationDef`
  - `ScalarHint`
  - `TouchState`
- Remove manual `SlotAccess`, `StaticSlotAccess`, and `SlotRecordAccess` impls for converted types.
- Keep tests green.

Out of scope:

- Complex roots such as `ShaderDef`, `ProjectDef`, `FixtureDef`, and `FixtureNode`.
- Dynamic `ShaderNode`.
- Enum derive.

## Code Organization Reminders

- Keep annotations readable and explicit.
- Do not preserve manual impls once the derive replaces them.
- Keep public API changes scoped to the mockup unless required by the derive.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-mockup/Cargo.toml`
- `lp-core/lpc-slot-mockup/src/source/output_def.rs`
- `lp-core/lpc-slot-mockup/src/source/texture_def.rs`
- `lp-core/lpc-slot-mockup/src/source/project_def.rs`
- `lp-core/lpc-slot-mockup/src/source/shader_def.rs`
- `lp-core/lpc-slot-mockup/src/engine/output_node.rs`
- `lp-core/lpc-slot-mockup/src/engine/fixture_node.rs`

Pay attention to root vs inline records:

- Root records use `#[slot(shape_id = "...")]`.
- Inline records only derive record shape/access.

## Validate

```bash
cargo test -p lpc-slot-mockup
cargo test -p lpc-slot-mockup -- --nocapture --test-threads=1
cargo test -p lpc-model --features derive
git diff --check
```
