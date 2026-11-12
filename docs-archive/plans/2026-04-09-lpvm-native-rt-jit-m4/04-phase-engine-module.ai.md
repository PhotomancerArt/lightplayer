# Phase 4: Create NativeJitEngine, Module, Instance

## Scope

Implement the `LpvmEngine`, `LpvmModule`, and `LpvmInstance` traits for the JIT path.

## Implementation Details

### 1. Create `lpvm-native/src/rt_jit/engine.rs`

```rust
//! LpvmEngine implementation for JIT compilation.

use alloc::sync::Arc;
use lpir::IrModule;
use lps_shared::LpsModuleSig;
use lpvm::{LpvmEngine, LpvmMemory};
use lpvm_emu::EmuSharedArena; // Reuse shared memory arena

use crate::error::NativeError;
use crate::native_options::NativeCompileOptions;

use super::builtins::BuiltinTable;
use super::compiler::JitEmitContext;
use super::module::NativeJitModule;

/// Engine that compiles LPIR to native RV32 JIT buffers.
///
/// Uses BuiltinTable for symbol resolution (populated at firmware startup).
pub struct NativeJitEngine {
    builtin_table: Arc<BuiltinTable>,
    options: NativeCompileOptions,
    arena: EmuSharedArena, // For shared memory (vmctx, etc.)
}

impl NativeJitEngine {
    /// Create new JIT engine.
    ///
    /// The builtin_table should be populated before creating the engine.
    pub fn new(builtin_table: Arc<BuiltinTable>, options: NativeCompileOptions) -> Self {
        Self {
            builtin_table,
            options,
            arena: EmuSharedArena::new(lpvm_emu::DEFAULT_SHARED_CAPACITY),
        }
    }
    
    /// Get reference to builtin table.
    pub fn builtin_table(&self) -> &BuiltinTable {
        &self.builtin_table
    }
}

impl LpvmEngine for NativeJitEngine {
    type Module = NativeJitModule;
    type Error = NativeError;

    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        use crate::abi::ModuleAbi;
        
        if ir.functions.is_empty() {
            return Err(NativeError::EmptyModule);
        }
        
        let module_abi = ModuleAbi::from_ir_and_sig(ir, meta);
        
        // Build sig map
        let sig_map: alloc::collections::BTreeMap<&str, &lps_shared::LpsFnSig> =
            meta.functions.iter().map(|s| (s.name.as_str(), s)).collect();
        
        // Emit all functions
        let mut ctx = JitEmitContext::new(&self.builtin_table);
        
        for func in &ir.functions {
            let default_sig = lps_shared::LpsFnSig {
                name: func.name.clone(),
                return_type: lps_shared::LpsType::Void,
                parameters: alloc::vec::Vec::new(),
            };
            let fn_sig = sig_map.get(func.name.as_str()).copied().unwrap_or(&default_sig);
            
            ctx.emit_function(
                func,
                ir,
                &module_abi,
                fn_sig,
                self.options.float_mode,
                self.options.alloc_trace,
            )?;
        }
        
        // Finalize into executable image
        let image = ctx.finalize()?;
        
        Ok(NativeJitModule {
            ir: ir.clone(),
            meta: meta.clone(),
            buffer: image.buffer,
            entry_offsets: image.entries,
            arena: self.arena.clone(),
            options: self.options,
        })
    }

    fn memory(&self) -> &dyn LpvmMemory {
        &self.arena
    }
}
```

### 2. Create `lpvm-native/src/rt_jit/module.rs`

```rust
//! LpvmModule implementation for JIT-compiled code.

use alloc::sync::Arc;
use lpir::IrModule;
use lps_shared::LpsModuleSig;
use lpvm::{LpvmMemory, LpvmModule};
use lpvm_emu::EmuSharedArena;

use crate::error::NativeError;
use crate::native_options::NativeCompileOptions;

use super::buffer::JitBuffer;
use super::instance::NativeJitInstance;

/// Compiled JIT module with entry points for each function.
#[derive(Clone)]
pub struct NativeJitModule {
    pub(crate) ir: IrModule,
    pub(crate) meta: LpsModuleSig,
    pub(crate) buffer: JitBuffer,
    pub(crate) entry_offsets: alloc::collections::BTreeMap<&'static str, usize>,
    pub(crate) arena: EmuSharedArena,
    pub(crate) options: NativeCompileOptions,
}

impl LpvmModule for NativeJitModule {
    type Instance = NativeJitInstance;
    type Error = NativeError;

    fn signatures(&self) -> &LpsModuleSig {
        &self.meta
    }

    fn instantiate(&self) -> Result<Self::Instance, Self::Error> {
        use lpvm::AllocError;
        
        // Allocate vmctx
        let align = 16usize;
        let size = lpvm_emu::GUEST_VMCTX_BYTES.max(align);
        let buf = self
            .arena
            .alloc(size, align)
            .map_err(|e: AllocError| NativeError::Alloc(alloc::format!("{e:?}")))?;
        
        // Initialize vmctx header
        unsafe {
            let slot = core::slice::from_raw_parts_mut(buf.native_ptr(), lpvm_emu::GUEST_VMCTX_BYTES);
            lpvm_emu::write_guest_vmctx_header(slot);
        }
        
        Ok(NativeJitInstance {
            module: self.clone(),
            vmctx_guest: buf.guest_base() as u32,
        })
    }
}
```

### 3. Create `lpvm-native/src/rt_jit/instance.rs`

```rust
//! LpvmInstance implementation for JIT-compiled code.
//!
//! Direct function calls to JIT buffer code.

use alloc::vec::Vec;
use lpir::FloatMode;
use lps_shared::{LpsType, ParamQualifier};
use lpvm::{CallError, LpvmInstance, flat_q32_words_from_f32_args, decode_q32_return, glsl_component_count, q32_to_lps_value_f32};

use crate::error::{NativeError};

use super::module::NativeJitModule;

/// Per-instance execution state.
pub struct NativeJitInstance {
    pub(crate) module: NativeJitModule,
    pub(crate) vmctx_guest: u32,
}

impl NativeJitInstance {
    /// Direct call to JIT function with flat i32 args.
    ///
    /// # Safety
    /// This is unsafe because we're calling into JIT-compiled code.
    unsafe fn invoke_flat(&self, name: &str, flat: &[i32]) -> Result<Vec<i32>, NativeError> {
        // Get entry point
        let offset = self.module.entry_offsets.get(name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
        
        let entry_ptr = self.module.buffer.entry_ptr(*offset);
        
        // Prepare args: vmctx + flat args
        let mut full: Vec<i32> = Vec::with_capacity(1 + flat.len());
        full.push(self.vmctx_guest as i32);
        full.extend_from_slice(flat);
        
        // Cast to callable function pointer
        // ABI: fn(vmctx: i32, ...args) -> (i32, i32) or sret
        type JitFunc = unsafe extern "C" fn(i32, ...) -> i32;
        let func: JitFunc = core::mem::transmute(entry_ptr);
        
        // TODO: Handle multi-arg calls properly
        // For now, support up to 8 args inline
        let result = match full.len() {
            1 => func(full[0]),
            2 => unsafe { core::mem::transmute::<_, extern "C" fn(i32, i32) -> i32>(func)(full[0], full[1]) },
            // ... more arities or use a loop with stack setup
            _ => return Err(NativeError::Call(CallError::Unsupported(alloc::string::String::from("too many args")))),
        };
        
        // For sret: result is buffer address, load from there
        // For direct return: result is in a0/a1
        // TODO: Detect sret from metadata and handle accordingly
        
        Ok(vec![result])
    }
}

impl LpvmInstance for NativeJitInstance {
    type Error = NativeError;

    fn call(&mut self, name: &str, args: &[lps_shared::lps_value_f32::LpsValueF32]) -> Result<lps_shared::lps_value_f32::LpsValueF32, Self::Error> {
        // Convert F32 args to Q32 words
        let gfn = self.module.meta.functions.iter()
            .find(|f| f.name == name)
            .cloned()
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
        
        if self.module.options.float_mode != FloatMode::Q32 {
            return Err(NativeError::Call(CallError::Unsupported(alloc::string::String::from("JIT requires Q32"))));
        }
        
        // Check parameters
        for p in &gfn.parameters {
            if matches!(p.qualifier, ParamQualifier::Out | ParamQualifier::InOut) {
                return Err(NativeError::Call(CallError::Unsupported(alloc::string::String::from("out/inout not supported"))));
            }
        }
        
        if gfn.return_type == LpsType::Void {
            return Err(NativeError::Call(CallError::Unsupported(alloc::string::String::from("void return"))));
        }
        
        if gfn.parameters.len() != args.len() {
            return Err(NativeError::Call(CallError::Arity {
                expected: gfn.parameters.len(),
                got: args.len(),
            }));
        }
        
        // Flatten args
        let flat = flat_q32_words_from_f32_args(&gfn.parameters, args)?;
        
        // Call
        let words = self.call_q32(name, &flat)?;
        
        // Decode return
        let gq = decode_q32_return(&gfn.return_type, &words)?;
        q32_to_lps_value_f32(&gfn.return_type, gq)
            .map_err(|e| NativeError::Call(CallError::TypeMismatch(e.to_string())))
    }

    fn call_q32(&mut self, name: &str, args: &[i32]) -> Result<Vec<i32>, Self::Error> {
        unsafe { self.invoke_flat(name, args) }
    }
}
```

## Notes

- `invoke_flat` needs proper ABI handling for different arities
- Sret detection needed for proper return value handling
- Stack setup for many args needs implementation

## Validate

```bash
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf
```

## Next Phase

Once engine/module/instance are created, proceed to Phase 5: Firmware integration.
