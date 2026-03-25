//! Run linked RV32 images in `lp-riscv-emu` (feature `riscv32-emu`).
//!
//! Multi-return / struct-return paths are not handled here yet.

use alloc::vec::Vec;

use cranelift_codegen::data_value::DataValue;
use cranelift_codegen::ir::Signature;
use cranelift_codegen::isa::CallConv;
use lp_riscv_elf::ElfLoadInfo;
use lp_riscv_emu::{LogLevel, Riscv32Emulator};
use lpir::module::IrModule;

use crate::compile_options::CompileOptions;
use crate::emit;
use crate::error::CompilerError;
use crate::object_link::link_object_with_builtins;
use crate::object_module::object_bytes_from_ir;

/// Run `func_name` after object emission, link, and load. Arguments and single `i32` return (Q32 / int).
pub fn run_lpir_function_i32(
    ir: &IrModule,
    options: &CompileOptions,
    func_name: &str,
    args: &[i32],
) -> Result<i32, CompilerError> {
    let object = object_bytes_from_ir(ir, options)?;
    let load = link_object_with_builtins(&object)?;
    run_loaded_function_i32(&load, ir, options, func_name, args)
}

/// Resolve `func_name` in `load_info` and invoke with the given Cranelift `Signature`.
pub fn run_loaded_function_i32(
    load_info: &ElfLoadInfo,
    ir: &IrModule,
    options: &CompileOptions,
    func_name: &str,
    args: &[i32],
) -> Result<i32, CompilerError> {
    let f = ir
        .functions
        .iter()
        .find(|f| f.name == func_name)
        .ok_or_else(|| {
            CompilerError::Codegen(crate::error::CompileError::unsupported(format!(
                "no IR function `{func_name}`"
            )))
        })?;
    let sig = emit::signature_for_ir_func(f, CallConv::SystemV, options.float_mode);
    run_loaded_function_i32_with_sig(load_info, &sig, func_name, args)
}

pub(crate) fn run_loaded_function_i32_with_sig(
    load_info: &ElfLoadInfo,
    signature: &Signature,
    func_name: &str,
    args: &[i32],
) -> Result<i32, CompilerError> {
    let entry = *load_info.symbol_map.get(func_name).ok_or_else(|| {
        CompilerError::Codegen(crate::error::CompileError::cranelift(format!(
            "symbol `{func_name}` not in linked image"
        )))
    })?;

    if args.len() != signature.params.len() {
        return Err(CompilerError::Codegen(
            crate::error::CompileError::unsupported(format!(
                "`{func_name}`: expected {} args, got {}",
                signature.params.len(),
                args.len()
            )),
        ));
    }

    let data_args: Vec<DataValue> = args.iter().copied().map(DataValue::I32).collect();

    let mut emu = Riscv32Emulator::new(load_info.code.clone(), load_info.ram.clone())
        .with_log_level(LogLevel::None);

    let ret = emu
        .call_function(entry, &data_args, signature)
        .map_err(|e| {
            CompilerError::Codegen(crate::error::CompileError::cranelift(format!(
                "emulator: {e:?}"
            )))
        })?;

    match ret.as_slice() {
        [DataValue::I32(v)] => Ok(*v),
        _ => Err(CompilerError::Codegen(
            crate::error::CompileError::cranelift(format!(
                "expected single i32 return, got {ret:?}"
            )),
        )),
    }
}

#[cfg(all(test, feature = "riscv32-emu"))]
mod tests {
    use lpir::parse_module;

    use crate::FloatMode;
    use crate::compile_options::CompileOptions;
    use crate::object_link::builtins_executable_bytes;
    use crate::q32::q32_encode_f64;

    use super::run_lpir_function_i32;

    #[test]
    #[ignore = "requires lp-glsl-builtins-emu-app; build with scripts/build-builtins.sh"]
    fn emu_q32_fadd_constants_returns_three() {
        assert!(
            !builtins_executable_bytes().is_empty(),
            "builtins exe missing"
        );
        let ir = parse_module(
            r"func @main() -> f32 {
  v0:f32 = fconst.f32 1.0
  v1:f32 = fconst.f32 2.0
  v2:f32 = fadd v0, v1
  return v2
}
",
        )
        .expect("parse");
        let opts = CompileOptions {
            float_mode: FloatMode::Q32,
        };
        let out = run_lpir_function_i32(&ir, &opts, "main", &[]).expect("emu");
        assert_eq!(out, q32_encode_f64(3.0));
    }
}
