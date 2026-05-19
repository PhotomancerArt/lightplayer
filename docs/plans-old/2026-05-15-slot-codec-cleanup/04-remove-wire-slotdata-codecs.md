# Phase 4: Remove Wire SlotData Codecs

## Scope Of Phase

Move old `lpc-wire` SlotData JSON/TOML callers to `lpc-model` registry writer
APIs, then delete the old writer files and exports.

Out of scope: redesigning project read messages.

## Code Organization Reminders

- Use registry APIs instead of duplicating shape walking.
- Keep wire slot sync/mutation types intact.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report.

## Implementation Details

Update:

- `lp-core/lpc-engine/src/engine/project_read_stream.rs`
- `lp-core/lpc-slot-mockup/src/tests/storage_codec.rs`
- `lp-core/lpc-wire/src/slot/mod.rs`
- `lp-core/lpc-wire/src/lib.rs`

Delete when unused:

- `lp-core/lpc-wire/src/slot/authored_toml.rs`
- `lp-core/lpc-wire/src/slot/slot_data_json.rs`

## Validate

```bash
cargo test -p lpc-slot-mockup storage_codec
cargo check -p lpc-engine
```
