# Phase 5: Update all callers (client, server, tests)

## Goal

Update all code that uses transports to work with the new API. Remove manual serialization/deserialization since transports now handle it internally.

## Tasks

1. Find all callers:
   - Search for `ClientTransport::send()` and `ClientTransport::receive()`
   - Search for `ServerTransport::send()` and `ServerTransport::receive()`
   - Search for `TransportMessage` or `Message as TransportMessage`
   - Check test files in `lp-client/tests/`

2. Update `lp-client/tests/fs_sync.rs`:
   - Remove manual serialization: `serde_json::to_vec(&request_msg)`
   - Remove manual deserialization: `serde_json::from_slice(&msg.payload)`
   - Update `ClientTransport::send()` calls to pass `ClientMessage` directly
   - Update `ClientTransport::receive()` calls to expect `ServerMessage` directly
   - Update `ServerTransport::send()` calls to pass `ServerMessage` directly
   - Update `ServerTransport::receive()` calls to expect `ClientMessage` directly
   - Update `process_messages()` helper function:
     - Remove serialization/deserialization logic
     - Pass messages directly to `server.tick()` and `client.tick()`
     - Handle `ServerMessage` and `ClientMessage` directly

3. Update `lp-client/tests/project_sync.rs` (if it uses transports):
   - Apply same changes as `fs_sync.rs`

4. Update any other test files that use transports:
   - Apply same pattern: remove manual serialization/deserialization

5. Update `lp-client/src/client.rs` (if it uses transports directly):
   - Remove any manual serialization/deserialization
   - Update to use new transport API

6. Update `lp-server` code (if it uses transports):
   - Remove manual serialization/deserialization
   - Update to use new transport API

7. Run tests:
   - Run `cargo test` in `lp-client` to verify tests pass
   - Run `cargo test` in `lp-server` to verify tests pass
   - Run `cargo test` in `lp-app` workspace to verify all tests pass

## Success Criteria

- [ ] All `ClientTransport::send()` calls pass `ClientMessage` directly
- [ ] All `ClientTransport::receive()` calls expect `ServerMessage` directly
- [ ] All `ServerTransport::send()` calls pass `ServerMessage` directly
- [ ] All `ServerTransport::receive()` calls expect `ClientMessage` directly
- [ ] No manual serialization/deserialization in callers
- [ ] All tests in `lp-app` pass
- [ ] Code compiles without warnings

## Style Notes

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Run `cargo +nightly fmt` on all changes before committing
- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready"
- Avoid emoticons
- Code is never done, never perfect, never fully ready, never fully complete
