# Phase 4: Project Slot Mutation

## Scope Of Phase

Wire the existing slot mutation model into real project requests and implement a narrow server-side authored-def mutation path.

In scope:

- Add project-scoped mutations to `ProjectReadRequest`/`ProjectReadResponse`.
- Route mutation through server/project manager to the target engine.
- Apply `SetValue` to value leaves on `node.<id>.def` roots.
- Use expected shape/data revisions for optimistic conflict checks.
- Reuse client pending mutation support.
- Add tests for accepted, stale, wrong-type, unknown-root, unknown-path, and unsupported mutations.

Out of scope:

- Container mutation.
- Runtime state mutation.
- Artifact writeback.
- Mutating bindings in this first pass unless it falls out naturally.

## Code Organization Reminders

- Keep mutation code out of giant files where possible.
- Suggested engine file: `lp-core/lpc-engine/src/engine/slot_mutation.rs`.
- Keep project request message code in `messages/project_*` or `project/` following current layout.
- Tests at bottom.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-wire/src/slot/mutation.rs`
- `lp-core/lpc-wire/src/messages/project_read/project_read_request.rs`
- `lp-core/lpc-wire/src/messages/project_read/project_read_response.rs`
- `lp-core/lpc-wire/src/server/api.rs`
- `lp-core/lpc-view/src/slot/mirror.rs`
- `lp-core/lpc-engine/src/engine/slot_mutation.rs`
- `lp-app/lpa-server/src/server.rs`
- `lp-app/lpa-server/src/handlers.rs`
- `lp-cli/src/client.rs`

Wire shape:

```rust
pub struct ProjectReadRequest {
    pub mutations: Vec<WireSlotMutationRequest>,
}

pub struct ProjectReadResponse {
    pub mutations: Vec<WireSlotMutationResponse>,
}
```

The server applies mutations before collecting read/probe results. A mutation-only
request is represented by an empty `queries`/`probes` list.

Root addressing:

- Use root names already synced to clients, e.g. `node.<id>.def`.
- Reject all roots not matching a known mutable def root.

Mutation algorithm:

1. Locate node id from root name.
2. Locate node entry and its `NodeDefHandle`.
3. Resolve mutable `NodeDef` from artifact/inline storage.
4. Check root shape id and registry shape revision.
5. Look up slot data revision at the requested `SlotPath`.
6. Validate incoming `LpValue` against the leaf shape.
7. Mutate the typed field.
8. Stamp current revision.
9. Mark node/def changed so project reads return updated slot data.

Implementation constraint:

- Avoid a parallel authored-def store if possible. The artifact store should remain the source of truth for loaded node defs. If the current artifact store API cannot expose mutable loaded defs, add a small method there rather than duplicating defs.

Tests:

- Accepted mutation changes `node.<id>.def.controls.running`.
- Accepted mutation changes `controls.rate`.
- Stale data revision rejects with `DataConflict`.
- Stale shape revision rejects with `ShapeConflict`.
- Wrong `LpValue` type rejects with `WrongType`.
- Runtime state root rejects with `UnsupportedTarget`.
- Unknown node/root/path reject correctly.
- Mutation response clears pending state in `lpc-view` when applied.

## Validate

```bash
cargo fmt
cargo test -p lpc-wire mutation
cargo test -p lpc-view mutation
cargo test -p lpc-engine mutation
cargo check -p lpa-server
```
