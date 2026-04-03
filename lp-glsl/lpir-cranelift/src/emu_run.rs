//! Run linked RV32 images in `lp-riscv-emu` (feature `riscv32-emu`).
//!
//! Multi-return / struct-return paths are not handled here yet.

use alloc::vec::Vec;

use cranelift_codegen::data_value::DataValue;
use cranelift_codegen::ir::{ArgumentPurpose, Signature};
use cranelift_codegen::isa::{self, CallConv};
use cranelift_codegen::settings::{self, Configurable};
use lp_glsl_abi::{GlslModuleMeta, GlslType};
use lp_riscv_elf::ElfLoadInfo;
use lp_riscv_emu::{LogLevel, Riscv32Emulator};
use lpir::FloatMode;
use lpir::module::IrModule;
use target_lexicon::Triple;

use crate::compile_options::CompileOptions;
use crate::emit;
use crate::error::CompilerError;
use crate::object_link::link_object_with_builtins;
use crate::object_module::object_bytes_from_ir;
use crate::values::{CallError, GlslQ32, GlslReturn, decode_q32_return, flatten_q32_arg};

fn riscv32_reference_isa() -> Result<cranelift_codegen::isa::OwnedTargetIsa, CompilerError> {
    let mut flag_builder = settings::builder();
    flag_builder.set("is_pic", "false").map_err(|e| {
        CompilerError::Codegen(crate::error::CompileError::cranelift(alloc::format!("{e}")))
    })?;
    let flags = settings::Flags::new(flag_builder);
    let triple: Triple = "riscv32imac-unknown-none-elf".parse().map_err(|e| {
        CompilerError::Codegen(crate::error::CompileError::cranelift(alloc::format!("{e}")))
    })?;
    isa::lookup(triple)
        .map_err(|e| {
            CompilerError::Codegen(crate::error::CompileError::cranelift(alloc::format!("{e}")))
        })?
        .finish(flags)
        .map_err(|e| {
            CompilerError::Codegen(crate::error::CompileError::cranelift(alloc::format!("{e}")))
        })
}

/// Q32 typed call through the linked RV32 image (same marshalling as [`crate::JitModule::call`]).
pub fn glsl_q32_call_emulated(
    load: &ElfLoadInfo,
    ir: &IrModule,
    glsl_meta: &GlslModuleMeta,
    options: &CompileOptions,
    name: &str,
    args: &[GlslQ32],
) -> Result<GlslReturn<GlslQ32>, CallError> {
    if options.float_mode != FloatMode::Q32 {
        return Err(CallError::Unsupported(
            "emulated Q32 call requires FloatMode::Q32".into(),
        ));
    }
    let gfn = glsl_meta
        .functions
        .iter()
        .find(|f| f.name == name)
        .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
    if gfn.params.len() != args.len() {
        return Err(CallError::Arity {
            expected: gfn.params.len(),
            got: args.len(),
        });
    }
    let idx = ir
        .functions
        .iter()
        .position(|f| f.name == name)
        .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
    let ir_func = &ir.functions[idx];
    let param_count = ir_func.param_count as usize;
    let mut flat: Vec<i32> = Vec::new();
    for (p, a) in gfn.params.iter().zip(args.iter()) {
        flat.extend(flatten_q32_arg(p, a)?);
    }
    if flat.len() != param_count {
        return Err(CallError::Unsupported(alloc::format!(
            "flattened argument count {} does not match IR param_count {}",
            flat.len(),
            param_count
        )));
    }
    let isa = riscv32_reference_isa().map_err(|e| CallError::Unsupported(alloc::format!("{e}")))?;
    let sig = emit::signature_for_ir_func(
        ir_func,
        CallConv::SystemV,
        options.float_mode,
        isa.pointer_type(),
        &*isa,
    );
    let n_ret = ir_func.return_types.len();
    let entry = *load.symbol_map.get(name).ok_or_else(|| {
        CallError::Unsupported(alloc::format!("symbol `{name}` not in linked RV32 image"))
    })?;
    let data_args: Vec<DataValue> = flat.iter().copied().map(DataValue::I32).collect();
    let mut emu =
        Riscv32Emulator::new(load.code.clone(), load.ram.clone()).with_log_level(LogLevel::None);
    let has_sr = sig
        .params
        .iter()
        .any(|p| p.purpose == ArgumentPurpose::StructReturn);
    let ret = if has_sr {
        emu.call_function_with_struct_return(entry, &data_args, &sig, n_ret * 4)
            .map_err(|e| CallError::Unsupported(alloc::format!("emulator: {e:?}")))?
    } else {
        emu.call_function(entry, &data_args, &sig)
            .map_err(|e| CallError::Unsupported(alloc::format!("emulator: {e:?}")))?
    };
    let mut words = Vec::with_capacity(ret.len());
    for dv in ret {
        match dv {
            DataValue::I32(w) => words.push(w),
            other => {
                return Err(CallError::Unsupported(alloc::format!(
                    "unexpected emulator return value: {other:?}"
                )));
            }
        }
    }
    if words.len() < n_ret {
        return Err(CallError::Unsupported(alloc::format!(
            "emulator returned {} words, signature expects {}",
            words.len(),
            n_ret
        )));
    }
    words.truncate(n_ret);
    if gfn.return_type == GlslType::Void {
        return Ok(GlslReturn {
            value: None,
            outs: Vec::new(),
        });
    }
    let value = decode_q32_return(&gfn.return_type, &words)?;
    Ok(GlslReturn {
        value: Some(value),
        outs: Vec::new(),
    })
}

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
    let isa = riscv32_reference_isa()?;
    let sig = emit::signature_for_ir_func(
        f,
        CallConv::SystemV,
        options.float_mode,
        isa.pointer_type(),
        &*isa,
    );
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

    let has_sr = signature
        .params
        .iter()
        .any(|p| p.purpose == ArgumentPurpose::StructReturn);
    let expected_args = if has_sr {
        signature.params.len().saturating_sub(1)
    } else {
        signature.params.len()
    };
    if args.len() != expected_args {
        return Err(CompilerError::Codegen(
            crate::error::CompileError::unsupported(format!(
                "`{func_name}`: expected {} args, got {}",
                expected_args,
                args.len()
            )),
        ));
    }

    let data_args: Vec<DataValue> = args.iter().copied().map(DataValue::I32).collect();

    let mut emu = Riscv32Emulator::new(load_info.code.clone(), load_info.ram.clone())
        .with_log_level(LogLevel::None);

    let ret = if has_sr {
        emu.call_function_with_struct_return(entry, &data_args, signature, 4)
            .map_err(|e| {
                CompilerError::Codegen(crate::error::CompileError::cranelift(format!(
                    "emulator: {e:?}"
                )))
            })?
    } else {
        emu.call_function(entry, &data_args, signature)
            .map_err(|e| {
                CompilerError::Codegen(crate::error::CompileError::cranelift(format!(
                    "emulator: {e:?}"
                )))
            })?
    };

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
            ..Default::default()
        };
        let out = run_lpir_function_i32(&ir, &opts, "main", &[]).expect("emu");
        assert_eq!(out, q32_encode_f64(3.0));
    }
}
