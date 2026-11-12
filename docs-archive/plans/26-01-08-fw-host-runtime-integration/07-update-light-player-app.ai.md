# Phase 7: Update LightPlayerApp to use LpApp

## Goal

Refactor `LightPlayerApp` to use `LpApp` instead of managing `ProjectRuntime` directly.

## Tasks

1. Update `fw-host/src/app.rs`:
   - Remove `project: Option<ProjectConfig>` and `runtime: Option<ProjectRuntime>`
   - Add `lp_app: LpApp`
   - Update constructor to create `LpApp` with `Platform`:
     - Create `HostOutputProvider`
     - Create `Platform { fs, output }`
     - Create `LpApp::new(platform)`
   - Update `init()` to call `lp_app.load_project("project.json")`
   - Update `create_default_project()` to save project.json and load it
   - Update `load_project()` to call `lp_app.load_project()`
   - Update `handle_command()` to convert `Command` to `MsgIn`, call `lp_app.tick()`, handle `MsgOut`
   - Update `process_messages()` to use `lp_app.tick()` for message processing
   - Keep UI-related methods (project(), runtime() for debug panel)

2. Update method signatures and error handling

## Success Criteria

- `LightPlayerApp` compiles
- Uses `LpApp` for all runtime operations
- Project loading works
- Message handling works
- Code compiles without warnings

