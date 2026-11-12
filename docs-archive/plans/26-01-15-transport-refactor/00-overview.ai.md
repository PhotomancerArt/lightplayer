# Overview: Transport Refactor

## Goal

Refactor the transport layer to handle serialization internally and work directly with `ClientMessage` and `ServerMessage` types from `lp-model`, rather than requiring callers to handle serialization of a `Message` wrapper type.

Additionally, move `MemoryTransport` from `lp-client` to `lp-shared` and refactor it to be explicitly single-threaded (`LocalMemoryTransport`).

## Scope

- **Transport Traits**: Update `ClientTransport` and `ServerTransport` to use `ClientMessage` and `ServerMessage` directly
- **Dependencies**: Add `lp-model` dependency to `lp-shared`
- **Memory Transport**: Refactor to `LocalMemoryTransport` (single-threaded, no std requirement) and move to `lp-shared`
- **Callers**: Update all code that uses transports (client, server, tests)
- **Tests**: Ensure all tests in `lp-app` pass

## Out of Scope

- Other transport implementations (stdio, websocket, etc.) - will be added later
- Multi-threaded memory transport - not needed for current use cases

## Success Criteria

- Transport traits work with `ClientMessage` and `ServerMessage` directly
- `LocalMemoryTransport` is in `lp-shared` and is single-threaded (no std requirement)
- All callers updated to use new transport API
- All tests in `lp-app` pass
- Code compiles without warnings
- Code is formatted with `cargo +nightly fmt`

## Phases

1. Add lp-model dependency to lp-shared
2. Update transport traits to use ClientMessage/ServerMessage
3. Refactor MemoryTransport to LocalMemoryTransport (single-threaded)
4. Move LocalMemoryTransport to lp-shared
5. Update all callers (client, server, tests)
6. Cleanup and finalization
