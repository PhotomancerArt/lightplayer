//! RISC-V32 relocatable ELF object emission from LPIR (feature `riscv32-emu`).

use alloc::vec::Vec;

use cranelift_codegen::isa::OwnedTargetIsa;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_object::{ObjectBuilder, ObjectModule};
use lpir::module::IrModule;
use target_lexicon::{
    Architecture, BinaryFormat, Environment, OperatingSystem, Riscv32Architecture, Triple, Vendor,
};

use crate::compile_options::CompileOptions;
use crate::error::{CompileError, CompilerError};
use crate::module_lower::{LpirFuncEmitOrder, lower_lpir_into_module};
use crate::process_sync;

/// Same triple as `lp-glsl-cranelift` `Target::riscv32_emulator` (RISC-V32 imafc, ELF).
fn riscv32_triple() -> Triple {
    Triple {
        architecture: Architecture::Riscv32(Riscv32Architecture::Riscv32imafc),
        vendor: Vendor::Unknown,
        operating_system: OperatingSystem::None_,
        environment: Environment::Unknown,
        binary_format: BinaryFormat::Elf,
    }
}

fn riscv32_flags() -> Result<cranelift_codegen::settings::Flags, CompilerError> {
    let mut flag_builder = settings::builder();
    flag_builder.set("is_pic", "true").map_err(|e| {
        CompilerError::Codegen(CompileError::cranelift(alloc::format!("is_pic: {e}")))
    })?;
    flag_builder
        .set("use_colocated_libcalls", "false")
        .map_err(|e| {
            CompilerError::Codegen(CompileError::cranelift(alloc::format!(
                "use_colocated_libcalls: {e}"
            )))
        })?;
    flag_builder
        .set("enable_multi_ret_implicit_sret", "true")
        .map_err(|e| {
            CompilerError::Codegen(CompileError::cranelift(alloc::format!(
                "enable_multi_ret_implicit_sret: {e}"
            )))
        })?;
    flag_builder
        .set("regalloc_algorithm", "single_pass")
        .map_err(|e| {
            CompilerError::Codegen(CompileError::cranelift(alloc::format!(
                "regalloc_algorithm: {e}"
            )))
        })?;
    Ok(settings::Flags::new(flag_builder))
}

fn riscv32_owned_isa() -> Result<OwnedTargetIsa, CompilerError> {
    use cranelift_codegen::isa::riscv32::isa_builder;
    let triple = riscv32_triple();
    isa_builder(triple).finish(riscv32_flags()?).map_err(|e| {
        CompilerError::Codegen(CompileError::cranelift(alloc::format!("riscv32 ISA: {e}")))
    })
}

/// Compile LPIR to a RISC-V32 ELF **relocatable object** (not linked with builtins).
pub fn object_bytes_from_ir(
    ir: &IrModule,
    options: &CompileOptions,
) -> Result<Vec<u8>, CompilerError> {
    let _codegen_guard = process_sync::codegen_guard();

    let isa = riscv32_owned_isa()?;
    let builder = ObjectBuilder::new(isa, b"lpir", cranelift_module::default_libcall_names())
        .map_err(|e| {
            CompilerError::Codegen(CompileError::cranelift(alloc::format!(
                "ObjectBuilder: {e}"
            )))
        })?;
    let mut object_module = ObjectModule::new(builder);
    lower_lpir_into_module(&mut object_module, ir, *options, LpirFuncEmitOrder::Name)?;
    let product = object_module.finish();
    product
        .emit()
        .map_err(|e| {
            CompilerError::Codegen(CompileError::cranelift(alloc::format!("object emit: {e}")))
        })
        .map(|v| v.to_vec())
}

#[cfg(all(test, feature = "riscv32-emu"))]
mod tests {
    use lpir::parse_module;

    use crate::FloatMode;
    use crate::compile_options::CompileOptions;

    use super::object_bytes_from_ir;

    #[test]
    fn object_bytes_elf_magic_integer_ir() {
        let ir = parse_module(
            r"func @add(v0:i32, v1:i32) -> i32 {
  v2:i32 = iadd v0, v1
  return v2
}
",
        )
        .expect("parse");
        let bytes = object_bytes_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::F32,
            },
        )
        .expect("object");
        assert!(bytes.len() > 4);
        assert_eq!(&bytes[0..4], b"\x7fELF");
    }
}
