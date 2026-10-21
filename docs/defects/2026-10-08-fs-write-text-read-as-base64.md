---
status: fixed
found: 2026-10-08      # how: review of #1057 (wire 41), which fixed the same ambiguity for edit bodies only
fixed: 2026-10-20, wire 44 (queue ticket lp2025/_auto/2026-10-08-fs-write-text-read-as-base64)
area: lpc-wire `serde_base64::serialize_smart` / `deserialize_smart` × `FsRequest::Write`, `WriteChunk`, `FsResponse::Read`, `FileChunk`
class: state-conflation
related:
  - docs/defects/2026-10-08-shader-edits-over-wi-fi-are-refused-board-memory-busy.md
---
# A text file whose contents are valid base64 is written to the board as binary

**Symptom** — a text file whose whole contents spell base64 of bytes that
are not UTF-8 (`abcd`, `1234`) was stored on the board as those bytes: three
binary bytes for `abcd`. The same file read back came out the same way.

**Root cause** — the file messages carried text and binary in one JSON
string slot. The writer sent UTF-8 as itself and anything else as base64;
the reader guessed which it had been handed from the string alone, and a
string that is valid base64 of non-UTF-8 bytes read as binary. One slot,
two facts (text, or base64 of binary), told apart by a guess.

#1057 (wire 41) fixed the same ambiguity for edit bodies by putting binary
in an object (`lpc_model::body_bytes`); the fs messages kept the guess.

**Fix** — wire 44. `FsRequest::Write`, `WriteChunk`, `FsResponse::Read` and
a `ChangesSince` page's `FileChunk` use `lpc_model::body_bytes`: text is a
string and is always the text, binary is `{"base64":"…"}`. The guessing
reader (`serialize_smart`, `deserialize_smart` and their `Option` forms) is
deleted, not tolerated beside the new one. Text files are byte-identical on
the wire to before; only binary bodies changed shape.
`lpc-wire/src/serde_base64.rs` round-trips `abcd`, `1234`, binary and empty
bodies through every one of the four messages.
