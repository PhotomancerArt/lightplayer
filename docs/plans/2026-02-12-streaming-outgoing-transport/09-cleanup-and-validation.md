# Phase 9: Cleanup and Validation

## Scope of phase

Final cleanup: remove TODOs, debug prints, fix warnings. Validate full workspace. Create plan summary.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions at the bottom of files
- Keep related functionality grouped together
- Any temporary code should be a TODO comment so we find it later

## Cleanup & Validation

1. Grep for `TODO`, `FIXME`, `dbg!`, `println!` (debug) in changed files
2. Remove or resolve
3. Run `just fmt` and `just check`
4. Fix all warnings
5. Run full test suite: `just ci` or `just check` + `just test`

## Plan Cleanup

Add summary to `docs/plans/2026-02-12-streaming-outgoing-transport/summary.md`:

- Streaming outgoing transport: serialize in io_task, Channel<ServerMessage, 1>
- Async ServerTransport trait
- Removed ChunkingSerWrite, OUTGOING_CHUNKS
- Kept lp-model ser-write-json tests and SerializableNodeDetail format
- test_json adapted to new transport

Move plan to `docs/plans-done/2026-02-12-streaming-outgoing-transport/`.

## Commit

```
feat(fw-esp32): streaming outgoing transport, async ServerTransport

- Serialize ServerMessage in io_task, stream to serial via ser-write-json
- Channel<ServerMessage, 1> for backpressure
- Async ServerTransport trait (send, receive, close)
- Removed ChunkingSerWrite, OUTGOING_CHUNKS
- Updated all transports and server loops
- Adapted test_json to new architecture
```
