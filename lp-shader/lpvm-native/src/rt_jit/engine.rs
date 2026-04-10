//! [`LpvmEngine`] for RV32 JIT (linked in firmware, no ELF).

use alloc::sync::Arc;

use lpir::IrModule;
use lps_shared::LpsModuleSig;
use lpvm::{BumpLpvmMemory, LpvmEngine, LpvmMemory};

use crate::error::NativeError;
use crate::native_options::NativeCompileOptions;

use super::builtins::BuiltinTable;
use super::compiler::compile_module_jit;
use super::module::{NativeJitModule, NativeJitModuleInner};

/// Default shared bump arena size (matches emulator order of magnitude).
const JIT_SHARED_CAPACITY: usize = 256 * 1024;

/// Compiles LPIR to a single in-memory RV32 image with patched builtin calls.
pub struct NativeJitEngine {
    builtin_table: Arc<BuiltinTable>,
    arena: Arc<BumpLpvmMemory>,
    options: NativeCompileOptions,
}

impl NativeJitEngine {
    #[must_use]
    pub fn new(builtin_table: Arc<BuiltinTable>, options: NativeCompileOptions) -> Self {
        Self {
            builtin_table,
            arena: Arc::new(BumpLpvmMemory::new(JIT_SHARED_CAPACITY)),
            options,
        }
    }

    #[must_use]
    pub fn builtin_table(&self) -> &BuiltinTable {
        &self.builtin_table
    }
}

impl LpvmEngine for NativeJitEngine {
    type Module = NativeJitModule;
    type Error = NativeError;

    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        let (buffer, entry_offsets) = compile_module_jit(
            ir,
            meta,
            &self.builtin_table,
            self.options.float_mode,
            self.options.alloc_trace,
        )?;
        Ok(NativeJitModule {
            inner: Arc::new(NativeJitModuleInner {
                ir: ir.clone(),
                meta: meta.clone(),
                buffer,
                entry_offsets,
                arena: Arc::clone(&self.arena),
                options: self.options,
            }),
        })
    }

    fn memory(&self) -> &dyn LpvmMemory {
        self.arena.as_ref()
    }
}
