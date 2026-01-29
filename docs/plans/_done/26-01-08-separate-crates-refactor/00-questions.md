# Questions: Separate Crates Refactor

## Goal

Refactor the LightPlayer codebase to clearly separate responsibilities into distinct crates:
- `lp-server`: Multi-project management and server functionality
- `lp-core`: Single-project runtime (cleaned up, no cross-project concerns)
- `lp-api`: Client/server shared protocol messages
- `lp-util`: Shared utilities (filesystem abstraction, logging)

## Current State

**Partially cleaned up:**
- `lp-core/src/protocol/` has been removed
- `MsgIn`/`MsgOut` moved to `lp-core/src/api/messages.rs` with new message types (ListNodes, WatchNode, UnwatchNode)
- `LpApp::handle_message()` is now a TODO stub

**Still needs cleanup:**
- `lp-core/src/api/mod.rs` still references non-existent `command` and `message` modules
- `lp-core-cli` still uses `parse_command` and tries to convert `Command` to `MsgIn` (broken)
- `MsgIn` references `crate::api::LogLevel` which doesn't exist
- `lp-core` still contains:
  - Filesystem abstraction (`fs/`) - should move to `lp-util`
  - Logging utilities (`log/`) - should move to `lp-util`
  - Cross-project concerns (`create_default_project()`) - should move to `lp-server`
  - `api/` module with messages - should move to `lp-api` crate

- `lp-core-cli` is a monolithic app that embeds server logic

## Questions

**First: Cleanup Phase**

1. **Broken references cleanup**: The `api/mod.rs` file references non-existent modules. Should we:
   - Option A: Remove the broken `command` and `message` module references immediately, fix `lp-core-cli` to not use them
   - Option B: Keep stubs temporarily to avoid breaking `lp-core-cli` during transition
   - **Suggested**: Option A - clean break, fix `lp-core-cli` to work without the old protocol

2. **MsgIn/MsgOut location**: Currently in `lp-core/src/api/messages.rs`. Should these:
   - Option A: Stay in `lp-core` as internal messages (move from `api/` to `app/`), `lp-server` translates to/from `lp-api` protocol
   - Option B: Move to `lp-api` crate, `lp-core` uses them directly
   - **Suggested**: Option A - keep internal to `lp-core`, they represent the interface between server and core runtime

3. **LogLevel location**: Currently referenced but doesn't exist. Where should it live?
   - Option A: `lp-api` (since it's part of the protocol)
   - Option B: `lp-util` (since it's a utility type)
   - **Suggested**: Option A - it's a protocol-level concept

**Refactoring Phase**

4. **Default project creation**: Where should project creation logic live?
   - Current: `LpApp::create_default_project()` creates a `ProjectConfig` with default values
   - Option A: `lp-server` provides `create_project(name)` that creates filesystem structure and initializes `LpApp`
   - Option B: External tool/command, `lp-server` just loads existing projects
   - **Suggested**: Option A - `lp-server` should handle project lifecycle

5. **Filesystem trait location**: The `Filesystem` trait is currently in `lp-core/fs/`. Should it:
   - Option A: Move entirely to `lp-util` (shared by all crates)
   - Option B: Keep trait in `lp-core`, implementations in `lp-util`
   - **Suggested**: Option A - it's a shared utility, all crates need it

6. **Error types**: Currently `Error` is in `lp-core`. Should we:
   - Option A: Keep `Error` in `lp-core`, create separate error types in other crates
   - Option B: Move shared error types to `lp-util`, keep crate-specific errors in each crate
   - **Suggested**: Option A - each crate has its own error domain, keep them separate

7. **lp-core-cli future**: What should happen to `lp-core-cli`?
   - **DECIDED**: `lp-core-cli` IS the server binary - it continues to work exactly as it does now
   - `lp-server` is a LIBRARY crate that `lp-core-cli` uses internally
   - `lp-core-cli` keeps its current functionality (GUI, embedded LpApp, etc.) but uses `lp-server` library
   - The refactoring is internal - external behavior stays the same
   - Later, we can create a separate `lp-cli` that connects to `lp-server` remotely
   - **Acceptance criteria**: Everything continues to work as it currently does
