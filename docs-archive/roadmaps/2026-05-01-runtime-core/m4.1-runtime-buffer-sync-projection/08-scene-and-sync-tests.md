# Phase 8: Scene and sync tests

## Scope of phase

Update integration tests so M4.1 verifies real detail/resource sync instead of
the M4 metadata-only behavior.

In scope:

- Update `scene_render`, `scene_update`, and `partial_state_updates` tests.
- Add focused wire/view tests for resource summary and payload behavior.
- Add tests for render-product full/native payload sync.
- Add tests for fixture colors runtime buffer sync.

Out of scope:

- Source reload/deletion parity assertions; M4.2 owns those.
- Multi-shader render order; M4.3 owns that.
- Manual `just demo` validation; user will run that.

## Code organization reminders

- Tests should be concise and focused.
- Put test helpers below tests.
- Do not weaken tests to match broken behavior.
- Prefer builders/helpers over repeated setup.

## Sub-agent reminders

- Do not commit.
- Do not mark tests ignored.
- Do not delete meaningful coverage to make tests pass.
- If behavior fails due to a real runtime bug, stop and report instead of
  weakening the assertion.

## Implementation details

Read:

- `lp-core/lpc-engine/tests/scene_render.rs`
- `lp-core/lpc-engine/tests/scene_update.rs`
- `lp-core/lpc-engine/tests/partial_state_updates.rs`
- `lp-core/lpc-view/tests`
- `lp-core/lpc-wire/src/legacy/project/api.rs`

Update tests so they assert M4.1 behavior:

- watched nodes receive `node_details`;
- resource summaries are populated when requested;
- fixture `lamp_colors` points to a fixture colors buffer;
- output channel payloads can be requested and resolved;
- render-product payloads materialize full/native texture bytes;
- `ProjectView` no longer leaves watched nodes waiting for state data after an
  initial detail sync.

Scene update tests should still not assert reload/deletion behavior until M4.2.
They can assert that resource sync remains stable across ticks.

## Validate

Run:

```bash
cargo test -p lpc-engine --test scene_render --test scene_update --test partial_state_updates
cargo test -p lpc-view
cargo test -p lpc-wire
```
