# Phase 1: Generated Node Def Views

## Scope Of Phase

Generate typed root views for all concrete authored node definitions.

In scope:

- Mark `ShaderDef`, `FixtureDef`, and `OutputDef` with `#[slot(root, view)]`.
- Export generated views from `lpc-model` where needed.
- Add or update compile/evidence tests for `ShaderDefView`, `FixtureDefView`,
  `OutputDefView`, and existing `TextureDefView`.

Out of scope:

- Runtime node behavior changes.
- Codegen feature expansion beyond root-field accessors.
- Aggregate typed view ergonomics.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep generated-view tests at the bottom of the corresponding def files.
- Do not hide new concepts in `mod.rs` beyond module declarations and re-exports.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/nodes/shader/shader_def.rs`
- `lp-core/lpc-model/src/nodes/fixture/fixture_def.rs`
- `lp-core/lpc-model/src/nodes/output/output_def.rs`
- `lp-core/lpc-model/src/nodes/texture/texture_def.rs`
- `lp-core/lpc-model/src/nodes/mod.rs`
- `lp-core/lpc-model/src/lib.rs`

Expected changes:

- Change node def attributes from `#[slot(root)]` to `#[slot(root, view)]`.
- Re-export generated view types where `TextureDefView` is currently exported.
- Add tests that:
  - register the static shape;
  - compile the generated view;
  - assert registry revision validity;
  - assert selected field paths match expected `SlotPath`s.

Edge cases:

- Generated views expose aggregate fields too. It is fine for tests to verify
  paths only; runtime phases decide which accessors can be read as values.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model
cargo test -p lpc-slot-codegen
cargo check -p lpc-model --features schema-gen
```
