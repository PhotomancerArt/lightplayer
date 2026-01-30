# Phase 7: Implement Client Sync Logic

## Goal

Implement the sync logic for `update_project()` and message handling.

## Tasks

1. Implement `update_project()` in `LpClient`:
   - Get or create `RemoteProject` for handle
   - Generate request ID
   - Build `ProjectRequest::GetChanges` with `since_frame = last_frame_id`
   - Wrap in `Message::Request { id, request }`
   - Send via transport
   - Wait for `Message::Response { id, response }` with matching ID
   - Handle `ProjectResponse::GetChanges`:
     - Update `last_frame_id` to `current_frame`
     - Update/insert nodes from `node_detail`
     - Remove nodes not in `node_handles` list
   - Return reference to updated `RemoteProject`

2. Implement `process_messages()` in `LpClient`:
   - Poll `transport.receive_message()`
   - Handle `Message::Response` - match to pending requests by ID
   - Handle `Message::Log` - can be ignored for now or logged
   - Handle other message types as needed

3. Add pending request tracking:
   - Add `pending_requests: HashMap<u64, RequestState>` field to `LpClient`
   - Store request state (type, callback, etc.)
   - Match responses to pending requests

4. Implement `load_project()`:
   - Send `ServerRequest::LoadProject { path }`
   - Wait for `ServerResponse::ProjectLoaded { handle }`
   - Create `RemoteProject` with initial state
   - Insert into `projects` map

5. Implement `unload_project()`:
   - Send `ServerRequest::UnloadProject { handle }`
   - Wait for `ServerResponse::ProjectUnloaded`
   - Remove from `projects` map

6. Implement `list_projects()`:
   - Send `ServerRequest::ListProjects` (if exists, or implement)
   - Wait for response with project list
   - Return `Vec<ProjectInfo>`

7. Implement `create_project()`:
   - Send `ServerRequest::CreateProject { path }` (if exists, or implement)
   - Wait for response
   - Return result

## Success Criteria

- `update_project()` successfully syncs project state
- Request/response correlation works correctly
- Message handling processes responses correctly
- `load_project()` and `unload_project()` work
- `RemoteProject` state updates correctly from sync responses
- All code compiles without warnings
