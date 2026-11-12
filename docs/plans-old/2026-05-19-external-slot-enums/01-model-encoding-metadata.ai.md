# Phase 1: Model Encoding Metadata

## Scope of Phase

Add slot-model support for enum encoding metadata without changing dynamic codec behavior.

In scope:

- Add a `SlotEnumEncoding` type to the slot model.
- Add an `encoding` field to `SlotShape::Enum`.
- Preserve current default behavior as tagged enum encoding with discriminator field `kind`.
- Add or update shape builder helpers.
- Add focused model tests for default serde compatibility and helper output.

Out of scope:

- Dynamic reader/writer support for external encoding.
- Derive macro attributes.
- Shader source model changes.

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

Relevant files:

- `lp-core/lpc-model/src/slot/slot_shape.rs`
- `lp-core/lpc-model/src/slot/slot_shape_builder.rs`
- `lp-core/lpc-model/src/slot/mod.rs`

Expected changes:

1. Add a public enum:

   ```rust
   pub enum SlotEnumEncoding {
       Tagged { field: SlotName },
       External,
   }
   ```

2. Implement `Default` for `SlotEnumEncoding` as:

   ```rust
   Tagged { field: SlotName::parse("kind").expect("valid static slot name") }
   ```

3. Add serde support so old serialized shapes without an `encoding` field decode as the default tagged encoding.

4. Update `SlotShape::Enum`:

   ```rust
   Enum {
       #[serde(default)]
       meta: SlotMeta,
       #[serde(default, skip_serializing_if = "SlotEnumEncoding::is_default")]
       encoding: SlotEnumEncoding,
       variants: Vec<SlotVariantShape>,
   }
   ```

5. Update all direct construction of `SlotShape::Enum` in tests and builders to include `encoding` or use a helper.

6. In `slot_shape_builder.rs`, add helpers such as:

   ```rust
   pub fn enum_tagged(variants: Vec<SlotVariantShape>) -> SlotShape
   pub fn enum_external(variants: Vec<SlotVariantShape>) -> SlotShape
   ```

   Keep existing helper names, if any, producing tagged enum shapes.

7. Add rust docs explaining that encoding affects authored codec shape only, not the runtime slot data model.

Edge cases:

- Do not place encoding in `SlotMeta`; metadata is human-facing and not validation/codec semantics.
- Keep old shape JSON/TOML metadata backwards-compatible.

## Validate

Run:

```bash
cargo test -p lpc-model slot_shape --lib
cargo test -p lpc-model slot_factory --lib
cargo check -p lpc-model
```

