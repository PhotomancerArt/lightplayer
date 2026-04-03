# Milestone I: VMContext Foundation — Design

## Scope of Work

Establish the VMContext type definition, header struct, and signature changes. Thread an empty VMContext (no uniforms or globals yet) through the entire system (Cranelift and WASM).

**Key deliverables:**
- `VmContextHeader` struct in `lp-glsl-abi` with well-known fields at fixed offsets
- `IrFunction.vmctx_vreg: VReg` — explicit VMContext in LPIR
- All function signatures include VMContext as first param
- `DirectCall`, `invoke`, WASM emission updated for VMContext
- Test harnesses allocate and pass VMContext
- Design doc `docs/design/uniforms-globals.md` covering full roadmap

## File Structure

```
lp-glsl/
├── lp-glsl-abi/
│   └── src/
│       ├── lib.rs                    # Re-export VmContextHeader
│       └── vmcontext.rs              # NEW: VmContextHeader struct, offsets
├── lpir/
│   └── src/
│       └── module.rs                 # UPDATE: Add vmctx_vreg field
├── lpir-cranelift/
│   ├── src/
│   │   ├── emit/mod.rs               # UPDATE: VMContext as first param
│   │   ├── lib.rs                    # UPDATE: DirectCall takes vmctx param
│   │   └── jit_module.rs             # UPDATE: Store vmctx in module
│   └── src/invoke.rs                 # UPDATE: Prepend vmctx to args
├── lp-glsl-wasm/
│   └── src/
│       ├── emit/mod.rs               # UPDATE: Add vmctx_local to FuncEmitCtx
│       └── func.rs                   # UPDATE: local.get 0 is vmctx
├── lp-glsl-filetests/
│   └── src/
│       └── test_run/                 # UPDATE: Allocate VMContext in tests
│           ├── q32_exec_common.rs    # UPDATE: Add vmctx allocation
│           └── wasm_runner.rs        # UPDATE: Add vmctx allocation
└── lp-glsl-exec/
    └── src/
        └── executable.rs             # UPDATE: Add vmctx param to calls

docs/design/
└── uniforms-globals.md               # NEW: Full design doc for all milestones
```

## Conceptual Architecture

### VMContext Header

```rust
#[repr(C)]
pub struct VmContextHeader {
    pub fuel: u64,                      // [0] Optional gas metering
    pub trap_handler: u32,              // [8] Optional callback pointer
    pub globals_defaults_offset: u32,   // [12] Offset to globals_defaults
}
// Total: 16 bytes, naturally aligned
```

The header lives at a fixed offset (0) in every VMContext. The host accesses these fields via the struct. Shader-specific data (uniforms, globals) follows the header at dynamic offsets.

### Function Signatures

All functions receive VMContext as the first parameter:

```
fn shader(vmctx: *mut VMContext, arg0: i32, arg1: i32) -> i32
          ^^^^^^^^^^^^^^^^^^^^^
          Always present, even if unused
```

**LPIR representation:**
- `IrFunction.vmctx_vreg: VReg` — always 0
- User params start at vreg 1

**Cranelift:**
- `signature.params[0]` = `AbiParam::new(pointer_type)`

**WASM:**
- `local.get 0` = vmctx pointer (i32)
- User params start at `local.get 1`

### Call Flow

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│   Host      │────▶│  VMContext   │────▶│  Shader     │
│ (Test/JS)   │alloc│  (memory)    │pass │  (RISC-V/   │
│             │     │              │ptr  │   WASM)     │
└─────────────┘     └──────────────┘     └─────────────┘
       │                    │
       │  ┌─────────────┐   │
       └──│   Header    │◀──┘
          │  fuel, etc  │
          └─────────────┘
          │ Uniforms    │ (future)
          │ Globals     │ (future)
          │ Defaults    │ (future)
          └─────────────┘
```

### Backward Compatibility Strategy

**No backward compatibility.** This milestone updates all call sites:

1. **Filetests**: Allocate minimal VMContext in `q32_exec_common.rs` and `wasm_runner.rs`
2. **JIT tests**: Create VMContext before `DirectCall::call_i32()`
3. **WASM runner**: Export VMContext allocation from host, call before shader

For tests that don't use globals/uniforms, provide `VmContext::minimal()`:

```rust
impl VmContext {
    /// Create minimal VMContext for tests (header only, no uniforms/globals)
    pub fn minimal() -> Box<[u8]> {
        let header = VmContextHeader {
            fuel: 0,
            trap_handler: 0,
            globals_defaults_offset: 0, // will be populated in Milestone III
        };
        let bytes = unsafe {
            core::slice::from_raw_parts(
                &header as *const _ as *const u8,
                core::mem::size_of::<VmContextHeader>()
            )
        };
        bytes.into()
    }
}
```

## Main Components and Interactions

### 1. VmContextHeader (`lp-glsl-abi`)

```rust
// vmcontext.rs
pub const VMCTX_OFFSET_FUEL: usize = 0;
pub const VMCTX_OFFSET_TRAP_HANDLER: usize = 8;
pub const VMCTX_OFFSET_GLOBALS_DEFAULTS_OFFSET: usize = 12;
pub const VMCTX_HEADER_SIZE: usize = 16;

#[repr(C)]
pub struct VmContextHeader { ... }
```

### 2. LPIR Module Update (`lpir`)

```rust
// module.rs
pub struct IrFunction {
    pub vmctx_vreg: VReg,  // NEW: Always VReg(0)
    pub param_count: u16,  // User-visible params (not including vmctx)
    pub vreg_types: Vec<IrType>,
    // vreg_types[0] is pointer type for vmctx
    // vreg_types[1..param_count+1] are user params
}
```

### 3. Cranelift Signature (`lpir-cranelift`)

```rust
// emit/mod.rs
pub fn signature_for_ir_func(
    func: &IrFunction,
    call_conv: CallConv,
    mode: FloatMode,
    pointer_type: types::Type,
    isa: &dyn TargetIsa,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    
    // VMContext always first
    sig.params.push(AbiParam::new(pointer_type));
    
    // User params follow
    for i in 0..func.param_count {
        sig.params.push(AbiParam::new(ir_type_for_mode(
            func.vreg_types[func.vmctx_vreg.0 as usize + 1 + i as usize],
            mode
        )));
    }
    // ... returns
    sig
}
```

### 4. WASM Emission (`lp-glsl-wasm`)

```rust
// emit/mod.rs
pub(crate) struct FuncEmitCtx<'a> {
    pub module: &'a EmitCtx<'a>,
    pub vmctx_local: Option<u32>,  // NEW: local index for vmctx, always Some(0)
    pub i64_scratch: Option<u32>,
    // ... rest
}

// func.rs
// local.get 0 is always vmctx
let vmctx = ctx.vmctx_local.expect("vmctx always present");
builder.local_get(vmctx);
```

### 5. DirectCall API (`lpir-cranelift`)

```rust
// lib.rs
impl DirectCall {
    pub unsafe fn call_i32(
        &self,
        vmctx: *const u8,        // NEW: VMContext pointer
        args: &[i32],
    ) -> Result<Vec<i32>, String> {
        // Prepend vmctx to args for invoke
        let mut full_args = Vec::with_capacity(1 + args.len());
        full_args.push(vmctx as i32);  // On 32-bit targets
        full_args.extend_from_slice(args);
        invoke::invoke_i32_args_returns(self.code, &full_args, ...)
    }
}
```

### 6. Test Harness Updates

```rust
// q32_exec_common.rs
pub fn exec_q32_shader(
    module: &JitModule,
    func_name: &str,
    args: &[i32],
) -> Result<Vec<i32>, String> {
    let vmctx = VmContext::minimal();  // NEW: Allocate minimal VMContext
    let ptr = vmctx.as_ptr();
    
    let dc = module.direct_call(func_name)?;
    dc.call_i32(ptr, args)  // NEW: Pass vmctx
}

// wasm_runner.rs
// Similar pattern: allocate VMContext in WASM memory before calling shader
```

## Milestone I Focus

This milestone intentionally does NOT include:
- Uniform collection or access
- Global collection or access
- `_init()` function
- Global defaults/reset logic
- Fuel metering implementation

It only establishes the **plumbing**—getting VMContext through the system so later milestones can build on it.
