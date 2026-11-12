# Phase 5: Implement LpApp::tick() and message handling

## Goal

Implement the main update loop and message processing.

## Tasks

1. Add `tick(&mut self, delta_ms: u32, incoming: &[MsgIn]) -> Result<Vec<MsgOut>, Error>` to `LpApp`:
   - Process incoming messages:
     - `MsgIn::UpdateProject` - load new project (call `load_project()`)
     - `MsgIn::GetProject` - return current project as `MsgOut::Project`
     - `MsgIn::Log` - log message (use platform or stderr)
   - If runtime exists, call `runtime.update(delta_ms, &platform.output)`
   - Collect outgoing messages and return

2. Add helper method `handle_message(&mut self, msg: MsgIn) -> Result<Vec<MsgOut>, Error>`:
   - Process individual message
   - Return appropriate `MsgOut` responses

3. Handle errors gracefully (log, continue processing)

## Success Criteria

- `tick()` compiles
- Processes incoming messages correctly
- Updates runtime if loaded
- Returns outgoing messages
- Errors are handled gracefully
- Code compiles without warnings

