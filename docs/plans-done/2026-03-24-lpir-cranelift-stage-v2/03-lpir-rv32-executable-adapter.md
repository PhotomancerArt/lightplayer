## Scope of phase

**Prerequisite:** same **`lps_exec::GlslExecutable`** boundary as phase 02
(phase 04 wiring complete).

Add **`lpir_rv32_executable.rs`**: **`GlslExecutable`** backed by **Stage V1**
pipeline (object + link + emulator).

- **Compile path:** GLSL → Naga → LPIR (reuse same lowering as `jit` — e.g. share
  a small internal `fn glsl_to_ir(source) -> (IrModule, GlslModuleMeta)` or call
  `lps_naga` + match `lpir_cranelift` compile helpers) → **object bytes** →
  **link** with builtins ELF → **emulator** instance.
- Mirror **emulator options** from `compile.rs` constants (`DEFAULT_MAX_MEMORY`,
  `DEFAULT_MAX_INSTRUCTIONS`, `log_level` from `run_detail`).

**Prerequisite:** Stage V1 public API exists (`object_bytes_from_ir`, link, run).

If V1 is not merged yet: add **`LpirRv32Executable`** behind **`cfg`** or stub
that returns a clear compile error, and **`compile_for_target`** maps **`Rv32`**
to stub — prefer **not** landing broken default targets; coordinate ordering
with V1 plan.

## Code organization reminders

- Isolate **ELF / emu** state in a struct; keep **`GlslExecutable`** methods thin.
- Host-only code is OK (`std`); match **`lps-filetests`** features.

## Implementation details

- **Symbol resolution:** same function naming as JIT (`GlslModuleMeta` / IR
  function names) so `call_f32("add", …)` finds the same symbol as `jit.q32`.
- **Traps / timeout:** align with existing **`run_detail`** trap expectation
  handling (compare with legacy `GlslEmulatorModule` behavior).
- **`format_emulator_state`:** optionally forward if V1 exposes debug state.

## Tests

- **Ignored-by-default** or **integration** test: one scalar shader on
  **`rv32.q32`** when builtins ELF is available (same policy as V1).
- Minimum: mock-free unit test only if emulator can run in CI; otherwise
  document local run in README.

## Validate

```bash
cd /Users/yona/dev/photomancer/lp2025/lps && cargo test -p lps-filetests --lib
# With V1 + builtins:
cargo test -p lps-filetests --test filetests --features ... 
```

`cargo +nightly fmt`.
