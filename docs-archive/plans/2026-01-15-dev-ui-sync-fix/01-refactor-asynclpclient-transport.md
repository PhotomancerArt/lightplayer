# Phase 1: Refactor AsyncLpClient for shared transport

## Goal

Refactor `AsyncLpClient` to accept `Arc<Mutex<dyn ClientTransport + Send>>` instead of `Box<dyn ClientTransport + Send>` to enable transport sharing between project loader and UI.

## Tasks

1. Update `lp-app/apps/lp-cli/src/commands/dev/async_client.rs`:
   - Change `transport` field type from `Box<dyn ClientTransport + Send>` to `Arc<Mutex<dyn ClientTransport + Send>>`
   - Update `new()` method signature to accept `Arc<Mutex<...>>`
   - Update all methods that use `self.transport` to lock it first:
     - Use `blocking_lock()` for sync operations
     - Use `lock().await` for async operations
   - Ensure all transport operations are properly locked

2. Update `lp-app/apps/lp-cli/src/commands/dev/handler.rs`:
   - Wrap transport in `Arc<Mutex<...>>` before creating `AsyncLpClient`
   - Pass shared transport to both project loader and UI
   - Ensure transport is created once and shared

3. Update any other call sites of `AsyncLpClient::new()`:
   - Check for other usages in the codebase
   - Update to wrap transport in `Arc<Mutex<...>>`

## Success Criteria

- `AsyncLpClient` accepts shared transport
- All transport operations are properly locked
- Transport can be shared between multiple `AsyncLpClient` instances
- Code compiles without errors
- No warnings related to transport usage

## Implementation Notes

- Use `std::sync::Mutex` for blocking locks (in sync context)
- Use `tokio::sync::Mutex` for async locks (in async context)
- Actually, since we're in async context, use `tokio::sync::Mutex` throughout
- Ensure transport trait is `Send` for cross-thread use
- Lock transport before each operation, release immediately after
