# Phase 3: Use Enum Value For Binding Endpoint

## Scope Of Phase

Move `BindingEndpoint` from debug/string-only `LpValue` storage to
`LpValue::Enum` storage if the base enum value support can express its variants
cleanly.

In scope:

- update `BindingEndpoint::to_lp_value`
- update `BindingEndpoint::from_lp_value`
- update `BindingEndpoint::value_shape`
- tests for `Unset`, `Bus`, `Node`, and `Literal`

Out of scope:

- changing `BindingDef` field wrappers
- removing serde impls
- compact authored syntax

## Code Organization Reminders

- Keep semantic conversion logic near `BindingEndpoint`.
- Do not add binding-specific branches to generic codec files unless the generic
  type system cannot represent the case and the blocker is reported.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Target mapping:

- variant `0`, `Unset` -> no payload
- variant `1`, `Bus` -> string payload
- variant `2`, `Node` -> string payload
- variant `3`, `Literal` -> value payload

Dynamic payload policy:

- `Literal(LpValue)` is represented with `LpType::Any`.
- That means the storage layer accepts any `LpValue`; semantic validation stays
  with the surrounding model/logic layer.
- Do not paper over this by serializing literals as debug strings.

## Validate

```bash
cargo test -p lpc-model binding::binding_endpoint
```
