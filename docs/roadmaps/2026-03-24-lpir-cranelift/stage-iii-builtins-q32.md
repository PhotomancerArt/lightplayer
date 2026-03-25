# Stage III: Builtins, Imports, and Q32 Emission

## Goal

Add import resolution, builtin declaration, Q32 float mode, memory ops,
and LPFX support. After this stage, the emitter can compile real shaders
in Q32 mode.

## Suggested plan name

`lpir-cranelift-stage-iii`

## Scope

**In scope:**
- Import resolution: LPIR `ImportDecl` → `BuiltinId` → Cranelift func ref
  - `glsl::sin` + Q32 → `__lp_glsl_sin_q32` → declared import, resolved
    via symbol lookup
  - `lpfx::fbm2` + Q32 → `__lp_lpfx_fbm2_q32` → same path
  - Resolution logic shared with WASM emitter (from Stage I's BuiltinId)
- Builtin declaration in JIT module:
  - Iterate relevant BuiltinIds for current float mode
  - Declare as imports with Cranelift signatures
  - Symbol lookup function providing `lp-glsl-builtins` function pointers
- Q32 float mode:
  - LPIR float ops (`Fadd`, `Fmul`, `Fdiv`, etc.) → calls to
    `__lp_lpir_<op>_q32` builtins instead of native CLIF instructions
  - LPIR float params/returns: `IrType::F32` → Cranelift `I32` in Q32 mode
  - LPIR `Fconst` → Q32-encoded i32 constant
  - Math calls (`glsl::sin`) → `__lp_glsl_sin_q32`
- Memory ops:
  - `Op::SlotAddr` → Cranelift stack slot address
  - `Op::Load` / `Op::Store` → Cranelift load/store from slot
  - `Op::Memcpy` → byte-level copy
  - Stack slot declarations from `IrFunction.slots`
- `Op::Call` for local (intra-module) function calls
- `Op::MathCall` if present — route through BuiltinId resolution
- Tests: hand-built LPIR with Q32 ops, builtin calls, memory ops
  → JIT compile → call → verify results match Q32 semantics

**Out of scope:**
- `jit()` from GLSL source (Stage IV)
- Level 1 typed call interface (Stage IV)
- GlslMetadata (Stage IV)
- Filetest integration (Stage V2)
- Native f32 builtin resolution (future — Q32 only for now)

## Key decisions

- Q32 mode changes the type mapping: `IrType::F32` → Cranelift `I32`
  (not `F32`). The emitter must track this per-VReg.
- For Q32 arithmetic, every LPIR float op becomes a call to the
  corresponding `__lp_lpir_*_q32` builtin. This is the "saturating"
  strategy from the old compiler.
- Import resolution uses the shared `BuiltinId` machinery from Stage I.
  The emitter doesn't contain symbol name strings — it asks BuiltinId.
- The symbol lookup function for JIT follows the same pattern as the old
  crate: iterate BuiltinId variants, match by name, return function
  pointer from `lp-glsl-builtins`.

## Open questions

- **Which `__lp_lpir_*_q32` builtins exist?**: The old compiler inlined
  some Q32 ops (add, sub, mul as i64 widen+op+saturate) and called builtins
  for others (div, sin, sqrt). For the new crate with Q32 saturating
  strategy, do we call builtins for ALL float ops (simplest) or inline
  the simple ones (add, sub, mul, neg)? Calling builtins for everything is
  simpler but may be slower. The old compiler's approach of inlining
  simple ops was a meaningful optimization. But: the goal is validation
  first, optimization later.
- **Q32 constant encoding**: `Fconst(1.5)` in Q32 → `iconst(Q32::from_f64(1.5))`.
  The Q32 encoding function needs to be available at compile time (in the
  emitter). Where does it live? Probably `lp-model` or a small shared util.
- **Signature construction for builtins**: The old crate generated
  `signature_for_builtin` in `registry.rs`. The new crate needs equivalent
  signature info. Should BuiltinId carry signature metadata, or should the
  emitter derive it from LPIR `ImportDecl.param_types`/`return_types`?
  Using LPIR's declared types is simpler and avoids signature duplication.
- **LPFX `lpfx_glsl_params` metadata**: The WASM emitter uses this to
  resolve LPFX import overloads (`glsl_lpfx_q32_builtin_id` needs GLSL
  param kinds, not flat IrTypes). The Cranelift emitter needs the same
  resolution. Confirm this metadata survives through the LPIR module.
- **Local function calls**: LPIR `Op::Call` with `CalleeRef` pointing to
  a local function needs the Cranelift `FuncRef` for that function. The
  emitter needs a pre-pass to declare all functions before emitting any,
  since function A can call function B and vice versa.

## Deliverables

- Import resolution: LPIR imports → BuiltinId → Cranelift func refs
- Q32 emission: float ops → builtin calls, correct type mapping
- Memory ops: slots, load, store
- Builtin declaration and symbol lookup for JIT
- Tests with Q32 arithmetic and builtin calls

## Dependencies

- Stage I (builtin naming) — BuiltinId must be in its final form
- Stage II (emitter core) — scalar ops and CF translation

## Estimated scope

~400–600 lines of builtin/import/Q32 code + ~200 lines of tests.
