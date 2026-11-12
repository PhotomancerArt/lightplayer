# Phase 8: Set up update loop in fw-host main.rs

## Goal

Set up the main update loop that calls `LpApp::tick()` each frame.

## Tasks

1. Update `fw-host/src/main.rs`:
   - Track frame time in `AppState`:
     - `last_frame_time: Option<Instant>` or similar
   - In `AppState::update()`:
     - Calculate `delta_ms` from frame timestamps
     - Read messages from transport (non-blocking)
     - Convert messages to `MsgIn` enum
     - Call `lp_app.tick(delta_ms, &messages)`
     - Handle outgoing `MsgOut` messages (send via transport if needed)
     - Call `ctx.request_repaint()` to keep loop running
   - Handle first frame (delta_ms = 0 or small value)

2. Update `AppState` struct to include frame timing

3. Ensure loop runs continuously (request_repaint)

## Success Criteria

- Update loop compiles
- Calls `tick()` each frame with correct delta_ms
- Processes messages correctly
- Loop runs continuously
- Code compiles without warnings

