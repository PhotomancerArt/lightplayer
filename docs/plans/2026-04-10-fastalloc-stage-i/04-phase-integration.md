## Phase 4: Integration and Filetest Validation

### Scope

Add `USE_FAST_ALLOC_EMIT` config flag. Wire up the adapter → new emitter path
in `emit_function_bytes()`. Run filetests, fix any issues.

### Code Organization Reminders

- Config flag goes in `config.rs`, used by `emit.rs` to select path
- Integration happens in the top-level emit entry point
- Keep the old path available behind the flag

### Implementation Details

**In `config.rs`:**

```rust
/// When `true`, use the new FastAllocation-based emitter.
/// When `false`, use the old Allocation-based emitter.
pub const USE_FAST_ALLOC_EMIT: bool = false; // default to old for now
```

**In `isa/rv32/emit.rs`, update `emit_function_bytes`:**

```rust
pub fn emit_function_bytes(
    func: &IrFunction,
    vinsts: &[VInst],
    func_abi: &FuncAbi,
    module_abi: &ModuleAbi,
    loop_regions: &[LoopRegion],
    options: EmitOptions,
) -> Result<EmittedFunction, NativeError> {
    if crate::config::USE_FAST_ALLOC_EMIT {
        // New path: allocate, adapt, emit
        let alloc = if crate::config::USE_LINEAR_SCAN_REGALLOC {
            LinearScan::new().allocate_with_func_abi(
                func, vinsts, func_abi, loop_regions, options.alloc_trace
            )?
        } else {
            GreedyAlloc::new().allocate_with_func_abi(func, vinsts, func_abi)?
        };
        
        let fast_alloc = AllocationAdapter::adapt(&alloc, vinsts, func_abi);
        emit_function_bytes_fast(func, vinsts, &fast_alloc, func_abi, module_abi, options)
    } else {
        // Old path: allocate and emit directly
        let alloc = if crate::config::USE_LINEAR_SCAN_REGALLOC {
            LinearScan::new().allocate_with_func_abi(
                func, vinsts, func_abi, loop_regions, options.alloc_trace
            )?
        } else {
            GreedyAlloc::new().allocate_with_func_abi(func, vinsts, func_abi)?
        };
        
        emit_function_bytes_old(func, vinsts, &alloc, func_abi, module_abi, loop_regions, options)
    }
}
```

**Rename existing function to `emit_function_bytes_old`:**

```rust
fn emit_function_bytes_old(
    func: &IrFunction,
    vinsts: &[VInst],
    alloc: &Allocation,
    func_abi: &FuncAbi,
    module_abi: &ModuleAbi,
    loop_regions: &[LoopRegion],
    options: EmitOptions,
) -> Result<EmittedFunction, NativeError> {
    // ... existing implementation ...
}
```

### Validation

**Step 1: Compilation**

```bash
cargo check -p lpvm-native
```

**Step 2: Run filetests with old path (baseline)**

```bash
scripts/glsl-filetests.sh --target rv32lp lpvm/native
```

Verify all pass with `USE_FAST_ALLOC_EMIT = false`.

**Step 3: Enable new path and run filetests**

Edit `config.rs`:
```rust
pub const USE_FAST_ALLOC_EMIT: bool = true;
```

```bash
scripts/glsl-filetests.sh --target rv32lp lpvm/native
```

**Expected issues to debug:**

1. **Instruction count mismatches**: The new path may produce different
   instruction counts due to edit ordering differences. Compare disassembly
   to understand why.

2. **Correctness failures**: If outputs differ, check:
   - Are operand registers being read from the right index?
   - Are edits being emitted in the right order (Before vs After)?
   - Is the frame layout (spill slots) the same?

3. **Panic/crash**: Check array bounds on `operand_allocs` access.

**Debugging tips:**

Add temporary debug output in `emit_function_bytes_fast`:

```rust
#[cfg(feature = "emu")]
{
    extern crate std;
    std::eprintln!("=== FastAllocation for {} ===", func.name);
    std::eprintln!("edits: {:?}", fast_alloc.edits);
    std::eprintln!("operand_allocs: {:?}", fast_alloc.operand_allocs);
}
```

Compare with old path output using `scripts/disasm.sh`.

### Expected Results

All filetests should pass with the new path. The instruction counts may
slightly differ (edits may be ordered differently), but correctness should be
identical.

If there are discrepancies:
- Document them in `00-notes.md`
- Decide whether to fix in this phase or accept as "different but correct"

### Final Validation Commands

```bash
# All filetests
scripts/glsl-filetests.sh --target rv32lp lpvm/native

# Specific test categories
scripts/glsl-filetests.sh --target rv32lp lpvm/native/perf
scripts/glsl-filetests.sh --target rv32lp lpvm/native/regalloc
scripts/glsl-filetests.sh --target rv32lp lpvm/native/builtin

# Emulator tests (if time permits)
cargo test -p fw-tests --test scene_render_emu --no-run
```
