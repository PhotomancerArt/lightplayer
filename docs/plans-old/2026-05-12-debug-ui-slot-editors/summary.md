# Debug UI Slot Editors Summary

## What Was Built

- Added generic debug UI slot editors for writable `bool` and `f32` value
  leaves.
- Routed slot row edits through `SlotMirrorView::prepare_set_value(...)`.
- Added a coalescing outgoing mutation queue keyed by `(root, path)`.
- Drained queued mutations into the next stateless `ProjectReadRequest`.
- Displayed lightweight pending/rejected row status from the client mirror.
- Default-opened clock node controls enough to make shader time debugging
  usable.

## Decisions For Future Reference

#### Slot Shape Drives Editor Choice

- **Decision:** The UI checks `SlotPolicy`, `SlotValueShape.ty`, and
  `ValueEditorHint`, in that order.
- **Why:** Policy gates intent, type gates correctness, and editor hints stay
  presentation-only.
- **Rejected alternatives:** Hardcoded clock controls; editor hints as the
  authoritative type contract.
- **Revisit when:** The real UI needs richer editors for strings, enums,
  resources, or aggregate values.

#### Pull-Based Mutation Transport

- **Decision:** Debug UI mutations ride the next `ProjectReadRequest`.
- **Why:** This keeps the server stateless with respect to clients and matches
  the current sync model.
- **Rejected alternatives:** Persistent subscriptions or push mutation channels.
- **Revisit when:** Multi-client mutation UX becomes a real requirement.

#### Coalesced Slider Mutations

- **Decision:** Unsent mutations are coalesced by `(root, path)`.
- **Why:** Scrubbing time should not send stale intermediate values over serial.
- **Rejected alternatives:** Sending every egui `.changed()` event.
- **Revisit when:** We need per-edit history, undo, or high-frequency local
  controls.
