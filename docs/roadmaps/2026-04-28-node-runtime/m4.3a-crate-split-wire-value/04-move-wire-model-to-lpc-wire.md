# Phase 4 — Move Wire Model to `lpc-wire`

## Scope of phase

Move engine-client wire contract types out of `lpc-model` into `lpc-wire`,
using `Wire*` names for ambiguous wire-facing types.

Out of scope:

- Do not move source/on-disk types; that is Phase 3.
- Do not move engine runtime implementation; that is Phase 5.
- Do not implement new sync behavior beyond types/helpers needed to compile.
- Do not make `lpc-wire` depend on `lps-shared`.
- Do not commit.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a `TODO` comment so it can be found later.

## Sub-agent reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If blocked by source-vs-wire boundary ambiguity, stop and report.
- Report back: files changed, validation run, validation result, and any
  deviations from this phase.

## Implementation details

Move these wire/protocol concepts from `lp-core/lpc-model/src` to
`lp-core/lpc-wire/src`:

- `message.rs`
- `json.rs`
- `transport_error.rs`
- `server/`
- `project/api.rs`
- `project/handle.rs` if it is visible in client requests/responses
- `tree/child_kind.rs`
- `tree/entry_state_view.rs`
- `tree/tree_delta.rs`
- `state/` macros/helpers used for partial state serialization
- `serde_base64.rs` if first/only consumer is wire serialization

Target structure:

```text
lp-core/lpc-wire/src/
├── lib.rs
├── message/
│   ├── mod.rs
│   ├── client_message.rs
│   ├── server_message.rs
│   └── message.rs
├── project/
│   ├── mod.rs
│   ├── wire_project_handle.rs
│   ├── wire_project_request.rs
│   ├── wire_project_status.rs
│   └── wire_project_view.rs
├── tree/
│   ├── mod.rs
│   ├── wire_child_kind.rs
│   ├── wire_entry_state.rs
│   └── wire_tree_delta.rs
├── state/
│   ├── mod.rs
│   ├── macros.rs
│   └── test_state.rs
├── json.rs
├── server.rs
└── transport_error.rs
```

Use subdirectories for `server/` if the existing code is already split and
keeping that structure is cleaner.

### Naming

Use `Wire*` names where the type would otherwise be ambiguous:

- `TreeDelta` -> `WireTreeDelta`.
- `EntryStateView` -> `WireEntryState` or `WireEntryStateView`.
- `ChildKind` -> `WireChildKind`.
- `ProjectHandle` -> `WireProjectHandle` if wire-visible.
- `ProjectRequest` -> `WireProjectRequest` if wire-visible.
- `NodeStatus` -> `WireNodeStatus` if wire-visible.

If a rename creates too much churn, define the `Wire*` primary type and add a
local `pub type` alias in `lpc-wire` only. Do not add aliases back in
`lpc-model`.

### Dependencies

`lpc-wire` may depend on:

- `lpc-model`
- `lpc-source` only when a wire message truly carries authored source data
- `serde`, JSON helpers, and existing no-std serialization crates as needed

`lpc-wire` must not depend on `lps-shared`.

### Legacy state helpers

The current `lpc-model/src/state/` module is used by legacy nodes such as
`lp-legacy/lpl-model/src/nodes/output/state.rs` to generate partial state
serialization. Move it to `lpc-wire` because its primary purpose is wire
updates.

Update legacy imports accordingly.

### Update exports

Remove moved wire exports from `lpc-model` crate root. Export them from
`lpc-wire` crate root or appropriate submodules.

Update immediate dependents enough for validation.

## Tests to preserve/add

- Preserve message serde tests, if any.
- Preserve tree delta serde tests, if any.
- Preserve legacy state macro tests after moving to `lpc-wire`.
- Add a narrow serde round-trip test for one renamed wire type if no existing
  tests cover the moved module.

## Validate

Run:

```bash
cargo test -p lpc-wire
cargo test -p lpc-model
cargo check -p lpc-wire --no-default-features
```

Also run the legacy model check if `state/` import sites moved:

```bash
cargo check -p lpl-model
```

If formatting changed, run:

```bash
cargo +nightly fmt
```
