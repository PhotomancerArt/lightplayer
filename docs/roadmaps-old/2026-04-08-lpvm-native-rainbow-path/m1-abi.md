# Milestone 1: ABI - sret, Multi-Return, Out-Params

**Goal**: Implement full RV32 calling convention for LPIR, matching Cranelift's ABI: multi-scalar returns, sret for large structs, out-parameters for builtins, and stack spill slots.

## Suggested Plan

`lpvm-native-abi-m1`

## Scope

### In Scope

- **Multi-scalar returns**: Return vec2 in a0-a1, vec3/vec4 in a0-a3
- **`sret` pointer**: First arg register (a0) holds dest address when return > 4 scalars
- **Out-parameters**: Pointer arguments for LPIR pointer types (builtin `gradient` param)
- **Stack frame layout**: Spill slots with proper alignment, saved ra, frame pointer
- **Greedy+spill checkpoint**: Basic spill support to unblock lowering milestones

### Out of Scope

- Linear scan (M3)
- Full control flow (M2)
- JIT buffer output (M4)
- Firmware integration (M5)

## Key Decisions

1. **Return classification**: Match Cranelift's logic—direct registers up to 4 scalars, sret pointer beyond
2. **Out-param ABI**: Pointer in arg register (a0-a7), value written to caller-allocated memory
3. **Spill slots**: Frame layout includes fixed spill area; emergency spills use reserved stack space
4. **Frame pointer**: s0 as frame pointer for debugging and spill base addressing

## Deliverables

| Deliverable | Location | Description |
|-------------|----------|-------------|
| `AbiAnalysis` | `isa/rv32/abi.rs` | Per-function ABI classification (return type, sret, arg mapping) |
| `FrameLayout` extended | `isa/rv32/abi.rs` | Spill slots, saved registers, total frame size |
| `emit_function_prologue` | `isa/rv32/emit.rs` | ra save, s0 setup, spill area allocation |
| `emit_function_epilogue` | `isa/rv32/emit.rs` | ra restore, s0 restore, return |
| Spill code emission | `isa/rv32/emit.rs` | sw/lw for spilled vregs |
| Tests | `isa/rv32/abi.rs` | Unit tests for return classification, arg mapping |

## Dependencies

- Previous POC: M1 from `2026-04-07-lpvm-native-poc` (basic ABI structure exists)
- Reference: Cranelift's RV32 ABI implementation in `lpvm-cranelift`

## Estimated Scope

- **Lines**: ~400-600
- **Files**: 2-3 modified (`abi.rs`, `emit.rs`)
- **Time**: 2-3 days

## Acceptance Criteria

1. Unit tests: `return_class(vec2) = [a0, a1]`, `return_class(vec4) = [a0, a1, a2, a3]`
2. Unit tests: `sret_class(vec5) = sret_ptr`, args start at a1 not a0
3. Unit tests: pointer params use arg registers, not expanded
4. Prologue/epilogue tests: ra saved at frame_size-4, s0 at frame_size-8
5. Spill emission tests: sw/lw use correct frame offsets
