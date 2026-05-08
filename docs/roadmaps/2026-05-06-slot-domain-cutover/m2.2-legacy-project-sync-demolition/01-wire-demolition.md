# Phase 1: Wire Demolition

## Scope Of Phase

In scope:

- Remove active legacy project/detail/node-state wire types.
- Delete `LegacyWireNodeSpecifier`.
- Remove `legacy_detail_specifier` from active project request vocabulary or
  disable the old project sync request shape.
- Keep generic message envelopes and server response envelopes.
- Keep resource summary/payload types and update docs that refer to legacy
  `GetChanges`.
- Remove or update wire tests that only prove legacy detail serialization.

Out of scope:

- Canonical project sync response design.
- Engine/view/client implementation.
- Runtime slot roots.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep resource sync in `lpc-wire/src/project/resource_sync.rs` unless there is
  a strong reason to move it.
- Put helpers lower in files and tests at the bottom.
- Mark disabled project sync with an explicit TODO pointing to M3 canonical
  project sync.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-wire/src/legacy/mod.rs`
- `lp-core/lpc-wire/src/legacy/project/api.rs`
- `lp-core/lpc-wire/src/legacy/project/mod.rs`
- `lp-core/lpc-wire/src/legacy/nodes/**`
- `lp-core/lpc-wire/src/project/legacy_wire_node_specifier.rs`
- `lp-core/lpc-wire/src/project/wire_project_request.rs`
- `lp-core/lpc-wire/src/project/resource_sync.rs`
- `lp-core/lpc-wire/src/project/mod.rs`
- `lp-core/lpc-wire/src/message/client.rs`
- `lp-core/lpc-wire/src/server/api.rs`
- `lp-core/lpc-wire/src/lib.rs`
- `lp-core/lpc-wire/tests/m4_get_changes_all_specifiers_roundtrip.rs`

Expected changes:

- Delete legacy project/detail state modules or remove them from active module
  exports.
- Delete `LegacyWireNodeSpecifier` and all exports for it.
- Remove `legacy_detail_specifier` from `WireProjectRequest`.
- If `WireProjectRequest::GetChanges` cannot be made meaningful without M3,
  replace it with a disabled placeholder variant or remove the variant and let
  server/client code be updated in later phases.
- Preserve `ServerMsgBody<R>` and `Message`/`ServerMessage` envelopes.
- Preserve resource summary and payload request/response types.
- Update docs in `resource_sync.rs` to refer to canonical project sync or
  generic project sync instead of legacy response.
- Delete legacy-only wire tests. Keep tests for resource sync types and generic
  envelopes.

Edge cases:

- `serde_json_core` limitations described in legacy compat bytes docs may still
  matter for canonical resource refs. If deleting `LegacyCompatBytesField`,
  record any useful serialization notes in `future.md` or a TODO near resource
  sync.
- Do not delete `WireNodeStatus` if engine/client lifecycle still needs it.

## Validate

Run:

```bash
cargo test -p lpc-wire
cargo check -p lpc-wire --features schema-gen
git diff --check
```

If downstream crates break after this phase, record the breakage for later
phases rather than preserving legacy wire types.

