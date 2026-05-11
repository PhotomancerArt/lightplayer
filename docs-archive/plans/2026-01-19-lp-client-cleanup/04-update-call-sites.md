# Phase 4: Update All Call Sites

## Description

Update all code that references `AsyncLpClient` to use `LpClient` instead.

## Tasks

1. Update `lp-app/apps/lp-cli/src/commands/dev/handler.rs`:
   - Change `AsyncLpClient` to `LpClient`
   - Update `AsyncClientTransport` references if needed
   - Update `client_connect` return type if needed

2. Update `lp-app/apps/lp-cli/src/commands/dev/push_project.rs`:
   - Change `AsyncLpClient` to `LpClient`

3. Update `lp-app/apps/lp-cli/src/commands/dev/pull_project.rs`:
   - Change `AsyncLpClient` to `LpClient`

4. Update `lp-app/apps/lp-cli/src/commands/dev/sync.rs`:
   - Change `AsyncLpClient` to `LpClient`

5. Update `lp-app/apps/lp-cli/src/commands/dev/fs_loop.rs`:
   - Change `AsyncLpClient` to `LpClient`
   - Update `AsyncClientTransport` references if needed

6. Update `lp-app/apps/lp-cli/src/debug_ui/ui.rs`:
   - Change `AsyncLpClient` to `LpClient`

7. Update `lp-app/apps/lp-cli/src/commands/dev/async_client.rs`:
   - Update re-exports to use `LpClient` instead of `AsyncLpClient`

8. Check for any other files that import or use `AsyncLpClient`:
   - Search codebase for `AsyncLpClient`
   - Update all references

## Success Criteria

- All `AsyncLpClient` references changed to `LpClient`
- All imports updated
- Code compiles (may have errors from transport implementation, that's expected)
