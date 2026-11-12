# Summary: Naga WASM scaffold (2026-03-20)

## Done

- **`lps-frontend`**: GLSL → `naga::Module` via `naga::front::glsl`, vertex stage, synthetic `main`
  when missing, `NagaModule` with per-function export metadata.
- **`lps-wasm`**: Rewritten to lower `naga::Module` with `wasm-encoder` (`emit.rs`, `locals.rs`);
  old `codegen/` tree removed. Q32 uses fixed-point rules aligned with Cranelift where the IR
  exposes them.
- **`lps-filetests`**: WASM path compiles through `lps-frontend` + `lps-wasm`; `wasm_runner` builds
  signatures from `WasmExport`.
- **Scalar `wasm.q32`**: `./scripts/filetests.sh --target wasm.q32 scalar` passes (432 tests);
  3 cases marked expected-failure via `@unimplemented(backend=wasm)` where Naga constant-folding
  differs from runtime semantics (`scalar/float/op-divide`, `op-subtract`, `scalar/uint/from-float`
  negative literal).
- **Cast fixes (wasm)**: `int`/`uint` → `float` (Q32) clamp to representable Q16.16 integer range
  before scaling; `float` → `int` (Q32) uses `i64.div_s` by 65536 for trunc-toward-zero then clamps;
  `float` → `uint` uses the same div path (no shr-u on negative). `BlockType::Result(ValType::I32)`
  for typed `if` blocks (wasm-encoder API).

## Follow-ups (not in this scaffold)

- Broader WASM coverage (vectors, control flow gaps, builtins).
- Any remaining Naga fold vs fixed-point ordering mismatches beyond annotated filetests.
