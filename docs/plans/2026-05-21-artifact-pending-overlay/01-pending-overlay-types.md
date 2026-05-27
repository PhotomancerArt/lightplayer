# Phase 1: Pending Overlay Types + Slotted Storage

## Scope of phase

Introduce **`ArtifactOverlay`**, **`ArtifactPending`**, and **`pending_slot_key`**
helpers. Wire types into `edit/mod.rs` exports. **Do not** change registry apply,
projection, or commit yet — old `SlotOverlay` may remain until phase 2 removes it.

**In scope:**

- `edit/artifact_overlay.rs` — core types + unit tests
- `edit/pending_slot_key.rs` — `SlotPath` ↔ canonical `String` key
- `edit/mod.rs` — export new types alongside legacy (temporary) or replace exports if
  compile allows

**Out of scope:**

- Registry field swap (`node_def_registry.rs`) — phase 2
- Deleting `def_draft.rs` / `slot_overlay.rs` — phase 6
- Wire sync

## Code organization reminders

- One concept per file (`artifact_overlay.rs`, `pending_slot_key.rs`).
- Public types and impl entry points at top; helpers at bottom; `#[cfg(test)] mod tests` last.
- Use `lpc_model::MapSlot` for revisioned maps.

## Sub-agent reminders

- Do **not** commit.
- Do **not** expand scope.
- Do **not** suppress warnings or weaken tests.
- If blocked, stop and report.
- Report: files changed, validation run, deviations.

## Implementation details

### `pending_slot_key.rs`

```rust
//! Canonical string keys for slot paths in overlay maps.

pub fn slot_path_key(path: &SlotPath) -> String { ... }
pub fn parse_slot_path_key(key: &str) -> Result<SlotPath, ...> { ... }
```

Use the same string form as existing wire/TOML path display (match how tests parse
`SlotPath::parse("controls.rate")`). Round-trip tests required.

### `artifact_overlay.rs`

```rust
use lpc_model::{MapSlot, Revision, SlotPath};
use crate::{ArtifactLocation, AssetEdit, SlotEdit};

#[derive(Clone, Debug, Default, PartialEq)]
pub enum AssetPending {
    #[default]
    None,
    Delete,
    ReplaceBody(alloc::vec::Vec<u8>), // raw bytes, not String — matches fs write
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtifactPending {
    pub slots: MapSlot<String, SlotEdit>,
    pub asset: AssetPending,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtifactOverlay {
    by_artifact: MapSlot<ArtifactLocation, ArtifactPending>,
}

impl ArtifactOverlay {
    pub fn new() -> Self;
    pub fn is_empty(&self) -> bool;
    pub fn contains(&self, location: &ArtifactLocation) -> bool;
    pub fn pending_at(&self, location: &ArtifactLocation) -> Option<&ArtifactPending>;
    pub fn pending_at_mut(&mut self, location: &ArtifactLocation) -> Option<&mut ArtifactPending>;
    pub fn ensure_pending(&mut self, location: ArtifactLocation) -> &mut ArtifactPending;
    pub fn remove(&mut self, location: &ArtifactLocation) -> bool;
    pub fn clear(&mut self);
    pub fn iter(&self) -> impl Iterator<Item = (&ArtifactLocation, &ArtifactPending)>;
}
```

**Note:** `ArtifactLocation` must work as `MapSlot` key — it already implements
`Ord`/`Eq`. If `MapSlot` requires `MapSlotKeyLike`, add a thin newtype wrapper or
use string URI key (`location.to_uri()`) consistently; document choice in report.

**Mutual exclusion helpers** (implement now, used in phase 2):

```rust
impl ArtifactPending {
    pub fn upsert_slot(&mut self, path: SlotPath, edit: SlotEdit);
    pub fn set_asset(&mut self, pending: AssetPending); // clears slots
    pub fn is_empty(&self) -> bool;
}
```

- `upsert_slot`: insert/replace `slots[slot_path_key(path)]`, set `asset = None`
- `set_asset`: assign asset, `slots = MapSlot::default()` (or clear map)

### Tests (in `artifact_overlay.rs`)

- Empty overlay
- Upsert two slot paths → two keys
- Upsert same path twice → second replaces first
- Set asset pending → slot map cleared
- Upsert slot after asset → asset cleared
- `remove` / `clear`

### `edit/mod.rs`

Export:

```rust
pub use artifact_overlay::{ArtifactOverlay, ArtifactPending, AssetPending};
pub use pending_slot_key::{parse_slot_path_key, slot_path_key};
```

Keep legacy `SlotOverlay` exports until phase 6 unless registry already broken — prefer
keeping both temporarily.

## Validate

```bash
cargo test -p lpc-node-registry artifact_overlay
cargo test -p lpc-node-registry pending_slot
cargo check -p lpc-node-registry
```
