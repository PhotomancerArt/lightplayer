# Phase 1: Slot Persistence Policy

## Scope Of Phase

Add explicit slot policy for persisted vs transient user-editable values.

In scope:

- Add `SlotPersistence` to `lpc-model`.
- Add `SlotPolicy` to `lpc-model`.
- Add `policy: SlotPolicy` to record field shapes.
- Keep `SlotMeta` presentation-only.
- Default all existing fields to read-only persisted.
- Add helpers for transient/writable policy.
- Update serde/schema tests as needed.

Out of scope:

- Mutation behavior.
- TOML writeback.
- Clock node implementation.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- If `SlotPersistence` gets its own identity, use `slot/slot_persistence.rs`.
- Keep tests at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot/slot_meta.rs`
- `lp-core/lpc-model/src/slot/slot_policy.rs`
- `lp-core/lpc-model/src/slot/mod.rs`
- `lp-core/lpc-model/src/slot/slot_shape.rs`
- `lp-core/lpc-model/src/slot/slot_value.rs`
- semantic slot files under `lp-core/lpc-model/src/slots/`

Expected changes:

- Add:

```rust
pub enum SlotPersistence {
    Persisted,
    Transient,
}
```

- Add:

```rust
pub struct SlotPolicy {
    pub writable: bool,
    pub persistence: SlotPersistence,
}
```

- Add a `policy` field to `SlotFieldShape`.
- Keep `SlotMeta::empty()` presentation-only.
- Add helpers such as:
  - `SlotPolicy::writable_persisted()`
  - `SlotPolicy::read_only_transient()`
  - `SlotPolicy::writable_transient()`
- Update docs to clarify:
  - `writable` controls whether clients may request mutation.
  - `persistence` is a tooling/save hint.
  - Policy is not presentation metadata and is not resolver dataflow semantics.

Tests:

- `SlotPolicy::default()` is read-only persisted.
- `SlotPersistence` serde uses readable snake_case.
- Transient policy round-trips.

## Validate

```bash
cargo fmt
cargo test -p lpc-model
cargo check -p lpc-model --features schema-gen
```
