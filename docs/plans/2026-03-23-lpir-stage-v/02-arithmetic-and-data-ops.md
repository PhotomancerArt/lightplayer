# Phase 2: Arithmetic + Data Ops (Float Mode Stub)

## Scope

Implement the core op dispatch in `ops.rs` for all non-control-flow,
non-call, non-memory ops. Use a simple "float mode" dispatch where float
ops emit native `f32.*` instructions as a placeholder — Q32 expansion
comes in Phase 3. This lets us test the basic structure with float mode
before layering Q32 on top.

Actually — since we decided Q32-only, and float mode is out of scope,
we implement Q32 directly. But the integer ops are mode-independent and
can be done first.

Revised: implement all integer ops and constant ops. Float ops are
stubbed with `todo!()` markers for Phase 3.

## Implementation

### `emit/func.rs` — function skeleton

```rust
pub(crate) fn emit_function(
    ir: &IrModule,
    func: &IrFunction,
    wasm_fn: &mut wasm_encoder::Function,
    import_count: u32,
    options: &WasmOptions,
) -> Result<(), String>
```

1. Walk `func.body` linearly, dispatching each `Op` to `ops::emit_op`.
2. Emit WASM `End` at the end of the function.

Local declaration:
- For each VReg beyond `param_count`, declare a WASM local with the
  appropriate type (`ValType::I32` for both `IrType::F32` and `IrType::I32`
  in Q32 mode).
- If any float ops exist, declare one extra `i64` local for Q32 scratch.

### `emit/ops.rs` — dispatch

```rust
pub(crate) fn emit_op(
    ctx: &mut EmitCtx,
    func: &IrFunction,
    op: &Op,
    wasm_fn: &mut wasm_encoder::Function,
) -> Result<(), String>
```

For each op, the pattern is:
```
local.get <operand(s)>
<WASM instruction(s)>
local.set <dst>
```

### Integer arithmetic — direct 1:1

| LPIR Op | WASM |
|---------|------|
| `Iadd` | `i32.add` |
| `Isub` | `i32.sub` |
| `Imul` | `i32.mul` |
| `IdivS` | `i32.div_s` (or 0 guard) |
| `IdivU` | `i32.div_u` (or 0 guard) |
| `IremS` | `i32.rem_s` |
| `IremU` | `i32.rem_u` |
| `Ineg` | `i32.const 0`, `local.get src`, `i32.sub` |

### Integer comparisons

| LPIR Op | WASM |
|---------|------|
| `Ieq` | `i32.eq` |
| `Ine` | `i32.ne` |
| `IltS` | `i32.lt_s` |
| `IleS` | `i32.le_s` |
| `IgtS` | `i32.gt_s` |
| `IgeS` | `i32.ge_s` |
| `IltU` | `i32.lt_u` |
| `IleU` | `i32.le_u` |
| `IgtU` | `i32.gt_u` |
| `IgeU` | `i32.ge_u` |

### Logic / bitwise

| LPIR Op | WASM |
|---------|------|
| `Iand` | `i32.and` |
| `Ior` | `i32.or` |
| `Ixor` | `i32.xor` |
| `Ibnot` | `i32.const -1`, `i32.xor` |
| `Ishl` | `i32.shl` |
| `IshrS` | `i32.shr_s` |
| `IshrU` | `i32.shr_u` |

### Constants

| LPIR Op | WASM (Q32) |
|---------|------------|
| `IconstI32 { value }` | `i32.const value` |
| `FconstF32 { value }` | `i32.const (clamp(value) * 65536.0) as i32` |

### Immediates

| LPIR Op | WASM |
|---------|------|
| `IaddImm` | `local.get src`, `i32.const imm`, `i32.add` |
| `IsubImm` | `local.get src`, `i32.const imm`, `i32.sub` |
| `ImulImm` | `local.get src`, `i32.const imm`, `i32.mul` |
| `IshlImm` | `local.get src`, `i32.const imm`, `i32.shl` |
| `IshrSImm` | `local.get src`, `i32.const imm`, `i32.shr_s` |
| `IshrUImm` | `local.get src`, `i32.const imm`, `i32.shr_u` |
| `IeqImm` | `local.get src`, `i32.const imm`, `i32.eq` |

### Select / Copy

| LPIR Op | WASM |
|---------|------|
| `Select` | `local.get if_true`, `local.get if_false`, `local.get cond`, `select` |
| `Copy` | `local.get src`, `local.set dst` |

### Float ops — stub for Phase 3

`Fadd`, `Fsub`, `Fmul`, `Fdiv`, `Fneg`, `Fabs`, `Fsqrt`, `Fmin`,
`Fmax`, `Ffloor`, `Fceil`, `Ftrunc`, `Fnearest`, `Feq`..`Fge`,
`FtoiSatS`, `FtoiSatU`, `ItofS`, `ItofU` — all `todo!("Q32 phase 3")`.

### `EmitCtx`

```rust
pub(crate) struct EmitCtx {
    pub depth: u32,
    pub control_stack: Vec<CtrlEntry>,
    pub import_count: u32,
    pub i64_scratch: Option<u32>,  // WASM local index for Q32 scratch
}
```

## Validate

```
cargo check -p lp-glsl-wasm
```

Integer-only GLSL programs could work end-to-end at this point
(if control flow and return are wired, which is Phase 4).
