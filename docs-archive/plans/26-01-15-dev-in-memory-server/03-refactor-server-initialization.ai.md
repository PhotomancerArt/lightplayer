# Phase 3: Refactor Server Initialization

## Description

Extract server creation logic from the `serve` command into a shared `server.rs` module. This allows both `serve` and `dev` commands to use the same server creation logic, minimizing duplication.

## Tasks

1. Create `lp-app/apps/lp-cli/src/server.rs`:
   - Add `create_server()` function:
     ```rust
     pub fn create_server(
         dir: Option<&Path>,
         memory: bool,
         init: Option<bool>,
     ) -> Result<(LpServer, Box<dyn LpFs>)>
     ```
   - Implementation:
     - If `memory` is true:
       - Create `LpFsMemory` filesystem
       - Use `ServerConfig::default()` (no file needed)
     - If `memory` is false:
       - Create `LpFsStd` filesystem with `dir` (or current directory)
       - If `init` is `Some(true)`: call `initialize_server()` to create/load config
       - If `init` is `Some(false)`: call `initialize_server()` to load config (error if missing)
       - If `init` is `None`: use default config (for backward compatibility)
     - Create `MemoryOutputProvider`
     - Create `LpServer` with filesystem and output provider
     - Return `(LpServer, Box<dyn LpFs>)`

2. Update `lp-app/apps/lp-cli/src/commands/serve/handler.rs`:
   - Import `create_server` from `crate::server`
   - Replace server creation logic with call to `create_server(Some(&server_dir), args.memory, Some(args.init))`
   - Keep WebSocket transport creation and sync server loop

3. Update `lp-app/apps/lp-cli/src/commands/serve/init.rs`:
   - Keep `initialize_server()` and `create_filesystem()` functions (still used by `create_server()`)
   - These can remain in `init.rs` or be moved to `server.rs` (prefer keeping in `init.rs` for now)

4. Add tests:
   - Test `create_server()` with memory filesystem
   - Test `create_server()` with disk filesystem and `init = Some(true)`
   - Test `create_server()` with disk filesystem and `init = Some(false)` (existing config)
   - Test `create_server()` with disk filesystem and `init = Some(false)` (missing config - should error)

## Success Criteria

- `create_server()` function exists in `server.rs`
- `serve` command uses `create_server()` and still works correctly
- Server creation logic is shared between commands
- All existing tests continue to pass
- New tests added for `create_server()`
- Code compiles without warnings

## Implementation Notes

- Keep `initialize_server()` and `create_filesystem()` in `init.rs` for now (can refactor later if needed)
- `create_server()` handles the logic of when to initialize vs use defaults
- For in-memory mode, no `server.json` file is needed or created
- Return both server and filesystem so caller can use filesystem if needed
