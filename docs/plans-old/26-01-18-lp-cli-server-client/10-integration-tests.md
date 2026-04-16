# Phase 10: Add Integration Tests

## Goal

Add integration tests using memory filesystem and in-memory transport to verify end-to-end functionality.

## Tasks

1. Create `lp-app/apps/lp-cli/tests/` directory

2. Create `lp-app/apps/lp-cli/tests/integration.rs`:
   - Test server startup with memory filesystem:
     - Create server with `LpFsMemory`
     - Verify server accepts connections
   - Test client connection with in-memory transport:
     - Create `LocalTransport` pair
     - Create client and server
     - Verify communication
   - Test end-to-end project push:
     - Create project on client side (memory fs)
     - Push to server (memory fs, in-memory transport)
     - Verify project exists on server
     - Verify project can be loaded
   - Test create command:
     - Create project with defaults
     - Verify structure
     - Verify project.json format

3. Add helper functions for tests:
   - `create_test_server() -> LpServer` - Server with memory fs
   - `create_test_client_transport() -> (ClientTransport, ServerTransport)` - In-memory transport pair
   - `create_test_project(fs: &mut dyn LpFs, name: &str) -> String` - Create test project, return uid

4. Ensure all tests use memory filesystem and in-memory transport where possible

## Success Criteria

- Integration tests exist for server startup
- Integration tests exist for client connection
- Integration tests exist for end-to-end project push
- All tests use memory filesystem (no real filesystem)
- All tests use in-memory transport (no real network)
- Tests pass
- Code compiles without warnings
