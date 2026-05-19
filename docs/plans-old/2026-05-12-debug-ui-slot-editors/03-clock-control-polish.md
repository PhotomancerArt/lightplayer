# Phase 3: Clock Control Polish

## Scope Of Phase

Make the new generic editors usable for shader time debugging in the node card
UI.

In scope:

- make writable clock controls visible and comfortable enough to use;
- show minimal pending/error state next to edited rows;
- verify clock changes affect shader time through existing bindings.

Out of scope:

- bespoke final clock transport UI;
- broad style overhaul;
- adding mutation support for other node types.

## Code Organization Reminders

- Keep clock-specific layout minimal.
- If a small helper makes the node card easier to scan, put it in
  `node_cards.rs` or a clearly named one-concept file.
- Do not hardcode clock mutation paths in the mutation layer.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report changed files, validation, and deviations.

## Implementation Details

Relevant files:

- `lp-cli/src/debug_ui/node_cards.rs`
- `lp-cli/src/debug_ui/slot_render.rs`
- `lp-cli/src/debug_ui/slot_edit.rs`
- `lp-core/lpc-model/src/nodes/clock/clock_controls.rs`

Expected changes:

- Ensure `def / config` for the clock node exposes:
  - `controls.running`;
  - `controls.rate`;
  - `controls.scrub_offset_seconds`.
- It is acceptable to default-open `controls` for clock nodes if that can be
  done cleanly, but avoid brittle path-specific hacks if it gets ugly.
- Pending state can be small text such as `pending`.
- Rejection state can be a compact red label with hover text for the full
  rejection.

Manual validation:

- Run `cargo run -p lp-cli dev examples/basic`.
- Toggle clock running.
- Change rate.
- Scrub offset and confirm visual shader time changes.

## Validate

```bash
cargo check -p lp-cli
cargo check -p lpa-server
```
