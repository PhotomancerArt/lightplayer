# Plan: Dev Command Architecture Refactor

## Overview

Refactor the `lp-cli dev` command architecture to create clean separation of concerns, improve transport abstraction, and establish clear patterns for async client-server communication. This plan introduces `AsyncClientTransport` for request/response correlation, `LocalServerTransport` for encapsulating in-memory server lifecycle, and separates file watching, UI updates, and project operations into dedicated modules.

## Phases

0. **Initial cleanup** - Remove old code, create stubs, get codebase compiling with tests passing
1. **Extend ClientTransport trait** - Add `receive_all()` and `close()` methods, update implementations
2. **Create LocalServerTransport** - Encapsulate server thread lifecycle, provide client transport
3. **Create client_connect() function** - Factory function for creating transports from HostSpecifier
4. **Create AsyncClientTransport** - Async wrapper with background polling and request/response correlation
5. **Create AsyncLpClient** - Thin wrapper around AsyncClientTransport using LpClient for request IDs
6. **Create push_project_async and pull_project_async** - Async versions of project sync operations
7. **Create fs_loop** - File watching and syncing loop
8. **Refactor handler.rs** - Simplify to use new architecture
9. **Add tests** - Unit tests for each new component
10. **Cleanup and finalization** - Fix warnings, ensure all tests pass, code formatting

## Success Criteria

- Clean separation: each concern in its own file
- Transport abstraction: `HostSpecifier` → `ClientTransport` via `client_connect()`
- Local server encapsulated: `LocalServerTransport` manages server thread
- Async client works: `AsyncClientTransport` handles request/response correlation
- Multiple consumers: `Arc<AsyncClientTransport>` shared without mutex
- Explicit lifecycle: `close()` methods work correctly
- Backwards compatible: existing `receive()` still works
- Testable: each component can be tested in isolation
- Code compiles and tests pass
