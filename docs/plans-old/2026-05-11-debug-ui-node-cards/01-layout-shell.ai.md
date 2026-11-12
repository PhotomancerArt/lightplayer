# Phase 1: Layout Shell

## Scope Of Phase

Replace the current left-tree / central-detail layout with the new workspace shell.

In scope:
- Top status panel remains.
- Central panel becomes the main node card workspace.
- Right side panel contains inspector navigation and selected debug detail.
- Add selection state for nodes/resources/shapes.

Out of scope:
- Rich slot row rendering.
- Binding badges.
- Resource/product detail fetches.

## Code Organization Reminders

- Prefer granular files with one main concept per file if `ui.rs` becomes unwieldy.
- Keep polling/application logic separate from rendering helpers.
- Put helper functions below main render entry points.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:
- `lp-cli/src/debug_ui/ui.rs`
- `lp-cli/src/debug_ui/mod.rs`
- `lp-cli/src/debug_ui/node_cards.rs`
- `lp-cli/src/debug_ui/inspector.rs`
- `lp-cli/src/debug_ui/slot_render.rs`
- `lp-cli/src/debug_ui/format.rs`

Expected changes:
- Split the current single-file UI into concept files:
  - `ui.rs`: polling and top-level layout.
  - `node_cards.rs`: central node workspace.
  - `inspector.rs`: right-side tree/detail panel.
  - `slot_render.rs`: shared compact/debug slot rendering.
  - `format.rs`: `LpValue`, `LpType`, product, and resource formatting.
- Add an `InspectorSelection` enum.
- Replace `selected_node: Option<NodeId>` with `selected: Option<InspectorSelection>` or add it alongside `selected_node` if simpler.
- In `eframe::App::update`:
  - Keep top panel.
  - Use `SidePanel::right("lp_debug_inspector")`.
  - Use `CentralPanel` for node cards.
- Move existing tree rendering into the right inspector.
- Keep existing selected node detail renderer as the first version of inspector detail.

## Validate

```bash
cargo fmt --check
cargo test -p lp-cli --no-run
cargo test -p lpc-view
```
