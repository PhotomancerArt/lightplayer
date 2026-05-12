# Phase 8 Design: Rainbow shader end-to-end

Plan reference: `2026-03-17-glsl-wasm-part-iii.md` Phase 8

## Goals

1. Compile rainbow.shader via glsl_wasm(); fix remaining compilation errors
2. Execute all functions via wasmtime; compare output to Cranelift Q32
3. Write integration test: compile rainbow, verify main() output for sample inputs
4. Remove @unimplemented(backend=wasm) from filetests that now pass
5. Run full filetest suite; verify no cranelift.q32 regressions

---

## 1. rainbow.shader location and structure

**Find:** `rainbow.shader` (or rainbow.glsl) in the repo. Likely in examples/, lp-shader/examples/,
or filetests.

**Structure:** Main function, user functions, builtins used (clamp, mix, smoothstep, etc.), LPFX (
psrdnoise?), vectors, control flow. Phases 1–7 implement these.

---

## 2. Compilation

**Action:** Run `glsl_wasm(rainbow_source, options)`. Fix any errors. Likely remaining: edge cases,
missing builtin overload, type coercion, etc.

**Systematic:** Add a test that compiles rainbow.shader. It should succeed after Phase 7. If it
fails, fix the reported error.

---

## 3. Execution and comparison

**Cranelift path:** lps has execute_main or similar that runs a function with given inputs (
fragCoord, outputSize, time) and returns pixel value.

**WASM path:** WasmExecutable.run("main", args) or equivalent. Args: vec of i32 (or whatever main
expects). Compare output to Cranelift for same inputs.

**Sample inputs:** e.g. (0,0), (100,100) for fragCoord; (256,256) for outputSize; 0.0, 1.0, 2.5 for
time. A few combinations.

---

## 4. Integration test

**Placement:** lps-wasm/tests/ or lps-filetests.

**Steps:**

1. Load rainbow.shader source
2. Compile with glsl_wasm
3. Instantiate with WasmExecutable (or wasmtime)
4. For each sample (fragCoord, outputSize, time), call main
5. Optionally compare to Cranelift output
6. Assert no panic, reasonable output (e.g. vec4 in expected range)

---

## 5. Remove @unimplemented annotations

**Process:** Run filetests with --target wasm.q32. For each test that now passes, remove
`@unimplemented(backend=wasm)` if present. The runner logic: if test passes and has the annotation,
we remove it. Manual or scripted.

**Careful:** Only remove when test actually passes. Don't remove prematurely.

---

## 6. Regression check

**Command:** `scripts/filetests.sh` or equivalent, default target (cranelift.q32).

**Expect:** Same pass count as before. No new failures.

---

## File change summary

| File                                  | Changes                                                |
| ------------------------------------- | ------------------------------------------------------ |
| rainbow.shader (if needed)            | Fix any source issues                                  |
| lps-wasm                              | Bug fixes for rainbow compilation                      |
| lps-wasm/tests/rainbow_integration.rs | New integration test                                   |
| filetests/\*.glsl                     | Remove @unimplemented(backend=wasm) from passing tests |

---

## Validation

- rainbow.shader compiles
- Integration test passes
- Full filetest suite: wasm.q32 pass count increased, cranelift.q32 unchanged
