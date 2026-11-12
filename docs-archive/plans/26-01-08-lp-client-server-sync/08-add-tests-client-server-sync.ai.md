# Phase 8: Add Tests for Client-Server Sync

## Goal

Create test infrastructure and tests to verify client-server sync works correctly.

## Tasks

1. Create in-memory transport for testing:
   - Create `lp-client/src/transport/memory.rs` or similar
   - Implement `ClientTransport` trait
   - Use channels or VecDeque to simulate message passing
   - Allow test code to inject messages and read sent messages

2. Create test helper utilities:
   - Helper to create test `LpClient` with in-memory transport
   - Helper to create test `ProjectRuntime` with sample nodes
   - Helper to verify sync state

3. Test handle assignment:
   - Create project with multiple nodes
   - Verify handles are assigned sequentially
   - Verify handles are unique

4. Test frame tracking:
   - Create project and run updates
   - Verify frame ID increments
   - Verify node frame tracking updates correctly

5. Test GetChanges:
   - Create project with nodes
   - Make changes to nodes
   - Request changes since initial frame
   - Verify correct nodes are returned
   - Verify node details are correct

6. Test client sync:
   - Create client and server (in-memory)
   - Load project on server
   - Load project on client
   - Sync project
   - Verify client state matches server state

7. Test incremental sync:
   - Sync project initially
   - Make changes to nodes
   - Sync again with `since_frame = last_frame_id`
   - Verify only changed nodes are returned

8. Test full resync:
   - Sync project with `since_frame = 0`
   - Verify all nodes are returned

9. Test node deletion:
   - Create project with nodes
   - Sync client
   - Delete node on server
   - Sync again
   - Verify deleted node is removed from client state

10. Test request/response correlation:
    - Send multiple requests
    - Verify responses match requests by ID
    - Test with interleaved log messages

## Success Criteria

- In-memory transport works correctly
- All tests pass
- Tests verify handle assignment
- Tests verify frame tracking
- Tests verify sync correctness
- Tests verify incremental vs full sync
- All code compiles without warnings
