# Phase 2: Node Cards And Slot Rows

## Scope Of Phase

Render the main node UI as boxes/cards with slot sections.

In scope:
- One card per node in stable tree order.
- Header with node label, id, status, state, and path.
- Prominent top-level `input` and `output` slot rows when present.
- Expandable `def/config` and `state` sections.
- Row-oriented slot display for common slot shapes.
- Children section after slots.

Out of scope:
- Real binding annotations beyond placeholders.
- Editing controls.
- Heavy previews.

## Code Organization Reminders

- Prefer helper functions such as `render_node_card`, `render_slot_section`, and `render_slot_row`.
- If splitting files, keep node card rendering in `node_cards.rs`.
- Keep the old recursive renderer available for debug detail.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:
- `lp-cli/src/debug_ui/ui.rs`
- Optional: `lp-cli/src/debug_ui/node_cards.rs`

Expected changes:
- Add helpers to find roots by name:
  - `node.<id>.def`
  - `node.<id>.state`
- For each root, inspect the top-level `SlotShape::Record` fields.
- Render fields named `input` and `output` prominently.
- Render config-ish/def content under an expandable section.
- Render state under an expandable section.
- Display rows as:
  - name
  - compact value
  - revision
  - type/editor hint when useful
- Product/resource `LpValue`s should render as skeleton rows with a disabled/detail placeholder.

Edge cases:
- Missing root shape: show a red diagnostic in the card.
- Shape/data mismatch: reuse the fallback renderer.
- Nodes with no state root should not show an error.

## Validate

```bash
cargo fmt --check
cargo test -p lp-cli --no-run
cargo test -p lpc-view
```

