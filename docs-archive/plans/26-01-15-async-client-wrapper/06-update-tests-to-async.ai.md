# Phase 6: Update Tests to Async

## Description

Convert integration tests to use async client and async server running on separate thread.

## Tasks

1. Update `lp-app/apps/lp-cli/tests/integration.rs`:
   - Change `#[test]` to `#[tokio::test]` for relevant tests
   - Update tests to use `AsyncLpClient`
   - Spawn server on separate thread in tests
   - Use `await` for async operations

2. Ensure test structure:
   - Server spawned on separate thread
   - Client uses `AsyncLpClient` on test thread
   - Proper cleanup/joining of server thread

3. Verify all tests pass:
   - `test_end_to_end_project_push` - Update to async
   - Any other tests that use client/server communication

## Success Criteria

- All tests compile with async
- All tests pass
- Server runs on separate thread in tests
- Client uses `AsyncLpClient` in tests
- No test failures

## Implementation Notes

- Use `#[tokio::test]` for async tests
- Spawn server thread in test setup
- Use `tokio::time::sleep()` for delays if needed
- Ensure server thread is properly joined/cleaned up
