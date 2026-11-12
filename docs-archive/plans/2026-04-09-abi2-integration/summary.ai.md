# Plan Summary: ABI2 Integration

## Overview

Wire the abi2 module into the actual compiler pipeline for register allocation, emission, and runtime handling of the sret calling convention.

## Phase Overview

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | FuncAbi helpers (precolor_of, sret_word_count, stack_alignment) | Pending |
| 2 | Regalloc integration (respect precolors, allocatable, s1 reservation) | Pending |
| 3 | Emitter integration (prologue, sret handling in Ret, epilogue) | Pending |
| 4 | Runtime integration (sret buffer alloc, arg shifting, readback) | Pending |
| 5 | Cleanup and validation | Pending |

## Key Design Decisions

### Regalloc takes &FuncAbi
The register allocator needs ABI constraints (precolors, allocatable set). Passing `&FuncAbi` is cleaner than extracting and passing individual fields.

### Emitter checks is_sret()
For `VInst::Ret`, the emitter checks `abi.is_sret()` and switches between:
- **Direct**: Move values to a0-a1
- **Sret**: Store values to buffer at s1-relative offsets

### Runtime argument shifting
For sret functions, the runtime prepends the sret buffer pointer to arguments:
- Non-sret: `[vmctx, arg0, arg1]` → a0=vmctx, a1=arg0
- Sret: `[vmctx, sret_ptr, arg0, arg1]` → a0=vmctx, a1=sret_ptr, a2=arg0

## Testing Strategy

1. Unit tests for each new helper method
2. Regalloc tests verify precolors are respected
3. Emitter tests verify sret stores vs direct moves
4. Runtime tests verify buffer allocation and readback
5. Filetests: `spill_pressure.glsl` and mat4 tests

## Estimated Scope

- Lines: ~400-600
- Files: 5-6 modified
- Time: 2-3 days
- Tests: 90+ (82 existing + 10-15 new)

## Dependencies

Requires abi2 module (completed in previous plan) which provides:
- `FuncAbi` with classification
- `FrameLayout` for stack computation
- `rv32::func_abi_rv32()` constructor

## Success Criteria

- All 82+ tests pass
- `spill_pressure.glsl` (mat4 return) passes
- All 22 mat4 operation tests pass
- `fw-esp32` builds without warnings
- No regressions in existing functionality
