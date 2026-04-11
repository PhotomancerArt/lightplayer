## Phase 6: Lpvm Trait Implementations (Stubs)

### Scope

Implement `NativeEngine`, `NativeModule`, `NativeInstance` with stub methods. These mirror `CraneliftEngine` structure but return `todo!("M2: ...")` or clear errors. Wire up `NativeCompileOptions`.

### Implementation details

**`engine.rs`:**

```rust
use lpir::module::IrModule;
use lps_shared::LpsModuleSig;
use lpvm::{LpvmEngine, LpvmMemory, LpvmBuffer, AllocError};

use crate::error::NativeError;
use crate::module::NativeModule;

#[derive(Clone, Copy, Debug, Default)]
pub struct NativeCompileOptions {
    pub float_mode: lpir::FloatMode,
}

pub struct NativeEngine {
    options: NativeCompileOptions,
    memory: BumpLpvmMemory,  // From lpvm crate, or minimal wrapper
}

impl NativeEngine {
    pub fn new(options: NativeCompileOptions) -> Self {
        Self {
            options,
            memory: BumpLpvmMemory::new(),
        }
    }
}

impl LpvmEngine for NativeEngine {
    type Module = NativeModule;
    type Error = NativeError;
    
    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        // M2: will lower all functions, allocate, emit
        Err(NativeError::NotYetImplemented("M2: compile".into()))
    }
    
    fn memory(&self) -> &dyn LpvmMemory {
        &self.memory
    }
}

// Simple bump allocator wrapper (or use lpvm::BumpLpvmMemory directly)
use lpvm::BumpLpvmMemory;
```

**`module.rs`:**

```rust
use lps_shared::LpsModuleSig;
use lpvm::{LpvmModule, LpvmInstance};

use crate::error::NativeError;
use crate::instance::NativeInstance;

pub struct NativeModule {
    // M3: will hold compiled code bytes
    _placeholder: (),
}

impl LpvmModule for NativeModule {
    type Instance = NativeInstance;
    type Error = NativeError;
    
    fn signatures(&self) -> &LpsModuleSig {
        todo!("M3: store metadata")
    }
    
    fn instantiate(&self) -> Result<Self::Instance, Self::Error> {
        Err(NativeError::NotYetImplemented("M3: instantiate".into()))
    }
}
```

**`instance.rs`:**

```rust
use alloc::vec::Vec;
use lps_shared::lps_value_f32::LpsValueF32;
use lpvm::LpvmInstance;

use crate::error::NativeError;

pub struct NativeInstance {
    // M3: will hold vmctx, code pointer
    _placeholder: (),
}

impl LpvmInstance for NativeInstance {
    type Error = NativeError;
    
    fn call(&mut self, name: &str, args: &[LpsValueF32]) -> Result<LpsValueF32, Self::Error> {
        let _ = (name, args);
        Err(NativeError::NotYetImplemented("M3: call".into()))
    }
    
    fn call_q32(&mut self, name: &str, args: &[i32]) -> Result<Vec<i32>, Self::Error> {
        let _ = (name, args);
        Err(NativeError::NotYetImplemented("M3: call_q32".into()))
    }
}
```

**`error.rs` additions:**

```rust
#[derive(Debug)]
pub enum NativeError {
    NotYetImplemented(alloc::string::String),
    Lower(LowerError),
    Alloc(AllocError),
}

impl core::fmt::Display for NativeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            NativeError::NotYetImplemented(s) => write!(f, "not yet implemented: {s}"),
            NativeError::Lower(e) => write!(f, "lower error: {e}"),
            NativeError::Alloc(e) => write!(f, "alloc error: {e}"),
        }
    }
}
```

### Validation

```bash
cargo check -p lpvm-native
cargo test -p lpvm-native --lib
```

Traits compile and type-check against `lpvm` crate.
