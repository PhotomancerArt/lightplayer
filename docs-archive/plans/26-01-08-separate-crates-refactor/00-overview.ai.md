# Overview: Separate Crates Refactor

## Goal

Refactor the LightPlayer codebase to clearly separate responsibilities into distinct crates while maintaining backward compatibility. `lp-core-cli` continues to work exactly as it does now, but uses the new library structure internally.

## Scope

This refactoring:
- Creates `lp-core-util` for shared utilities (filesystem, logging)
- Creates `lp-api` for client/server protocol messages
- Creates `lp-server` library for multi-project management
- Cleans up `lp-core` to be single-project focused
- Refactors `lp-core-cli` to use `lp-server` library internally
- Updates tests to use file-based projects
- **Maintains backward compatibility** - `lp-core-cli` works exactly as before

## Key Principles

1. **Incremental Migration**: Changes are internal - external behavior stays the same
2. **Clear Separation**: Each crate has a single, well-defined responsibility
3. **Backward Compatibility**: `lp-core-cli` continues to work exactly as it does now
4. **Test Coverage**: All tests updated and passing

## Acceptance Criteria

- All code compiles without warnings (except unused code that will be used later)
- All tests pass
- `lp-core-cli` works exactly as it does currently
- Clear separation of concerns between crates
- `lp-core` is single-project focused (no cross-project logic)
- `lp-server` handles multi-project management
- Tests use file-based project structure
