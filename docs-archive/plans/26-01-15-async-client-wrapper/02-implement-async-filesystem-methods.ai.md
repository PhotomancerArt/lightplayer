# Phase 2: Implement Async Filesystem Methods

## Description

Implement async versions of filesystem operations (`fs_read` and `fs_write`) on `AsyncLpClient` that use the `wait_for_response` helper.

## Tasks

1. Implement `fs_read(&self, path: String) -> Result<Vec<u8>>`:
   - Create request via `LpClient::fs_read()`
   - Send message via transport
   - Call `wait_for_response()` to get response
   - Extract response using `LpClient::extract_read_response()`
   - Return file contents

2. Implement `fs_write(&self, path: String, data: Vec<u8>) -> Result<()>`:
   - Create request via `LpClient::fs_write()`
   - Send message via transport
   - Call `wait_for_response()` to get response
   - Extract response using `LpClient::extract_write_response()`
   - Return success

## Success Criteria

- `fs_read()` method compiles and returns file contents
- `fs_write()` method compiles and returns success
- Both methods handle timeouts correctly
- Both methods handle transport and client errors correctly
- Code compiles without errors

## Implementation Notes

- Methods should be `async fn`
- Use `wait_for_response()` helper for consistency
- Wrap `ClientError` in `anyhow::Error` with context
- Ensure proper error messages for debugging
