# Debug UI Node Cards Design

## Scope

Build a more domain-shaped debug UI in `lp-cli` using the existing `ProjectView`. The UI should make node flow easier to inspect without adding new server/client sync features.

## File Structure

```text
lp-cli/src/debug_ui/
  mod.rs          # module exports only
  ui.rs           # DebugUiState, polling, top-level egui::App
  node_cards.rs   # main workspace cards + slot row overview
  inspector.rs    # right panel navigation + selected debug detail
  slot_render.rs  # shared compact/debug slot rendering helpers
  format.rs       # LpValue/LpType/resource/product display helpers
```

The file split is in scope. `ui.rs` should orchestrate polling and layout, not
hide the slot/node/resource renderers.

## Architecture

- `DebugUiState` keeps the polling flow unchanged.
- Main central panel becomes the primary node workspace:
  - Scrollable list/grid of node cards.
  - Each card shows node label, id, status, state, and path.
  - Each card shows prominent input/output rows inferred from synced slot roots.
  - Each card has expandable `def/config`, `state`, and `children` sections.
- Right panel becomes the compact debug inspector:
  - Tree/list tabs or headings for nodes, resources, and shapes.
  - Clicking an item updates a selected inspector item.
  - Details below use the existing recursive/debug renderer style.
- Slot value rendering uses two layers:
  - Main card renderer: compact rows, special treatment for products/resources, shallow by default.
  - Debug detail renderer: current recursive shape/data renderer, preserved and reused.

## Main Components

- `InspectorSelection`
  - `Node(NodeId)`
  - `Resource(ResourceRef)`
  - `Shape(SlotShapeId)`
- `render_main_node_workspace`
  - Iterates nodes in stable tree order.
  - Renders one node card per node.
- `render_node_card`
  - Header metadata.
  - Slot overview rows for `input`, `output`, and other top-level value slots.
  - Expandable `def/config`, `state`, and children sections.
- `render_slot_rows`
  - Row-style rendering for records/maps/enums/options.
  - Uses shape metadata to avoid dumping raw `Debug` unless needed.
- `render_value_skeleton`
  - Products: show `visual product`, `control product`, owner/output ids, and a disabled/detail placeholder.
  - Resources: show domain/id and a disabled/detail placeholder.
  - Arrays/structs: show compact type/length with optional expansion.
- `render_debug_inspector`
  - Right-side navigable tree and detail box.

## Out Of Scope

- Binding sync and true binding badges.
- Product probes or resource payload fetches.
- Editing/mutation UI.
- Final visual design system.
