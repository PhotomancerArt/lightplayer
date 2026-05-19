# Debug UI Slot Editors Notes

## Scope

Add a small generic editing layer to the temporary egui debug UI so writable
slot value leaves render as controls and send project-read mutations.

Immediate goal:

- control clock `controls.running` with a checkbox;
- control clock `controls.rate` with a slider;
- control clock `controls.scrub_offset_seconds` with a slider;
- show pending / rejected mutation state enough to trust the round trip.

Out of scope:

- a polished final UI;
- server-side persistence/writeback policy beyond the existing transient clock
  controls;
- editing aggregate slot containers, map keys, enum variants, text fields, or
  shader parameters beyond what falls out naturally;
- new mutation support on the engine for arbitrary node defs.

## Current State

### Slot Shape And Policy

`lp-core/lpc-model/src/slot/slot_shape.rs`

- `SlotShape::Value { shape: SlotValueShape }` carries the value contract.
- `SlotShape::Record { fields }` stores `SlotFieldShape`.
- `SlotFieldShape` has:
  - `name`;
  - `shape`;
  - `semantics`;
  - `policy`.
- `SlotPolicy` is on the field, not directly on `SlotValueShape`.

`lp-core/lpc-model/src/slot/slot_policy.rs`

- `SlotPolicy::writable` is the client mutation gate.
- `SlotPolicy::persistence` distinguishes persisted vs transient edit intent.

`lp-core/lpc-model/src/slot/slot_value.rs`

- `SlotValueShape` carries:
  - `ty: LpType`;
  - `meta: SlotMeta`;
  - `editor: ValueEditorHint`.
- Current editor hints include `Plain`, `Number`, `Slider`, `Dropdown`, domain
  hints like `NodeRef`, `Path`, `Resource`, `VisualProduct`.

### Clock Controls

`lp-core/lpc-model/src/nodes/clock/clock_controls.rs`

- `ClockControls` has:
  - `running: ValueSlot<bool>`;
  - `rate: ValueSlot<f32>`;
  - `scrub_offset_seconds: ValueSlot<f32>`.
- All three fields use `SlotPolicy::writable_transient()`.
- `rate` and `scrub_offset_seconds` use `ValueEditorHint::Slider`:
  - rate: `0.0..4.0`, step `0.05`;
  - scrub offset: `-10.0..10.0`, step `0.05`.

### Mutation Path

`lp-core/lpc-wire/src/slot/mutation.rs`

- `WireSlotMutationRequest` has:
  - `id`;
  - `root`;
  - `path`;
  - `expected_shape_version`;
  - `expected_data_version`;
  - `op`.
- Only operation is `SetValue(LpValue)`.

`lp-core/lpc-view/src/slot/mirror.rs`

- `SlotMirrorView::prepare_set_value(...)` validates value type, computes
  expected shape/data revisions, and records pending mutation state.
- `apply_mutation_response(...)` clears pending or records rejection.
- This is exactly what the debug UI should call before sending mutations.

`lp-core/lpc-engine/src/engine/slot_mutation.rs`

- Engine mutation currently supports only clock def paths:
  - `controls.running`;
  - `controls.rate`;
  - `controls.scrub_offset_seconds`.
- That is enough for the time-debugging goal.

### Debug UI

`lp-cli/src/debug_ui/ui.rs`

- Polls `ProjectReadRequest` every 100ms.
- Applies project-read responses into `ProjectView`.
- Currently sends `mutations: Vec::new()` every time.

`lp-cli/src/debug_ui/slot_render.rs`

- `render_value_row` renders non-resource/non-product leaves as text.
- It receives `SlotValueShape`, `LpValue`, revision, and selection.
- It does not receive:
  - root name;
  - slot path;
  - field policy;
  - a mutation sink.

`lp-cli/src/debug_ui/node_cards.rs`

- Renders `def / config` and `state`.
- Does not pass root/path/edit context into slot rendering.

## Design Pressure

The renderer choice should be layered:

1. **Field policy:** if the containing `SlotFieldShape.policy.writable` is false,
   render read-only.
2. **Hard value type:** use `SlotValueShape.ty` and actual `LpValue` variant as
   the correctness boundary.
3. **Editor hint:** use `SlotValueShape.editor` as a preference only.
4. **Fallback:** mismatched or unsupported editor/type combinations render
   read-only with a small debug hint, never panic.

This keeps the UI dumb in the right way: the shape decides what the UI can
attempt, the client mirror validates the mutation, and the server remains
authoritative.

## Suggested Shape

Add a small editor context passed through slot rendering:

```rust
pub(crate) struct SlotEditContext<'a> {
    pub root: &'a str,
    pub path: SlotPath,
    pub mutations: &'a mut Vec<WireSlotMutationRequest>,
    pub next_id: &'a mut MutationIdAllocator,
}
```

The final shape may use borrowed path segments rather than cloning a `SlotPath`
at each row, but the concept is:

- each row knows its root and path;
- each row knows whether it is writable from field policy;
- a successful editor change calls `ProjectView.slots.prepare_set_value(...)`;
- the returned request is queued into the next `ProjectReadRequest`.

Because `SlotPolicy` lives on record fields, not value shapes, render traversal
needs to pass the current field policy down alongside shape/data.

## Open Questions

### Q1. Where should the mutation queue live?

Context:

- `DebugUiState` owns the polling loop and constructs every
  `ProjectReadRequest`.
- `ProjectView.slots.prepare_set_value(...)` mutates client-side pending state.

Suggested answer:

- Store `pending_outgoing_mutations: Vec<WireSlotMutationRequest>` and
  `next_mutation_id: u64` on `DebugUiState`.
- While rendering, collect UI edits into a local queue and apply them after the
  view lock is released, or restructure the lock so `ProjectView` can be
  mutably borrowed while rendering.
- On the next poll, drain queued mutations into `ProjectReadRequest`.

Reason:

- Keeps client state local to the debug UI.
- Avoids adding subscriptions or server client-state.
- Matches the existing pull-only request model.

Decision:

- Store outgoing mutation state on `DebugUiState`.
- Renderers produce edit intents; the UI prepares mutations through
  `ProjectView.slots.prepare_set_value(...)` after slot rendering.
- Drain queued mutations into the next `ProjectReadRequest`.

### Q2. Should slider mutations fire continuously or on release?

Context:

- egui sliders can emit many changes while dragged.
- Clock scrub wants responsive feedback, but serial/ESP32 links are tight.

Suggested answer:

- For this first pass, send on ordinary `.changed()` and rely on the 100ms poll
  cadence to naturally coalesce because outgoing mutations are sent once per
  poll.
- If spam is visible, add per-root/path replacement in the outgoing queue so
  only the most recent unsent value for a path is kept.

Decision:

- Coalesce queued unsent mutations by `(root, path)` from the beginning.
- This keeps scrubbing responsive in the local UI without sending stale
  intermediate slider positions over serial.

### Q3. Do we render generic writable text/number fields now?

Context:

- Clock only needs `Bool` and `F32`.
- Text editing has commit/focus semantics and more UX edge cases.

Suggested answer:

- Implement:
  - writable `Bool` checkbox;
  - writable `F32` slider when editor is `Slider`;
  - writable `F32` drag value for `Number` or `Plain`.
- Leave strings/dropdowns as future work unless they fall out trivially.

Decision:

- Implement the small generic set above only.
- Do not hardcode clock paths in the renderer.

### Q4. How should pending/rejected state display?

Context:

- `SlotMirrorView` tracks pending/error by mutation id, not root/path.
- The UI will need path-oriented display.

Suggested answer:

- Add a small debug UI local map:
  - `last_mutation_by_slot: BTreeMap<(String, SlotPath), WireSlotMutationId>`.
- Show a tiny `pending` label if that id is still pending.
- Show a small rejection label if that id has an error.

Decision:

- Track path-oriented pending/error affordances locally in the debug UI.
- Treat the client mirror as authoritative for validation and mutation status.

## User Notes

- Use editor hints as hints, not authoritative truth.
- Min/max/step belong on the value shape, not in the UI.
- Do not hardcode this just for clock.
- This is temporary/dev UI, but it should exercise the real slot model.
