# Phase 2: Create MsgIn/MsgOut enums

## Goal

Create message enums for communication between firmware and LpApp.

## Tasks

1. Create `lp-core/src/app/messages.rs`:
   - `MsgIn` enum:
     - `UpdateProject { project: ProjectConfig }`
     - `GetProject`
     - `Log { level: LogLevel, message: String }`
   - `MsgOut` enum:
     - `Project { project: ProjectConfig }`
     - (Future: status updates, errors, etc.)
   - Both enums derive `Debug, Clone`

2. Update `lp-core/src/app/mod.rs` to export `messages::{MsgIn, MsgOut}`

3. Add `serde` derives if needed (for JSON serialization later)

## Success Criteria

- `MsgIn` and `MsgOut` enums compile
- Exported from `lp-core` crate
- Can be used in function signatures
- Code compiles without warnings

