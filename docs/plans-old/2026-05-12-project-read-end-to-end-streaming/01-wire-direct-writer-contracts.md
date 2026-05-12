# Phase 1: Wire Direct Writer Contracts

## Scope Of Phase

Add or consolidate canonical direct JSON writers in `lpc-wire` for large
response payloads.

In scope:

- Server envelope writer for `ProjectRequest { response: ProjectReadResponse }`.
- Direct writer tests for project-read responses.
- Direct writer support/tests for filesystem read responses or a documented
  writer surface ready for Phase 4.
- Tests that deserialize direct-written JSON using serde-derived wire types.
- Keep existing serde structs as the canonical semantic shape.

Out of scope:

- Changing `ServerTransport`.
- Changing `lpa-server` routing.
- ESP serial integration.
- Deep streaming of every individual engine query result.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep direct writer modules close to the wire types they serialize.
- Put helpers lower in files when that improves readability.
- Mark temporary code with clear `TODO`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-wire/src/json/json_write.rs`
- `lp-core/lpc-wire/src/json/json_writer.rs`
- `lp-core/lpc-wire/src/messages/project_read/stream_response.rs`
- `lp-core/lpc-wire/src/messages/mod.rs`
- `lp-core/lpc-wire/src/lib.rs`
- `lp-core/lpc-wire/src/server/fs_api.rs`
- `lp-core/lpc-wire/src/server/mod.rs`

Expected changes:

- Add a canonical writer for the full server envelope around a project-read
  response. It should write the same JSON shape as `WireServerMessage` with
  `ServerMsgBody::ProjectRequest`.
- Reuse `ProjectReadResponseWriter` for the inner response.
- Add a direct writer for filesystem read responses or a small writer API that
  Phase 4 can use without duplicating JSON in firmware.
- Avoid heap allocation in direct writer logic beyond caller-provided sinks.
- Tests must:
  - write a project-read server message through the direct writer,
  - deserialize it as `WireServerMessage`,
  - compare the decoded value to the expected semantic value,
  - write a filesystem read response with representative data,
  - deserialize it with serde and compare.

Constraints:

- Do not change the client-visible JSON shape.
- Do not rely only on string comparison; serde round-trip is the drift guard.
- Resource payload byte writers should continue using streaming base64 helpers.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-wire
cargo check -p lpc-wire --features schema-gen
```

