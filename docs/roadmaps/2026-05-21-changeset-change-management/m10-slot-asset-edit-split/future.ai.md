# M10 Future — Asset edit extensions

## Partial text diffs

- **Idea:** Add patch-style variants to `AssetEdit` (range replace, splice, unified diff hunks).
- **Why not now:** Overlay and apply already work with whole-body `ReplaceBody`; slot path covers structured TOML edits.
- **Useful context:** `AssetEdit` enum in `asset_edit.rs`; `SlotOverlayEntry::Bytes` may need merge semantics instead of last-write-wins for incremental patches.

## Binary assets

- **Idea:** `ReplaceBody(Vec<u8>)` or separate `ReplaceBytes` when non-UTF-8 assets matter.
- **Why not now:** Current model uses `String` for text assets; UTF-8 assumption matches GLSL/SVG/TOML escape hatch.
- **Useful context:** `apply_bytes` in `slot_overlay.rs`.

## TOML import escape hatch typing

- **Idea:** Explicit `ArtifactEdit::Asset` on `.toml` paths for bulk import vs `Slot` for normal authoring — already enforced by kind tag; could add apply-time warning in debug builds.
- **Why not now:** Convention + type split is sufficient for v1.
