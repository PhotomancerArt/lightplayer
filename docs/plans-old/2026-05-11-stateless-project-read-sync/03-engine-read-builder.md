# Phase 3: Engine Read Builder

## Scope Of Phase

Add engine-side helpers to answer `ProjectReadRequest` for shapes, nodes, and
resources.

In scope:

- Add `Engine::read_project(&mut self, request: ProjectReadRequest) ->
  ProjectReadResponse` or an equivalent helper.
- Answer shape queries from `SlotShapeRegistry`.
- Answer node queries from `NodeTree` using existing tree sync helpers.
- Answer slot/root detail for node-authored and runtime state roots where
  practical.
- Answer resource queries from runtime buffer/resource stores.
- Keep probe results explicit but shallow if execution is not implemented.
- Add tests using `examples/basic` or existing engine test fixtures.

Out of scope:

- Fine-grained registry diffs.
- Full minimal slot patching across every root.
- Mutation.
- UI behavior.

## Code Organization Reminders

- Put project read helpers under `lp-core/lpc-engine/src/engine/`.
- Keep resource read and node read helper code split when it improves scan.
- Keep one concept per file.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/node/sync.rs`
- `lp-core/lpc-engine/src/node/node_tree.rs`
- `lp-core/lpc-engine/src/artifact/artifact_store.rs`
- `lp-core/lpc-engine/src/resource/`
- `lp-core/lpc-engine/src/resources/buffer/`

Since behavior:

- `since: None`: return full data for selected queries.
- `since: Some(rev)`: return changed data where existing revision helpers make
  that easy; full replacement detail is acceptable for selected changed roots.

Ordering:

- Response results must align with query order.
- Internal item order should be deterministic.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-engine
```
