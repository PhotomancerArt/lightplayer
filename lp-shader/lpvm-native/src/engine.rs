//! [`LpvmEngine`] — compile LPIR module to RV32 ELF.

use lpir::IrModule;
use lps_shared::LpsModuleSig;
use lpvm::{BumpLpvmMemory, LpvmEngine, LpvmMemory};

use crate::error::NativeError;
use crate::isa::rv32::emit::emit_module_elf;
use crate::module::NativeModule;

/// Backend-specific compile options (not shared with Cranelift / WASM).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeCompileOptions {
    pub float_mode: lpir::FloatMode,
    /// When true, emission records LPIR op indices per instruction (for disassembly / future DWARF).
    pub debug_info: bool,
}

impl Default for NativeCompileOptions {
    fn default() -> Self {
        Self {
            float_mode: lpir::FloatMode::Q32,
            debug_info: false,
        }
    }
}

/// Default bump arena size for shared memory until firmware wires a real region.
const DEFAULT_BUMP_BYTES: usize = 64 * 1024;

/// Native code generator: LPIR → RV32 ELF object.
pub struct NativeEngine {
    pub options: NativeCompileOptions,
    memory: BumpLpvmMemory,
}

impl NativeEngine {
    pub fn new(options: NativeCompileOptions) -> Self {
        Self {
            options,
            memory: BumpLpvmMemory::new(DEFAULT_BUMP_BYTES),
        }
    }
}

impl LpvmEngine for NativeEngine {
    type Module = NativeModule;
    type Error = NativeError;

    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        let elf = emit_module_elf(ir, self.options.float_mode)?;
        Ok(NativeModule::from_parts(elf, meta.clone()))
    }

    fn memory(&self) -> &dyn LpvmMemory {
        &self.memory
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::vec;

    use lpir::types::VRegRange;
    use lpir::{IrFunction, IrModule, Op};
    use lps_shared::LpsModuleSig;

    use super::*;

    fn minimal_iadd_module() -> IrModule {
        IrModule {
            imports: vec![],
            functions: vec![IrFunction {
                name: String::from("add"),
                is_entry: true,
                vmctx_vreg: lpir::VReg(0),
                param_count: 2,
                return_types: vec![lpir::IrType::I32],
                vreg_types: vec![
                    lpir::IrType::I32,
                    lpir::IrType::I32,
                    lpir::IrType::I32,
                    lpir::IrType::I32,
                ],
                slots: vec![],
                body: vec![
                    Op::Iadd {
                        dst: lpir::VReg(3),
                        lhs: lpir::VReg(1),
                        rhs: lpir::VReg(2),
                    },
                    Op::Return {
                        values: VRegRange { start: 0, count: 1 },
                    },
                ],
                vreg_pool: vec![lpir::VReg(3)],
            }],
        }
    }

    #[test]
    fn compile_produces_elf_magic() {
        let engine = NativeEngine::new(NativeCompileOptions::default());
        let ir = minimal_iadd_module();
        let meta = LpsModuleSig::default();
        let m = engine.compile(&ir, &meta).expect("compile");
        assert!(m.elf.len() > 16);
        assert_eq!(&m.elf[0..4], &[0x7f, b'E', b'L', b'F']);
    }
}
