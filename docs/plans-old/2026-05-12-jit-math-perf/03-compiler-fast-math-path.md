# Phase 3: Compiler Fast Math Path

## Scope Of Phase

Integrate the selected math wins into the normal shader compile/execute path. Normal rendering should use fast math by default; reference math remains available for tests and future debug probes.

Out of scope:

- A full debug math probe.
- LICM/CSE/SSA middle-end work.
- Removing every historical math-mode enum if doing so creates broad churn unrelated to the measured wins.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep backend lowering explicit and close to existing `Fmul` / `Fdiv` code.
- Keep shared Q32 helpers in `lps-q32` only when more than one crate needs them.
- Put tests at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Use the Phase 2 report to choose exact changes. The expected likely changes are below.

### Make fast math the normal default

Update:

- `lp-shader/lps-q32/src/q32_options.rs`
- `lp-shader/lpir/src/compiler_config.rs`
- `lp-core/lpc-model/src/nodes/shader/glsl_opts.rs`
- `lp-core/lpc-engine/src/nodes/shader/shader_node.rs`
- example `shader.toml` files as needed.

Expected direction:

- `Q32Options::default()` becomes:
  - `add_sub = Wrapping`
  - `mul = Wrapping`
  - `div = Reciprocal`
- Product-facing shader defaults should match those fast defaults.
- Existing `q32.add_sub`, `q32.mul`, and `q32.div` filetest override keys can stay temporarily so reference tests still target old behavior.
- If `GlslOpts` remains in the model for compatibility, treat it as legacy/compat input rather than a normal authored performance choice.

Do not remove saturating helper functions from `lps-builtins`; they are useful references for tests and future probes.

### Inline reciprocal divide on native if Phase 2 says it pays

Update:

- `lp-shader/lpvm-native/src/vinst.rs`
- `lp-shader/lpvm-native/src/isa/rv32/encode.rs`
- `lp-shader/lpvm-native/src/isa/rv32/emit.rs`
- `lp-shader/lpvm-native/src/isa/rv32/debug/disasm.rs`
- `lp-shader/lpvm-native/src/lower.rs`

Expected work:

- Add `AluOp::MulHu` for RV32M unsigned high multiply (`mulhu`, funct3 `011`, funct7 `0000001`).
- Add encoder and emitter support.
- In `LpirOp::Fdiv` with `DivMode::Reciprocal`, emit inline VInsts that mirror `__lp_lpir_fdiv_recip_q32` exactly:
  - zero-divisor saturation,
  - sign detection,
  - unsigned abs for dividend/divisor,
  - `divu` for reciprocal,
  - `mul` + `mulhu` for the wide unsigned product,
  - shift/sign apply.
- Add native lowering tests that compare emitted shape and runtime outputs against `__lp_lpir_fdiv_recip_q32`.

If Phase 2 shows helper-call overhead is tiny compared with code-size or register pressure, skip this and document the reason.

### Add const-divisor specialization if generated LPIR has enough constants

First inspect actual LPIR from the steady-render shaders:

```bash
cargo run -p lp-cli -- shader-lpir examples/basic/shader.glsl
cargo run -p lp-cli -- shader-lpir examples/rocaille/shader.glsl
```

Then implement only if literal divisors are common enough to matter.

Likely implementation:

- Extend `lp-shader/lpir/src/const_fold.rs` or add a new pass file if the logic grows.
- Track Q32 constants from `FconstF32` when compiling in Q32 mode.
- Rewrite `Fdiv(lhs, const_rhs)` under reciprocal mode to:
  - multiply by a precomputed reciprocal,
  - or a shift for power-of-two divisors.
- Keep the rewrite bit-equivalent to the chosen fast reciprocal semantics for normal non-zero constants.
- Preserve divisor-zero behavior.

If the pass needs compile options, thread the relevant `CompilerConfig` through the pass rather than using globals or features.

### Ship selected fast trig

Update:

- `lp-shader/lps-builtins/src/builtins/glsl/sin_q32.rs`
- `lp-shader/lps-builtins/src/builtins/glsl/cos_q32.rs`
- `lp-shader/lps-builtins/src/builtins/glsl/sincos_q32.rs`
- optional new LUT files if the LUT candidate wins.

Expected direction:

- Replace the normal `__lps_sin_q32` implementation with the winning fast candidate.
- Keep a private or test-only reference implementation if useful for quality tests.
- If a LUT wins:
  - use generated table data,
  - document table size and encoding,
  - keep no-std compatibility,
  - ensure the table does not accidentally move the compiler behind `std`.
- Add tests for key angles, periodicity, and quality envelope.

If no trig candidate beats the current implementation once LUT cost is included, document that result and skip trig changes.

## Validate

Run targeted tests first:

```bash
cargo test -p lps-q32
cargo test -p lps-builtins
cargo test -p lpir
cargo test -p lpvm-native
cargo run -p lps-filetests-app -- --target rv32n.q32
```

Then validate firmware compile paths:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

Also run:

```bash
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```
