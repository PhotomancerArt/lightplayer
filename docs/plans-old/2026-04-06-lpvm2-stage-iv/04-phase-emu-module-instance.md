## Phase 4: Implement EmuModule and EmuInstance

Complete the LPVM trait implementations with `LpvmModule` and `LpvmInstance`.

### Code Organization

**File: `lp-shader/lpvm-emu/src/module.rs`**

Place `EmuModule` struct and `LpvmModule` impl at top.

```rust
//! EmuModule - compiled RV32 module for emulator.

use alloc::sync::Arc;
use lp_riscv_elf::ElfLoadInfo;
use lpvm::LpvmModule;
use lps_shared::LpsModuleSig;
use parking_lot::Mutex;

use crate::instance::{EmuInstance, InstanceError};
use crate::memory::EmuMemory;

/// Compiled module for RV32 emulator.
///
/// Stores the linked ElfLoadInfo (code bytes, symbol map, traps) ready for
/// instantiation. Multiple instances can be created, each with independent
/// RAM but sharing the same code.
pub struct EmuModule {
    load_info: ElfLoadInfo,
    signatures: LpsModuleSig,
}

impl EmuModule {
    pub(crate) fn new(load_info: ElfLoadInfo, signatures: LpsModuleSig) -> Self {
        Self {
            load_info,
            signatures,
        }
    }
    
    /// Get the ElfLoadInfo (for instance creation).
    pub(crate) fn load_info(&self) -> &ElfLoadInfo {
        &self.load_info
    }
}

impl LpvmModule for EmuModule {
    type Instance = EmuInstance;
    type Error = InstanceError;
    
    fn signatures(&self) -> &LpsModuleSig {
        &self.signatures
    }
    
    fn instantiate(&self) -> Result<Self::Instance, Self::Error> {
        // Create new EmuInstance with:
        // - Clone of code from load_info
        // - Fresh RAM (clone from load_info.ram or empty?)
        // - Reference to shared memory (from engine - but we don't have engine here!)
        
        unimplemented!("Need to pass shared memory reference somehow")
    }
}
```

**Problem**: `LpvmModule::instantiate(&self)` doesn't have access to the engine/shared memory. Need to rethink.

**Solution**: Store `Arc<Mutex<Vec<u8>>>` (shared memory) in `EmuModule` at compile time. This is reasonable because the shared memory is fixed per engine, and the module is tied to that engine.

Updated `EmuModule`:

```rust
pub struct EmuModule {
    load_info: ElfLoadInfo,
    signatures: LpsModuleSig,
    shared_memory: Arc<Mutex<Vec<u8>>>,  // Reference to engine's shared memory
    shared_start: u32,  // 0x40000000
}

impl EmuModule {
    pub(crate) fn new(
        load_info: ElfLoadInfo,
        signatures: LpsModuleSig,
        shared_memory: Arc<Mutex<Vec<u8>>>,
        shared_start: u32,
    ) -> Self {
        Self {
            load_info,
            signatures,
            shared_memory,
            shared_start,
        }
    }
}

impl LpvmModule for EmuModule {
    type Instance = EmuInstance;
    type Error = InstanceError;
    
    fn signatures(&self) -> &LpsModuleSig {
        &self.signatures
    }
    
    fn instantiate(&self) -> Result<Self::Instance, Self::Error> {
        EmuInstance::new(
            self.load_info.clone(),
            self.shared_memory.clone(),
            self.shared_start,
        )
    }
}
```

Update `EmuEngine::compile()` to pass shared memory to EmuModule.

**File: `lp-shader/lpvm-emu/src/instance.rs`**

```rust
//! EmuInstance - execution instance for RV32 emulator.

use alloc::sync::Arc;
use alloc::vec::Vec;
use lp_riscv_elf::ElfLoadInfo;
use lp_riscv_emu::{Memory, Riscv32Emulator};
use lpvm::{LpvmBuffer, LpvmInstance, LpvmMemory, LpvmPtr};
use lps_shared::lps_value::LpsValue;
use parking_lot::Mutex;

/// Instance error type.
#[derive(Debug)]
pub enum InstanceError {
    Emulator(String),
    UnknownFunction(String),
    TypeMismatch(String),
}

impl core::fmt::Display for InstanceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Emulator(e) => write!(f, "emulator error: {e}"),
            Self::UnknownFunction(n) => write!(f, "unknown function: {n}"),
            Self::TypeMismatch(e) => write!(f, "type mismatch: {e}"),
        }
    }
}

/// Execution instance for RV32 emulator.
pub struct EmuInstance {
    emulator: Riscv32Emulator,
    // Keep load_info to access symbol map
    load_info: ElfLoadInfo,
}

impl EmuInstance {
    pub(crate) fn new(
        load_info: ElfLoadInfo,
        shared_memory: Arc<Mutex<Vec<u8>>>,
        shared_start: u32,
    ) -> Result<Self, InstanceError> {
        // Create Memory with three regions
        let mem = Memory::new_with_shared(
            load_info.code.clone(),
            load_info.ram.clone(),  // Fresh RAM copy
            shared_memory,
            0x0,          // code_start
            shared_start, // shared_start
            0x80000000,   // ram_start
        );
        
        // Create emulator with traps from load_info
        let emulator = Riscv32Emulator::with_traps(
            load_info.code.clone(),
            load_info.ram.clone(),
            &load_info.traps,
        );
        // TODO: Need to set Memory in emulator - Riscv32Emulator::with_traps
        // doesn't currently accept custom Memory. May need to add constructor
        // or modify approach.
        
        unimplemented!("Need to integrate Memory with Riscv32Emulator")
    }
}

impl LpvmInstance for EmuInstance {
    type Error = InstanceError;
    
    fn call(&mut self, name: &str, args: &[LpsValue]) -> Result<LpsValue, Self::Error> {
        // 1. Look up function in symbol map
        let entry = self.load_info.symbol_map.get(name)
            .ok_or_else(|| InstanceError::UnknownFunction(name.to_string()))?;
        
        // 2. Allocate VmContext in shared memory
        // TODO: Need LpvmMemory reference to allocate
        
        // 3. Marshal args (LpsValue -> i32/DataValue)
        
        // 4. Call emulator
        
        // 5. Unmarshal return (LpsValue)
        
        unimplemented!("Full LPVM call not yet implemented")
    }
}
```

**Issue**: `Riscv32Emulator` doesn't currently expose a way to set custom Memory with shared region. Need to check current API.

Looking at `lp-riscv-emu/src/emu/emulator/state.rs`:
- `Riscv32Emulator::with_traps(code, ram, traps)` - uses `Memory::with_default_addresses`
- Need to add `with_memory(Memory)` or similar

### Tests

Add to `lp-shader/lpvm-emu/src/lib.rs` test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use lpir::parse_module;
    use lpvm::{LpvmEngine, LpvmInstance, LpvmModule};
    
    #[test]
    fn compile_and_instantiate() {
        let engine = EmuEngine::new(Default::default());
        
        let ir = parse_module(r#"
            func @test() -> i32 {
                v0:i32 = iconst.i32 42
                return v0
            }
        "#).unwrap();
        
        let meta = lps_shared::LpsModuleSig::default(); // TODO: create proper meta
        
        let module = engine.compile(&ir, &meta).unwrap();
        let instance = module.instantiate().unwrap();
        
        // Can't call yet (not implemented) but instantiation works
    }
}
```

### Validate

```bash
cargo check -p lpvm-emu
cargo check -p lpvm-emu --no-default-features
```

This phase will likely have compilation issues due to API mismatches. Document them and we'll fix in the next phase.
