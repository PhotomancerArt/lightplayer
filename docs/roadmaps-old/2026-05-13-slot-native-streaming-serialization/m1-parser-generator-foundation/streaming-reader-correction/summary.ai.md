# Streaming Reader Correction Summary

## What was built

- Replaced the tree-backed `SlotReader` with a streaming reader over
  `SyntaxEventSource`.
- Removed `SyntaxNode` completely from the code path.
- Changed `JsonSyntaxSource` so `next_event()` parses on demand with a small
  parser stack instead of prebuilding `Vec<SyntaxEvent>`.
- Added source spans to syntax events and errors.
- Added object scanning, array scanning, `expect_discriminator`, `skip_value`,
  scalar reads, and length-checked base64 tuple reads.
- Updated mockup manual reader tests to use scanner-style generated-code shape
  directly against JSON/TOML sources.
- Added discriminator error coverage that includes actual and expected values.

## Decisions for future reference

#### No Syntax Tree

- **Decision:** Do not keep a generic syntax tree helper.
- **Why:** The primary design goal is to avoid whole-message buffering on
  embedded JSON paths, and an unused debug tree would invite accidental use.
- **Rejected alternatives:** Keep `SyntaxNode` as a reference/debug adapter.

#### Discriminator Helper

- **Decision:** Use `expect_discriminator("kind")` for ordered first-field enum
  discriminators.
- **Why:** This names the operation honestly and gives room for targeted error
  messages.
- **Rejected alternatives:** A generic `prop("kind")` API that looks like field
  lookup.

#### Record Scanner

- **Decision:** Normal records use `object().next_prop()?` plus generated
  `match` code.
- **Why:** Streams cannot do unordered random field lookup without buffering.
- **Rejected alternatives:** Order-dependent record fields; random-access
  reader API.
