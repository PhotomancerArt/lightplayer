# ABI2 Core Plan Summary

## Goal
Create a clean, testable, and correct ABI abstraction for lpvm-native that fixes the sret register allocation bug and provides a solid foundation for future backend work.

## Architecture

Two-layer design inspired by QBE and Cranelift:

1. **ISA-level rules** (`abi2::rv32`) - static constants for RV32 ILP32 calling convention
2. **Per-function ABI** (`FuncAbi`) - computed from signature, consumed by regalloc and emission

### Data Flow

```
LpsFnSig
    │
    ▼
classify_params/return ──► FuncAbi ──► regalloc uses allocatable(), precolors()
    │                                      reports used_callee_saved
    │                                           │
    └───────────────────────────────────────────┘
                                                ▼
                                        FrameLayout::compute()
                                                │
                                                ▼
                                            emission
```

## File Structure

```
lpvm-native/src/
├── abi2/
│   ├── mod.rs        # Public API re-exports
│   ├── regset.rs     # PReg, RegClass, PregSet
│   ├── classify.rs   # classify_params, classify_return
│   ├── func_abi.rs   # FuncAbi struct
│   └── frame.rs      # FrameLayout, SlotKind
└── isa/rv32/
    └── abi2.rs       # RV32 register constants and sets
```

## Key Design Decisions

| Aspect | Decision | Rationale |
|--------|----------|-----------|
| Register abstraction | `PReg { hw, class }` with `RegClass::Int/Float` | Type safety, future F extension |
| Register sets | `PregSet(u64)` bitmask | Efficient set operations |
| Classification | Pure functions | Testable, no mutation |
| FuncAbi | Immutable struct, constructed once | Clear data flow |
| FrameLayout | `compute(&FuncAbi, spill_count)` free function | Matches QBE split |
| Stack slots | `SlotKind::Spill/Lpir` | Unified stack management |
| Transition | `abi2/` subdirectory, delete old when done | Clean separation |

## Bug Fix: sret register clobbering

**Problem**: sret pointer saved to s1 in prologue, but s1 was in `ALLOCA_REGS`, so regalloc assigned a vreg to it.

**Solution**: `FuncAbi::allocatable()` excludes s1 when `is_sret()`. Regalloc cannot assign to the sret preservation register.

## Testability

Each layer testable in isolation without full compiler context:
- `PregSet` operations
- Classification for various types (void, direct, sret thresholds)
- FuncAbi allocatable sets and precolors
- Frame layout computation

## Phases

1. **PregSet** - Register abstraction and set operations
2. **RV32 constants** - Individual registers and pre-built sets
3. **Classification** - Pure functions for params and returns
4. **FuncAbi** - Per-function ABI state
5. **FrameLayout** - Stack frame computation
6. **Module integration** - mod.rs tying it all together
7. **Cleanup** - Validation and next plan preparation

## Not in Scope

- Regalloc integration (next plan)
- Emission updates (next plan)
- Performance optimization (future)
- RV32F float registers (future - stubbed)

## Validation

```bash
cargo test -p lpvm-native abi2
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf
```

## Transition Path

This plan ends with a fully tested abi2 module that is not yet wired into the backend.

Next plan: wire abi2 into regalloc and emission, switch filetests, delete old abi.rs.
