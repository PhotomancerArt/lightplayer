# Phase 1: Authored Serde For Slot Wrappers

## Scope Of Phase

Add authored serde behavior for the generic typed slot wrappers.

In scope:

- `ValueSlot<T>` serializes/deserializes as `T`.
- `MapSlot<K,V>` serializes/deserializes as a normal map.
- `OptionSlot<T>` serializes/deserializes as `Option<T>`.
- Deserialization stamps versions with `current_state_version()`.
- Tests prove clean serde and version stamping.

Out of scope:

- Semantic slot serde.
- Mockup source mapping changes.
- Real `lpc-source` conversion.
- Wire/snapshot serde for `SlotData`.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep wrapper serde impls near the wrapper types in `value_slot.rs`.
- Keep helper functions lower in the file when that improves readability.
- Tests stay at the bottom of the file.
- Preserve `no_std + alloc`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot/value_slot.rs`
- `lp-core/lpc-model/src/slot/slot_data.rs`
- `lp-core/lpc-model/src/project/state_version.rs`

Expected behavior:

- `serde` for `ValueSlot<T>` requires `T: Serialize` / `T: Deserialize`.
- `Deserialize` for `ValueSlot<T>` should call `ValueSlot::with_version(current_state_version(), value)`.
- `serde` for `MapSlot<K,V>` should serialize only `entries`.
- `Deserialize` for `MapSlot<K,V>` should call `MapSlot::with_version(current_state_version(), entries)`.
- `serde` for `OptionSlot<T>` should serialize only `data`.
- `Deserialize` for `OptionSlot<T>` should set `presence_changed_frame` to `current_state_version()`.
- Add small convenience predicates only if needed by later source structs:
  - `MapSlot::is_empty()`
  - `OptionSlot::is_none()`

Tests to add or update:

- `ValueSlot<u32>` JSON/TOML round-trips as a number, not as `{ inner = ... }`.
- `MapSlot<String, ValueSlot<u32>>` round-trips as a normal map.
- `OptionSlot<ValueSlot<u32>>` round-trips as `Some` and `None`.
- Before deserializing, call `set_current_state_version(FrameId::new(7))` and assert parsed wrappers have changed frame `7`.

Constraints:

- Do not serialize `Versioned<T>` internals.
- Do not add `std` requirements.
- Avoid custom TOML-only code; this should be normal serde behavior.

## Validate

```bash
cargo fmt --package lpc-model
cargo test -p lpc-model --lib --tests
cargo check -p lpc-model --features schema-gen
```
