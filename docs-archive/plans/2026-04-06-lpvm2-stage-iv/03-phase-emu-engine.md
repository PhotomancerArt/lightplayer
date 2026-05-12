## Phase 3: Implement EmuEngine

Implement `LpvmEngine` trait for the emulator. This provides compilation and shared memory.

### Code Organization

**File: `lp-shader/lpvm-emu/src/engine.rs`**

Place `EmuEngine` struct and `LpvmEngine` impl at top. Helper methods below.

```rust
//! EmuEngine - LPVM engine for RV32 emulator.

use alloc::sync::Arc;
use lpir::module::IrModule;
use lpvm::{AllocError, LpvmBuffer, LpvmEngine, LpvmMemory, LpvmPtr};
use lps_shared::LpsModuleSig;
use lpvm_cranelift::{CompileOptions, CompilerError, object_bytes_from_ir};
use parking_lot::Mutex;

use crate::compile::CompileError;
use crate::memory::EmuMemory;
use crate::module::EmuModule;

/// Engine for compiling and running shaders in the RV32 emulator.
///
/// Owns the shared memory region that all instances from this engine share.
pub struct EmuEngine {
    options: CompileOptions,
    memory: EmuMemory,
    shared_arc: Arc<Mutex<Vec<u8>>>,  // Shared with emulator instances
}

impl EmuEngine {
    pub fn new(options: CompileOptions) -> Self {
        let memory = EmuMemory::new();
        let shared_arc = Arc::new(Mutex::new(memory.buffer().clone()));
        // TODO: Actually we need the Arc to point to the same memory
        // Need to restructure - EmuMemory should use Arc internally
        unimplemented!("Need to restructure EmuMemory to use Arc<Mutex<Vec<u8>>> for sharing")
    }
    
    pub fn with_memory_size(options: CompileOptions, bytes: usize) -> Self {
        unimplemented!()
    }
    
    /// Get the shared memory Arc for creating emulator instances.
    pub(crate) fn shared_memory(&self) -> Arc<Mutex<Vec<u8>>> {
        self.shared_arc.clone()
    }
    
    /// Get compile options.
    pub fn options(&self) -> &CompileOptions {
        &self.options
    }
}

impl LpvmEngine for EmuEngine {
    type Module = EmuModule;
    type Error = CompileError;
    
    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        // 1. Generate RV32 object code via lpvm-cranelift
        let object_bytes = object_bytes_from_ir(ir, &self.options)
            .map_err(|e| CompileError::Codegen(e))?;
        
        // 2. Link with builtins (via lpvm-cranelift::link_object_with_builtins)
        let load_info = lpvm_cranelift::link_object_with_builtins(&object_bytes)
            .map_err(|e| CompileError::Link(e))?;
        
        // 3. Create EmuModule with the linked ElfLoadInfo
        Ok(EmuModule::new(load_info, meta.clone()))
    }
    
    fn memory(&self) -> &dyn LpvmMemory {
        &self.memory
    }
}

/// Errors during compilation/linking for emu target.
#[derive(Debug)]
pub enum CompileError {
    Codegen(CompilerError),
    Link(String),  // lp-riscv-elf error
    Unsupported(&'static str),
}

impl core::fmt::Display for CompileError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Codegen(e) => write!(f, "codegen error: {e}"),
            Self::Link(e) => write!(f, "link error: {e}"),
            Self::Unsupported(s) => write!(f, "unsupported: {s}"),
        }
    }
}
```

### Restructure EmuMemory

Need to update `EmuMemory` to use `Arc<Mutex<Vec<u8>>>` internally so it can be shared with emulator instances:

**File: `lp-shader/lpvm-emu/src/memory.rs` updates:**

```rust
use alloc::sync::Arc;
use parking_lot::Mutex;

pub struct EmuMemory {
    buffer: Arc<Mutex<Vec<u8>>>,
    next_offset: AtomicUsize,
}

impl EmuMemory {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_SHARED_BYTES)
    }
    
    pub fn with_capacity(bytes: usize) -> Self {
        let buffer = Arc::new(Mutex::new(vec![0u8; bytes]));
        Self {
            buffer,
            next_offset: AtomicUsize::new(0),
        }
    }
    
    /// Clone the Arc for sharing with emulator instances.
    pub fn share(&self) -> Arc<Mutex<Vec<u8>>> {
        self.buffer.clone()
    }
}

// Update alloc implementation to use self.buffer.lock() for access
```

### New file: `lp-shader/lpvm-emu/src/compile.rs`

Error types and helper types for compilation:

```rust
//! Compilation helpers and error types.

use lpvm_cranelift::CompilerError;

pub use lpvm_cranelift::CompileOptions;

/// Compilation error for emu target.
#[derive(Debug)]
pub enum CompileError {
    Codegen(CompilerError),
    Link(String),
    Unsupported(&'static str),
}

impl core::fmt::Display for CompileError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Codegen(e) => write!(f, "codegen error: {e}"),
            Self::Link(e) => write!(f, "link error: {e}"),
            Self::Unsupported(s) => write!(f, "unsupported: {s}"),
        }
    }
}
```

### Validate

```bash
cargo check -p lpvm-emu  # Verify it compiles
cargo check -p lpvm-emu --no-default-features  # no_std still works
```

Don't worry about tests yet - those need EmuModule which comes next phase.
