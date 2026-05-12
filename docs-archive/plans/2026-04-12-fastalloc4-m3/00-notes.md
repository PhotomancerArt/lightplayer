# M3: Advanced Straight-Line - Analysis Notes

**DECISION: Split into M3.1 and M3.2**
- M3.1: Advanced straight-line (this plan) - spill pressure, entry param moves
- M3.2: Calls + Sret - ABI handling, clobbers, sret buffers

## Scope

Extend the allocator to handle:
1. **Spill pressure testing** - verify eviction logic with limited registers
2. **Entry param moves** - record moves when params get evicted from ABI regs
3. **Robust straight-line validation** - comprehensive filetests

## Current State

From M2 we have:
- `walk_linear()` working for simple straight-line code
- `AllocOutput` with per-operand allocs and edit list
- Snapshot testing framework with `expect_alloc()`
- 23 filetests passing for straight-line code

**What's NOT tested well:**
- Spill pressure (eviction logic)
- Entry parameter moves (when params get moved from ABI regs)
- Calls (completely unimplemented)
- Sret (completely unimplemented)

## Questions

### Q1: How do we add register pressure control to unit tests?

**Context:** We need to test spill logic with limited registers. Current `RegPool` uses `ALLOC_POOL` (16 regs). For testing, we might want only 2-3 regs available.

**DECISION:** Builder pattern API:

```rust
// Basic usage
alloc_test()
    .pool_size(4)
    .lpir("fn test() -> i32 { ... }")
    .expect_vinst("expected annotated vinst...");

// Testing stack-passed args (limit ABI arg regs)
alloc_test()
    .pool_size(4)
    .arg_reg_limit(1)  // Only a0 for args, forces stack for rest
    .lpir("fn test(a: i32, b: i32) -> i32 { ... }")
    .expect_vinst("...");

// VInst input
alloc_test()
    .pool_size(4)
    .vinst("i0 = IConst32 10...")
    .expect_vinst("expected annotated vinst...");
```

**Components:**
- `AllocTestBuilder` - configures test (pool_size, arg_reg_limit, input source)
- `AllocTestRunner` - executes allocation
- `AllocTestResult` - holds output, provides expectations

**Impl notes:**
- Add `RegPool::with_capacity(n)` constructor
- Add `FuncAbi::with_arg_reg_limit(n)` for testing
- Builder pattern gives flexibility for future options

### Q2: Entry param move recording - when do we emit the moves?

**Context:** Entry params start in ABI registers (a0, a1, etc.). If they get evicted to different registers or spilled, we need entry moves.

**Current behavior:** We seed the pool with params at ABI regs but don't record entry moves yet.

**DECISION:** After backward walk completes:
1. Compare final vreg locations to original ABI regs
2. For each param that moved: generate `Edit::Move(abi_reg → final_reg)` or `Edit::Move(abi_reg → slot)`
3. Insert at `EditPoint::Before(0)`
4. Render shows `; move: param_i0: a0 -> t1` or `; move: param_i0: a0 -> slot0`

### Q3: Filetest validation for each phase?

**Context:** We need to carefully validate each step with filetests.

**DECISION - Phase to filetest mapping:**

| Phase | Filetest | What it validates |
|-------|----------|-------------------|
| Phase 1 (Pool control) | New: `spill_pressure_3regs.glsl` | Forced spilling with limited pool |
| Phase 2 (Spill logic) | `spill_simple.glsl` | Basic spill/reload works |
| Phase 3 (Entry moves) | New: `param_eviction.glsl` | Params move from ABI regs |
| Phase 4 (Integration) | `native-multi-function.glsl` | Multiple straight-line fns |

**Additional validation:**
- Unit tests with builder pattern for each component
- All phases must keep existing 23 filetests passing

## Notes

- The render format needs to show `; move: ...` for edits (not just `; spill:`)
- Need to add `--show-alloc` flag to CLI for debugging
- Consider adding `LPVM_ALLOC_TRACE=1` env var output to filetest runner
