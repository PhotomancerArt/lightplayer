## Phase 8: Cleanup and Commit

### Scope

Final validation, documentation updates, and commit. Ensure:
- All code compiles with and without `runtime` feature
- No stray TODOs or debug prints (keep only intentional TODOs with context)
- Workspace builds
- Plan files moved to done

### Cleanup Steps

1. **Check for temporary code:**
   ```bash
   grep -r "TODO" lpvm/lpvm-wasm/src/ | grep -v "TODO(lpvm-wasm):"
   ```
   Review each — keep only those with clear context.

2. **Check for debug prints:**
   ```bash
   grep -r "println!" lpvm/lpvm-wasm/src/
   grep -r "dbg!" lpvm/lpvm-wasm/src/
   ```
   Remove or convert to proper logging if needed.

3. **Formatting:**
   ```bash
   cargo +nightly fmt -p lpvm-wasm
   ```

4. **Final validation:**
   ```bash
   # no_std build (emission only)
   cargo check -p lpvm-wasm --no-default-features
   
   # Full runtime build
   cargo check -p lpvm-wasm --features runtime
   
   # Tests (some may be incomplete — note which)
   cargo test -p lpvm-wasm --no-default-features
   cargo test -p lpvm-wasm --features runtime
   
   # Workspace check
   cargo check -p lps-filetests
   ```

5. **Documentation:**
   - Update `lpvm/lpvm-wasm/README.md` (create if missing):
     - Purpose of crate
     - Feature flags
     - Basic usage example
   - Ensure lib.rs module docs are complete

### Plan Cleanup

Create summary file:

```bash
cat > docs/plans/2026-04-05-lpvm-wasm-stage-i/summary.md << 'EOF'
# LPVM WASM Stage I - Summary

## Completed

- Created `lpvm/lpvm-wasm/` crate with emission and runtime
- Ported LPIR → WASM emission from `lps-wasm` (parallel implementation)
- Implemented `LpvmEngine`, `LpvmModule`, `LpvmInstance` traits for wasmtime
- Added unit tests for emission (WASM bytes valid)
- Added integration tests for runtime (compile → instantiate → call)

## Key Design Decisions

1. **Parallel infrastructure:** Emission copied, not moved. `lps-wasm` intact.
2. **WasmEngine holds:** wasmtime `Engine` + builtins bytes (shared resources)
3. **WasmRuntimeModule:** Wraps both emission output and wasmtime `Module`
4. **Error unification:** `WasmError` enum covers all operations

## Known Limitations

- Runtime tests require builtins WASM (may be skipped if not built)
- Q32 marshaling (value * 65536.0) may be partially implemented
- `out`/`inout` params not supported by semantic `call()` API

## Next Steps (M2 Stage II)

- Refine Q32 marshaling accuracy
- Implement full builtins linking
- Migrate `lps-filetests` to use new backend (M5)
- Browser runtime support (wasm-bindgen)
EOF
```

Move plan to done:

```bash
mv docs/plans/2026-04-05-lpvm-wasm-stage-i docs/plans-done/
```

### Commit

```
feat(lpvm-wasm): add LPVM WASM backend with trait implementations

- Add lpvm-wasm crate at lpvm/lpvm-wasm/
- LPIR → WASM emission (no_std + alloc, wasm-encoder)
- wasmtime runtime implementing LpvmEngine/LpvmModule/LpvmInstance
- Emission unit tests (WASM bytes validation)
- Runtime integration tests (compile → instantiate → call)
- Parallel infrastructure: lps-wasm unchanged

Note: Runtime tests require lps-builtins-wasm. Some tests may be
skipped if builtins not available.
```

### Final Validation Command

```bash
cargo check -p lpvm-wasm --no-default-features && \
cargo check -p lpvm-wasm --features runtime && \
cargo test -p lpvm-wasm --no-default-features && \
echo "Phase 8 complete"
```
