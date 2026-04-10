# M4: Integration and Filetests

## Scope of Work

Wire the rv32fa pipeline into `emit_function_bytes` and run the first filetests.

## Files to Modify

```
lp-shader/lpvm-native/src/
├── isa/rv32/mod.rs          # UPDATE: export frame layout computation for reuse
└── lib.rs or emit.rs        # UPDATE: add rv32fa branch in emit_function_bytes
```

## Implementation Details

### 1. Frame Layout Sharing

Move frame layout computation to a shared location that both rv32/ and rv32fa/ can use:

```rust
// isa/mod.rs or shared module
pub struct FrameLayout {
    pub spill_slots: u32,
    pub incoming_stack_params: Vec<(VReg, i32)>,
    pub is_sret: bool,
    pub has_call: bool,
}

pub fn compute_frame_layout(
    func: &IrFunction,
    vinsts: &[VInst],
    abi: &FuncAbi,
) -> FrameLayout {
    // Extract from existing rv32/emit.rs logic
}
```

### 2. Integration in `emit_function_bytes`

```rust
pub fn emit_function_bytes(
    func: &IrFunction,
    vreg_info: &VRegInfo,
    debug_info: Option<&DebugInfo>,
) -> Result<FunctionBytes, NativeError> {
    let vinsts = lower_ops(func)?;

    // Check allocator selection
    match config::REG_ALLOC_ALGORITHM {
        RegAllocAlgorithm::Fast => {
            if has_control_flow(&vinsts) {
                return Err(NativeError::FastallocUnsupportedControlFlow {
                    ir_function_name: func.name.clone(),
                    message: "Fast allocator only supports straight-line code".into(),
                    trace: None,
                });
            }

            let frame = compute_frame_layout(func, &vinsts, &func.abi);
            let result = rv32fa::alloc::FastAlloc::allocate(
                &vinsts,
                func.vreg_types.len(),
                &func.abi,
            )?;

            // Log trace for debugging
            if log::log_enabled!(log::Level::Debug) {
                log::debug!("{}", result.trace.format_table(&func.name));
            }

            let bytes = rv32fa::emit::emit(&result.physinsts)?;

            return Ok(FunctionBytes {
                bytes,
                spill_slots: frame.spill_slots,
                // ... other metadata
            });
        }

        RegAllocAlgorithm::LinearScan | RegAllocAlgorithm::Greedy => {
            // Existing path
            // ...
        }
    }
}
```

### 3. Error Enhancement

Update `NativeError` to include trace on failure:

```rust
pub enum NativeError {
    FastallocUnsupportedControlFlow {
        ir_function_name: String,
        message: String,
        trace: Option<String>,  // Formatted trace if available
    },
    FastallocError {
        ir_function_name: String,
        message: String,
        trace: String,  // Always present on allocation errors
    },
    // ... other variants
}
```

### 4. Run Filetests

Test with the simplest cases first:

```bash
# Test 1: native-rv32-iadd.glsl
cargo test -p lps-filetests --test filetest -- native-rv32-iadd

# Test 2: debug1.glsl (minimal failing case from v1)
cargo test -p lps-filetests --test filetest -- 2026-04-10-debug1

# If these pass, try more complex ones
```

### 5. Debug Workflow

When a test fails:

1. Run with debug logging to see the trace:

   ```bash
   RUST_LOG=debug cargo test -p lps-filetests --test filetest -- 2026-04-10-debug1 2>&1 | head -100
   ```

2. Check the formatted trace shows expected decisions

3. If trace looks wrong, add a unit test reproducing the specific VInst sequence

4. Fix allocator, verify with unit test, re-run filetest

## Validate

```bash
cd lp-shader

# Core tests
cargo test -p lpvm-native --lib

# Filetests (basic)
cargo test -p lps-filetests --test filetest -- native-rv32-iadd

# If debug1.glsl exists:
cargo test -p lps-filetests --test filetest -- debug1
```

## Success Criteria

- `native-rv32-iadd.glsl` passes
- `2026-04-10-debug1.glsl` (or equivalent minimal test) passes
- Debug trace is useful for understanding allocator decisions
- Error messages include trace on allocation failure
