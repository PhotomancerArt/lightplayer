# Phase 2: Dynamic Codec Support

## Scope of Phase

Teach the dynamic slot reader and writer to support externally tagged enum encoding.

In scope:

- Dispatch enum read/write behavior based on `SlotEnumEncoding`.
- Preserve existing tagged enum behavior.
- Support external enum payloads for value, record, unit, map, option, and reference shapes.
- Add focused tests for TOML and JSON behavior if both writers/readers share the implementation.

Out of scope:

- Derive macro attributes.
- New authored shader syntax.
- Field-presence / `#[slot(key)]` discrimination.

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

- `lp-core/lpc-model/src/slot_codec/dynamic_slot_reader.rs`
- `lp-core/lpc-model/src/slot_codec/dynamic_slot_writer.rs`
- `lp-core/lpc-model/src/slot_codec/mod.rs`
- `lp-core/lpc-model/src/slot_codec/toml_syntax_source.rs`
- `lp-core/lpc-model/src/slot_codec/json_syntax_source.rs`

Expected reader changes:

1. Change `read_enum` to dispatch:

   ```rust
   match encoding {
       SlotEnumEncoding::Tagged { field } => read_tagged_enum(...),
       SlotEnumEncoding::External => read_external_enum(...),
   }
   ```

2. Tagged enum behavior should remain equivalent to today, except the discriminator field comes from metadata rather than hard-coded `"kind"`.

3. External enum behavior:

   - Read the enum value as an object.
   - Require exactly one property.
   - Property name must match a variant.
   - Switch enum to that variant.
   - Decode the property value into the selected variant payload by calling `apply_reader_to_slot`.
   - Error on zero properties.
   - Error on multiple properties.
   - Error on unknown variant property.

Expected writer changes:

1. Change TOML and JSON enum writers to dispatch by encoding.

2. Tagged writer stays equivalent to today, except the discriminator field comes from metadata.

3. External writer:

   - Creates an object/table with exactly one property.
   - Property name is the active variant name.
   - Property value is the payload written through the normal shape writer.

Tests to add:

- Tagged enum decode/write still emits `kind`.
- External value/newtype payload:

  ```toml
  file = "compute.glsl"
  ```

- External record payload:

  ```toml
  [a]
  x = 10
  y = 10
  ```

- External unit payload, using either an empty table/object or whatever representation the writer naturally produces.
- External enum rejects empty object.
- External enum rejects multiple variant properties.
- External enum rejects unknown variant property.

Edge cases:

- `TomlSyntaxSource` sorts `kind` first today. External encoding should not depend on property ordering.
- If JSON writer and TOML writer require separate helper paths, keep behavior parallel and tests explicit.

## Validate

Run:

```bash
cargo test -p lpc-model slot_codec --lib
cargo test -p lpc-model dynamic_slot --lib
cargo check -p lpc-model
```

