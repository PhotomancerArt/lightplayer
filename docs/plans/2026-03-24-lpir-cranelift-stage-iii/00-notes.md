# Stage III: Builtins, Imports, and Q32 Emission — Notes

## Scope of work

Add import resolution, builtin declaration, Q32 float mode, and LPFX support
to `lpir-cranelift`. After this stage, the emitter can compile real shaders
in Q32 mode (with hand-built LPIR — GLSL source compilation is Stage IV).

Note: memory ops and local function calls are now in Stage II. Stage III is
purely about:
- Import resolution (LPIR `ImportDecl` → `BuiltinId` → Cranelift func ref)
- Builtin declaration and JIT symbol lookup
- Q32 float mode (type mapping, op remapping, constant encoding)
- Q32-specific inline ops (fneg, fabs, fmin, fmax, floor, ceil, trunc)
- Tests proving Q32 arithmetic and builtin calls work end-to-end

## Current state

### BuiltinId (from Stage I)

Self-describing enum in `lp-glsl-builtin-ids`. Variants like `LpGlslSinQ32`,
`LpLpirFaddQ32`, `LpLpfxFbm2Q32`. Methods: `name()` → symbol string,
`module()` → `Module::{Glsl,Lpir,Lpfx}`, `fn_name()` → logical name,
`mode()` → `Option<Mode::{Q32,F32}>`.

### Import resolution (existing, shared)

`glsl_builtin_mapping.rs` in `lp-glsl-builtin-ids` provides:
- `glsl_q32_math_builtin_id(name, arg_count) → Option<BuiltinId>` — glsl module
- `lpir_q32_builtin_id(name, arg_count) → Option<BuiltinId>` — lpir module
- `glsl_lpfx_q32_builtin_id(base, &[GlslParamKind]) → Option<BuiltinId>` — lpfx

The WASM emitter's `resolve_builtin_id` dispatches on `ImportDecl.module_name`
and calls these. The Cranelift emitter needs the same logic.

### Q32 builtins that exist (6 ops)

Only these LPIR float ops have `__lp_lpir_*_q32` builtins:
- `fadd`, `fsub`, `fmul`, `fdiv` — saturating Q16.16 arithmetic
- `fsqrt` — fixed-point square root
- `fnearest` — fixed-point round-to-even

### Float ops without builtins (inlined in old crate)

These are handled as inline Q32 integer ops:
- `fneg` → `ineg` (negate the i32)
- `fabs` → `select(icmp_imm(sge, v, 0), v, ineg(v))`
- `fmin` → `select(icmp(sle, a, b), a, b)`
- `fmax` → `select(icmp(sge, a, b), a, b)`
- `ffloor` → `band(v, !0xFFFF)` (mask out fractional bits, round toward −∞
  needs sign handling)
- `fceil` → floor + conditional add ONE
- `ftrunc` → mask fractional bits toward zero (sign-aware)

### Q32 type mapping

In Q32 mode, `IrType::F32` maps to Cranelift `I32` (not `F32`). The fixed-point
value is stored as a signed Q16.16 integer. This affects:
- Function signatures (params and returns)
- VReg type declarations
- Constant encoding (`Fconst 1.5` → `iconst 98304` which is `1.5 * 65536`)
- Comparison ops (`fcmp` → `icmp` since values are integers)
- Cast ops (`FtoiSat`, `Itof` change meaning)

### Q32 constant encoding

Q16.16 format: `value_i32 = (f64_value * 65536.0).round() as i32` with
saturation to `[i32::MIN, 0x7FFF_FFFF]`. The `Q32` struct in
`lp-glsl-builtins` has `from_f32` (truncating) and test helpers use a
rounding+saturating variant. No `const fn` available.

### Old crate's declare_builtins + symbol_lookup

1. `declare_builtins`: iterates `BuiltinId::all()`, filters by float mode,
   declares each as `Linkage::Import` with derived signature
2. `symbol_lookup_fn`: closure that matches symbol name → `get_function_pointer`
   which is a big `match BuiltinId → fn as *const u8`
3. Set on `JITBuilder` before `JITModule::new`

### What Stage II provides (assumed complete)

- `EmitCtx` with `func_refs`, `slots`, `ir`, `pointer_type`
- `emit/` module structure: `mod.rs`, `scalar.rs`, `control.rs`, `memory.rs`,
  `call.rs`
- Structured control flow, switch, memory ops, local calls all working
- `seal_all_blocks()` strategy

## Questions

### Q1: Call builtins for ALL Q32 float ops, or inline the simple ones?

**Context**: The old crate inlines fneg/fabs/fmin/fmax/floor/ceil/trunc as
integer ops in Q32 mode, and only calls builtins for saturating add/sub/mul/div
plus sqrt and fnearest. There are no `__lp_lpir_*_q32` builtins for the
inlined ops.

Options:
- (a) **Create new builtins** for all float ops (fneg, fabs, etc.) so the
  emitter calls builtins for everything. Simplest emitter, but creates new
  builtin functions that don't exist yet, and adds call overhead.
- (b) **Inline the simple Q32 ops** in the emitter (same as old crate),
  call builtins only for the 6 that exist. More emitter code but matches
  proven behavior.
- (c) **Builtins for everything as a first pass**, then inline as optimization
  later.

**Answer**: (b) — inline the simple ops. Rule of thumb: inline if <=~10
instructions. fneg is 1 inst (ineg), fabs is 3 (icmp+select+ineg), fmin/fmax
are 2 (icmp+select), floor/ceil/trunc are ~4-6 (shifts+masks). All well within
the threshold. Only call builtins for the 6 that exist (saturating
add/sub/mul/div, sqrt, fnearest).

### Q2: Where should Q32 constant encoding live?

**Context**: The emitter needs to convert `Fconst(1.5f32)` to `iconst(98304i32)`
at compile time. The `Q32::from_f32` in `lp-glsl-builtins` is not `const` and
lives in a different crate. Options:

- (a) Depend on `lp-glsl-builtins` for `Q32::from_f32`. Heavy dependency
  for one function.
- (b) Add a small `q32_encode(f32) -> i32` function directly in `lpir-cranelift`.
  Self-contained.
- (c) Add it to `lp-model` or a shared util crate.

**Answer**: (b) — small inline function in `lpir-cranelift`. It's
`((value as f64) * 65536.0).round() as i32` with saturation. Extract later
if needed.

### Q3: Should `lpir-cranelift` depend on `lp-glsl-builtin-ids` directly?

**Context**: The crate needs to resolve LPIR imports to BuiltinId, then
declare them as Cranelift imports and set up symbol lookup. The WASM emitter
depends on `lp-glsl-builtin-ids` directly.

**Answer**: Yes, direct dependencies on both `lp-glsl-builtin-ids` and
`lp-glsl-builtins`. Same pattern as the WASM emitter.

### Q4: Where does `FloatMode` live?

**Context**: The emitter needs a `FloatMode` enum to decide between native f32
emission and Q32 emission. The old crate has one, but it's coupled to the old
API.

Options:
- (a) Define `FloatMode` in `lpir-cranelift`. Keep it simple.
- (b) Put it in `lpir` crate (shared with interpreter, WASM emitter).
- (c) Put it in `lp-glsl-builtin-ids` (where `Mode` already exists).

**Answer**: Move `FloatMode` into the `lpir` crate (`types.rs`), rename
`Float` → `F32` for consistency. Re-export from `lpir::FloatMode`. Update
`lp-glsl-naga` and `lp-glsl-wasm` to `use lpir::FloatMode`. The `lpir` crate
is the natural home — it describes how consumers interpret `IrType::F32`.

### Q5: Q32 comparisons — fcmp becomes icmp?

**Context**: In Q32 mode, float values are I32 (Q16.16). Float comparison ops
(`Feq`, `Flt`, etc.) should use `icmp` (signed integer comparison) instead of
`fcmp`, since the values are integers and Q16.16 preserves ordering.

**Answer**: Yes. Same as old Cranelift crate. `Feq` → `icmp(Equal)`,
`Flt` → `icmp(SignedLessThan)`, etc. Q16.16 preserves ordering.

### Q6: FtoiSat and Itof in Q32 mode?

**Context**: In native f32 mode, `FtoiSatS` converts `f32 → i32` (saturating).
In Q32 mode, the "float" is already an I32 (Q16.16). What should these do?

- `FtoiSatS { dst, src }` in Q32: convert Q16.16 → integer. This is
  `src >> 16` (arithmetic shift right by 16) since the integer part is in
  the upper 16 bits.
- `ItofS { dst, src }` in Q32: convert integer → Q16.16. This is
  `src << 16` (shift left by 16).

**Answer**: Follow the old crate's proven Q32Strategy implementations:
- `emit_to_sint`: negative-biased round-toward-zero (not plain `sshr`);
  adds `(1 << 16) - 1` bias for negatives before `sshr 16`. ~6 instructions.
- `emit_to_uint`: `emit_to_sint` then clamp negatives to 0.
- `emit_from_sint`: clamp to [-32768, 32767] then `ishl 16`. ~5 instructions.
- `emit_from_uint`: `ishl 16` with size-dependent extend/clamp.
All inline, no builtins needed. Port the existing `Q32Strategy` methods.
