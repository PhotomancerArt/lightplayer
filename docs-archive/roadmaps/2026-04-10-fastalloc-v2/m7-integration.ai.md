# M7: Integration

## Scope of Work

Wire the rv32fa pipeline into the main compilation flow via `emit_function_bytes`.

## Files to Modify

```
lp-shader/lpvm-native/src/
└── isa/
    └── rv32/
        └── emit.rs            # UPDATE: add rv32fa branch
```

## Implementation Details

### 1. Update `emit_function_bytes`

Add a branch at the start for `RegAllocAlgorithm::Fast`:

```rust
pub fn emit_function_bytes(
    func: &lpir::IrFunction,
    ir: &lpir::IrModule,
    module_abi: &ModuleAbi,
    fn_sig: &lps_shared::LpsFnSig,
    float_mode: lpir::FloatMode,
    debug_info: bool,
    alloc_trace: bool,
) -> Result<EmittedFunction, NativeError> {
    let mut lowered = crate::lower::lower_ops(func, ir, module_abi, float_mode)?;
    crate::peephole::optimize(&mut lowered.vinsts);

    // NEW: Fast allocator branch
    if config::REG_ALLOC_ALGORITHM == RegAllocAlgorithm::Fast {
        return emit_fast(func, &lowered.vinsts, fn_sig, debug_info);
    }

    // Existing path continues...
    let vinsts = &lowered.vinsts;
    // ...
}
```

### 2. Create `emit_fast` Function

```rust
fn emit_fast(
    func: &lpir::IrFunction,
    vinsts: &[VInst],
    fn_sig: &lps_shared::LpsFnSig,
    debug_info: bool,
) -> Result<EmittedFunction, NativeError> {
    use crate::isa::rv32fa::{FastAlloc, emit::emit, debug::physinst::format_physinsts};

    // Build function ABI
    let slots = func.total_param_slots() as usize;
    let func_abi = super::abi::func_abi_rv32(fn_sig, slots);

    // Allocate
    let result = match FastAlloc::allocate(vinsts, func.vreg_types.len(), &func_abi) {
        Ok(r) => r,
        Err(e) => {
            // On error, try to show trace
            if let Some(trace) = e.trace() {
                log::debug!("Allocation failed:\n{}", trace.format_table(&func.name));
            }
            return Err(e);
        }
    };

    // Log trace if requested
    if alloc_trace || log::log_enabled!(log::Level::Debug) {
        log::debug!("{}", result.trace.format_table(&func.name));
    }

    // Emit
    let bytes = emit(&result.physinsts)?;

    // Build relocations (simplified - extract from result or scan bytes)
    let relocs = Vec::new();

    Ok(EmittedFunction {
        bytes,
        relocs,
        spill_slots: result.spill.total_slots(),
        is_sret: func_abi.is_sret(),
        has_call: vinsts.iter().any(|v| v.is_call()),
    })
}
```

### 3. Update AllocResult

Make sure `AllocResult` includes all needed fields:

```rust
pub struct AllocResult {
    pub physinsts: Vec<PhysInst>,
    pub trace: AllocTrace,
    pub cfg: CFG,
    pub spill: SpillAlloc,
}
```

### 4. Error Enhancement

Update `NativeError` to capture trace:

```rust
pub enum NativeError {
    // ... existing variants

    FastallocError {
        message: String,
        trace: String,
    },
}

impl NativeError {
    pub fn trace(&self) -> Option<&str> {
        match self {
            NativeError::FastallocError { trace, .. } => Some(trace),
            _ => None,
        }
    }
}
```

### 5. Frame Layout Sharing

If needed, extract frame layout computation:

```rust
// isa/mod.rs or shared location
pub struct FrameLayout {
    pub spill_slots: u32,
    pub is_sret: bool,
    pub has_call: bool,
    pub incoming_stack_params: Vec<(VReg, i32)>,
}

pub fn compute_frame_layout(
    func: &IrFunction,
    vinsts: &[VInst],
    abi: &FuncAbi,
) -> FrameLayout {
    // Extract from existing rv32/emit.rs logic
}
```

## Tests

```rust
#[test]
fn test_fastalloc_integration() {
    // Set config to Fast
    std::env::set_var("REG_ALLOC_ALGORITHM", "fast");

    // Compile a simple function
    let func = test_function_add();
    let result = emit_function_bytes(&func, ...);

    // Should succeed
    assert!(result.is_ok());
}
```

## Validate

```bash
# Build
cargo check -p lpvm-native

# Test with simple file
cargo run -p lp-cli -- shader-rv32fa test.glsl --trace

# Or via filetests
cargo test -p lps-filetests --test filetest -- native-rv32-iadd
```

## Success Criteria

1. `emit_function_bytes` branches to rv32fa when config is Fast
2. Control flow is detected and rejected with clear error
3. Successful allocation logs trace (when DEBUG=1)
4. Failed allocation attaches trace to error
5. Output bytes are valid RISC-V machine code
