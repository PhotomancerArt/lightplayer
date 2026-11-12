# Phase 1: Edit Intents And Renderer Plumbing

## Scope Of Phase

Add the UI-side data structures and slot-render traversal context needed for
writable leaves to emit edit intents. Do not send mutations to the server yet.

In scope:

- add `lp-cli/src/debug_ui/slot_edit.rs`;
- define `SlotEditIntent`;
- define a small status lookup type or borrowed context used by value rows;
- pass root name, `SlotPath`, and `SlotPolicy` through slot row traversal;
- render supported writable controls and push intents.

Out of scope:

- mutation queueing;
- calling `ProjectView.slots.prepare_set_value(...)`;
- server response handling changes;
- editing non-leaf containers.

## Code Organization Reminders

- Prefer one clear concept per file.
- Keep `slot_edit.rs` focused on edit intent/status/editor choice.
- Keep traversal in `slot_render.rs`; avoid moving layout into `slot_edit.rs`.
- Tests, if any are added, go at the bottom of the file.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report changed files, validation, and deviations.

## Implementation Details

Relevant files:

- `lp-cli/src/debug_ui/mod.rs`
- `lp-cli/src/debug_ui/slot_render.rs`
- `lp-cli/src/debug_ui/node_cards.rs`
- `lp-core/lpc-model/src/slot/slot_policy.rs`
- `lp-core/lpc-model/src/slot/slot_value.rs`

Expected changes:

- Add `slot_edit.rs` with:
  - `SlotEditIntent { root: String, path: SlotPath, value: LpValue }`;
  - helper for rendering/editing one value leaf, or a pure helper that returns
    an optional edited `LpValue`;
  - type-safe support for:
    - `LpValue::Bool` as checkbox;
    - `LpValue::F32` with `ValueEditorHint::Slider`;
    - `LpValue::F32` with `ValueEditorHint::Number` or `Plain`.
- Update `render_slot_root_rows` to accept root name and an optional mutable
  edit-intent vector.
- When descending records, append `.<field>` to the current `SlotPath` and pass
  `field.policy` to the child.
- For maps/enums/options, keep rows read-only in this phase unless the existing
  path builder can represent them cleanly without extra scope.
- Keep products/resources as skeleton displays, not editable values.

Edge cases:

- If policy is not writable, render exactly as today.
- If editor hint and value type disagree, render read-only and keep hover
  metadata.
- Avoid cloning large `SlotData`; only clone small root/path/value when an edit
  happens.

## Validate

```bash
cargo check -p lp-cli
```
