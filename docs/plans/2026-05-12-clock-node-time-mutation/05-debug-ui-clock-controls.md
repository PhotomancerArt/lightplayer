# Phase 5: Debug UI Clock Controls

## Scope Of Phase

Expose clock controls in the debug UI using the real project mutation path.

In scope:

- Add a small clock-control panel to the debug UI.
- Find clock nodes from synced node/slot data.
- Render running/rate/scrub controls from `node.<id>.def.controls`.
- Send slot mutation requests through `LpClient`.
- Show pending and rejected mutation states.
- Keep raw node/slot inspector behavior available.

Out of scope:

- Building the final product UI.
- Optimistic local writes.
- Persistent save UI.
- Editing arbitrary slot fields outside the clock controls.

## Code Organization Reminders

- Use a separate `lp-cli/src/debug_ui/clock_controls.rs`.
- Keep generic mutation helper code separate from egui rendering if it starts to grow.
- Do not turn `ui.rs` into a giant file again.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-cli/src/debug_ui/ui.rs`
- `lp-cli/src/debug_ui/node_cards.rs`
- `lp-cli/src/debug_ui/clock_controls.rs`
- `lp-cli/src/client.rs`
- `lp-core/lpc-view/src/slot/mirror.rs`
- `lp-core/lpc-wire/src/slot/mutation.rs`

UI behavior:

- A clock panel should show:
  - current produced seconds,
  - running checkbox,
  - rate slider,
  - scrub offset slider `-10..10s`.
- Controls should create `WireSlotMutationRequest` through `ProjectView.slots.prepare_set_value`.
- Do not mutate the local slot mirror optimistically.
- Show a small spinner/text while a mutation id is pending.
- Show rejection reason if server rejects the mutation.

Client behavior:

- Add an `LpClient` method for project mutation if one does not exist.
- Keep request/response small and materialized; no streaming path is needed for mutation responses.

Integration details:

- The panel can initially appear above the node workspace or inside clock node cards.
- Prefer making it visible enough that shader-debugging workflow is obvious.
- The raw inspector should still show the actual def/state slots for verification.

Tests:

- If UI unit tests are impractical, add client/wire/server tests and rely on `cargo check -p lp-cli`.
- Add focused test for preparing a mutation request against `controls.rate` in the slot mirror if not already covered.

## Validate

```bash
cargo fmt
cargo check -p lp-cli
cargo test -p lpc-view mutation
cargo test -p lpc-wire mutation
```
