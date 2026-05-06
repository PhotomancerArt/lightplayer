# Phase 3: Mockup Uses Real Wire/View

## Scope Of Phase

Replace mockup-local protocol and client mirror types with real `lpc-wire` and
`lpc-view` types.

In scope:

- Update `lpc-slot-mockup` dependencies.
- Remove or shrink `lpc-slot-mockup/src/view`.
- Use `lpc_wire::slot` types from mockup snapshot/diff helpers.
- Use `lpc_view::SlotMirrorView` in tests.
- Keep mockup debug print helpers and runtime traversal local.

Out of scope:

- Mutation application.
- Transport integration.
- Real engine/source integration.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep mockup-only debugging in the mockup.
- Do not weaken the trace-heavy tests.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-mockup/Cargo.toml`
- `lp-core/lpc-slot-mockup/src/lib.rs`
- `lp-core/lpc-slot-mockup/src/view/*`
- `lp-core/lpc-slot-mockup/src/wire/types.rs`
- `lp-core/lpc-slot-mockup/src/wire/snapshot.rs`
- `lp-core/lpc-slot-mockup/src/wire/diff.rs`
- `lp-core/lpc-slot-mockup/src/tests/fixture.rs`

Expected result:

- Mockup full sync returns `WireSlotFullSync`.
- Mockup diffs return `Vec<WireSlotPatch>`.
- Tests use `SlotMirrorView` as the client mirror.

## Validate

```bash
cargo test -p lpc-slot-mockup -- --nocapture --test-threads=1
```
