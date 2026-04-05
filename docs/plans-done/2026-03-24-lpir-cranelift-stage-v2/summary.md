# Stage V2 summary (lpir-cranelift / filetests)

## Completed

- **Target matrix:** `jit.q32`, `wasm.q32`, `rv32.q32`; annotations `@jit` / `@wasm` / `@rv32`;
  legacy `cranelift` backend name removed from the filetest runner.
- **`DEFAULT_TARGETS`:** `jit.q32` only for fast default runs (`scripts/glsl-filetests.sh` with no
  `--target`, and `run_filetest` / ignored integration test).
- **Execution stack:** filetests compile and run through `lp-glsl-exec` and related crates (
  `lps-shared`, `lpvm`, `lp-glsl-diagnostics`); legacy `lp-glsl-cranelift` is not a
  filetests dependency.
- **Adapters:** `LpirJitExecutable`, `LpirRv32Executable`, wasm path wired per plan.
- **Parallel JIT / object codegen:** process-wide serialization in `lpir-cranelift` (`process_sync`)
  to avoid crashes when multiple workers codegen; panic hook installed with `Once` in the concurrent
  runner.
- **CI parity:** `just test-filetests` (and `just test` / `test-glsl-filetests`) runs the script for
  **jit** (default pass), **wasm.q32**, and **rv32.q32** sequentially.
- **Docs:** `lp-glsl-filetests/README.md` updated for the above; corpus comments that referred to
  `cranelift.q32` updated where found.

## Deferred / follow-ups

- **`lib.rs` TODOs:** compile-only and transform filetest kinds still stubs (“Phase 4”).
- **Integration test:** remains `#[ignored]`; full target sweep is via the script /
  `just test-filetests`, not `cargo test` alone.
- **Legacy crate:** `lp-glsl-cranelift` remains in the workspace for other packages (e.g. ESP32 JIT,
  metrics); not removed in this stage.

## Validation commands (local)

```bash
cd lp2025 && just check build-ci test   # CI-shaped path
cd lp2025/lp-glsl && cargo clippy -p lp-glsl-filetests -- -D warnings
cargo +nightly fmt --all
```
