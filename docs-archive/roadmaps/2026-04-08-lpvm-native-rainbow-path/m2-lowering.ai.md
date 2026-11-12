# Milestone 2: Expanded Lowering for Rainbow LPIR

**Goal**: Lower all LPIR ops needed for `rainbow.glsl`: arithmetic, comparisons, control flow, function calls, and memory operations.

## Suggested Plan

`lpvm-native-lowering-m2`

## Scope

### In Scope

- **Arithmetic**: All integer ops, Q32 float ops (via builtins)
- **Comparisons**: Integer compare, float compare (Q32 builtin)
- **Selection**: `select` for ternary-like operations (used by `smoothstep`, `mix`)
- **Control flow**: `br`, `br_if`, `label` for if/else (loops in future milestone)
- **Function calls**: Direct calls to user functions and builtins
- **Memory**: `load`, `store`, `stack_slot` for spills and locals
- **Constants**: All `iconst` variants, `fconst` (Q32 representation)

### Out of Scope

- Loops (while, for) — not in rainbow
- Indirect calls — not in rainbow
- Vector shuffle/swizzle expansion — assume Cranelift-style lowering already in LPIR
- Full 64-bit ops — stub only

## Key Decisions

1. **Control flow**: Lower to VInst branch/jump with labels, emit as RV32 conditional branches
2. **Builtin calls**: Same as POC—auipc+jalr with relocation, but now with out-param pointers
3. **Spill integration**: Lowering marks spill slots; emit uses ABI frame layout
4. **Q32 float**: Continue soft-float via builtins (no F extension)

## Deliverables

| Deliverable | Location | Description |
|-------------|----------|-------------|
| Extended `lower_op` | `lower.rs` | All rainbow-required ops |
| `VInst` additions | `vinst.rs` | Br, BrIf, Label, Load, Store, StackSlot, Call (extended for out-params) |
| `lower_function` | `lower.rs` | Full function lowering with control flow |
| Branch emission | `isa/rv32/emit.rs` | beq, bne, blt, bge, jal, jalr for control flow |
| Stack slot emission | `isa/rv32/emit.rs` | Frame-relative load/store for spill slots |
| Rainbow filetest | `filetests/` | Target `rv32lp.q32` for rainbow.glsl (may have spill traffic) |
| Instruction counter | `lps-filetests` | Expose emulator cycle count for perf data |

## Dependencies

- M1: ABI with spills, sret, out-params
- `rainbow.glsl` LPIR from current Cranelift path (oracle for coverage audit)

## Estimated Scope

- **Lines**: ~800-1200
- **Files**: 4-5 modified (`lower.rs`, `vinst.rs`, `emit.rs`, filetests)
- **Time**: 4-6 days

## Acceptance Criteria

1. `rainbow.glsl` compiles without `UnsupportedOp` errors
2. `rv32lp.q32` filetests for rainbow pass (numeric parity with `jit.q32`)
3. Instruction count extractable from filetest runner (comparative metric)
4. Code may have heavy spill traffic (greedy allocator)—that's expected, will improve in M3
