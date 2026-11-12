# Phase 5: Registry Pending Introspection API

## Scope of phase

Expose read-only API for **pending map** contents — prep for wire/client sync without
implementing wire yet.

**In scope:**

- `ArtifactOverlay` public iteration (if not already public)
- `NodeDefRegistry` methods
- `lib.rs` re-exports
- Unit tests for introspection

**Out of scope:**

- `lpc-wire` types
- Session/version CAS enforcement (document revision fields available on MapSlot)

## Code organization reminders

- Keep introspection on registry as thin delegates to overlay.
- No mutation APIs beyond existing apply/discard/remove.

## Sub-agent reminders

- Do **not** commit.
- Do **not** expand scope.

## Implementation details

### Registry API

```rust
impl NodeDefRegistry {
    /// Whether any artifact has pending edits.
    pub fn overlay_active(&self) -> bool;

    /// Pending edits for one artifact, if any.
    pub fn pending_at(&self, location: &ArtifactLocation) -> Option<&ArtifactPending>;

    /// Iterate artifacts with pending edits (stable order).
    pub fn iter_pending(&self) -> impl Iterator<Item = (&ArtifactLocation, &ArtifactPending)>;

    /// Whether a specific slot path has a pending edit within an artifact.
    pub fn has_pending_slot(&self, location: &ArtifactLocation, path: &SlotPath) -> bool;
}
```

Deprecate or alias old names:

- `slot_overlay_active` → `overlay_active` (keep deprecated alias one release if exported)
- `slot_overlay_contains_path(path)` → resolve location, check `overlay.contains`

Document in module docs on `NodeDefRegistry` that pending is **address-keyed current
edits**, syncable via future wire.

### `ArtifactPending` accessors

```rust
impl ArtifactPending {
    pub fn slot_edits(&self) -> impl Iterator<Item = (&str, &SlotEdit)>;
    pub fn asset_pending(&self) -> &AssetPending;
    pub fn slots_revision(&self) -> Revision;  // delegate to MapSlot if available
}
```

### Tests

- Load project, apply edit, assert `iter_pending` yields one artifact with one slot key
- Apply asset edit, assert slot map empty in introspection
- Commit clears `iter_pending`

## Validate

```bash
cargo test -p lpc-node-registry
```
