# Phase 4: Metadata, Docs, And Legacy Audit

## Scope Of Phase

In scope:

- Add `SlotMeta::writable` with positive wording and conservative default.
- Document conventional root names and M1 bridge decisions.
- Clarify `RuntimeProduct` docs.
- Audit `Kind` and old prop vocabulary, marking it transitional where appropriate.

Out of scope:

- Final product UI metadata vocabulary.
- Replacing `Kind`.
- Runtime slot root implementation.

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

Update:

- `lp-core/lpc-model/src/slot/slot_meta.rs`
- `lp-core/lpc-model/src/slot/slot_shape_builder.rs` if builder helpers should expose writable metadata later.
- `lp-core/lpc-engine/src/runtime_product/runtime_product.rs`
- `lp-core/lpc-model/src/prop/mod.rs` and any relevant `Kind` docs if still misleading.
- Plan notes/design if execution reveals a better convention.

`SlotMeta` should become:

```rust
pub struct SlotMeta {
    pub label: Option<String>,
    pub description: Option<String>,
    pub writable: bool,
}
```

Use `#[serde(default, skip_serializing_if = "is_false")]` or equivalent so existing serialized shapes remain clean.

Add tests that default metadata is not writable and JSON round-trips.

## Validate

```bash
cargo fmt -p lpc-model -p lpc-engine
cargo test -p lpc-model
cargo check -p lpc-model --features schema-gen
cargo check -p lpc-engine
```

