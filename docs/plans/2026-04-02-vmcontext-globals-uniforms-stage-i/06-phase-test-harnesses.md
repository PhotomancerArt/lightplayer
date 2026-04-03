# Phase 6: Update Test Harnesses

## Scope of Phase

Update all test harnesses to allocate and pass VMContext when calling shaders. This includes:
- `lp-glsl-filetests` (Q32 and WASM runners)
- `lp-glsl-exec` (executable helpers)
- Any integration tests

## Code Organization Reminders

- Create helper functions for VMContext allocation
- Update call sites to use the new DirectCall API
- Keep test utilities at the bottom of files

## Implementation Details

### 1. Update `lp-glsl-filetests/src/test_run/q32_exec_common.rs`

Add VMContext allocation:

```rust
use lp_glsl_abi::minimal_vmcontext;

pub fn exec_q32_shader(
    module: &JitModule,
    func_name: &str,
    args: &[i32],
) -> Result<Vec<i32>, String> {
    let vmctx = minimal_vmcontext();  // NEW: Allocate minimal VMContext
    let ptr = vmctx.as_ptr();
    
    let dc = module.direct_call(func_name)
        .ok_or_else(|| format!("Function {} not found", func_name))?;
    
    unsafe {
        dc.call_i32(ptr, args)  // NEW: Pass vmctx
    }
}

// For multi-invocation tests that need to preserve VMContext:
pub struct Q32ExecContext {
    vmctx: Box<[u8]>,
    module: JitModule,
}

impl Q32ExecContext {
    pub fn new(module: JitModule) -> Self {
        Self {
            vmctx: minimal_vmcontext(),
            module,
        }
    }
    
    pub fn call(&self, func_name: &str, args: &[i32]) -> Result<Vec<i32>, String> {
        let dc = self.module.direct_call(func_name)
            .ok_or_else(|| format!("Function {} not found", func_name))?;
        
        unsafe {
            dc.call_i32(self.vmctx.as_ptr(), args)
        }
    }
}
```

### 2. Update `lp-glsl-filetests/src/test_run/wasm_runner.rs`

For WASM, VMContext is allocated in WASM memory:

```rust
use lp_glsl_abi::{VmContextHeader, VMCTX_HEADER_SIZE};

pub fn run_wasm_shader(
    module: &wasmtime::Module,
    func_name: &str,
    args: &[i32],
) -> Result<Vec<i32>, String> {
    let mut store = create_store();
    let instance = wasmtime::Instance::new(&mut store, module, &[])
        .map_err(|e| format!("Failed to instantiate: {}", e))?;
    
    // Allocate VMContext in WASM memory
    let memory = instance.get_memory(&mut store, "memory")
        .ok_or("Memory export not found")?;
    
    // Allocate VMContext at a known location (e.g., after stack)
    let vmctx_addr = allocate_vmcontext(&mut store, &memory)?;
    
    // Initialize header
    let header = VmContextHeader::new();
    write_vmcontext_header(&mut store, &memory, vmctx_addr, &header)?;
    
    // Get shader function
    let func = instance.get_typed_func::<(i32, ...), (i32, ...)>(&mut store, func_name)
        .map_err(|e| format!("Function not found: {}", e))?;
    
    // Call with vmctx as first arg
    let result = func.call(&mut store, (vmctx_addr, ...args))
        .map_err(|e| format!("Call failed: {}", e))?;
    
    Ok(result)
}

fn allocate_vmcontext(
    store: &mut Store,
    memory: &Memory,
) -> Result<i32, String> {
    // Allocate after shadow stack (64KB)
    const VMCTX_BASE: i32 = 65536;
    
    // Ensure memory is large enough
    let current_pages = memory.size(store);
    let needed_pages = (VMCTX_BASE + VMCTX_HEADER_SIZE as i32 + 65535) / 65536;
    if current_pages < needed_pages as u64 {
        memory.grow(store, needed_pages as u64 - current_pages)
            .map_err(|e| format!("Failed to grow memory: {}", e))?;
    }
    
    Ok(VMCTX_BASE)
}

fn write_vmcontext_header(
    store: &mut Store,
    memory: &Memory,
    addr: i32,
    header: &VmContextHeader,
) -> Result<(), String> {
    let bytes = unsafe {
        core::slice::from_raw_parts(
            header as *const _ as *const u8,
            core::mem::size_of::<VmContextHeader>()
        )
    };
    
    memory.write(store, addr as usize, bytes)
        .map_err(|e| format!("Failed to write VMContext: {}", e))?;
    
    Ok(())
}
```

### 3. Update `lp-glsl-exec/src/executable.rs`

If this crate has execution helpers, update them similarly.

### 4. Update integration tests

Find all tests that call `DirectCall::call_i32` or similar and update them:

```rust
// OLD:
let result = dc.call_i32(&[1, 2]).unwrap();

// NEW:
let vmctx = minimal_vmcontext();
let result = dc.call_i32(vmctx.as_ptr(), &[1, 2]).unwrap();
```

## Tests to Write

Mostly updating existing tests, but add one explicit test:

```rust
#[test]
fn vmctx_passed_to_shader() {
    // Compile a shader that returns a magic value from VMContext
    // Verify the magic value is received
}
```

## Validate

```bash
# Run filetests
cargo test -p lp-glsl-filetests

# Check all affected crates
cargo check -p lp-glsl-filetests
cargo check -p lp-glsl-exec
```

## Notes

- This phase touches many files but the changes are mechanical
- Most call sites just need to add `vmctx.as_ptr()` as first arg
- WASM is more complex because VMContext lives in WASM memory
