# Phase 5: Debug UI Regressions

## Scope Of Phase

Add end-to-end-ish coverage for the debug UI symptoms that exposed the protocol
issue.

In scope:

- prove paged shape sync completes before slot roots are requested;
- prove node def roots apply after paging;
- prove selected resource payloads are requested and cached;
- keep the UI behavior consistent with existing design.

Out of scope:

- visual redesign of the debug UI;
- adding a new resource browser workflow;
- changing runtime/resource wire payload shapes.

## Code Organization Reminders

- Keep tests in `#[cfg(test)] mod tests` at the bottom of `ui.rs`.
- Prefer small request-builder tests over brittle egui rendering tests.
- Avoid adding sleeps or timing-dependent tests.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-cli/src/debug_ui/ui.rs`
- `lp-cli/src/debug_ui/inspector.rs`
- `lp-cli/src/debug_ui/node_cards.rs`
- `lp-core/lpc-view/src/project/apply_project_read.rs`
- `lp-core/lpc-engine/src/engine/project_read_stream.rs`

Test ideas:

- Unit-test `debug_ui_project_read`:
  - when `shapes_synced` is false, it requests a shape page and does not include
    slots unless mutations require them;
  - when `shapes_synced` is true and the view has no roots, it includes slots;
  - when a resource is selected, it requests
    `ResourcePayloadRead::ByRefs([selected])`.
- Engine/view regression:
  - create a small engine/project with at least root project def and one
    resource-backed node;
  - page shapes with `limit = 1` into a `ProjectView`;
  - request node slots and resources;
  - apply the response and assert `node.0.def` exists and the selected resource
    payload is cached.

If a full debug UI state test becomes too invasive, keep the test at the
request-builder and `ProjectView` application layer. The bug is protocol/view
state, not egui rendering.

Potential follow-up after this phase:

- If one bad domain should not block other result domains in the debug UI, add a
  separate error-aggregation plan. Do not hide slot protocol errors during this
  cleanup.

## Validate

```bash
cargo test -p lp-cli debug_ui
cargo test -p lpc-engine project_read_stream
```
