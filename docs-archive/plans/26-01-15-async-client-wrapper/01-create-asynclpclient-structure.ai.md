# Phase 1: Create AsyncLpClient Structure

## Description

Create the basic `AsyncLpClient` wrapper structure with `LpClient` and transport, and implement the core `wait_for_response` helper method that polls for responses with proper timeout handling.

## Tasks

1. Create `lp-app/apps/lp-cli/src/commands/dev/async_client.rs` module
2. Implement `AsyncLpClient` struct with:
   - `client: LpClient` field
   - `transport: Box<dyn ClientTransport + Send>` field
3. Implement `new()` constructor
4. Implement `wait_for_response()` helper method:
   - Takes `request_id` and polls for response
   - Processes messages via `LpClient::tick()`
   - Yields with `tokio::task::yield_now()` and small sleep
   - Uses `tokio::time::timeout()` with 5 second default
   - Returns `Result<ServerResponse>`

## Success Criteria

- `AsyncLpClient` struct compiles
- `new()` method creates instance successfully
- `wait_for_response()` helper compiles and handles timeout correctly
- Code compiles without errors
- No warnings (except unused code that will be used in later phases)

## Implementation Notes

- Use `anyhow::Result` for error handling
- Timeout is hardcoded to `Duration::from_secs(5)`
- Polling loop should yield frequently to allow server to process
- Use `tokio::time::sleep(Duration::from_millis(1))` for small delays between polls
