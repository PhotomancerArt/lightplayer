# Summary: lpvm-native Rainbow Path Stage III - Function Calls

## Overview

Implemented function call support for the native RV32 backend (`lpvm-native`), enabling user functions to call other user functions and builtins with proper ABI handling for both direct and sret returns.

## What Was Implemented

### Core Components

1. **ModuleAbi** - Pre-computed ABI information for all functions in a module
   - Maps function names to their `FuncAbi`
   - Tracks maximum sret buffer size needed for any callee
   - Enables efficient callee lookup during lowering and emission

2. **VInst::Call Enhancement** - Added `callee_uses_sret` flag
   - Tells emission whether callee uses sret (caller-side handling needed)
   - Maintains backward compatibility with existing code

3. **Lowering Update** - Handle `Op::Call` in lowering pipeline
   - Resolve `CalleeRef` to function names via `IrModule`
   - Detect callee sret usage via `ModuleAbi`
   - Generate proper `VInst::Call` with sret flag

4. **FrameLayout Enhancement** - Caller-side sret slot
   - Pre-allocated buffer for calling sret functions
   - Sized to the maximum needed across all callees
   - Integrated into frame layout calculation

5. **Emission Update** - Caller-side sret handling
   - Two emission paths: direct return and sret return
   - Sret path: buffer in a0, args shifted to a1-a7, load results from buffer
   - Direct path: args in a0-a7, results in a0-a1

### Filetests Added

| Test | Description |
|------|-------------|
| `call-simple.glsl` | Basic scalar return |
| `call-vec2-return.glsl` | Direct return (2 scalars) |
| `call-vec4-return.glsl` | Sret return (4 scalars) |
| `call-mat4-return.glsl` | Large sret (16 scalars) |
| `call-multi-args.glsl` | Multiple arguments (a0-a7) |
| `call-nested.glsl` | Nested function calls |
| `multi-function.glsl` | Multiple functions calling each other |
| `call-with-control-flow.glsl` | Calls inside if/else |
| `call-in-loop.glsl` | Calls inside loops |

## Files Changed

### Modified

- `lp-shader/lpvm-native/src/abi/mod.rs` - Export `ModuleAbi`
- `lp-shader/lpvm-native/src/abi/func_abi.rs` - Add `ModuleAbi` struct and impl
- `lp-shader/lpvm-native/src/abi/frame.rs` - Add caller-side sret slot
- `lp-shader/lpvm-native/src/vinst.rs` - Add `callee_uses_sret` to `VInst::Call`
- `lp-shader/lpvm-native/src/lower.rs` - Handle `Op::Call`
- `lp-shader/lpvm-native/src/isa/rv32/emit.rs` - Caller-side sret emission
- `lp-shader/lpvm-native/src/error.rs` - Add `MissingSretSlot` error
- `lp-shader/lpvm-native/src/regalloc/greedy.rs` - Update test code
- `lp-shader/lpvm-native/src/lib.rs` - Re-export `ModuleAbi`

### Added

- `lp-shader/lps-filetests/filetests/function/call-simple.glsl`
- `lp-shader/lps-filetests/filetests/function/call-vec2-return.glsl`
- `lp-shader/lps-filetests/filetests/function/call-vec4-return.glsl`
- `lp-shader/lps-filetests/filetests/function/call-mat4-return.glsl`
- `lp-shader/lps-filetests/filetests/function/call-multi-args.glsl`
- `lp-shader/lps-filetests/filetests/function/call-nested.glsl`
- `lp-shader/lps-filetests/filetests/function/multi-function.glsl`
- `lp-shader/lps-filetests/filetests/function/call-with-control-flow.glsl`
- `lp-shader/lps-filetests/filetests/function/call-in-loop.glsl`

## Lines of Code

- Added: ~400 lines
- Modified: ~150 lines
- Total: ~550 lines

## Test Coverage

- Unit tests: `ModuleAbi`, `lower_call`, `emit_call_sret`, `frame_with_sret`
- Filetests: 9 new GLSL filetests covering various call patterns
- Integration: Verified with existing filetest suite

## Key Design Decisions

1. **ModuleAbi pre-computation** - Computing all FuncAbis once at module level rather than per-function reduces redundant work and enables callee lookup.

2. **Caller-side sret slot** - Pre-allocating a single sret slot sized to the maximum callee need is simpler than dynamic allocation per-call and matches standard compiler practice.

3. **VInst::Call flag** - Adding `callee_uses_sret` to the VInst makes the emission decision explicit and avoids needing to look up callee info again during emission.

## Known Limitations / Future Work

- Indirect calls (function pointers) not implemented
- Variadic functions not supported
- Tail call optimization not implemented
- Stack spilling for >7 arguments not yet implemented (args past a7)

## Validation

All validation commands pass:
- `cargo test -p lpvm-native`
- `cargo test --test filetest_runner -- --backend rv32lp.q32 function/`
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server`

## Commit Message

```
feat(lpvm-native): implement function calls (M2.3)

Add support for calling user functions and builtins with proper ABI handling:

- Add ModuleAbi for pre-computed per-module ABI information
- Add callee_uses_sret flag to VInst::Call for caller-side sret detection
- Implement Op::Call lowering with CalleeRef resolution
- Add caller-side sret slot to FrameLayout
- Implement caller-side sret emission path (buffer in a0, load after call)
- Add comprehensive filetests for function calls

Enables multi-function shaders with both direct and sret returns.
```
