# Phase 2: Authored Serde For Semantic Slots

## Scope Of Phase

Add authored serde behavior for all current semantic slot leaf types.

In scope:

- Semantic slots in `lpc-model/src/slot/slots/` serialize as clean authored
  values.
- Semantic slot deserialization stamps versions with `current_state_version()`.
- Tests cover every current semantic slot type.

Out of scope:

- Adding new real source-specific semantic slots unless a current semantic slot
  cannot represent the mockup needs.
- Real `lpc-source` conversion.
- Mockup mapping changes.

## Code Organization Reminders

- Keep one semantic concept per file.
- Do not add noisy suffixes inside `slot/slots/`; use `resource_ref.rs`, not
  `resource_ref_slot.rs`.
- Keep serde impls near the semantic slot type they belong to.
- Tests stay at the bottom of files, or use a focused grouped test module if
  that is clearer.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot/slots/ratio.rs`
- `lp-core/lpc-model/src/slot/slots/positive_f32.rs`
- `lp-core/lpc-model/src/slot/slots/xy.rs`
- `lp-core/lpc-model/src/slot/slots/dim2u.rs`
- `lp-core/lpc-model/src/slot/slots/affine2d.rs`
- `lp-core/lpc-model/src/slot/slots/color_order.rs`
- `lp-core/lpc-model/src/slot/slots/relative_node_ref.rs`
- `lp-core/lpc-model/src/slot/slots/render_order.rs`
- `lp-core/lpc-model/src/slot/slots/source_path.rs`
- `lp-core/lpc-model/src/slot/slots/artifact_path.rs`
- `lp-core/lpc-model/src/slot/slots/resource_ref.rs`
- `lp-core/lpc-model/src/slot/slots/mod.rs`

Expected behavior:

- `RatioSlot`, `PositiveF32Slot`, `RenderOrderSlot`, and `XySlot` serialize as
  scalar/vector values.
- `SourcePathSlot`, `ArtifactPathSlot`, and `RelativeNodeRefSlot` serialize as
  strings.
- `Dim2uSlot` and `Affine2dSlot` serialize as their semantic value structs.
- `ColorOrderSlot` serializes as the semantic enum/string value.
- `ResourceRefSlot` serializes through `ResourceRef`.
- Every semantic slot deserializer uses `current_state_version()`.

Tests:

- Round-trip every semantic slot through `serde_json`.
- For TOML-compatible slots, add TOML checks where the authored shape matters.
- Assert version stamping for at least representative scalar, string, record,
  enum, and resource slots.

Constraints:

- Do not leak `Versioned<T>` internals.
- Do not make semantic slots depend on `std`.
- If a semantic inner value lacks serde, add serde to that value narrowly and
  keep it in `lpc-model`.

## Validate

```bash
cargo fmt --package lpc-model
cargo test -p lpc-model --lib --tests
cargo check -p lpc-model --features schema-gen
```
