# Phase 5: Implement Client Methods for Project Operations

## Goal

Add project management methods to `LpClient` to match the server handlers.

## Tasks

1. Update `lp-client/src/client.rs`:
   - Add project management methods:
     - `project_load(path: String) -> (Message, u64)`
     - `project_unload(handle: ProjectHandle) -> (Message, u64)`
     - `project_get_changes(handle: ProjectHandle, since_frame: FrameId, detail_specifier: ApiNodeSpecifier) -> (Message, u64)`
     - `project_list_available() -> (Message, u64)`
     - `project_list_loaded() -> (Message, u64)`
   - Add response extractor methods:
     - `extract_load_project_response(request_id, response) -> Result<ProjectHandle, ClientError>`
     - `extract_unload_project_response(request_id, response) -> Result<(), ClientError>`
     - `extract_get_changes_response(request_id, response) -> Result<SerializableProjectResponse, ClientError>`
     - `extract_list_available_projects_response(request_id, response) -> Result<Vec<AvailableProject>, ClientError>`
     - `extract_list_loaded_projects_response(request_id, response) -> Result<Vec<LoadedProject>, ClientError>`

2. Add necessary imports:
   - `ProjectHandle`, `ProjectRequest`, `ApiNodeSpecifier`, `FrameId`
   - `SerializableProjectResponse`, `AvailableProject`, `LoadedProject`

## Success Criteria

- [ ] All project management methods exist
- [ ] All extractor methods exist
- [ ] Methods return correct request messages and IDs
- [ ] Extractors parse responses correctly
- [ ] Code compiles without warnings
