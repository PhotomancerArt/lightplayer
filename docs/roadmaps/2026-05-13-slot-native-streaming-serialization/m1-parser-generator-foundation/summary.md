# M1 Summary: Parser And Generator Foundation

## What was built

- Added `lpc_wire::slot::native` with syntax events, event sources, a JSON
  syntax parser, and a TOML value adapter.
- Added a streaming `SlotReader` as the first manual construction target for
  future generated code.
- Added `SlotJsonWriter` as a slot-native facade over the existing JSON writer,
  including length-prefixed base64 tuple output.
- Added wire-level tests for JSON events, TOML adapter semantics, chunked
  strings, typed reads, and binary tuple decoding.
- Added mockup manual reader/writer round-trip tests that exercise the API
  shape codegen is expected to target.

## Decisions for future reference

#### Streaming Reader Correction

- **Decision:** Remove the intermediate syntax tree from the primary reader
  path.
- **Why:** The point of the architecture is to avoid whole-message buffering on
  embedded JSON paths.
- **Rejected alternatives:** Keep a reference/debug tree around without a
  concrete need.

#### JSON Parser Scope

- **Decision:** The first JSON parser is small and local to the syntax event
  foundation.
- **Why:** It proves the no-std event vocabulary without committing to the
  final parser implementation.
- **Rejected alternatives:** Use `serde_json::Value` as the event source;
  choose a third-party streaming parser before the reader shape is validated.

#### Writer Facade

- **Decision:** Reuse the existing `JsonWriter` and layer `SlotJsonWriter` on
  top.
- **Why:** The existing writer already solves comma handling and bounded byte
  output; M1 should avoid parallel JSON writer machinery.
- **Rejected alternatives:** Create another independent JSON output stream.
