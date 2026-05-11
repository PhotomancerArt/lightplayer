# Phase 2: Probe Vocabulary And Docs

## Scope Of Phase

Add request-scoped probe vocabulary and the first domain documentation.

In scope:

- Add `ProjectProbeRequest` and `ProjectProbeResult`.
- Add `RenderProductProbeRequest` / `RenderProductProbeResult`.
- Add `ExplainSlotProbeRequest` / `ExplainSlotProbeResult`.
- Include commented future enum variants for shader/control/fs/io probes.
- Add `docs/lp-core/probes.md` and link it from `docs/lp-core/overview.md`.
- Add wire JSON roundtrip tests.

Out of scope:

- Full engine-side probe execution.
- Shader debug internals.
- Streaming probe payloads.

## Code Organization Reminders

- Put probe files under `lp-core/lpc-wire/src/messages/project_read/probe/`.
- Commented future enum variants are intentional here; keep them short and
  clearly marked as future.
- Keep docs concise and developer-facing.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Probe semantics:

- Probes are stateless, request-scoped diagnostics.
- Probes are not subscriptions.
- Probes are not authored graph state.
- Probes do not imply persistent resources.

`ExplainSlot` should model the future ability to re-resolve a slot with trace
logging enabled. The first implementation can serialize the request/response
shape without requiring engine support.

`RenderProduct` should model the future ability to ask a visual product to
render into bytes for inspection. It must remain probe data, not resource sync
data.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-wire
```
