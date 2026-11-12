# Current State Analysis

## Already Completed

1. **lp-core-util crate created**:
   - Filesystem code moved (`LpFs` trait, `LpFsMemory`, `LpFsStd`)
   - Has `std` feature support
   - `lp-core` already depends on it and uses `lp_core_util::fs::LpFs`

2. **lp-api crate created**:
   - Empty `lib.rs` - ready for protocol messages

3. **lp-server crate created**:
   - Empty `lib.rs` - ready for server implementation
   - Already has dependency on `lp-core-util` with `std` feature

4. **lp-core cleanup**:
   - `fs/` directory removed from `lp-core`
   - `lp-core` now uses `lp_core_util::fs::LpFs`
   - `api/mod.rs` cleaned up (only references `messages` module)

5. **lp-core-cli**:
   - Uses `lp_core_util::fs::LpFsMemory` (but has typo: `lp_core_util::::LpFsMemory`)

## Still Needs Work

1. **lp-core**:
   - `create_default_project()` still exists and is used in `load_project()`
   - `log/` module still exists (should move to `lp-core-util`)
   - Tests in `nodes/output/runtime.rs`, `nodes/texture/runtime.rs`, `runtime/contexts.rs` still use old `ProjectConfig` structure with `nodes` field (needs updating)

2. **lp-api**:
   - Empty - needs `ClientMsg`/`ServerMsg` protocol definitions

3. **lp-server**:
   - Empty - needs implementation

4. **lp-core-cli**:
   - Typo on line 21: `use lp_core_util::::LpFsMemory;` (extra colons)
   - Still embeds `LpApp` directly - needs to use `lp-server` library

5. **Test cleanup**:
   - Several tests create `ProjectConfig` with old `nodes: Nodes { ... }` structure
   - These need to be updated to just use `ProjectConfig { uid, name }`

## Notes

- All crates should support `std` feature (optional, for no_std compatibility)
- Tests in `app/lp_app.rs` and `project/loader.rs` already use file-based projects correctly
- Tests in `nodes/*/runtime.rs` and `runtime/contexts.rs` need updating
