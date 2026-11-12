# Phase 1: Engine Session Vocabulary

## Scope Of Phase

Rename/evolve the slot-centric session vocabulary into a single `EngineSession` concept.

In scope:

- Replace `ResolveSession` as the public execution-session type with `EngineSession`.
- Replace `ResolveHost` with `EngineSessionHost` or equivalent.
- Keep `Resolver` as a lower-level cache/materialization helper.
- Keep existing slot resolution behavior and tests green.
- Update names in tests and node contexts where needed.

Out of scope:

- Render product ownership changes.
- Shader render state movement.
- `NodeEntryState::Executing`.
- Deleting `RenderProductStore`.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- If renaming files, keep search-friendly names such as `engine_session.rs` and `engine_session_host.rs`.
- Do not leave old `ResolveSession` and new `EngineSession` as long-lived parallel abstractions.
- Keep compatibility re-exports only if they reduce churn inside this phase; remove them by the cleanup phase.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/resolver/resolve_session.rs`
- `lp-core/lpc-engine/src/resolver/resolve_host.rs`
- `lp-core/lpc-engine/src/resolver/tick_resolver.rs`
- `lp-core/lpc-engine/src/resolver/mod.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/node/contexts.rs`
- `lp-core/lpc-engine/src/node/node_runtime.rs`

Expected changes:

- Rename `ResolveSession` to `EngineSession`.
- Rename `ResolveHost` to `EngineSessionHost`.
- Rename `SessionHostResolver` if needed; a name like `SessionTickResolver` or `EngineSessionTickResolver` is acceptable as a transitional bridge.
- Keep `QueryKey` for slot resolution unless renaming is cheap. Avoid taking on a broad request-key rewrite in this phase.
- Keep `ResolveError` / `SessionResolveError` names only if changing them expands scope too much; otherwise choose `EngineSessionError`.
- Update docs to explain that `EngineSession` handles more than slot resolution, while `Resolver` remains slot cache/helper machinery.

Validation focus:

- Existing resolver tests should still pass.
- Engine tests should still compile and pass.

## Validate

```bash
cargo check -p lpc-engine
cargo test -p lpc-engine resolver::
cargo test -p lpc-engine engine::
cargo test -p lpc-engine node::
```

