# Plan: HostJit no_std and Embedded Support

## Overview

Expand `HostJit` target to support both std and no_std environments, enabling JIT compilation on embedded systems like ESP32. This removes the std requirement from HostJit while maintaining backward compatibility for existing std-only code.

## Phases

1. Add `create_host_isa()` helper function
2. Update `create_isa()` to use the helper
3. Make `riscv32_triple()` available without emulator feature
4. Add extern declarations for no_std host functions
5. Update `get_host_function_pointer()` for no_std support
6. Test and cleanup

## Success Criteria

- `Target::HostJit` can be used in no_std mode
- `create_isa()` works for HostJit in no_std mode (riscv32 only)
- Host functions resolve correctly in no_std mode via extern functions
- ESP32 app can use HostJit without workarounds
- All existing std-only code continues to work
- No breaking changes to public API
