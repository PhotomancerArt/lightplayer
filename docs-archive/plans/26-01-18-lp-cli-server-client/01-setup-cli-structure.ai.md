# Phase 1: Set up CLI Structure and Dependencies

## Goal

Set up the basic CLI structure with command parsing, error handling, and module organization.

## Tasks

1. Update `lp-app/apps/lp-cli/Cargo.toml`:
   - Add dependencies:
     - `clap = { version = "4", features = ["derive"] }`
     - `anyhow = "1"`
     - `serde = { version = "1", features = ["derive"] }`
     - `serde_json = "1"`
     - `lp-model` (workspace)
     - `lp-server` (workspace)
     - `lp-client` (workspace)
     - `lp-shared` (workspace)

2. Create `lp-app/apps/lp-cli/src/main.rs`:
   - Use `clap` with derive macros to define command structure
   - Define `Cli` enum with variants: `Serve`, `Dev`, `Create`
   - Parse arguments and route to command handlers
   - Use `anyhow::Result` for error handling

3. Create `lp-app/apps/lp-cli/src/commands/mod.rs`:
   - Re-export `serve`, `dev`, `create` modules
   - Define command handler function signatures

4. Create directory structure:
   - `src/commands/serve/`
   - `src/commands/dev/`
   - `src/commands/create/`
   - `src/transport/`
   - `src/config/`
   - `src/error.rs`
   - `src/messages.rs`

5. Create stub modules:
   - `src/commands/serve/mod.rs` - Export serve command
   - `src/commands/dev/mod.rs` - Export dev command
   - `src/commands/create/mod.rs` - Export create command
   - `src/error.rs` - CLI-specific error types (can be minimal, use anyhow for now)
   - `src/messages.rs` - Stub for message formatting (implement in phase 9)

## Success Criteria

- CLI structure compiles
- `lp-cli --help` shows command structure
- Commands can be parsed (even if handlers are stubs)
- Code compiles without warnings
- Module structure matches design
