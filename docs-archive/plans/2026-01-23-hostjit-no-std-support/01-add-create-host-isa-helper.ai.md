# Phase 1: Add `create_host_isa()` Helper Function

## Description

Create a helper function `create_host_isa()` that handles ISA creation for HostJit in both std and no_std modes. In std mode, it uses `cranelift_native` for auto-detection. In no_std mode, it only supports riscv32 and uses the architecture-specific builder.

## Implementation

1. Add `create_host_isa(flags: Flags) -> Result<OwnedTargetIsa, GlslError>` function in `target.rs`
2. In std mode: use `cranelift_native::builder()` (current behavior)
3. In no_std mode: only support riscv32, use `riscv32::isa_builder(riscv32_triple())`
4. Return error in no_std if architecture is not riscv32

## Success Criteria

- Function compiles in both std and no_std modes
- Returns correct ISA for std mode (auto-detected)
- Returns riscv32 ISA for no_std mode
- Returns error for non-riscv32 in no_std mode
