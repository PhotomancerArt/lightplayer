# Stage V: Filetest Integration (jit.q32)

## Goal

Add `jit.q32` as a filetest target. Run the existing scalar filetest
corpus through the new LPIR→Cranelift pipeline on the host CPU. This is
the primary correctness validation gate.

## Suggested plan name

`lpir-cranelift-stage-v`

## Scope

**In scope:**
- Add `Backend::Jit` (or similar) variant to filetest target system
- Add `jit.q32` to `DEFAULT_TARGETS` or as a selectable `--target`
- Implement `compile_for_target` dispatch for the new backend:
  - GLSL source → `lpir_cranelift::jit(source, Q32)` → JitModule
  - JitModule needs to satisfy whatever interface filetests use to call
    functions and check results
- Bridge between filetest execution and the new crate's call interface:
  - Filetests currently use `GlslExecutable` from the old crate
  - Either: implement a compatibility adapter, or update the filetest
    runner to use the new crate's API directly
  - The Level 1 typed call interface (`GlslQ32`, `GlslReturn`) should
    map naturally to filetest expectations
- Run all scalar filetests against `jit.q32`:
  - `filetests/scalar/arithmetic/`
  - `filetests/scalar/bool/`
  - `filetests/scalar/builtins/`
  - `filetests/scalar/lpfx/`
  - Debug shaders (rainbow.glsl)
- Triage failures:
  - LPIR lowering gaps → fix in `lp-glsl-naga`
  - Emitter bugs → fix in `lpir-cranelift`
  - Expected differences (Q32 precision) → annotate
- Annotations: add `// @unimplemented(backend=jit)` or similar for any
  tests that use features not yet supported

**Out of scope:**
- `rv32.q32` target (Stage VI)
- lp-engine migration (Stage VI)
- Vector filetests (LPIR is scalarized, vector tests are future work)
- `cranelift.q32` removal (Stage VII)

## Key decisions

- The filetest runner currently couples to `GlslExecutable` from the old
  crate. The cleanest path is to update the runner to support both
  interfaces (old for `cranelift.q32`, new for `jit.q32`) during the
  transition. Or: if we're comfortable dropping `cranelift.q32` from
  default targets at this point, just switch to the new interface.
- `jit.q32` runs on the host CPU. This is much faster than the RISC-V
  emulator. Filetest CI time should improve noticeably.
- The `approximate` (`~=`) comparisons in filetests need to work with
  Q32 values from the new pipeline. Verify tolerance handling.

## Open questions

- **Filetest `GlslExecutable` coupling**: The filetest runner uses
  `GlslExecutable` as the common trait for both cranelift and wasm
  runners. The wasm runner (`wasm_runner.rs`) also implements it. If we
  change the runner to use the new crate's API for `jit.q32`, the wasm
  runner still needs the old trait (or its own interface). Options:
  (a) Keep `GlslExecutable` for wasm, add new path for jit — messy.
  (b) Define a minimal filetest-internal trait that both satisfy.
  (c) Rework the wasm runner to also use a new common interface.
  Need to assess how much the filetest runner depends on `GlslExecutable`
  specifics vs just "call function, get values."
- **Default targets**: Should `jit.q32` replace `cranelift.q32` in
  `DEFAULT_TARGETS`, or run alongside it? Running both doubles test time
  but validates the new path against the old. Running only `jit.q32`
  requires confidence in the new path. Probably: add alongside initially,
  remove old once stable.
- **Annotation backend names**: Current annotations use
  `backend=cranelift` and `backend=wasm`. What's the new backend name?
  `jit`? `lpir-cranelift`? Should match the target name prefix: `jit`.
- **Test failures from Naga lowering gaps**: If the Naga→LPIR lowering
  doesn't support a construct that existing filetests use, those tests
  will fail on `jit.q32`. This is expected and is how we discover
  lowering gaps. But it means `jit.q32` may start with fewer passing
  tests than `cranelift.q32`. Track and fix incrementally.
- **Performance comparison**: This is a good stage to start informal
  benchmarks: filetest suite runtime on `jit.q32` (host JIT) vs
  `cranelift.q32` (RV32 emulator). The host JIT should be significantly
  faster. Note: this measures test execution speed, not embedded
  performance — that's Stage VI.

## Deliverables

- `jit.q32` filetest target, working with `--target jit.q32`
- Majority of scalar filetests passing
- Gap analysis: which tests fail and why (lowering gaps vs emitter bugs)
- Any necessary `lp-glsl-naga` fixes for lowering gaps discovered

## Dependencies

- Stage IV (compiler API) — `jit()` must return a callable JitModule
- Stages I–III — builtins, Q32, and core emitter must be complete

## Estimated scope

~300 lines of filetest integration code + ~100 lines of annotations/fixes.
Bug fixing in `lp-glsl-naga` and `lpir-cranelift` is variable — depends
on how many gaps surface.
