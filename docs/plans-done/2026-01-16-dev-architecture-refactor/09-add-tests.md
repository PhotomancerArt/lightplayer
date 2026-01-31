# Phase 9: Add Tests

## Description

Add comprehensive unit tests for each new component. Each component should be testable in isolation using mocks or test doubles.

## Tasks

1. Add tests for `client_connect.rs`:
   - Test with `HostSpecifier::Local` - creates local server transport
   - Test with `HostSpecifier::WebSocket` - creates websocket transport
   - Test with `HostSpecifier::Serial` - returns error
   - Test error handling for invalid websocket URLs

2. Add tests for `local_server.rs`:
   - Test server spawns correctly
   - Test `client_transport()` returns working transport
   - Test sending/receiving messages through local server
   - Test `close()` stops server and waits for thread
   - Test `Drop` calls `close()` if not already called

3. Add tests for `async_transport.rs`:
   - Test with mock `ClientTransport` (use `LocalTransport` from lp-shared)
   - Test `send_request()` waits for response
   - Test request/response correlation (multiple concurrent requests)
   - Test error channel receives transport errors
   - Test `close()` stops polling task
   - Test `Drop` calls `close()` if not already called

4. Add tests for `async_client.rs`:
   - Test with mock `AsyncClientTransport` (create test implementation)
   - Test each async method (`fs_read`, `fs_write`, `project_load`)
   - Test error propagation from transport
   - Test request ID extraction from `LpClient`

5. Add tests for `push_project.rs`:
   - Test with mock `AsyncLpClient` and `LpFs`
   - Test pushing multiple files
   - Test error handling (file read errors, transport errors)

6. Add tests for `pull_project.rs`:
   - Test with mock `AsyncLpClient` and `LpFs`
   - Test pulling multiple files
   - Test error handling (transport errors, file write errors)

7. Add tests for `fs_loop.rs`:
   - Test file watching and change collection
   - Test debouncing logic
   - Test syncing changes via `sync_changes()`
   - Test error channel monitoring
   - Test graceful shutdown

8. Update integration tests if needed:
   - Update existing integration tests to use new architecture
   - Ensure they still pass

## Success Criteria

- Tests exist for each new component
- All tests pass
- Components are testable in isolation
- Mock/test doubles are used appropriately
- Integration tests updated and passing
- Code compiles without errors

## Implementation Notes

- Use `LocalTransport` from `lp-shared` for testing `AsyncClientTransport`
- Create test implementations of `AsyncClientTransport` for testing `AsyncLpClient`
- Use `LpFsMemory` for testing filesystem operations
- Consider using `tokio::test` for async tests
- Use `tokio::time::timeout()` to prevent hanging tests
- Mock channels can be created with `mpsc::unbounded_channel()` for testing
