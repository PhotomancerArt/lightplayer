# Phase 2: Streaming Reader API

## Scope Of Phase

Replace the `SyntaxNode`-backed `SlotReader` with a streaming reader that
consumes `SyntaxEventSource` directly.

In scope:

- Remove `SyntaxNode` and all primary API usage of it.
- Add `StreamingSlotReader` or rename `SlotReader` to the streaming
  implementation.
- Add object scanning via `object().next_prop()`.
- Add array scanning helpers.
- Add `expect_discriminator`.
- Add scalar reads and `binary_base64_tuple`.
- Add `skip_value`.
- Add path-aware and span-aware diagnostic constructors.

Out of scope:

- Generated code.
- Full slot-shape-driven deserialization.
- Backward compatibility with the tree-backed API.

## Code Organization Reminders

- Prefer one clear concept per file if splitting.
- Keep reader cursor/state code readable; avoid clever generic abstractions
  until the semantics are stable.
- Helpers should live below the public reader API.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot_codec/`
- `lp-core/lpc-wire/src/slot/mod.rs`
- `lp-core/lpc-wire/src/lib.rs`
- `lp-core/lpc-wire/src/slot/mod.rs`
- `lp-core/lpc-wire/src/lib.rs`

Expected API shape:

```rust
let mut reader = SlotReader::new(JsonSyntaxSource::new(json)?, &registry);
let mut object = reader.object()?;
while let Some(prop) = object.next_prop()? {
    match prop.name() {
        "pin" => pin = Some(prop.value().u32()?),
        other => return Err(prop.unknown_field(other, &["pin"])),
    }
}
```

Discriminator shape:

```rust
let kind = reader.expect_discriminator("kind")?.string()?;
```

Important cursor behavior:

- `object()` consumes `StartObject`.
- `next_prop()` returns `None` after consuming `EndObject`.
- `expect_discriminator("kind")` consumes the next property name and value, but
  leaves the reader inside the current object for variant parsing.
- `skip_value()` consumes nested arrays/objects correctly.
- Unknown fields should error by default in tests.

Diagnostics:

- Path should update for `prop`, array item, and discriminator contexts.
- Errors should carry expected values when generated/manual code passes them.
- Include optional JSON span when available.

## Validate

```bash
cargo test -p lpc-model slot_codec
cargo test -p lpc-wire slot
cargo check -p lpc-wire --no-default-features
```
