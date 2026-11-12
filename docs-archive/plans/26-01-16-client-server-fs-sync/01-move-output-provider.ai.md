# Phase 1: Move OutputProvider to lp-shared

## Goal

Move `OutputProvider` trait and `MemoryOutputProvider` from `lp-engine` to `lp-shared` so both `lp-server` and `lp-engine` can use it.

## Tasks

1. Create `lp-shared/src/output/mod.rs`:
   - Export `provider` and `memory` modules

2. Create `lp-shared/src/output/provider.rs`:
   - Move `OutputProvider` trait from `lp-engine/src/output/provider.rs`
   - Move `OutputChannelHandle` and `OutputFormat` types
   - Change error type from `lp-engine::Error` to `OutputError` (to be created)

3. Create `lp-shared/src/output/memory.rs`:
   - Move `MemoryOutputProvider` from `lp-engine/src/output/memory.rs`
   - Update to use `OutputError` instead of `lp-engine::Error`

4. Add `OutputError` to `lp-shared/src/error.rs`:
   - Create `OutputError` enum with variants:
     - `PinAlreadyOpen { pin: u32 }`
     - `InvalidHandle { handle: i32 }`
     - `InvalidConfig { reason: String }`
     - `DataLengthMismatch { expected: u32, actual: usize }`
     - `Other { message: String }`
   - Implement `Display` trait

5. Update `lp-shared/src/lib.rs`:
   - Export `output` module

6. Update `lp-engine/src/output/provider.rs`:
   - Re-export from `lp-shared::output::provider`
   - Or remove and update imports

7. Update `lp-engine/src/output/memory.rs`:
   - Re-export from `lp-shared::output::memory`
   - Or remove and update imports

8. Update `lp-engine/src/output/mod.rs`:
   - Re-export from `lp-shared::output` if keeping module structure
   - Or update to import from `lp-shared`

9. Update `lp-engine` imports:
   - Find all uses of `OutputProvider`, `MemoryOutputProvider`, etc.
   - Update to import from `lp-shared`

10. Update `lp-server/src/project.rs`:
    - Change import from `lp_engine::MemoryOutputProvider` to `lp_shared::output::MemoryOutputProvider`
    - Update `Project::new()` signature to take `Rc<RefCell<dyn OutputProvider>>` instead of creating its own

11. Update `lp-engine/src/project/runtime.rs`:
    - Ensure it imports `OutputProvider` from `lp-shared`
    - Update to use `OutputError` instead of `lp-engine::Error` for output operations

## Success Criteria

- `OutputProvider` trait is in `lp-shared/src/output/provider.rs`
- `MemoryOutputProvider` is in `lp-shared/src/output/memory.rs`
- `OutputError` is in `lp-shared/src/error.rs`
- `lp-engine` imports `OutputProvider` from `lp-shared`
- `lp-server` imports `OutputProvider` from `lp-shared`
- `Project::new()` takes `OutputProvider` as parameter
- All code compiles without warnings
- Existing tests still pass
