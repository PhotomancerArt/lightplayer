# Phase 4: Cleanup And Validation

## Scope Of Phase

Clean up the corrected foundation and run final validation.

In scope:

- Remove `SyntaxNode` completely.
- Remove stale documentation that says the primary reader is tree-backed.
- Update M1 summary or add correction summary.
- Search for scratch/debug artifacts.
- Run final focused validation.

Out of scope:

- New feature work.
- Codegen.
- Production adoption.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep module exports coherent.
- Do not leave commented-out experiments or temporary TODOs.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant checks:

- `rg -n "SyntaxNode|tree-backed|temporary syntax tree" lp-core/lpc-wire/src docs/roadmaps/2026-05-13-slot-native-streaming-serialization/m1-parser-generator-foundation`
- `rg -n "TODO|dbg!|eprintln!|println!" lp-core/lpc-wire/src/slot lp-core/lpc-slot-mockup/src/tests/native_stream.rs`

Expected final state:

- No production/test reader path depends on `SyntaxNode`.
- Docs describe the corrected streaming reader design.
- JSON source does not materialize all syntax events before reading.

## Validate

```bash
cargo fmt
cargo test -p lpc-model slot_codec
cargo test -p lpc-wire slot
cargo test -p lpc-slot-mockup native_stream
cargo check -p lpc-wire --no-default-features
```
