# Phase 1: Streaming Syntax Source

## Scope Of Phase

Replace eager event-vector syntax sources with stream-backed sources.

In scope:

- Make `JsonSyntaxSource::next_event()` parse on demand.
- Add optional source spans to syntax events or event metadata.
- Make `TomlSyntaxSource` expose the same event source contract without
  prebuilding `Vec<SyntaxEvent>` if practical.
- Preserve event vocabulary needed by the reader.
- Remove parser behavior that materializes the whole JSON document as events.

Out of scope:

- Streaming typed reader implementation.
- Codegen.
- Production loader/message adoption.

## Code Organization Reminders

- Prefer granular files with one main concept per file under `lpc-model/src/slot_codec/`.
- Keep parser helper functions lower in the file.
- Avoid commented-out experiments.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot_codec/`

Expected changes:

- Replace `JsonSyntaxSource { events: Vec<SyntaxEvent>, index }` with cursor
  state over the input.
- Track enough parser state to emit object props, array delimiters, scalars,
  and close delimiters in sequence.
- Record byte spans for JSON events where straightforward.
- Replace `TomlSyntaxSource { events: Vec<SyntaxEvent>, index }` with borrowed
  traversal state if practical. If that becomes too large, keep TOML simpler
  but document that JSON is the streaming proof.
- Add tests proving multiple `next_event()` calls advance the parser without
  an event vector.

Constraints:

- Keep `lpc-wire --no-default-features` working.
- Do not use `std` types in the implementation.
- String events may allocate the current chunk but not the whole document.

## Validate

```bash
cargo test -p lpc-model slot_codec
cargo test -p lpc-wire slot
cargo check -p lpc-wire --no-default-features
```
