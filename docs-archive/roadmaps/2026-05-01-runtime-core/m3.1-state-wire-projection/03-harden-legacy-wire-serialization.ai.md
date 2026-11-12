# Phase 3: Harden Legacy Wire Serialization

## Scope of Phase

Make the supported legacy `SerializableProjectResponse` serialization behavior
explicit and covered by passing tests.

In scope:

- Replace the ignored `SerializableProjectResponse` test with passing coverage.
- Prefer a real `SerializableProjectResponse` JSON round trip if the current
  custom serializer and derived deserializer are compatible after small fixes.
- If full round trip is not truthful for partial-state payloads, replace the
  ignored test with explicit server-to-client serialization tests that cover the
  supported behavior.
- Clean up misleading comments in the state serialization macro if they are no
  longer temporary.

Out of scope:

- `ProjectView` config application; that is Phase 2.
- New wire formats or protocol redesign.
- New runtime product/buffer storage.
- Rewriting the entire `impl_state_serialization!` macro.

## Code Organization Reminders

- Keep tests concise and focused.
- Put helpers at the bottom of the test module.
- Do not add broad abstractions unless they remove real duplication.
- Any temporary code should have a TODO comment so it can be found later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If the serializer/deserializer shape requires a design decision beyond a small
  fix, stop and report rather than improvising a new protocol.
- Report back: what changed, what was validated, and any deviations.

## Implementation Details

Primary files:

- `lp-core/lpc-wire/src/legacy/project/api.rs`
- `lp-core/lpc-wire/src/state/macros.rs`

Current problem:

- `SerializableProjectResponse` has a custom `Serialize` implementation.
- It derives `Deserialize`.
- A test named `test_serializable_project_response_serialization` is currently
  ignored with a TODO about deserialization not matching custom serialization.

Target behavior:

- There should be no ignored serialization test for this path.
- The tests should clearly prove what the wire layer supports.

Preferred path:

1. Try to make the ignored round-trip test pass without changing the external
   wire shape.
2. Verify the serialized JSON shape still contains:
   - top-level `GetChanges`;
   - `node_details`;
   - externally tagged `SerializableNodeDetail` variants;
   - externally tagged `NodeState` variants;
   - partial state fields omitted according to `since_frame`.
3. If derived deserialization cannot match the custom serialization without a
   non-trivial custom deserializer, do not invent a new protocol. Instead:
   - remove `#[ignore]`;
   - replace the round-trip assertion with serialization shape assertions and a
     separate test that deserializes the supported derived format if useful;
   - update comments to say `SerializableProjectResponse` is the server-to-client
     compatibility encoder unless round-trip is intentionally supported.

Be careful with partial-state semantics:

- Initial sync (`since_frame == FrameId::default()`) includes all fields.
- Later sync only includes fields whose `changed_frame() > since_frame`.
- Deserializing omitted fields creates defaults, so client merge semantics are
  handled by `NodeState::merge_from` / per-state `merge_from`.

Macro cleanup:

- In `lp-core/lpc-wire/src/state/macros.rs`, the comment
  `Temporary: Simple Serialize implementation for NodeState compatibility` may
  be stale. If the implementation is now an intentional compatibility path,
  update the comment to describe that.
- Do not otherwise rewrite the macro unless required to make tests pass.

## Validate

Run:

```bash
cargo test -p lpc-wire
```
