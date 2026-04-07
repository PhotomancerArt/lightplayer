//! LPVM trait implementation: CraneliftEngine

use lpir::module::IrModule;
use lps_shared::LpsModuleSig;
#[cfg(not(feature = "std"))]
use lpvm::BumpLpvmMemory;
use lpvm::{LpvmEngine, LpvmMemory};

use crate::compile_options::CompileOptions;
#[cfg(feature = "std")]
use crate::cranelift_host_memory::CraneliftHostMemory;
use crate::error::CompilerError;
use crate::lpvm_module::CraneliftModule;

/// Default shared-memory arena size for [`CraneliftEngine`] when built without `std` (bump heap).
#[cfg(not(feature = "std"))]
const DEFAULT_LPVM_SHARED_MEMORY_BYTES: usize = 256 * 1024;

/// Cranelift JIT engine implementing [`LpvmEngine`].
///
/// This is the new trait-based API for LPVM compilation. It coexists with
/// the existing [`crate::jit_module::JitModule`] API until M7 (migration complete).
///
/// # Memory
///
/// With **`std`**, shared memory uses the host heap ([`CraneliftHostMemory`]): `LpvmBuffer` carries
/// the same address as `guest_base` (single address space for JIT).
///
/// Without **`std`**, a fixed [`BumpLpvmMemory`] arena is used until a no_std allocator + lock
/// strategy is wired.
pub struct CraneliftEngine {
    options: CompileOptions,
    #[cfg(feature = "std")]
    shared_memory: CraneliftHostMemory,
    #[cfg(not(feature = "std"))]
    shared_memory: BumpLpvmMemory,
}

impl CraneliftEngine {
    /// Create a new Cranelift JIT engine with the given compile options.
    pub fn new(options: CompileOptions) -> Self {
        Self {
            options,
            #[cfg(feature = "std")]
            shared_memory: CraneliftHostMemory::new(),
            #[cfg(not(feature = "std"))]
            shared_memory: BumpLpvmMemory::new(DEFAULT_LPVM_SHARED_MEMORY_BYTES),
        }
    }
}

impl LpvmEngine for CraneliftEngine {
    type Module = CraneliftModule;
    type Error = CompilerError;

    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        CraneliftModule::compile(ir, meta, self.options)
    }

    fn memory(&self) -> &dyn LpvmMemory {
        &self.shared_memory
    }
}
