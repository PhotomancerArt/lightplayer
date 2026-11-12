# Debug UI Node Cards Summary

## What Was Built

- Split the debug UI into focused files: `ui`, `node_cards`, `inspector`, `slot_render`, and `format`.
- Replaced the left-tree/selected-node layout with a central node-card workspace and right-side debug inspector.
- Added row-oriented slot rendering for card surfaces while preserving recursive slot debug rendering in the inspector.
- Added resource and shape listings to the inspector.
- Added read-only resource summary iteration to `lpc-view`.

## Decisions For Future Reference

#### Binding Badges Wait For Real Sync

- **Decision:** Render authored `bindings` from def slots for now; do not add a new binding wire/view mirror.
- **Why:** The current project read response does not expose runtime binding entries, and this pass should stay UI-only.
- **Rejected alternatives:** Expanding `WireTreeDelta` or adding a binding read query in this UI pass.
- **Revisit when:** The debug UI needs source/target badges that distinguish authored defaults from resolved runtime bindings.

#### Product And Resource Details Stay Skeletons

- **Decision:** Products and resources render as compact skeleton rows without fetching payloads or running probes.
- **Why:** Default debug reads intentionally avoid expensive resource payloads and product probes.
- **Rejected alternatives:** Requesting all resource payloads from the debug UI by default.
- **Revisit when:** Product probe UX or resource payload opt-in controls are implemented.
