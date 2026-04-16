# Milestone 7: Cleanup

## Goal

Delete obsolete code, remove dead traits and crates, verify everything builds
and passes across all targets.

## Suggested plan name

`lpvm2-m7`

## Scope

### In scope

- Delete `GlslExecutable` trait and `lps-exec` crate (if still exists)
- **M5 filetests / LPVM follow-ups** (completed, see
  [`docs/plans-done/2026-04-07-lpvm2-m5-filetests/summary.md`](../../plans-done/2026-04-07-lpvm2-m5-filetests/summary.md)):
  - ✅ **M5 phase 8** complete: `call_q32`, `debug_state`, filetest migration,
    plan moved to `docs/plans-done/`.
  - **`lps-filetests` / `wasm_link.rs`:** still required by
    `tests/lpfx_builtins_memory.rs` (wasmtime builtins + `env.memory`). Either
    migrate that test to the **`lpvm-wasm`** instantiate/link path or extract a
    tiny shared wasmtime helper, then **delete** `wasm_link.rs` and any duplicate
    linker logic.
  - **`lps-filetests` GLSL → IR:** today `CompiledShader::compile_glsl` lowers
    once per compile; optional hygiene — **reuse one `CraneliftEngine` /
    `EmuEngine` / `WasmLpvmEngine` per runner session** (or per target) if we
    want less allocator churn (not required for correctness).
  - **Filetest `--debug` output:** CLIF / VCode / disassembly blocks were tied
    to `GlslExecutable` and dropped from `run_detail::format_error`. If we still
    want them, plumb optional strings from **`CraneliftModule`** / **`WasmLpvmModule`**
    metadata (or a small `FiletestDebug` holder) instead of restoring
    `GlslExecutable`.
  - **`glsl_q32_call_emulated`** (`lpvm-emu`) vs **`EmuInstance`** invoke path:
    consider **one implementation** called from both the legacy helper and
    `call`/`call_q32` to avoid emulator/ABI drift.
- Delete old `JitModule` / `jit()` API from `lpvm-cranelift` (if no longer
  used after M6)
- Remove any remaining `emu_run.rs` remnants from `lpvm-cranelift`
- Remove unused feature flags
- Audit and remove dead dependencies
- Update documentation (AGENTS.md, crate-level docs)
- Run full validation suite:
  - `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server`
  - `cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu`
  - `cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu`
  - `cargo check -p lp-server`
  - `cargo test -p lps-filetests`
  - Full **filetest matrix** (CI parity): `jit.q32` / `jit.f32` / `rv32.q32` /
    `rv32.f32` / `wasm.q32` / `wasm.f32` as listed in M5 phase 8 — e.g.
    `just test-filetests` or `./scripts/glsl-filetests.sh` with builtins built
    (`scripts/build-builtins.sh` for RV32-linked paths).
  - `cargo +nightly fmt --check`
- Verify no warnings in affected crates

### Out of scope

- New features
- fw-wasm implementation
- Performance optimization

## Key Decisions

1. **Old API removal is gated on M6 completion**: Only delete `JitModule` /
   `jit()` / old `GlslExecutable` paths once all consumers have migrated.
   **Filetests** no longer depend on `GlslExecutable`; confirm with
   `rg 'lps_exec|GlslExecutable' lp-shader/lps-filetests` (expect no hits).
   **Workspace-wide**, only `lp-shader/legacy/lps-exec` should remain until the
   crate is deleted.

2. **AGENTS.md update**: The validation commands and architecture diagram
   should reflect the new LPVM-based architecture.

## Deliverables

- Removed obsolete code and crates
- Updated documentation
- Clean build across all targets with no warnings
- All tests passing

## Dependencies

- Milestone 6 (engine migration) — all consumers must be migrated before
  deleting old APIs

## Estimated scope

~200–400 lines deleted, ~100 lines of documentation updates. Small milestone
focused on hygiene.
