# Phase 8: Cleanup and Validation

## Scope of Phase

Final cleanup, validation, and commit. Ensure all tests pass, no warnings, and the implementation is
complete.

## Cleanup Checklist

### Code Quality

- [ ] Remove any `TODO` comments that were temporary scaffolding
- [ ] Remove any debug `println!` statements
- [ ] Fix all compiler warnings
- [ ] Run `cargo +nightly fmt` on all changed files
- [ ] Check for unused imports or dead code

### Test Validation

Run the following validation commands:

```bash
# Core crate tests
cargo test -p lpvm -p lpir -p lpir-cranelift -p lps-naga

# WASM emission tests
cargo test -p lps-wasm

# Filetests (these will exercise the full pipeline)
cargo test -p lps-filetests

# Embedded builds (must pass for embedded JIT)
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu

# Format check
cargo +nightly fmt -- --check
```

### Documentation

- [ ] Verify `docs/design/uniforms-globals.md` exists and is complete
- [ ] Update crate READMEs if needed (lpvm, lpir-cranelift)
- [ ] Check that new public APIs have doc comments

## Commit Message

Once everything passes, commit with:

```bash
git add -A
git commit -m "$(cat <<'EOF'
feat(glsl): VMContext foundation for uniforms and globals

- Add VmContextHeader with fuel, trap_handler, globals_defaults_offset
- Add vmctx_vreg to IrFunction (vreg 0 is always VMContext)
- Update Cranelift signatures to include VMContext as first param
- Update WASM emission with vmctx_local in FuncEmitCtx
- Update DirectCall API to accept VMContext pointer
- Update all test harnesses to allocate and pass VMContext
- Create design doc for uniforms-globals architecture

All functions now receive VMContext as explicit first parameter.
This enables future milestones to add uniform and global access.

EOF
)"
```

## Post-Commit

After committing:

1. Create a summary file at `docs/plans/2026-04-02-vmcontext-globals-uniforms-stage-i/summary.md`
2. Move plan to `docs/plans-done/2026-04-02-vmcontext-globals-uniforms-stage-i/`

## Summary Template

```markdown
# Milestone I: VMContext Foundation — Summary

## Completed Work

- Defined VmContextHeader with well-known field offsets
- Added vmctx_vreg to IrFunction (explicit VMContext in LPIR)
- Updated Cranelift emission for VMContext-first signatures
- Updated WASM emission for VMContext as first local
- Updated DirectCall and invoke APIs
- Updated all test harnesses to allocate VMContext
- Created design document for full architecture

## Files Changed

- lpvm/src/vmcontext.rs (NEW)
- lpvm/src/lib.rs
- lpir/src/module.rs
- lpir-cranelift/src/emit/mod.rs
- lpir-cranelift/src/lib.rs
- lpir-cranelift/src/invoke.rs
- lps-wasm/src/emit/mod.rs
- lps-wasm/src/emit/func.rs
- lps-filetests/src/test_run/q32_exec_common.rs
- lps-filetests/src/test_run/wasm_runner.rs
- docs/design/uniforms-globals.md (NEW)

## Validation

All tests pass:

- cargo test -p lpvm -p lpir -p lpir-cranelift -p lps-naga
- cargo test -p lps-wasm
- cargo test -p lps-filetests
- cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
- cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu

## Next Steps

Proceed to Milestone II: Uniforms and Readonly Globals
```

## Notes

- If any validation fails, fix it before committing
- This milestone intentionally breaks backward compatibility
- Future milestones will build on this foundation
