# Phase 1: Remove lp-client Dependency and Delete Crate

## Description

Remove the `lp-client` dependency from `lp-cli` and delete the `lp-client` crate from the workspace.

## Tasks

1. Remove `lp-client` dependency from `lp-app/apps/lp-cli/Cargo.toml`:
   - Remove `lp-client = { path = "../../crates/lp-client" }` from dependencies
   - Remove `lp-client` from dev-dependencies if present

2. Remove `lp-client` from workspace `Cargo.toml`:
   - Remove `"crates/lp-client"` from the members list

3. Delete the `lp-client` crate directory:
   - Delete `lp-app/crates/lp-client/` directory

4. Update `lp-app/apps/lp-cli/src/client/mod.rs`:
   - Remove `pub mod async_client;` (file doesn't exist anyway)
   - Remove `pub mod async_transport;` (if file doesn't exist)
   - Remove `pub use async_client::...` and `pub use async_transport::...` lines
   - Keep other modules (client_connect, local, local_server, specifier, transport_ws)

## Success Criteria

- `lp-client` dependency removed from `lp-cli/Cargo.toml`
- `lp-client` removed from workspace `Cargo.toml`
- `lp-client` crate directory deleted
- `mod.rs` updated to remove non-existent module references
- Code compiles (will have errors from missing `AsyncLpClient`, that's expected)
