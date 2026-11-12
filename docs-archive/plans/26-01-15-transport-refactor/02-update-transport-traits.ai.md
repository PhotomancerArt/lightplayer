# Phase 2: Update transport traits to use ClientMessage/ServerMessage

## Goal

Update `ClientTransport` and `ServerTransport` traits to work directly with `ClientMessage` and `ServerMessage` types instead of the `Message` wrapper type. Remove the `Message` wrapper from the transport module.

## Tasks

1. Update `lp-shared/src/transport/mod.rs`:
   - Remove the `Message` struct definition
   - Remove `Message::new()` implementation
   - Update module documentation to reflect new design
   - Export `ClientTransport` and `ServerTransport` (already done)

2. Update `lp-shared/src/transport/client.rs`:
   - Change `send()` parameter from `Message` to `ClientMessage`
   - Change `receive()` return type from `Option<Message>` to `Option<ServerMessage>`
   - Update trait documentation to note that transport handles serialization
   - Update example code in documentation
   - Add import: `use lp_model::ClientMessage;`

3. Update `lp-shared/src/transport/server.rs`:
   - Change `send()` parameter from `Message` to `ServerMessage`
   - Change `receive()` return type from `Option<Message>` to `Option<ClientMessage>`
   - Update trait documentation to note that transport handles serialization
   - Update example code in documentation
   - Add import: `use lp_model::ServerMessage;`

4. Verify compilation:
   - Run `cargo check` in `lp-shared` to ensure traits compile
   - Note that callers will fail to compile (expected, will be fixed in phase 5)

## Success Criteria

- [ ] `Message` wrapper removed from transport module
- [ ] `ClientTransport::send()` takes `ClientMessage`
- [ ] `ClientTransport::receive()` returns `Option<ServerMessage>`
- [ ] `ServerTransport::send()` takes `ServerMessage`
- [ ] `ServerTransport::receive()` returns `Option<ClientMessage>`
- [ ] Documentation updated to reflect new API
- [ ] `lp-shared` compiles (callers will fail, that's expected)
- [ ] Code compiles without warnings

## Style Notes

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Run `cargo +nightly fmt` on all changes before committing
- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready"
- Avoid emoticons
- Code is never done, never perfect, never fully ready, never fully complete
