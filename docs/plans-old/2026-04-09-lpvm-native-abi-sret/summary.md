# Plan Summary: lpvm-native ABI Sret Implementation

## Overview

Completes M1 ABI milestone by implementing sret (struct-return) calling convention for RV32 native backend.

## Problem Statement

Functions returning >4 scalars (mat4 = 16 scalars) were failing with "TooManyReturns(16)" because the emitter only handled direct register returns (a0-a3). The RV32 calling convention requires these large returns to use an sret buffer passed by the caller.

## Solution

Match Cranelift's approach:
1. **Classification**: ReturnClass::Sret for >4 scalars
2. **Caller**: Allocates buffer, passes ptr in a0, shifts args
3. **Callee**: Stores return values to a0-relative buffer
4. **Readback**: Caller reads results from buffer after call

## Phase Overview

| Phase | Status | Description |
|-------|--------|-------------|
| 1 | ✅ Complete | **AbiInfo struct** - Per-function ABI classification from LpsFnSig |
| 2 | ✅ Complete | **Thread signature to emit** - Plumb LpsFnSig through emission pipeline |
| 3 | ✅ Complete | **Sret emission** - Store to buffer instead of register moves |
| 4 | ✅ Complete | **Caller handling** - Buffer alloc, arg shifting, readback |
| 5 | ✅ Complete | **Filetest validation** - spill_pressure.glsl and 22 mat4 tests pass |
| 6 | Pending | **Cleanup** - Remove TODOs, fix warnings, format |

## Implementation Details

### Phase 4: Caller-side Sret Handling

Modified `lp-shader/lpvm-native/src/rt_emu/instance.rs`:

- Added imports for `AbiInfo`, `ReturnClass`, and `LpvmMemory` trait
- Updated `invoke_flat` to:
  1. Get function signature from `self.module.meta`
  2. Create `AbiInfo` using `AbiInfo::from_lps_sig()`
  3. For sret functions: allocate buffer from arena using `LpvmMemory::alloc()`
  4. Prepend sret pointer to arguments (buffer address goes in a0, shifting real args)
  5. After call: read return values from buffer instead of registers

The implementation correctly handles:
- Buffer allocation from `EmuSharedArena`
- Argument shifting (vmctx + sret_ptr + real_args)
- Reading 16x32-bit values for mat4 returns from the buffer

## Validation Results

- ✅ `spill_pressure.glsl` (mat4 return with spilling) passes
- ✅ All 22 mat4 operation tests pass (mat4/op-add, op-subtract, etc.)
- ✅ All 25 ABI unit tests pass including sret classification tests

## Estimated Scope

- Lines: ~300-400
- Files: 5-6 modifications
- Time: 2-3 days
