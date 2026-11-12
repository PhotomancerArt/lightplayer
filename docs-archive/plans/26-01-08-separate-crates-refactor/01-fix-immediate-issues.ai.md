# Phase 1: Fix Immediate Issues

## Goal

Fix immediate compilation issues and move logging utilities to `lp-core-util`.

## Tasks

1. **Fix typo in lp-core-cli**:
   - Fix `use lp_core_util::::LpFsMemory;` (remove extra colons)
   - Should be `use lp_core_util::fs::LpFsMemory;`

2. **Move log/ module to lp-core-util**:
   - Move `lp-core/src/log/mod.rs` to `lp-core-util/src/log/mod.rs`
   - Update `lp-core-util/src/lib.rs` to export log module
   - Add `std` feature support if needed

3. **Update lp-core to use lp-core-util for logging**:
   - Remove `log/` module from `lp-core/src/lib.rs`
   - Update any imports in `lp-core` to use `lp_core_util::log`
   - Update `lp-core/Cargo.toml` if needed

4. **Verify compilation**:
   - Ensure `lp-core` compiles
   - Ensure `lp-core-cli` compiles
   - Ensure `lp-core-util` compiles

## Success Criteria

- Typo fixed in `lp-core-cli`
- Logging module moved to `lp-core-util`
- All code compiles without errors
- No warnings (except unused code that will be used later)
