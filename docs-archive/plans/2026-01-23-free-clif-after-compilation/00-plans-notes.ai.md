# Plan: Free CLIF IR After Compilation

## Questions

### Q1: Conditional Freeing Mechanism

**Context**: We need to free CLIF `Function` IR structures after compilation to reduce memory usage, but we also need to keep them available for tests and debugging scenarios.

**Answer**: Create a new memory-optimized function `build_jit_executable_memory_optimized()` that:
- Aggressively frees CLIF IR after each function compilation
- Extracts only what's needed and drops the rest of GlModule
- Will be used in ESP32 test code
- Existing `build_jit_executable()` remains unchanged for backward compatibility
- May be incorporated into generic code later, but not yet

### Q2: When to Free CLIF IR

**Context**: We need to determine the exact point in the compilation pipeline where CLIF IR can be safely freed.

**Answer**: Free immediately after each `define_function()` call completes successfully, before moving to the next function. This provides incremental memory freeing and reduces peak memory usage.

### Q3: How to Free CLIF IR

**Context**: We need to decide how to represent "freed" CLIF IR in the data structures.

**Answer**: Make the `function` field in `GlFunc` optional (`Option<Function>`), and set it to `None` after compilation when freeing is enabled. This clearly indicates when CLIF IR has been freed and avoids storing empty/unused data.

### Q4: Impact on Transform Pipeline

**Context**: Transforms operate on CLIF IR before compilation. We need to ensure transforms still work correctly.

**Suggested Answer**: Transforms happen before compilation (in `apply_transform`), so they will always have access to CLIF IR. No changes needed.

**Question**: Do we need any changes to the transform pipeline?
- Answer: No - transforms happen before compilation, so CLIF IR is always available at that point.

### Q5: Impact on Debugging and Error Reporting

**Context**: Error reporting and debugging utilities may need access to CLIF IR for formatting and diagnostics.

**Answer**: Only provide debugging when `retain-clif` is enabled. When disabled, provide a friendly error message explaining that CLIF IR is not available because the `retain-clif` feature is disabled. This prioritizes memory savings in constrained environments while being helpful to users.

### Q6: Testing Strategy

**Context**: Tests may need access to CLIF IR for validation and debugging.

**Answer**: 
- Existing tests continue using `build_jit_executable()` (retains CLIF IR)
- ESP32 code will use `build_jit_executable_memory_optimized()` (frees CLIF IR)
- Both functions can coexist - no need for feature flags or conditional compilation

## Notes

- The panic shows a function with 525+ virtual registers, indicating significant memory usage
- Memory allocation failure occurred during compilation: "memory allocation of 18436 bytes failed"
- The CLIF IR includes CFG, DFG, and other intermediate representations that can be large
- After compilation, only function pointers, signatures, and func_ids are needed for execution
- The emulator codegen path (`build_emu_executable`) may also benefit from this optimization
- `GlModule` contains unused fields: `function_registry`, `source_text`, `source_loc_manager`, `source_map` - these can be dropped early
- The `jit_module` field in `GlslJitModule` is marked `#[allow(dead_code)]` but must stay alive for function pointers to work
- New function will extract only what's needed and drop the rest immediately
