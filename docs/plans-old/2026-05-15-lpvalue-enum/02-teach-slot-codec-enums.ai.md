# Phase 2: Teach Slot Codec Enum Values

## Scope Of Phase

Teach existing slot value readers/writers to handle `LpType::Enum` and
`LpValue::Enum`.

In scope:

- JSON/event reader support in `slot_value_codec.rs`
- JSON writer support in `slot_value_codec.rs`
- TOML writer support in `dynamic_slot_writer.rs`
- tests for valid unit/payload variants, unknown variants, missing payload, and
  mismatched payload type

Out of scope:

- compact single-key enum syntax
- untyped literal payload inference
- binding endpoint migration

## Code Organization Reminders

- Keep shared JSON/event value helpers in
  [slot_value_codec.rs](/Users/yona/dev/photomancer/feature/lightplayer-serialize/lp-core/lpc-model/src/slot_codec/slot_value_codec.rs).
- Keep TOML-specific value writing in
  [dynamic_slot_writer.rs](/Users/yona/dev/photomancer/feature/lightplayer-serialize/lp-core/lpc-model/src/slot_codec/dynamic_slot_writer.rs).
- Helpers should live below the public read/write entry points.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Reader behavior:

- `LpType::Enum` expects an object.
- Read `kind` first or through normal property scanning.
- Find the matching `ModelEnumVariant`.
- Store the matched variant as its `u32` index in `LpValue::Enum`.
- If the variant has no payload, reject an authored payload field.
- If the variant has a payload type, require `payload` and read it with
  `read_lp_value(payload_ty, ...)`.
- Unknown properties are errors.
- Unknown variant should report valid variant names.

Initial syntax:

```json
{"kind":"Unset"}
{"kind":"Value","payload":0.75}
```

Writer behavior:

- Write object with `kind`.
- Resolve `LpValue::Enum.variant` as an index into `LpType::Enum.variants` and
  write that variant's name.
- Write `payload` only when present.
- Validate the value variant is declared by the `LpType::Enum`.
- Validate payload presence and payload type.

Add tests near existing slot value codec tests.

## Validate

```bash
cargo test -p lpc-model slot_codec::slot_value_codec
cargo test -p lpc-model slot_codec::dynamic_slot_writer
```
