# Phase 5: Engine Result Streaming

## Scope Of Phase

Reduce per-result allocations inside `Engine::write_project_read_json` so large
top-level arrays stream directly from engine state where practical.

In scope:

- Stream shape registry result without unnecessary clones where possible.
- Stream node read result arrays/slot roots with bounded intermediate data.
- Stream resource summaries/payload arrays through existing resource writers.
- Keep semantic equivalence with `Engine::read_project`.

Out of scope:

- Registry diff protocol.
- Replacing all internal `Vec` use in the engine.
- Reworking slot snapshot data representation.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep normal semantic builders for tests and desktop convenience if they remain
  useful.
- Put helpers lower in files when that improves readability.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/engine/project_read_stream.rs`
- `lp-core/lpc-engine/src/engine/project_read_nodes.rs`
- `lp-core/lpc-engine/src/engine/project_read_shapes.rs`
- `lp-core/lpc-engine/src/engine/project_read_resources.rs`
- `lp-core/lpc-wire/src/messages/project_read/stream_response.rs`
- `lp-core/lpc-wire/src/slot/access_sync.rs`

Expected changes:

- Audit `Engine::write_project_read_json` for per-query allocations.
- For any large query result, prefer direct writer helpers over constructing
  `ProjectReadResult`.
- If a direct writer would require too much churn, leave a measured TODO and
  keep the result-builder path for that query.
- Add tests comparing streamed output to `Engine::read_project` for:
  - default debug read,
  - resource payload read,
  - node detail with slots,
  - shape detail.

Constraints:

- Do not change the JSON shape.
- Use serde-deserialization of streamed bytes as the correctness check.
- Keep the old semantic path until replacement has strong test coverage.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-engine project_read
cargo test -p lpc-wire
```

