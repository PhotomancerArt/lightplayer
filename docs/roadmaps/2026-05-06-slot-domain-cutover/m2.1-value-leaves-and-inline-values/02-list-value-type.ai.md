# Phase 2: List Value Type

## Goal

Represent variable-length value payloads without turning them into slot maps.

`LpValue::Array` remains the portable sequence payload. `LpType` distinguishes
fixed arrays from variable-length lists.

## Work

- Add `LpType::List(Box<LpType>)`.
- Treat `LpType::Array(_, len)` as fixed-size validation.
- Treat `LpType::List(_)` as variable-length homogeneous validation.
- Keep shader projection for lists unsupported for now; lists are value-domain
  payloads until the shader ABI story is designed.
- Update local validation and client patch checks that understand `LpType`.

## Validation

- `cargo test -p lpc-model`
- Focused checks for slot/view crates touched by validation changes.
