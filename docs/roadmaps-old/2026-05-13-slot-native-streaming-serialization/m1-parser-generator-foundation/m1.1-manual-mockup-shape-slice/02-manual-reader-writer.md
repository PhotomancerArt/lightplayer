# Phase 2: Manual Reader And Writer

## Scope Of Phase

Implement hand-written read/write functions for the M1.1 fixture.

In scope:

- JSON write using `lpc_model::slot_codec::SlotJsonWriter`.
- JSON read using `JsonSyntaxSource` and `SlotReader`.
- TOML read using `TomlSyntaxSource` and the same typed read functions used for
  JSON.
- Manual helpers that look like plausible codegen output.
- Positive round-trip tests and focused negative diagnostics.

Out of scope:

- Codegen.
- Replacing existing Serde/authored TOML tests.
- General shape-driven dynamic deserialization through `SlotShapeRegistry`.

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

Relevant files and symbols:

- `lp-core/lpc-slot-mockup/src/tests/manual_shape_codec.rs`
- `lpc_model::slot_codec::{JsonSyntaxSource, TomlSyntaxSource, SlotReader, SlotJsonWriter}`
- Current small example:
  `lp-core/lpc-slot-mockup/src/tests/native_stream.rs`

Expected reader pattern:

```rust
let mut object = reader.object()?;
while let Some(mut prop) = object.next_prop()? {
    match prop.name() {
        "field" => field = Some(read_field(prop.value())?),
        other => return Err(prop.unknown_field(other, FIELDS)),
    }
}
```

Expected enum pattern:

```rust
reader.start_object()?;
let kind = reader.expect_discriminator("kind")?.string()?;
match kind.as_str() {
    "OutputDef" => read_output_def_body(reader),
    "FixtureDef" => read_fixture_def_body(reader),
    other => Err(reader.invalid_discriminator_value("kind", other, EXPECTED)),
}
```

Tests to add:

- JSON round-trip for the full manual source bundle.
- TOML read for the full manual source bundle, using TOML that resembles the
  current authored mockup TOML.
- Unknown field error includes the field and expected names.
- Invalid discriminator error includes actual value and valid variants.
- Missing required field error is explicit enough to guide an author.

Edge cases:

- Numeric TOML map keys may arrive as strings depending on table shape; the
  manual helpers should document the chosen representation.
- Options should use one universal rule for M1.1. Suggested rule: omitted means
  default/none according to the generated reader for that field; explicit
  `null` is accepted only if the source can represent it cleanly.
- Required-field errors can be simple helper constructors in the test module if
  `SlotReader` does not yet expose a first-class helper.

## Validate

```bash
cargo test -p lpc-slot-mockup manual_shape_codec
cargo test -p lpc-model slot_codec
```
