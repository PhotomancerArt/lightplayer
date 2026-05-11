# SlotPath And Refs

## Scope Of Phase

Introduce `SlotPath` and update slot references to address paths through a slot
tree rather than one flat slot name.

In scope:

- Add `slot_path.rs`.
- Export `SlotPath`.
- Update `SlotRef` from `{ owner, slot: SlotName }` to `{ owner, path:
  SlotPath }`.
- Update `ValueRef` tests/docs for the new `SlotRef`.
- Update compile fallout in `lpc-model` tests and direct dependents if needed.

Out of scope:

- Runtime resolver/binding changes.
- Applying `SlotPath` to source files.
- Slot data/shape definitions.

## Code Organization Reminders

- Keep `SlotPath` parsing/display in `slot_path.rs`.
- Use `SlotName` segments, not `ValuePath::Segment`.
- Tests stay at the bottom of each file.
- Keep rustdocs crisp: `SlotPath` addresses slot tree nodes; `ValuePath`
  traverses inside a leaf `ModelValue`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot/mod.rs`
- `lp-core/lpc-model/src/slot/slot_name.rs`
- `lp-core/lpc-model/src/slot/slot_ref.rs`
- `lp-core/lpc-model/src/slot/value_ref.rs`
- `lp-core/lpc-model/src/lib.rs`

Expected `SlotPath` API:

```rust
pub struct SlotPath(Vec<SlotName>);
```

Add helpers roughly like:

- `SlotPath::root()`
- `SlotPath::parse(&str) -> Result<Self, SlotPathError>`
- `SlotPath::from_segments(Vec<SlotName>)`
- `is_root()`
- `segments()`
- `child(SlotName) -> SlotPath` or equivalent
- `Display`
- serde as string

Parsing rules:

- Text parsing rejects an empty string.
- `SlotPath::root()` is the explicit empty/root path.
- Use dot-separated names for now, e.g. `config.mapping.shapes`.
- Segment names reuse `SlotName::parse`.

Update:

```rust
pub struct SlotRef {
    pub owner: SlotOwner,
    pub path: SlotPath,
}
```

Tests to add/update:

- root path exists but empty string parse fails.
- `config.size` round-trips through display/serde.
- `SlotRef` contains owner + path.
- `ValueRef` combines `SlotRef` + `ValuePath` and docs/tests make clear the
  latter is projection only.

## Validate

```bash
cargo test -p lpc-model
cargo test -p lpc-source
cargo test -p lpc-wire
cargo test -p lpc-view
```

