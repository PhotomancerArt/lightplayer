# Phase 6: Tests and Validation

## Primary validation

```bash
scripts/glsl-filetests.sh
```

This is the main correctness check. The filetests cover the full
compiler pipeline — parsing, semantic analysis, codegen, and output
verification. If they pass with direct Q32 emission, the pipeline is
correct.

## Unit tests

```bash
cargo test -p lps-compiler --features std
```

Includes the streaming-specific tests in `frontend/mod.rs::tests`:

- `test_glsl_jit_streaming_basic` — basic compilation
- `test_glsl_jit_streaming_multi_function` — cross-function calls
- `test_streaming_returns_correct_value` — output matches batch path
- `test_streaming_multi_function_cross_calls` — complex call graph

The last two compare streaming output against batch output. Since both
now use direct emission, they should produce identical results.

## Emulator tests (if emulator feature is available)

```bash
cargo test -p lps-compiler --features std,emulator
```

## What to watch for

- **Signature mismatches**: If SignatureBuilder produces different
  signatures than the old transform path, function calls will fail
  with type errors. The verifier (if enabled) catches these.
- **Missing builtin declarations**: If a Q32 builtin isn't declared
  before codegen tries to reference it, `get_builtin_func_ref` will
  error. The existing `declare_builtins()` should prevent this.
- **TestCase relocation errors**: These should not appear in Q32 mode
  anymore — the codegen calls Q32 builtins directly (Plan C) instead
  of emitting TestCase-named float libcalls.
- **Streaming memory**: With a single module, the streaming path
  should use less peak memory than before. Not a correctness concern
  but worth checking (heap traces in Plan E).
