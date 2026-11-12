# Phase 5: Server And Client Integration

## Scope Of Phase

Wire the new project read request through the app server/client path.

In scope:

- Update `lpa-server` project handler to answer `WireProjectRequest::Read`.
- Update `lpa-client` public helper(s) to send `ProjectReadRequest`.
- Add or update local/basic smoke tests if the current harness makes this
  reasonable.
- Keep probe execution as unsupported/empty unless implemented cleanly in
  phase 3.

Out of scope:

- New UI.
- Mutation.
- Transport changes.

## Code Organization Reminders

- Keep handler code thin; project read construction should live on the engine
  side where possible.
- Preserve existing filesystem/load/unload/list request behavior.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-app/lpa-server/src/handlers.rs`
- `lp-app/lpa-server/src/server.rs`
- `lp-app/lpa-client/src/client.rs`
- `lp-core/lpc-wire/src/server/api.rs`

The request/response envelope remains:

```rust
ClientRequest::ProjectRequest { handle, request }
ServerMsgBody::ProjectRequest { response }
```

## Validate

```bash
cargo fmt --check
cargo check -p lpa-server
cargo check -p lpa-client
```
