# ABI2 Core Plan - Notes

## Scope of Work

Create a new ABI system (abi2) for lpvm-native that provides a clean, testable, and correct abstraction for:

1. Register set management (PReg, PRegSet bitmasks)
2. ISA-level ABI constants (RV32 ILP32 calling convention)
3. Per-function ABI classification (params, returns, sret detection)
4. Frame layout computation

The goal is textbook correctness: each component is pure, testable in isolation, and composes cleanly. Performance optimization is explicitly out of scope for this phase.

## Current State

The current ABI code in `lp-shader/lpvm-native/src/isa/rv32/abi.rs` has several issues:

1. **Mixed concerns**: `AbiInfo` combines ISA-level constants with per-function state
2. **No register abstraction**: Raw `u8` indices used throughout, no type safety for register classes
3. **Inflexible allocation**: `ALLOCA_REGS` is a static slice that cannot adapt to sret functions needing to exclude s1
4. **Disconnected frame layout**: `FrameLayout` exists but is computed separately from allocation constraints
5. **Hard to test**: Many functions depend on global constants or require full `IrFunction` context

The result is bugs like: sret pointer saved to s1 in prologue, but s1 is in ALLOCA_REGS so regalloc assigns a vreg to it, clobbering the sret buffer address.

## Key Questions

### 1. Should PReg include a class field now or YAGNI?

**Context**: RV32 only has integer registers today. The F extension would add float registers (f0-f31). A `RegClass` enum would future-proof but adds complexity now.

**Options**:
- A) Include `RegClass` field in PReg now, with single `Int` variant
- B) Just use `u8` for hardware index, add class later when F extension arrives
- C) Use newtype wrapper `struct PReg(u8)` without class, add class enum later

**Answer**: Include both `Int` and `Float` register classes now. RV32 will get F extension soon enough; use `unimplemented!()` for Float branches initially.

### 2. Should FuncAbi own frame layout computation or be separate?

**Context**: Frame layout depends on:
- ABI constraints (which callee-saved regs need saving)
- Regalloc output (spill count)
- ISA requirements (stack alignment)

**Options**:
- A) `FuncAbi::finalize_frame(spill_count) -> FrameLayout` method
- B) Separate `FrameLayout::compute(&FuncAbi, spill_count)` free function
- C) `FrameLayout` builder that takes `&FuncAbi` and regalloc results

**Answer**: B - `FrameLayout::compute(&FuncAbi, spill_count)` free function. FuncAbi is a pure value from the signature; FrameLayout is a derived value from ABI + regalloc results. This matches QBE's split between classification and stack frame computation.

### 3. Should we include stack slot tracking in ABI2?

**Context**: LPIR has "slots" for array storage, separate from spill slots. Current code conflates these.

**Options**:
- A) Include `StackSlot` type and offset computation in abi2
- B) Keep abi2 focused on registers and calling convention only
- C) Include slot types but have separate module for LPIR-specific slot allocation

**Answer**: A - include both spill slots and LPIR semantic slots with a classification indicator (e.g., `SlotKind::Spill` vs `SlotKind::Lpir`). The frame layout owns all stack space, so it should understand both kinds. The distinction is just which base offset you start from.

### 4. How do we handle the existing ABI code during transition?

**Context**: We need to build abi2 alongside the existing system, then switch over.

**Options**:
- A) Create abi2/ subdirectory, keep old abi.rs, delete when done
- B) Replace abi.rs module by module, feature-flag during transition
- C) Create abi2.rs next to abi.rs, gradual migration

**Answer**: A - clean separation in abi2/ subdirectory. When abi2 is complete and tested, we delete abi.rs and rename. No feature flags, no mixing during development.

## Design Decisions (Post-Question)

To be filled after user answers above questions.

## Notes

- QBE reference: `rv64/abi.c` for classification, `riscv64/` for frame layout
- Cranelift reference: `cranelift-codegen/src/isa/riscv64/abi.rs` for regsets, `cranelift-codegen/src/machinst/abi.rs` for generic ABI traits
- Key insight from both: classification is pure functions on signature; frame layout is computed from classification + regalloc results
