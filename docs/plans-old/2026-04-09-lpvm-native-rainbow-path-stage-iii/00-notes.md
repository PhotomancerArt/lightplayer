# Plan: lpvm-native Rainbow Path Stage III - Function Calls

## Scope of Work

Enable calling user functions and builtins with proper ABI handling in the native RV32 compiler (lpvm-native).

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

## Current State

**LPIR already has Call operation:**
- `Op::Call { callee: CalleeRef, args: VRegRange, results: VRegRange }` in `lpir/src/op.rs`
- `CalleeRef` is a wrapper around u32 that indexes into imports (builtin) or local functions

**VInst already has Call variant:**
- `VInst::Call { target: SymbolRef, args: Vec<VReg>, rets: Vec<VReg> }` in `vinst.rs`
- `SymbolRef` holds the function name as a String
- `defs()` and `uses()` properly report register read/written
- `is_call()` returns true, which marks the function as non-leaf

**Emission already handles VInst::Call:**
- `emit.rs` lines 649-681: Moves args to a0-a7, emits auipc+jalr, records relocation, moves rets from a0-a1
- Relocations use `R_RISCV_CALL_PLT` format
- This already works for float builtins (fadd, fsub, fmul)

**Lowering only handles float builtins:**
- `lower.rs` lines 219-242: Fadd/Fsub/Fmul in Q32 mode lower to `VInst::Call` with hardcoded `__lp_lpir_f*` symbol names
- **MISSING**: General `Op::Call` handling for user functions (lines 273-275 fall through to error)

**Runtime linking already works:**
- `link_object_with_builtins()` from `lpvm-cranelift` resolves symbols across functions
- `NativeEmuModule` holds the linked `ElfLoadInfo` with symbol_map
- `NativeEmuInstance::invoke_flat` looks up function entry points by name

**ABI infrastructure is in place:**
- `abi.rs` has `ARG_REGS[a0-a7]` and `RET_REGS[a0-a1]`
- Return classification already handles Direct vs Sret
- sret handling already implemented in `emit.rs` prologue/epilogue and `VInst::Ret`

## Questions

### Q1: How to resolve CalleeRef to function name during lowering?

**Context:** `Op::Call` has `CalleeRef(u32)` which is an index into either imports (0..import_count-1) or local functions (import_count..). Lowering needs to produce a `SymbolRef { name: String }` for the `VInst::Call`.

**Current situation:** The `lower_op` function receives `&IrFunction` but not the full `IrModule`, so it can't look up the callee name from the imports or other functions.

**Decision:** Pass `&IrModule` to `lower_ops` and `lower_op`. This provides direct access to:
- `ir.imports[callee.0 as usize].func_name` for builtins
- `ir.functions[(callee.0 - import_count) as usize].name` for local functions

This is cleaner than a closure or pre-resolution map.

### Q2: How to handle builtin vs user function calls?

**Context:** Builtins (like `__lp_lpir_fadd_q32`) are provided by the runtime. User functions are emitted as separate ELF symbols.

**Current situation:** The emission code treats all calls the same - it just emits auipc+jalr with a relocation. The linker resolves the symbol to either a builtin address or another function in the same object.

**Decision:** No special handling needed. Following the standard RV32 ABI, calls are uniform. The linker resolves undefined symbols to builtins or other functions in the module.

### Q3: How to handle caller-side sret when calling a function that returns >2 scalars?

**Context:** The ABI supports up to 2 scalars in a0-a1 (direct return) or >2 scalars via sret.

**Current situation:** `VInst::Call::rets` is `Vec<VReg>`, but the emission code only handles `RET_REGS[0]` and `RET_REGS[1]` (a0 and a1). For sret returns, the caller needs to allocate a buffer, pass it in a0, and the callee stores results there.

**Decision:** Create a `ModuleAbi` struct that pre-computes `FuncAbi` for all functions in the module:

```rust
pub struct ModuleAbi {
    func_abis: BTreeMap<String, FuncAbi>,
    max_callee_sret_bytes: u32,  // for caller-side pre-allocation
}
```

**Caller-side sret pattern:**
1. `emit_module_elf` creates `ModuleAbi` once from `LpsModuleSig`
2. `FrameLayout` includes pre-allocated sret slot (sized to `max_callee_sret_bytes`)
3. For sret calls: pass buffer address in a0 (args shift to a1-a7), callee stores results
4. After call: load return values from buffer into result vregs
5. For direct calls: current behavior (results in a0-a1)

## Notes

- Dependencies: This stage depends on M2.2 (control flow) and M1 ABI (sret support already done)
- The `lpvm-native` emitter already marks functions with calls as non-leaf (via `is_call()`)
- Frame layout already handles saving/restoring ra for non-leaf functions
- Pointer arguments (for builtins like `gradient`) already work via the existing load/store infrastructure
- `ModuleAbi` will also be passed to lowering so `Op::Call` resolution can access callee info

## Filetests to Add

| Test | Purpose |
|------|---------|
| `function/call-simple.glsl` | Basic function call with scalar return |
| `function/call-vec2-return.glsl` | Function returning vec2 (2 scalars, direct return) |
| `function/call-vec4-return.glsl` | Function returning vec4 (4 scalars, sret return) |
| `function/call-mat4-return.glsl` | Function returning mat4 (16 scalars, large sret) |
| `function/call-multi-args.glsl` | Function with multiple arguments (testing arg register assignment) |
| `function/call-nested.glsl` | Nested function calls (A calls B, B calls C) |
| `function/multi-function.glsl` | Module with multiple functions calling each other |
| `function/call-with-control-flow.glsl` | Function calls inside if/else and loops |
