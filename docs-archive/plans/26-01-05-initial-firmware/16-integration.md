# Phase 16: Integration - Link all components and establish basic communication

## Goal

Integrate all components together and establish basic communication flow.

## Tasks

1. Create main application logic in both `fw-esp32` and `fw-host`:
   - Initialize all systems (filesystem, transport, LED output)
   - Set up command handler loop
   - Handle `UpdateProject`, `GetProject`, `Log` commands
2. Implement project loading/saving:
   - Load `project.json` from filesystem on startup
   - Handle `UpdateProject` command to save project
   - Handle `GetProject` command to return current project
3. Implement status tracking:
   - Update `ProjectRuntime` when nodes are loaded/validated
   - Generate log messages when statuses change
4. Test end-to-end:
   - Host can send `UpdateProject` to ESP32
   - ESP32 can respond with `GetProject`
   - Log messages flow correctly
   - Project persists across restarts

## Success Criteria

- All components work together
- Commands can be sent/received between host and ESP32
- Project loading/saving works
- Status tracking works
- All code compiles without warnings

