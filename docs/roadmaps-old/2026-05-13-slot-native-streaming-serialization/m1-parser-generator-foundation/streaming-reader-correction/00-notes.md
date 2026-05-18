# Streaming Reader Correction Notes

Date: 2026-05-13

## Scope

Correct the M1 foundation so the primary JSON reader path does not materialize
an in-memory syntax tree.

The current committed M1 implementation is useful evidence, but it chose the
wrong center of gravity: JSON parses into `Vec<SyntaxEvent>`, then into
`SyntaxNode`, then typed code reads from that tree. That repeats the memory
shape the roadmap is trying to avoid.

This plan should replace the primary reader path with stream-backed cursors
that manual and generated construction can consume directly.

## User Notes To Preserve

- The whole point is to avoid an in-memory tree.
- The event source should not know target slot shape.
- The generated/manual reader API should still feel like:

```rust
Self {
    brightness: reader.prop("brightness")?.f32()?,
    mapping: reader.prop("mapping")?.slot_root("Mapping")?,
}
```

- TOML may remain tree-backed internally because authored TOML is small and
  TOML syntax is harder to stream cleanly, but the reader interface should not
  require a generic `SyntaxNode`.
- JSON must prove the streaming shape early.

## Previous Code State

Relevant files:

- `lp-core/lpc-wire/src/slot/native.rs` before the move to `lpc-model/src/slot_codec/`
- `lp-core/lpc-slot-mockup/src/tests/native_stream.rs`

The old `native.rs` included:

- `SyntaxEvent`
- `SyntaxEventSource`
- `JsonSyntaxSource`
- `TomlSyntaxSource`
- `SyntaxNode`
- `SlotReader<'a>` over `&'a SyntaxNode`
- `SlotJsonWriter`

Problems:

- `JsonSyntaxSource` parses the entire input into `Vec<SyntaxEvent>`.
- `TomlSyntaxSource` also eagerly emits `Vec<SyntaxEvent>`.
- `SyntaxNode::from_source` materializes the entire syntax tree.
- `SlotReader` requires `&SyntaxNode`, so the reader API cannot be streaming.
- Mockup tests validate round trips but do not prove streaming consumption.

Useful pieces to keep:

- Event vocabulary direction.
- Writer facade direction.
- Manual reader/writer test shape.
- Chunked string and `[len, base64]` convention.

## Desired Shape

JSON path:

```text
&str / byte source
    -> JsonSyntaxSource
    -> StreamingSlotReader
    -> manual/generated typed construction
```

TOML path:

```text
toml::Value
    -> TomlSyntaxSource or TomlReader adapter
    -> same reader-facing semantics
    -> manual/generated typed construction
```

Tree/reference path:

```text
SyntaxEventSource
    -> SyntaxNode
```

This should be removed. Do not keep things the design does not need.

## Design Pressure

### Prop Lookup In A Stream

The ergonomic API wants `reader.prop("brightness")?.f32()?`.

In a stream, object properties arrive sequentially. We need to choose whether
`prop("x")`:

- reads forward until it finds `x`, skipping unknown/other properties, or
- expects the next property to be `x`, making generated code property-order
  dependent, or
- creates a field scanner that generated code drives once and assigns values
  by matching names.

Updated direction:

- The ordered-token assertion helper should be named for its intended use,
  probably `expect_discriminator("kind")`.
- `expect_discriminator("kind")` should mean "read the next token; it must be a
  prop named `kind`; otherwise error with a discriminator-specific diagnostic."
- This is not random access and not a forward search.
- It is appropriate for places where order is part of the storage contract:
  discriminators must be first so generated code can narrow the type.
- Avoid property-order dependence for normal record fields.
- Use an object reader/scanner API as the generated-code workhorse for regular
  records:

```rust
let mut object = reader.object()?;
while let Some(prop) = object.next_prop()? {
    match prop.name() {
        "brightness" => brightness = Some(prop.value().f32()?),
        "mapping" => mapping = Some(prop.value().slot_root("Mapping")?),
        other => return Err(prop.unknown_field(other)),
    }
}
```

- Avoid a general `reader.prop("x")` name for this operation because it implies
  lookup. Use discriminator-specific naming unless another ordered-field use
  case appears.

Expected generated enum shape:

```rust
let kind = reader.expect_discriminator("kind")?.string()?;
match kind {
    "TextureDef" => TextureDef::deserialize(reader),
    "OutputDef" => OutputDef::deserialize(reader),
    other => Err(reader.unknown_variant(other)),
}
```

This implies:

- the enum/object reader has already consumed `start_object`
- `expect_discriminator("kind")` consumes exactly the next property name and
  its value
- after the discriminator value is consumed, the reader remains positioned
  inside the same object so the selected variant can continue reading the
  remaining fields
- discriminator-first ordering is part of the enum storage contract

### Skipping Values

A streaming object reader must be able to skip values for:

- unknown fields, when policy allows
- fields that generated code does not need in a specific mode
- error recovery in tests

Suggested direction:

- Implement `skip_value()` on the syntax source or reader using event nesting.
- Unknown fields should still error by default for domain data.

### Strings

Strings should remain chunked at the event/source level.

For M1 correction:

- scalar `string()` may allocate a `String` when the target asks for one
- future generated code can stream chunks into a destination buffer or decoder
- binary tuple decoding should decode base64 chunks into the destination vector
  after reading the declared length

### JSON Parser

The current JSON parser is not truly streaming because it builds all events.

Suggested direction:

- Change `JsonSyntaxSource::next_event()` to parse one event at a time from the
  input cursor.
- Maintain a small stack for object/array state.
- Avoid storing all events.
- It is acceptable for string parsing to allocate the current string/chunk in
  M1, but not the whole document.

### Diagnostics

Errors should be friendly and specific without becoming a huge diagnostic
framework in the first correction.

Desired style:

```text
E123: Invalid kind: "Blark12". Expected one of: TextureDef, OutputDef, FixtureDef.
```

Useful diagnostic ingredients:

- stable error code or category
- clear operation context, such as discriminator, field, enum variant, value
  type, binary payload
- actual value when safe/reasonable
- expected value or list of valid values
- path within the object/slot, such as `mapping.paths[0].kind`
- syntax span when the source can provide one

Span and path serve different purposes:

- path is format-independent and useful for generated slot/domain errors
- span is source-specific and useful for pointing into authored JSON/TOML text

Suggested direction:

- Add a lightweight `SyntaxSpan` / `SourceSpan` to syntax events if the source
  can provide it.
- Track reader path independently of span.
- Make span optional so TOML adapters or synthetic tests can omit it.
- Define error constructors like:
  - `expected_discriminator(expected_name)`
  - `invalid_discriminator_value(name, actual, expected_values)`
  - `unknown_field(name, expected_fields)`
  - `expected_type(expected, actual)`
- Do not overfit exact error codes yet, but leave room for stable codes.

### TOML Adapter

TOML already starts from `toml::Value`.

Suggested direction:

- It may remain backed by a stack over borrowed `toml::Value` nodes.
- It should emit events lazily instead of prebuilding `Vec<SyntaxEvent>`, if
  that is reasonably small.
- This is less critical than JSON, but keeping the same event API avoids
  format-specific manual construction code.

## Open Questions

### Q1. Should `prop("field")` survive as the main API?

Context:

- The user's original `reader.prop("brightness")?` example was misleading for
  normal record fields.
- In the intended model, `prop("field")` is a semantic helper on top of the
  stream that asserts the next token is exactly that prop.
- This works well for discriminator tokens that are required to appear first.
- It does not solve unordered record-field loading.

User response:

> The ordered assertion helper should probably be renamed to
> `expect_discriminator` so it has a coherent error message and does not look
> like random-access field lookup. It should read the next token and fail unless
> it is exactly the expected discriminator prop. For general record fields, use
> the object-scanner approach; since this is codegen, matching fields as they
> arrive is fine.

### Q2. Should `SyntaxNode` be removed?

Context:

- The current implementation uses `SyntaxNode` as the main reader backing.
- Keeping a tree helper around risks future code accidentally depending on the
  path we are explicitly trying to avoid.

Suggested answer:

- Remove it entirely in this correction.
- Do not keep a reference/debug syntax tree until a concrete need appears.

User response:

> Remove the tree. Do not keep things we do not need.

### Q3. How strict should M1 correction be about direct base64 streaming?

Context:

- The binary tuple shape exists to avoid weird partial-string parsing.
- The immediate correction should focus on removing whole-document trees.

Suggested answer:

- Decode `[len, "base64"]` into a preallocated `Vec<u8>` using the declared
  length.
- It is acceptable for M1 correction to receive the base64 string as chunks
  from the source and decode chunk-by-chunk if feasible; otherwise allocate
  only the base64 field string, not the whole JSON message.

User response:

> Proceed with the suggested direction.

### Q4. Should M1 correction include path and span diagnostics?

Context:

- The user wants clear errors that say what was expected and include valid
  values when possible.
- Generated code will know expected field names and enum variants, so it can
  provide better diagnostics than a generic parser.
- The streaming reader must preserve enough source context to report useful
  locations without buffering the full input.

Suggested answer:

- Include path tracking in M1 correction.
- Include optional byte spans on syntax events for JSON.
- Keep TOML span support optional/out of scope unless the current TOML parser
  exposes it cheaply.
- Make error constructors carry expected values for discriminators and known
  fields.

User response:

> Proceed with the suggested direction. Errors should clearly say what was
> expected, include valid values when available, and be friendly without getting
> too elaborate.
