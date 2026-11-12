# Phase 05: Fast-Only Cleanup And Final Validation

## Scope Of Phase

Clean up the implementation, reduce mode-split complexity where practical, and run final validation.

In scope:

- Remove temporary debugging artifacts.
- Delete or simplify any new compatibility branches that are no longer needed.
- Update comments/docs to say normal render math is fast-first and probes are the later correctness mechanism.
- Decide whether old `q32.*` compile options and model slots are left as compatibility or removed in this pass.
- Run final validation.

Out of scope:

- Implementing debug probes.
- Large user-facing schema migration if it becomes a separate project.
- Dynamic inline div unless it was already completed safely.

## Code Organization Reminders

- Do not leave commented-out experiments.
- Keep helper names semantic: `fdiv_const`, `q32_recip_const`, etc.
- Tests stay at the bottom of Rust files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-shader/lps-q32/src/q32_options.rs`
- `lp-shader/lpir/src/compiler_config.rs`
- `lp-core/lpc-model/src/nodes/shader/glsl_opts.rs`
- `lp-core/lpc-model/src/nodes/shader/shader_def.rs`
- `lp-core/lpc-engine/src/nodes/shader/shader_node.rs`
- `lp-shader/lpvm-native/src/lower.rs`
- `lp-shader/lpvm-wasm/src/emit/ops.rs`
- `docs/reports/` if documenting final perf result

Cleanup decision:

- If removing public `GlslOpts` math mode slots is small and tests are straightforward, do it here.
- If it creates schema/UI churn, leave the public slots as compatibility but make compiler lowering ignore them for normal render. Record removal as future work.
- Do not add new mode-dependent behavior for `FdivConstF32`.

Final report should mention:

- filetest cycle counts for const-div vs dynamic div
- whether wasm mirrors native fast semantics or conservatively supports the op
- any mode cleanup completed or deferred

## Validate

Required:

```sh
cargo fmt --all
cargo test -p lpir
cargo test -p lps-frontend
cargo test -p lpvm-native
cargo test -p lpvm-wasm
scripts/filetests.sh --target rv32n.q32 --detail \
  scalar/float/q32fast-div-const.glsl \
  scalar/float/q32fast-div-recip.glsl \
  scalar/float/op-divide.glsl
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

Shader-pipeline final checks:

```sh
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```

Optional hardware confirmation if filetest/profile results look ambiguous:

```sh
ESPFLASH_PORT=/dev/cu.usbmodem1101 just fwtest-jit-math-perf-esp32c6
```
