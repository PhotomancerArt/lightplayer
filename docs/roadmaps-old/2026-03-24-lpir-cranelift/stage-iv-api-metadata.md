# Stage IV: JitModule API, GlslMetadata, and Compiler Orchestration

## Goal

Build the public compiler API (`jit()`, `jit_from_ir()`), the Level 1
typed call interface (`GlslQ32`, `GlslReturn`), the Level 3 direct call
interface, and GlslMetadata extraction. After this stage, the crate is a
complete compiler: GLSL in, callable module out.

## Suggested plan name

`lpir-cranelift-stage-iv`

## Scope

**In scope:**
- `compile.rs` — full pipeline orchestration:
  - GLSL → Naga (via naga's GLSL frontend)
  - Naga → LPIR (via `lps-naga::lower`)
  - Drop Naga module after lowering
  - Lower IrFunctions → CLIF one at a time, biggest first
  - Drop each IrFunction after defining in Cranelift module
  - Finalize → JitModule
- `GlslMetadata`:
  - Extracted during Naga → LPIR lowering
  - Per-function: name, GLSL-typed params with in/out/inout qualifiers,
    GLSL return type
  - Stored in JitModule alongside compiled code
  - Update `lps-naga` lowering to produce this metadata
- `values.rs` — typed value types:
  - `GlslQ32` enum: `Float(f64)`, `Vec2(f64, f64)`, `Vec3(..)`, `Vec4(..)`,
    `Int(i32)`, `IVec2(..)`, etc.
  - `GlslF32` enum: same variants (for future f32 mode)
  - `GlslReturn<V> { value: Option<V>, outs: Vec<V> }`
  - `CallResult<V> = Result<GlslReturn<V>, CallError>`
- Level 1 call interface:
  - `module.call(name, &[GlslQ32]) -> CallResult<GlslQ32>`
  - Uses GlslMetadata to:
    - Scalarize vector args (Vec2 → 2 scalars)
    - Encode Q32 (f64 → i32 fixed-point)
    - Set up out-param memory (allocate slots, pass pointers)
    - Call the compiled function
    - Read back out/inout param values from memory
    - Reassemble scalar results into GLSL-typed values
    - Decode Q32 (i32 → f64)
- Level 3 direct call interface:
  - `module.direct_call(name) -> Option<DirectCall>`
  - `DirectCall { call: fn(*const u32, *mut u32), param_count, return_count }`
  - Abstracts calling convention (struct-return handled internally)
- `lib.rs` public API:
  - `pub fn jit(source: &str, options: CompileOptions) -> Result<JitModule>`
  - `pub fn jit_from_ir(ir: &IrModule, options: CompileOptions) -> Result<JitModule>`
  - `pub struct CompileOptions { float_mode: FloatMode }`
- Tests: GLSL source → `jit()` → `call()` → verify results

**Out of scope:**
- Filetest integration (Stage V2)
- Embedded readiness (Stage VI-A)
- lp-engine migration / fw-emu (Stage VI-B)
- Object/emulator emission (Stage V1)
- Level 2 mode-agnostic call interface (deferred)

## Key decisions

- Memory-conscious compilation: the orchestrator sorts functions by size
  (biggest first), lowers each to CLIF individually, and drops the
  IrFunction after `define_function`. Peak memory = largest IrFunction +
  its CLIF.
- GlslMetadata is extracted during `lps-naga::lower()`. This requires
  a small update to the lowering API — either a new return type
  `(IrModule, GlslMetadata)` or metadata attached to IrModule.
- The Level 3 `DirectCall` wraps the raw function pointer with a thin
  trampoline or Rust closure that handles struct-return mechanics. The
  caller sees a flat `(args_ptr, results_ptr)` interface.
- Q32 encoding/decoding for Level 1 uses f64 as the interchange format
  to preserve precision during round-trips (Q32 has more integer bits
  than f32 mantissa).

## Open questions

- **GlslMetadata location**: Should it live in the `lpir` crate (since
  it's metadata about LPIR functions), in `lps-naga` (since that's
  where it's extracted), or in the new `lpir-cranelift` crate (since
  that's the primary consumer)? It's also needed by the WASM emitter's
  test harness and potentially by filetests. Probably a shared location
  like `lpir` or a small new crate.
- **IrModule ownership for per-function draining**: To drop functions
  after lowering, we need ownership transfer. Options:
  `ir_module.functions.drain(..)`, `Vec<Option<IrFunction>>` with take,
  or `into_iter()`. The emitter needs import declarations to persist
  (for resolving CalleeRefs) while functions are dropped. May need to
  split IrModule into `{ imports, functions }` where functions is
  consumable.
- **DirectCall trampoline**: How to abstract calling convention? Options:
  (a) JIT-compile a tiny trampoline alongside the shader, (b) Rust
  `unsafe fn` that knows the ABI and does the struct-return dance,
  (c) Cranelift's `cranelift-jit` may already handle struct-return
  transparently if we set up the signature right. Need to investigate
  what Cranelift does automatically.
- **Naga dependency**: The `jit()` function takes GLSL source, so the
  crate depends on `naga` (for parsing) and `lps-naga` (for LPIR
  lowering). These are `std`-only (naga requires `std`). On ESP32,
  `jit_from_ir()` would be used with pre-lowered LPIR — but currently
  the firmware compiles from GLSL source. How does Naga work on
  `no_std`? If it doesn't, the ESP32 path needs GLSL→LPIR to happen
  on the host (pre-compilation), or Naga needs to work on ESP32. This
  is how the old compiler works too (naga runs on device), so probably
  fine, but worth verifying.
- **Error types**: `CompileError` wraps Naga parse errors, LPIR lowering
  errors, and Cranelift codegen errors. Should error messages include
  source locations? The old compiler had `GlSourceLoc` for this. Worth
  threading through or deferring.

## Deliverables

- `jit()` and `jit_from_ir()` public API
- GlslMetadata extraction in `lps-naga`
- Level 1 typed call interface with `GlslQ32`
- Level 3 direct call interface with `DirectCall`
- Memory-conscious per-function compilation
- End-to-end test: GLSL source → Q32 JIT → typed call → correct results

## Dependencies

- Stage III (builtins, Q32 emission) — emitter must handle real shaders
- Stage I (builtin naming) — BuiltinId and import module names finalized

## Estimated scope

~500–700 lines (compile orchestration, values, metadata, API) + ~200
lines of tests. Plus ~100 lines of changes to `lps-naga` for
metadata extraction.
