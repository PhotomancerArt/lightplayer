# Phase 7: Server and demo sync wiring

## Scope of phase

Update server/client/demo sync request construction so the temporary dev UI asks
for the new details, summaries, and payloads it needs.

In scope:

- Update `lpa-client` / `lp-cli dev` sync request construction.
- Auto-request store summaries and resource payloads for watched node details in
  the current dev UI path.
- Keep all sync inside `GetChanges`.
- Add integration tests that mimic the demo request shape.

Out of scope:

- Fancy UI panes.
- Agent-readable CLI inspector.
- Transport compression/chunking.

## Code organization reminders

- Keep request-building helpers small and testable.
- Avoid adding server-held subscription state.
- Helpers near the bottom of files.
- No unrelated UI refactors.

## Sub-agent reminders

- Do not commit.
- Do not expand into UI polish.
- Do not suppress warnings or weaken tests.
- If the demo request path is unclear, stop and report the exact file/symbol.

## Implementation details

Read:

- `00-notes.md` Q28-Q29.
- `lp-app/lpa-client`
- `lp-cli`
- `lp-app/lpa-server/src/handlers.rs`
- `lp-core/lpc-view/src/project/project_view.rs`

Find where `WireProjectRequest::GetChanges` is constructed for the dev/demo
path. Extend it to include:

- node detail specifier as before;
- resource summary request for buffers and render products;
- payload request for resources referenced by watched details, or `All` for the
  simple local dev path if that is easiest and bounded.

Keep this pragmatic: the current UI is temporary and may auto-subscribe. Do not
build new UI panes.

Add or update tests in `lpa-server`/`lpa-client` as appropriate to ensure the
request shape reaches the server and a response can be applied by `ProjectView`.

## Validate

Run:

```bash
cargo test -p lpa-server
cargo test -p lpa-client
cargo check -p lp-cli
```
