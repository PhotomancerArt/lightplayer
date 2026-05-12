# Debug UI Node Cards Notes

## Scope

- Rework the temporary `lp-cli` debug UI from a selected-node slot tree into a more node-centric workspace.
- Keep the UI lightweight and dev-focused; this is a place to validate the domain model, not the final product UI.
- Main area should show every node as a box/card with visible slot sections.
- Right panel should keep a compact inspector tree for nodes, resources, and shapes.
- Slot rendering should be row-oriented, with expanders where structure needs them.

## Current Code

- `lp-cli/src/debug_ui/ui.rs` is currently a single-file egui app.
- It polls `project_read_default_debug` every 500 ms and applies responses through `lpc_view::apply_project_read_response`.
- `ProjectView` exposes:
  - `tree.nodes` for node metadata and hierarchy.
  - `slots.root_shapes` and `slots.roots` for synced node slot roots.
  - `slots.registry` for slot shapes.
  - `resource_cache.summary_count()` and individual summary APIs.
- Current layout:
  - Top status panel.
  - Left node tree.
  - Central selected node detail with `def` and `state` recursive slot trees.
- Current slot renderer is useful as a fallback/debug detail renderer, but it is too tree-like for the main node UI.

## User Notes

- The node tree is good as a sanity check but should move to the right panel.
- The main UI should be boxes for each node.
- Inputs and outputs should be clearly visible.
- Def/config and state should be expandable.
- Values should be shown as a stack of rows, not primarily as nested tree headers.
- Bindings should appear on the slots themselves, with optional debug detail later.
- Children should appear inside a node after other sections when they exist.
- Render products and resources should show as space-holding skeletons with affordances for detail, without actually fetching heavy detail yet.
- Right side should contain a tree of nodes, resources, and shapes; selected item details appear below.

## Constraints

- Do not expand wire protocol scope in this pass unless needed for the UI to compile.
- Do not implement expensive resource/product detail reads yet.
- Bindings are not currently mirrored into `lpc-view`, so real binding badges are likely out of scope.
- Keep code organized even if `ui.rs` starts as the only edited file; split helpers if the file becomes harder to scan.

## Open Questions

### Should this pass add binding sync?

- **Context:** Bindings live on `NodeTree` in `lpc-engine`, but current `ProjectView` / `WireTreeDelta` does not expose a binding list.
- **Suggested answer:** No. Render slot-level binding placeholders now and capture binding sync as future work.
- **Decision:** Use this suggested answer.

### Should the right inspector include shapes/resources immediately?

- **Context:** The client has enough data to list shape ids and resource summaries.
- **Suggested answer:** Yes, but as compact debug lists. Clicking a resource/shape shows metadata/debug text, not rich visualization.
- **Decision:** Use this suggested answer.

### Should the UI request resource payloads for previews?

- **Context:** Default debug read intentionally uses resource summaries only.
- **Suggested answer:** No. Show skeleton/summary cards and leave payload reads for a later probe/detail workflow.
- **Decision:** Use this suggested answer.

