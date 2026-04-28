# Phase 4: Tests & Validation

## Primary validation

- `scripts/filetests.sh` — full shader test suite, catches regressions
- ESP32 heap trace — compare peak before and after each phase

## Per-phase validation

### Phase 1 (format-aware builtins)

- Filetests pass (Q32 mode works without F32 builtins declared)
- If a shader uses an LPFX function, the correct Q32 builtin is declared
- Error message if a shader somehow references an undeclared builtin

### Phase 2 (release AST borrow)

- Filetests pass (no behavioral change from lookup-by-name)
- Verify StreamingFuncInfo is smaller (no lifetime parameter)

### Phase 3 (reduce streaming overhead)

- Filetests pass
- Verify GlslCompiler reuse works (FunctionBuilderContext reset between uses)
- Verify Context reuse works (clear_context between uses)
- Cranelift signatures are correct when rebuilt from GLSL signatures

## Measurement

Run heap trace after each phase to measure incremental improvement.
Compare against the current baseline (219,125 bytes peak).
