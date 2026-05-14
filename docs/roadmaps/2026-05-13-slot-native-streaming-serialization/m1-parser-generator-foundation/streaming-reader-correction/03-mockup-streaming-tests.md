# Phase 3: Mockup Streaming Tests

## Scope Of Phase

Rewrite mockup manual tests so they prove the streaming reader path instead of
building a syntax tree.

In scope:

- Update `native_stream.rs` to construct readers directly from
  `JsonSyntaxSource` and `TomlSyntaxSource`.
- Use object scanner/match for normal record fields.
- Add a discriminator-first enum/wrapper-style test.
- Add friendly error assertions for invalid discriminator values and unknown
  fields.
- Keep binary tuple round-trip coverage.

Out of scope:

- Codegen.
- Production model adoption.
- Broad mockup domain conversion.

## Code Organization Reminders

- Tests should have the actual `#[test]` functions first and helpers below.
- Keep manual reader/writer functions shaped like future generated code.
- Avoid noisy test printing unless it materially helps diagnose failure.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-mockup/src/tests/native_stream.rs`
- `lp-core/lpc-slot-mockup/src/tests/mod.rs`

Expected manual record reader shape:

```rust
let mut object = reader.object()?;
while let Some(prop) = object.next_prop()? {
    match prop.name() {
        "brightness" => brightness = Some(prop.value().f32()?),
        "payload" => payload = Some(prop.value().binary_base64_tuple()?),
        other => return Err(prop.unknown_field(other, EXPECTED_FIELDS)),
    }
}
```

Expected discriminator test:

```json
{"kind":"TextureDef","size":{"width":64,"height":48}}
```

Validate an error along these lines:

```text
Invalid discriminator `kind`: "Blark12". Expected one of: TextureDef, OutputDef.
```

Exact wording can differ, but the test should prove that actual and valid
values are present.

## Validate

```bash
cargo test -p lpc-slot-mockup native_stream
```

