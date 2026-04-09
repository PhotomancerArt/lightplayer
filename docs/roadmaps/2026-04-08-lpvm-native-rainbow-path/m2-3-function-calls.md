# Milestone 2.3: Function Calls

**Goal**: Enable calling user functions and builtins with proper ABI handling.

## Suggested Plan

`lpvm-native-m2-3-function-calls`

## Scope

### In Scope

- **User function calls**: Direct calls to other shader functions
- **Builtin calls**: Calls to runtime builtins (gradient, etc.)
- **Call emission**: auipc+jalr with relocation
- **Argument passing**: Respect ABI register assignment (a0-a7)
- **Return handling**: Both direct and sret returns from callees
- **Pointer arguments**: Support for pointer-type arguments (already work via load/store)

### Out of Scope

- Indirect calls (function pointers)
- Variadic functions
- Tail call optimization

## Key Decisions

1. **Call VInst**: Reuse existing Call VInst, extend for user functions
2. **Relocation**: Continue using auipc+jalr with R_RISCV_CALL_PLT relocation
3. **ABI compliance**: Argument registers a0-a7 per RV32 calling convention
4. **Builtin linking**: Runtime provides builtin symbols at link time

## Deliverables

| Deliverable | Location | Description |
|-------------|----------|-------------|
| `lower_op` extension | `lower.rs` | Lower LPIR Call op to VInst::Call |
| User function calls | `lower.rs` | Handle calls to module functions |
| Call emission | `isa/rv32/emit.rs` | Emit auipc+jalr, handle returns |
| Relocation | `isa/rv32/emit.rs` | Record call relocations |
| Module linking | `rt_emu/` | Resolve symbols across functions |
| Tests | `emit.rs`, integration | Call/return tests |
| Filetests | `filetests/` | function-call.glsl, multi-function.glsl |

## Dependencies

- M2.2: Control flow (calls can have control flow effects)
- M1: Full ABI support for sret returns from callees

## Estimated Scope

- **Lines**: ~250-350
- **Files**: 3-4 modified (`lower.rs`, `emit.rs`, `rt_emu/`)
- **Time**: 1-2 days

## Acceptance Criteria

1. User functions can call other user functions
2. Builtin calls work (gradient with pointer arg)
3. Calls respect ABI (args in a0-a7, sret in a0)
4. Return values correctly received from callees
5. Multi-function shaders execute correctly
