# Phase 3: Update ProjectManager to Use Handle-Based Mapping

## Goal

Refactor `ProjectManager` to use `ProjectHandle` as the primary identifier instead of project name, while maintaining name->handle mapping for convenience.

## Tasks

1. Update `lp-server/src/project_manager.rs`:
   - Change `projects: HashMap<String, Project>` to `projects: HashMap<ProjectHandle, Project>`
   - Add `name_to_handle: HashMap<String, ProjectHandle>` for reverse lookup
   - Add `next_handle_id: u32` counter (starts at 1)
   - Update `load_project()` signature:
     - Change parameter from `name: String` to take `base_fs: &mut dyn LpFs` and extract name from path
     - Generate new `ProjectHandle` (increment `next_handle_id`)
     - Extract project name from path (last component, strip `projects_base_dir` prefix if present)
     - Create project-scoped filesystem using `base_fs.chroot()`
     - Auto-initialize project runtime (load_nodes, init_nodes, ensure_all_nodes_initialized)
     - Store handle -> project mapping
     - Store name -> handle mapping
     - Return `ProjectHandle`
   - Update `unload_project()` to take `ProjectHandle` instead of `&str`
   - Update `get_project()` and `get_project_mut()` to take `ProjectHandle`
   - Update `list_loaded_projects()` to return `Vec<LoadedProject>` with handles
   - Add `get_handle_by_name()` helper method

2. Update `lp-server/src/server.rs`:
   - Update `LpServer::tick()` to pass `&mut *self.base_fs` to handlers

3. Add tests:
   - Test handle generation (sequential, starting at 1)
   - Test name extraction from path
   - Test project scoping with chroot
   - Test auto-initialization

## Success Criteria

- [ ] `ProjectManager` uses handle-based mapping
- [ ] Handles are generated sequentially starting from 1
- [ ] Project name is extracted correctly from path
- [ ] Project filesystem is scoped using `chroot()`
- [ ] Projects are auto-initialized on load
- [ ] All existing tests pass (may need updates)
- [ ] Code compiles without warnings
