# Phase 10: Update call sites in lp-client and lp-cli

## Description

Update all call sites in `lp-client` and `lp-cli` crates/apps to use `&LpPath`/`P: AsRef<LpPath>` in function parameters and `LpPathBuf` for storage/returns.

## Implementation

1. Update `lp-client/src/client.rs`:
   - Update function parameters to use `&LpPath` or `P: AsRef<LpPath>`
   - Update `LpFs` method calls (if any)

2. Update `apps/lp-cli/src/`:
   - `commands/dev/handler.rs` - Update `LpFs` calls
   - `commands/dev/pull_project.rs` - Update path handling
   - `commands/dev/push_project.rs` - Update path handling
   - `commands/dev/sync.rs` - Update path handling
   - `commands/dev/watcher.rs` - Update path handling
   - `commands/serve/init.rs` - Update path handling
   - `commands/create/project.rs` - Update path handling
   - `server/create_server.rs` - Update path handling
   - `client/client.rs` - Update path handling

3. Update `apps/lp-cli/tests/`:
   - Update test code to use `LpPath`/`LpPathBuf`

## Success Criteria

- All `lp-client` and `lp-cli` call sites updated
- Function parameters use `&LpPath` or `P: AsRef<LpPath>`
- Storage uses `LpPathBuf`
- Code compiles
- Tests pass
