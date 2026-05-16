# Phase 1: Add Value Enum Types

## Scope Of Phase

Add the data model types for atomic enum values.

In scope:

- add `LpValue::Enum`
- add `LpType::Enum`
- add `ModelEnumVariant`
- add `LpType::Any` for dynamic payloads where the surrounding slot/model
  context owns semantic validation
- add focused unit tests for construction/equality/serde while serde is still
  present

Out of scope:

- migrating `BindingEndpoint`
- compact `{ ref = ... }` / `{ value = ... }` syntax
- removing serde

## Code Organization Reminders

- Keep `LpValue` changes in `lp-core/lpc-model/src/value/lp_value.rs`.
- Keep `LpType` changes in `lp-core/lpc-model/src/value/lp_type.rs`.
- Put tests at the bottom of each file.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Update [lp_value.rs](/Users/yona/dev/photomancer/feature/lightplayer-serialize/lp-core/lpc-model/src/value/lp_value.rs):

```rust
Enum {
    variant: u32,
    payload: Option<Box<LpValue>>,
}
```

Update [lp_type.rs](/Users/yona/dev/photomancer/feature/lightplayer-serialize/lp-core/lpc-model/src/value/lp_type.rs):

```rust
Enum {
    name: Option<String>,
    variants: Vec<ModelEnumVariant>,
}

pub struct ModelEnumVariant {
    pub name: String,
    pub payload: Option<LpType>,
}
```

Also add:

```rust
Any,
```

to `LpType` for values like `BindingEndpoint::Literal(LpValue)`.

Test at least:

- unit enum value with no payload
- enum value with scalar payload
- enum type with unit and scalar payload variants
- enum type with dynamic payload variant

## Validate

```bash
cargo test -p lpc-model value::lp_value value::lp_type
```
