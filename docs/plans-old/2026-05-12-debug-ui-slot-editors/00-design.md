# Debug UI Slot Editors Design

## Scope

Add a small generic slot editing layer to the temporary egui debug UI. The
first usable target is clock control, but the implementation should follow the
slot model rather than hardcoding clock paths.

In scope:

- render writable boolean leaves as checkboxes;
- render writable `f32` slider leaves using `ValueEditorHint::Slider`;
- render writable plain/number `f32` leaves as drag values;
- prepare project-read mutations through `SlotMirrorView`;
- coalesce unsent mutations by `(root, path)`;
- show minimal pending/rejected mutation state next to edited rows.

Out of scope:

- polished final app UI;
- editing strings, maps, records, enum variants, or option presence;
- broad engine-side mutation beyond existing clock control support;
- persistent subscriptions or server-side client state.

## File Structure

```text
lp-cli/src/debug_ui/
  ui.rs
    DebugUiState mutation queue, poll request construction, response handling.

  node_cards.rs
    Node card layout; passes root identity and edit context into slot rows.

  slot_render.rs
    Generic slot traversal and row layout; passes field policy and slot path
    to value-row rendering.

  slot_edit.rs
    New small module for edit intents, mutation status lookup, and editor
    selection from SlotPolicy + SlotValueShape + LpValue.
```

## Architecture

Slot editing is a layered decision:

1. `SlotPolicy.writable` gates whether the UI may attempt an edit.
2. `SlotValueShape.ty` and the current `LpValue` decide whether a widget is
   type-correct.
3. `SlotValueShape.editor` chooses the preferred widget.
4. Unsupported or inconsistent combinations render read-only with a hover/debug
   hint; they do not panic and do not emit mutations.

The renderer does not mutate the project directly. It emits:

```rust
SlotEditIntent {
    root: String,
    path: SlotPath,
    value: LpValue,
}
```

`DebugUiState` consumes those intents after the UI pass, calls
`ProjectView.slots.prepare_set_value(...)`, and stores the resulting
`WireSlotMutationRequest` in a coalescing queue. The next project poll drains
that queue into `ProjectReadRequest.mutations`.

The server remains stateless with respect to clients. The only client state is:

- the synced `ProjectView`;
- pending mutation state already tracked by `SlotMirrorView`;
- debug-UI-local path-to-last-mutation ids for row affordances;
- the outgoing coalescing queue.

## Main Interactions

1. Slot rows receive root name, current slot path, inherited field policy, and a
   mutable edit-intent sink.
2. A writable value row renders an editor when the value type and editor hint
   are supported.
3. On egui change, the row pushes a `SlotEditIntent`.
4. After rendering, `DebugUiState` prepares each edit through `SlotMirrorView`.
5. Prepared requests replace any older unsent request for the same `(root,
   path)`.
6. The next `ProjectReadRequest` includes drained mutations.
7. The response applies mutation results through the existing view sync path.
8. Rows show lightweight pending/error labels by consulting the last mutation
   id recorded for their `(root, path)`.

## Validation

Primary validation is interactive, because this is temporary egui tooling:

- clock `running` toggles;
- clock `rate` changes the speed of shader time;
- clock `scrub_offset_seconds` moves shader time;
- rejected/stale mutations do not crash the UI.

Compile-level validation should cover the touched host crates and the firmware
server path:

```bash
cargo check -p lp-cli
cargo check -p lpa-server
cargo test -p lpc-view
cargo test -p lpc-engine
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```
