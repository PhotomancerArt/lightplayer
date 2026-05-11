# M3.1: State Wire Projection Design

## Scope of Work

M3.1 hardens the legacy client sync projection before M4 ports legacy runtime
behavior onto the core engine. The milestone keeps the existing legacy
`ProjectResponse::GetChanges` compatibility shape, but makes the boundary and
tests explicit enough that M4 can project core-engine state into a form clients
can use.

The core concept is `SyncProjection`: given a client frame id and watch/detail
interests, project the current project/runtime state into a client-usable form
that lets that client come up to speed.

In scope:

- Document the `SyncProjection` boundary and the current `LegacySyncProjection`
  compatibility path.
- Make `ProjectView` apply real config details instead of placeholder configs.
- Harden tests around partial state, config detail application, and legacy
  response serialization.
- Inventory heavy wire snapshots that should become store-backed
  product/buffer data in M3.2.

Out of scope:

- A new transport shape for sync.
- A new texture/buffer product store.
- Binary chunks, compression, scaling, throttling, or texture diffs.
- Porting legacy runtime behavior onto the core engine.
- Removing `LegacyProjectRuntime` or replacing the legacy `GetChanges` path.

## File Structure

```text
docs/roadmaps/2026-05-01-runtime-core/
├── m3.1-state-wire-projection.md
└── m3.1-state-wire-projection/
    ├── 00-notes.md
    ├── 00-design.md
    ├── 01-document-sync-projection-boundary.md
    ├── 02-fix-project-view-config-details.md
    ├── 03-harden-legacy-wire-serialization.md
    ├── 04-cleanup-validation-summary.md
    └── summary.md

lp-core/
├── lpc-wire/
│   └── src/
│       ├── legacy/project/api.rs
│       └── state/macros.rs
└── lpc-view/
    ├── src/project/project_view.rs
    └── tests/client_view.rs
```

## Conceptual Architecture

```text
client request
  ├─ since_frame
  └─ watch/detail specifier
        │
        ▼
SyncProjection
  ├─ compares current project/runtime state to since_frame
  ├─ filters detail by watch interests
  ├─ emits structural/change facts
  ├─ emits config snapshots where requested
  └─ emits external state snapshots/deltas where requested
        │
        ▼
LegacySyncProjection
  └─ ProjectResponse::GetChanges / SerializableProjectResponse
        │
        ▼
ProjectView
  ├─ prunes removed nodes
  ├─ applies status/config/state versions
  ├─ stores real concrete config details
  └─ merges partial state snapshots
```

`SyncProjection` is a design boundary, not necessarily a new public trait in
M3.1. It names the step that derives a client sync view from internal state. The
projected payload may include snapshots, deltas, frame versions, status, and
legacy compatibility fields, but it is not the authoritative runtime storage
model.

`LegacySyncProjection` is the current compatibility projection that emits
legacy `GetChanges` payloads. M4 can keep this shape while the core engine
becomes the underlying runtime owner.

## Main Components

### Legacy `GetChanges`

`ProjectResponse::GetChanges` remains the M4 compatibility payload. It carries
node handles, node changes, requested details, and frame/version metadata.

`SerializableProjectResponse` remains the serde-friendly wrapper around the
trait-object-bearing `ProjectResponse`. M3.1 should make its supported
serialization semantics explicit and tested.

### Project View

`ProjectView` is the client-side mirror for legacy project sync. It should
faithfully apply config and state details. Placeholder configs are acceptable
for nodes that have only a `Created` change and no detail, but once detail is
present the view must store the real concrete config from the response.

### Wire State Snapshots

Legacy `NodeState` fields remain compatibility snapshots for M4:

- texture bytes plus width/height/format;
- fixture lamp colors, mapping cells, texture/output handles;
- output channel bytes;
- shader diagnostic/runtime state.

M3.1 should document these as projected snapshots. M3.2 owns the runtime
buffer/product identity model, including refs, diffs, and transport policy.

## Phase Outline

1. Document SyncProjection Boundary              [sub-agent: main,       model: gpt-5.5,   parallel: -]
2. Fix ProjectView Config Details                [sub-agent: yes,        model: kimi-k2.5, parallel: 3]
3. Harden Legacy Wire Serialization              [sub-agent: yes,        model: kimi-k2.5, parallel: 2]
4. Cleanup, review, and validation               [sub-agent: supervised, model: gpt-5.5,   parallel: -]
