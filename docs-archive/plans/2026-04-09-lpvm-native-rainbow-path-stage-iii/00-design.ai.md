# Design: lpvm-native Rainbow Path Stage III - Function Calls

## Scope of Work

Enable calling user functions and builtins with proper ABI handling in the native RV32 compiler (`lpvm-native`).

**In Scope:**
- User function calls: Direct calls to other shader functions within the same module
- Builtin calls: Calls to runtime builtins (already partially working for float ops)
- Call emission: auipc+jalr with relocation (already implemented)
- Argument passing: Respect ABI register assignment (a0-a7)
- Return handling: Both direct and sret returns from callees
- Pointer arguments: Support for pointer-type arguments (already work via load/store)

**Out of Scope:**
- Indirect calls (function pointers)
- Variadic functions
- Tail call optimization

## File Structure

```
lp-shader/lpvm-native/src/
├── abi/
│   ├── mod.rs                    # UPDATE: add ModuleAbi export
│   └── func_abi.rs               # UPDATE: add ModuleAbi struct and impl
├── lower.rs                      # UPDATE: handle Op::Call, thread IrModule + ModuleAbi
├── isa/rv32/emit.rs             # UPDATE: caller-side sret, thread ModuleAbi
├── vinst.rs                      # UPDATE: add callee_uses_sret flag to VInst::Call
├── error.rs                      # UPDATE: add MaxCalleeSretTooLarge error
└── lib.rs                        # UPDATE: re-export ModuleAbi

lp-shader/lps-filetests/filetests/function/
├── call-simple.glsl              # NEW: scalar return
├── call-vec2-return.glsl         # NEW: direct return (2 scalars)
├── call-vec4-return.glsl         # NEW: sret return (4 scalars)
├── call-mat4-return.glsl         # NEW: large sret (16 scalars)
├── call-multi-args.glsl          # NEW: multiple arguments
├── call-nested.glsl              # NEW: nested function calls
├── multi-function.glsl           # NEW: multiple functions calling each other
└── call-with-control-flow.glsl   # NEW: calls inside if/else/loops
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Module Compilation Flow                        │
└─────────────────────────────────────────────────────────────────────────┘

LpsModuleSig ─────┐
                  ▼
┌──────────────────────────────────────┐
│         ModuleAbi::from_sig()        │  ← NEW: Pre-compute all FuncAbis
│  • Build func_abis map (name → ABI)  │
│  • Track max_callee_sret_bytes       │
└──────────────────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                    Per-Function Processing                               │
│                                                                          │
│  ┌──────────────┐     ┌──────────────┐     ┌────────────────────────┐  │
│  │   lower_ops  │ ──▶ │   regalloc   │ ──▶ │      emit_vinsts       │  │
│  │              │     │              │     │                        │  │
│  │ Op::Call ──▶ │     │ VInst::Call  │     │ Direct return: a0-a1   │  │
│  │ SymbolRef    │     │ with sret    │     │ Sret return: buffer    │  │
│  └──────────────┘     │ flag         │     │ pre-alloc, load after  │  │
│     ↑ IrModule        └──────────────┘     └────────────────────────┘  │
│     ↑ ModuleAbi                                        │                 │
│                                              FrameLayout with sret_slot  │
└─────────────────────────────────────────────────────────────────────────┘
```

## Main Components

### 1. ModuleAbi (NEW)

Pre-computed ABI information for the entire module:

```rust
pub struct ModuleAbi {
    func_abis: BTreeMap<String, FuncAbi>,
    max_callee_sret_bytes: u32,
}

impl ModuleAbi {
    pub fn from_lps_module_sig(sig: &LpsModuleSig) -> Self;
    pub fn func_abi(&self, name: &str) -> Option<&FuncAbi>;
    pub fn max_callee_sret_bytes(&self) -> u32;
}
```

### 2. VInst::Call Update

Add callee sret flag for emission to know if it needs caller-side sret handling:

```rust
pub enum VInst {
    Call {
        target: SymbolRef,
        args: Vec<VReg>,
        rets: Vec<VReg>,
        callee_uses_sret: bool,  // NEW: tells emission to use caller-side sret
        src_op: Option<u32>,
    },
    // ...
}
```

### 3. Lowering Update

`lower_ops` and `lower_op` now receive `&IrModule` and `&ModuleAbi`:

```rust
pub fn lower_op(
    op: &Op,
    float_mode: FloatMode,
    src_op: Option<u32>,
    func: &IrFunction,
    ir: &IrModule,           // NEW: for CalleeRef resolution
    abi: &ModuleAbi,         // NEW: for callee sret detection
) -> Result<VInst, LowerError>
```

For `Op::Call`:
1. Resolve `CalleeRef` to name via `ir.imports` or `ir.functions`
2. Look up callee's `FuncAbi` via `abi.func_abi(name)`
3. Create `VInst::Call` with `callee_uses_sret` flag set from `func_abi.is_sret()`

### 4. FrameLayout Update

Add pre-allocated caller-side sret slot:

```rust
pub struct FrameLayout {
    // ... existing fields ...
    pub sret_slot_offset: Option<i32>,  // NEW: offset from fp if any callees use sret
    pub sret_slot_size: u32,            // NEW: size in bytes (0 if not needed)
}
```

### 5. Emission Update

`emit_function_bytes` receives `&ModuleAbi`:

```rust
pub fn emit_function_bytes(
    func: &IrFunction,
    abi: &ModuleAbi,              // NEW: replaces fn_sig parameter
    float_mode: lpir::FloatMode,
    debug_info: bool,
) -> Result<EmittedFunction, NativeError>
```

For `VInst::Call` emission:
- If `callee_uses_sret == false`: current behavior (args in a0-a7, results from a0-a1)
- If `callee_uses_sret == true`:
  1. Compute sret buffer address: `fp + sret_slot_offset`
  2. Move buffer address to a0
  3. Move user args to a1-a7 (shifted by 1)
  4. Emit auipc+jalr
  5. Load return values from buffer into result vregs

## How Components Interact

1. **Module level** (`emit_module_elf`):
   - Creates `ModuleAbi` once from `LpsModuleSig`
   - Passes it to each function's emission

2. **Lowering** (`lower_ops`):
   - Uses `IrModule` to resolve `CalleeRef` to names
   - Uses `ModuleAbi` to detect if callee uses sret
   - Produces `VInst::Call` with `callee_uses_sret` flag

3. **Frame layout** (`FrameLayout::compute`):
   - Receives `max_callee_sret_bytes` from `ModuleAbi`
   - Allocates sret slot if > 0
   - Adjusts frame size accordingly

4. **Emission** (`emit_vinst` for `VInst::Call`):
   - Checks `callee_uses_sret` flag
   - Uses direct return path or caller-side sret path
   - Loads results from appropriate source (registers or buffer)

## Acceptance Criteria

1. User functions can call other user functions with scalar returns
2. User functions can call other user functions with sret returns (vec4, mat4)
3. Builtin calls continue to work (fadd, fsub, fmul)
4. Calls respect ABI (args in a0-a7, sret buffer in a0 when needed)
5. Return values correctly received from callees (direct and sret)
6. Multi-function shaders execute correctly
7. All filetests pass on `rv32lp.q32` backend
