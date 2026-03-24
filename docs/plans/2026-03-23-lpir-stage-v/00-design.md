# Stage V: LPIR → WASM Emission — Design

## Scope

Rewrite the WASM emitter to consume `IrModule` instead of `naga::Module`.
Q32 mode only. Delete the old Naga-direct emitter entirely. The emitter
is purely mechanical: each Op maps to one or a small number of WASM
instructions.

## File structure

### Deletions

```
lp-glsl-wasm/src/emit.rs      # OLD: 1970-line Naga-direct emitter
lp-glsl-wasm/src/emit_vec.rs   # OLD: vector lowering (LPIR is scalarized)
lp-glsl-wasm/src/locals.rs     # OLD: complex local allocation
lp-glsl-wasm/src/lpfx.rs       # OLD: LPFX resolution from Naga
lp-glsl-wasm/src/types.rs      # OLD: Naga type → WASM type mapping
```

### New structure

```
lp-glsl-wasm/src/
  emit/
    mod.rs          # emit_module(IrModule, WasmOptions) → Vec<u8>
    func.rs         # per-function: local declaration, prologue/epilogue
    ops.rs          # Op → WASM instruction(s) dispatch
    q32.rs          # Q32 inline expansion (add_sat, sub_sat, mul, div, etc.)
    control.rs      # structured control flow (if, loop, switch, break, continue)
    memory.rs       # shadow stack, slot_addr, load, store, memcpy
    imports.rs      # @std.math + @lpfx → builtins module resolution
  lib.rs            # UPDATE: new public API, add lpir dep
  module.rs         # KEEP: WasmModule, WasmExport (update to use IrModule metadata)
  options.rs        # KEEP: WasmOptions
```

### Dependency changes (`Cargo.toml`)

Add:
- `lpir = { path = "../lpir" }`

Keep:
- `lp-glsl-naga` (for `compile()`, `NagaModule`, `FloatMode`)
- `lp-glsl-builtin-ids` (for `BuiltinId`, name resolution)
- `naga` (transitive via lp-glsl-naga, but direct dep can be removed if
  not needed after old emitter deletion)
- `wasm-encoder`

## Conceptual architecture

```
GLSL source
  │
  ▼
compile(glsl) ──────────── lp-glsl-naga (existing)
  │
  ▼
NagaModule
  │
  ▼
lower(&NagaModule) ─────── lp-glsl-naga::lower (Stage IV)
  │
  ▼
IrModule
  │
  ▼
emit_module(&IrModule, &WasmOptions) ── emit/mod.rs (new)
  │
  ├─ collect all imports ──────────────── emit/imports.rs
  │   ├─ @std.math::sin → builtins::__lp_q32_sin
  │   ├─ @lpfx::lpfx_hash1 → builtins::__lpfx_hash_1
  │   └─ allocate WASM import indices
  │
  ├─ build WASM type section
  ├─ build WASM import section (builtins + env.memory when needed)
  ├─ build WASM function section
  ├─ build WASM export section
  │
  ├─ for each IrFunction: ────────────── emit/func.rs
  │   ├─ declare locals (VReg types → WASM locals)
  │   ├─ shadow stack prologue (if slots)
  │   ├─ emit body ops ───────────────── emit/ops.rs
  │   │   ├─ arithmetic → Q32 inline ── emit/q32.rs
  │   │   ├─ control flow ────────────── emit/control.rs
  │   │   ├─ memory ops ──────────────── emit/memory.rs
  │   │   └─ calls → import/func index
  │   ├─ shadow stack epilogue (if slots)
  │   └─ WASM End
  │
  ▼
Vec<u8> (WASM binary)
```

## Key design decisions

### VReg → WASM local mapping

VReg N maps directly to WASM local N. Parameters are the first
`param_count` VRegs (WASM convention: params are the first locals).
Non-parameter VRegs are declared as function-local variables.

Type mapping (Q32 mode):
- `IrType::F32` → `ValType::I32` (Q16.16 fixed-point)
- `IrType::I32` → `ValType::I32`

Additional scratch locals per function:
- One `i64` local for Q32 widening/saturation (only when float ops exist)

### Q32 arithmetic expansion

LPIR float ops expand to Q16.16 integer arithmetic:

| LPIR Op | Q32 WASM expansion |
|---------|--------------------|
| `Fadd` | i64 extend both → i64.add → saturate → i32.wrap |
| `Fsub` | i64 extend both → i64.sub → saturate → i32.wrap |
| `Fmul` | i64 extend both → i64.mul → i64.shr_s 16 → saturate → i32.wrap |
| `Fdiv` | numerator i64 extend → shl 16 → i64.div_s → i32.wrap |
| `Fneg` | 0 - src (i32.sub) |
| `Fabs` | call `builtins::__lp_q32_abs` or inline (compare + negate) |
| `Fsqrt` | call `builtins::__lp_q32_sqrt` |
| `Fmin` | inline: compare + select |
| `Fmax` | inline: compare + select |
| `Ffloor` | inline: mask off fractional bits (and 0xFFFF0000), adjust for negative |
| `Fceil` | inline: floor + conditional add 0x10000 |
| `Ftrunc` | inline: toward-zero truncation |
| `Fnearest` | call `builtins::__lp_q32_roundeven` |
| `FconstF32` | `i32.const (value * 65536.0) as i32` (clamped) |
| `FtoiSatS` | Q32 → int: shr_s 16, clamp |
| `FtoiSatU` | Q32 → uint: shr_u 16, clamp |
| `ItofS` | int → Q32: shl 16, clamp |
| `ItofU` | uint → Q32: shl 16, clamp |
| `Feq/Fne/Flt/...` | `i32.eq`/`i32.ne`/`i32.lt_s`/... (Q32 values are ordered as i32) |

Integer ops (`Iadd`, `Imul`, etc.) emit directly as `i32.*`.

### Q32 saturation helper

```
fn emit_q32_sat(wasm_fn):
    // i64 value on stack → clamp to [Q32_MIN, Q32_MAX] → i32.wrap
    local.tee $i64_scratch
    i64.const Q32_MIN  // -2147483648 (i32::MIN as i64)
    i64.lt_s
    if
        i32.const i32::MIN
    else
        local.get $i64_scratch
        i64.const Q32_MAX  // 2147483647 (i32::MAX as i64)
        i64.gt_s
        if
            i32.const i32::MAX
        else
            local.get $i64_scratch
            i32.wrap_i64
        end
    end
```

### Control flow mapping

| LPIR | WASM |
|------|------|
| `IfStart` | `local.get cond`, `if (blocktype)` |
| `Else` | `else` |
| `End` (if) | `end` |
| `LoopStart` | `block { loop { block {` (3-construct pattern) |
| `End` (loop body inner block) | `end` (close inner block) |
| continuing section | emitted between inner block end and loop end |
| `End` (loop) | `br 0` (back to loop top), `end` (loop), `end` (outer block) |
| `Break` | `br N` (to outer block) |
| `Continue` | `br N` (to inner block end → falls into continuing) |
| `BrIfNot` | `local.get cond`, `i32.eqz`, `br_if N` (to outer block = break) |
| `SwitchStart` | nested WASM blocks + `br_table` |
| `Return` | load return values, `return` |

The emitter maintains a depth counter and a control stack to compute
`br` target depths, similar to the current `EmitCtx`.

### Import resolution

All `@std.math::*` and `@lpfx::*` imports map to the `builtins` WASM
module for Q32 mode:

- `@std.math::sin` → `builtins::__lp_q32_sin` (i32 → i32)
- `@std.math::cos` → `builtins::__lp_q32_cos` (i32 → i32)
- `@std.math::pow` → `builtins::__lp_q32_pow` (i32, i32 → i32)
- `@lpfx::lpfx_hash1` → `builtins::__lpfx_hash_1` (i32, i32 → i32)
- etc.

The `imports.rs` module:
1. Walks all `ImportDecl` in the `IrModule`
2. Maps each to a `BuiltinId` by name matching
3. Looks up the Q32 WASM signature
4. Produces the WASM import section entries
5. Returns a mapping: LPIR `CalleeRef` → WASM function index

When any import maps to `builtins`, `env.memory` is also imported
(required by the builtins WASM module).

### Shadow stack

Functions with `slots` need linear memory for `SlotAddr`/`Load`/`Store`:

- A mutable WASM global `$sp` (i32) is declared
- Functions with slots emit a prologue:
  ```
  global.get $sp
  i32.const frame_size
  i32.sub
  global.set $sp
  ```
- `SlotAddr { slot }` → `global.get $sp`, `i32.const slot_offset`,
  `i32.add`
- `Load { base, offset }` → `local.get base`, `i32.load offset=offset`
- `Store { base, offset, value }` → `local.get value`, `local.get base`,
  `i32.store offset=offset`
- Epilogue before return: `global.get $sp`, `i32.const frame_size`,
  `i32.add`, `global.set $sp`

The `$sp` global and `env.memory` import are only emitted when at least
one function has slots.

### Public API

```rust
pub fn glsl_wasm(source: &str, options: WasmOptions) -> Result<WasmModule, GlslWasmError> {
    let naga_module = lp_glsl_naga::compile(source)?;
    let ir_module = lp_glsl_naga::lower::lower(&naga_module)
        .map_err(|e| GlslWasmError::Codegen(e.to_string()))?;
    let wasm_bytes = emit::emit_module(&ir_module, &options)
        .map_err(GlslWasmError::Codegen)?;
    let exports = collect_exports(&ir_module, &naga_module, &options);
    Ok(WasmModule { bytes: wasm_bytes, exports })
}
```

The `WasmExport` metadata still needs GLSL-level type info (return type,
param types) for the filetest runner's `GlslExecutable` impl. This comes
from the `NagaModule::functions` metadata, not from the `IrModule`.
