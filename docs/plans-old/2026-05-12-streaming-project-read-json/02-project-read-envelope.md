# Phase 2: Streamed Project Read Envelope

## Scope Of Phase

Add a streamed writer for the `ProjectReadResponse` JSON envelope while preserving the existing JSON shape.

In scope:

- Add `ProjectReadResponseWriter` or similarly named helper under `lpc-wire` project-read messages.
- Write `revision`, `results`, and `probes` fields with the semantic JSON writer.
- Allow results/probes to be appended one at a time.
- Add serde bridge support for writing existing result/probe structs.
- Add equivalence tests against normal `serde_json` output/deserialization.

Out of scope:

- Engine integration.
- ESP transport integration.
- Resource payload special casing beyond normal serde bridge.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

Suggested files:

```text
lp-core/lpc-wire/src/messages/project_read/stream_response.rs
lp-core/lpc-wire/src/messages/project_read/mod.rs
```

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

The streamed JSON must deserialize to the existing type:

```rust
serde_json::from_str::<ProjectReadResponse>(&streamed_json)
```

Expected writer shape, names flexible:

```rust
let mut response = ProjectReadResponseWriter::begin(out, revision)?;
response.write_result(&ProjectReadResult::Nodes(nodes))?;
response.write_result(&ProjectReadResult::Resources(resources))?;
response.write_probe(&probe)?;
response.finish()?;
```

Important behavior:

- `results` is emitted as a JSON array, even if empty.
- `probes` is emitted as a JSON array, even if empty, unless preserving serde skip behavior is easy and tested. Prefer stable explicit arrays for streaming simplicity only if clients/tests accept it.
- The output must be valid JSON and semantically equivalent to the normal response.
- The serde bridge should not require the whole project response to be allocated.

Tests:

- Empty response equivalence/deserialization.
- Response with at least one `Shapes` result.
- Response with multiple results and probes.
- JSON generated in chunks still deserializes.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-wire project_read
cargo test -p lpc-wire
```
