# Phase 1: Wire Read Vocabulary

## Scope Of Phase

Add the canonical stateless project read request/response vocabulary in
`lpc-wire`.

In scope:

- Add `ProjectReadRequest`, `ProjectReadResponse`, `ProjectReadQuery`,
  `ProjectReadResult`, and `ReadLevel`.
- Add shape, node, and resource read query/result types.
- Replace `WireProjectRequest::SyncDisabled` with
  `WireProjectRequest::Read(ProjectReadRequest)`.
- Re-export the new types from `lpc-wire` and project modules.
- Add JSON roundtrip tests.

Out of scope:

- Engine execution.
- Client view apply code.
- Server handler behavior beyond compile updates needed by the enum change.
- Probe execution.

## Code Organization Reminders

- Use granular files under `lp-core/lpc-wire/src/project/read/`.
- Use concise names inside `lpc-wire`; do not prefix every new type with
  `Wire`.
- Keep `mod.rs` files mostly declarations and re-exports.
- Put tests at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-wire/src/project/wire_project_request.rs`
- `lp-core/lpc-wire/src/project/mod.rs`
- `lp-core/lpc-wire/src/lib.rs`
- `lp-core/lpc-wire/src/project/resource_sync.rs`
- `lp-core/lpc-wire/src/slot/sync.rs`
- `lp-core/lpc-wire/src/tree/wire_tree_delta.rs`

Expected shape:

```rust
pub enum WireProjectRequest {
    Read(ProjectReadRequest),
}

pub struct ProjectReadRequest {
    pub since: Option<Revision>,
    pub queries: Vec<ProjectReadQuery>,
    pub probes: Vec<ProjectProbeRequest>,
}

pub enum ProjectReadQuery {
    Shapes(ShapeReadQuery),
    Nodes(NodeReadQuery),
    Resources(ResourceReadQuery),
}
```

Resource queries should replace the public role of `ResourceSummarySpecifier`
and `RuntimeBufferPayloadSpecifier`, but those old types may remain until later
phases remove callers.

Use existing payload structs where practical:

- `SlotShapeRegistrySnapshot`
- `WireTreeDelta`
- `WireSlotFullSync`
- `WireSlotRootSnapshot`
- `WireResourceSummary`
- `WireRuntimeBufferPayload`

## Validate

```bash
cargo fmt --check
cargo test -p lpc-wire
```
