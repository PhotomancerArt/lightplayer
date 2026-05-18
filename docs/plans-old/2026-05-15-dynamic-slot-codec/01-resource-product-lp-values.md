# Phase 01: Resource/Product LpValue Codec Support

## Scope Of Phase

Teach the slot-native value codec to read and write `LpType::Resource` and
`LpType::Product(_)` leaves.

In scope:

- Extend `read_lp_value` in
  `lp-core/lpc-model/src/slot_codec/slot_value_codec.rs`.
- Extend `write_lp_value` and `write_untyped_lp_value` where appropriate.
- Add focused `lpc-model` tests for resource and product round trips through
  the slot codec reader/writer.

Out of scope:

- Dynamic object reading.
- Validation of unset/zero ids.
- Changing serde representations.

## Code Organization Reminders

- Keep resource/product helper functions in `slot_value_codec.rs`, below the
  top-level public functions.
- Prefer small helpers such as `read_resource_ref`, `read_product_ref`,
  `write_resource_ref`, and `write_product_ref`.
- Tests remain at the bottom of the file or existing test module.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope into dynamic object reading.
- Do not suppress warnings or weaken tests.
- If product syntax reveals a real design blocker, stop and report it.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot_codec/slot_value_codec.rs`
- `lp-core/lpc-model/src/product/product_ref.rs`
- `lp-core/lpc-model/src/resource/resource_ref.rs`
- `lp-core/lpc-model/src/products/control/control_product.rs`
- `lp-core/lpc-model/src/products/visual/visual_product.rs`

Expected syntax:

```json
{"domain":"runtime_buffer","id":7}
{"domain":"unset","id":0}
{"kind":"visual","node":2,"output":0}
{"kind":"control","node":3,"output":0,"preferred_extent":{"rows":1,"samples_per_row":12}}
```

For `LpType::Product(ProductKind::Visual)`, reject `kind = "control"` with a
clear syntax error, and vice versa.

Prefer using the existing `ValueReader` object APIs. If direct `null`
handling is needed and not ergonomic, defer it.

## Validate

```bash
cargo fmt -p lpc-model --check
cargo test -p lpc-model slot_codec
cargo test -p lpc-model resource
cargo test -p lpc-model product
```
