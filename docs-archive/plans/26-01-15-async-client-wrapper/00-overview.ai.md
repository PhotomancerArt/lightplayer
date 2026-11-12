# Plan: Async Client Wrapper

## Overview

Create an async wrapper around `LpClient` to enable proper async/await coordination with an async server running in a tokio runtime. The wrapper provides async request/response methods with timeout handling, properly yields to the runtime so the server can process requests, and maintains backward compatibility by keeping `LpClient` synchronous for WebAssembly compatibility.

This addresses the current issue where synchronous polling blocks the async runtime, preventing the server from processing requests and sending responses in a timely manner.

## Phases

1. **Create AsyncLpClient structure** - Basic wrapper struct with LpClient and transport, helper method for waiting for responses
2. **Implement async filesystem methods** - `fs_read()`, `fs_write()` async methods
3. **Implement async project methods** - `project_load()` async method
4. **Update push.rs to async** - Replace sync `push_project` and `load_project` with async versions using `AsyncLpClient`
5. **Update handler.rs for separate threads** - Spawn server on separate thread, use `AsyncLpClient` on main thread
6. **Update tests to async** - Convert integration tests to use async client and async server thread
7. **Cleanup and finalization** - Fix warnings, ensure all tests pass, code formatting

## Success Criteria

- `AsyncLpClient` provides async versions of `LpClient` methods (`fs_read`, `fs_write`, `project_load`)
- Server runs on separate thread, client on main thread
- Requests complete successfully with proper timeout handling (5 seconds)
- No blocking of async runtime
- `lp-cli dev` works correctly with in-memory server
- All tests updated to async and pass
- Code compiles without warnings
