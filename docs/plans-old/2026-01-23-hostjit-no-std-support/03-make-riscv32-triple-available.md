# Phase 3: Make `riscv32_triple()` Available Without Emulator Feature

## Description

The `riscv32_triple()` helper function is currently gated behind `#[cfg(feature = "emulator")]`, but we need it for no_std HostJit support. Make it available without the emulator feature.

## Implementation

1. Remove `#[cfg(feature = "emulator")]` from `riscv32_triple()` function
2. Or create a separate version for no_std use (if we want to keep emulator version separate)
3. Ensure it's accessible from `create_host_isa()` in no_std mode

## Success Criteria

- `riscv32_triple()` is available in no_std mode
- Can be called from `create_host_isa()` without emulator feature
- No conflicts with existing emulator code
