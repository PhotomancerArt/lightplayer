# Phase 4: Implement Server Handlers for Project Requests

## Goal

Implement handlers for all project management requests in `lp-server/src/handlers.rs`.

## Tasks

1. Update `handle_client_message()` in `lp-server/src/handlers.rs`:
   - Route project requests to appropriate handlers
   - Pass `project_manager` and `base_fs` to handlers

2. Implement `handle_load_project()`:
   - Extract project name from path using `ProjectManager::extract_project_name_from_path()`
   - Call `ProjectManager::load_project()`
   - Return `ServerResponse::LoadProject { handle }`

3. Implement `handle_unload_project()`:
   - Call `ProjectManager::unload_project(handle)`
   - Return `ServerResponse::UnloadProject`

4. Implement `handle_project_request()`:
   - Get project by handle
   - Match `ProjectRequest` variant:
     - `GetChanges { since_frame, detail_specifier }`:
       - Call `runtime.get_changes(since_frame, detail_specifier)`
       - Convert `ProjectResponse` to `SerializableProjectResponse`
       - Return `ServerResponse::ProjectRequest { response }`

5. Implement `handle_list_available_projects()`:
   - Call `ProjectManager::list_available_projects(base_fs)`
   - Convert names to `Vec<AvailableProject>` (with full paths)
   - Return `ServerResponse::ListAvailableProjects { projects }`

6. Implement `handle_list_loaded_projects()`:
   - Call `ProjectManager::list_loaded_projects()`
   - Return `ServerResponse::ListLoadedProjects { projects }`

## Success Criteria

- [ ] All project request variants are handled
- [ ] LoadProject extracts name correctly and returns handle
- [ ] UnloadProject removes project correctly
- [ ] ProjectRequest::GetChanges works and converts to serializable
- [ ] ListAvailableProjects returns correct list
- [ ] ListLoadedProjects returns correct list with handles
- [ ] All tests pass
- [ ] Code compiles without warnings
