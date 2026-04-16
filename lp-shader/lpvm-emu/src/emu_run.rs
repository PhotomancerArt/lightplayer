//! Run linked RV32 images in `lp-riscv-emu` with LPVM shared memory at [`DEFAULT_SHARED_START`](lp_riscv_emu::DEFAULT_SHARED_START).

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;

use cranelift_codegen::data_value::DataValue;
use cranelift_codegen::ir::{ArgumentPurpose, Signature};
use cranelift_codegen::isa::{self, CallConv};
use cranelift_codegen::settings::{self, Configurable};
use lp_riscv_elf::ElfLoadInfo;
use lp_riscv_emu::{DEFAULT_RAM_START, DEFAULT_SHARED_START, LogLevel, Memory, Riscv32Emulator};
use lpir::FloatMode;
use lpir::lpir_module::LpirModule;
use lps_shared::{LpsModuleSig, LpsType};
use lpvm::DEFAULT_VMCTX_FUEL;
use lpvm::{CallError, GlslReturn, LpsValueQ32, decode_q32_return, flatten_q32_arg};
use lpvm_cranelift::error::CompileError;
use lpvm_cranelift::{CompileOptions, CompilerError, signature_for_ir_func};
use target_lexicon::Triple;

use crate::memory::DEFAULT_SHARED_CAPACITY;

/// RV32 guest [`VmContext`](lpvm::VmContext) header: `fuel` (u64 LE) + `trap` (u32) + `metadata` (u32).
pub(crate) const GUEST_VMCTX_BYTES: usize = 16;

pub(crate) fn riscv32_reference_isa()
-> Result<cranelift_codegen::isa::OwnedTargetIsa, CompilerError> {
    let mut flag_builder = settings::builder();
    flag_builder
        .set("is_pic", "false")
        .map_err(|e| CompilerError::Codegen(CompileError::cranelift(alloc::format!("{e}"))))?;
    let flags = settings::Flags::new(flag_builder);
    let triple: Triple = "riscv32imac-unknown-none-elf"
        .parse()
        .map_err(|e| CompilerError::Codegen(CompileError::cranelift(alloc::format!("{e}"))))?;
    isa::lookup(triple)
        .map_err(|e| CompilerError::Codegen(CompileError::cranelift(alloc::format!("{e}"))))?
        .finish(flags)
        .map_err(|e| CompilerError::Codegen(CompileError::cranelift(alloc::format!("{e}"))))
}

pub(crate) fn write_guest_vmctx_header(out: &mut [u8]) {
    assert!(out.len() >= GUEST_VMCTX_BYTES);
    out[0..8].copy_from_slice(&DEFAULT_VMCTX_FUEL.to_le_bytes());
    out[8..12].copy_from_slice(&0u32.to_le_bytes());
    out[12..16].copy_from_slice(&0u32.to_le_bytes());
}

fn shared_backing_for_call() -> std::sync::Arc<std::sync::Mutex<Vec<u8>>> {
    let mut v = vec![0u8; DEFAULT_SHARED_CAPACITY];
    write_guest_vmctx_header(&mut v[..GUEST_VMCTX_BYTES]);
    std::sync::Arc::new(std::sync::Mutex::new(v))
}

fn emulator_with_shared(
    load: &ElfLoadInfo,
    shared: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
) -> Riscv32Emulator {
    let mem = Memory::new_with_shared(
        load.code.clone(),
        load.ram.clone(),
        shared,
        0,
        DEFAULT_SHARED_START,
        DEFAULT_RAM_START,
    );
    Riscv32Emulator::from_memory(mem, &[]).with_log_level(LogLevel::None)
}

/// Q32 typed call through the linked RV32 image (same marshalling as [`lpvm_cranelift::CraneliftModule::call`]).
pub fn glsl_q32_call_emulated(
    load: &ElfLoadInfo,
    ir: &LpirModule,
    glsl_meta: &LpsModuleSig,
    options: &CompileOptions,
    name: &str,
    args: &[LpsValueQ32],
) -> Result<GlslReturn<LpsValueQ32>, CallError> {
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
    if gfn.parameters.len() != args.len() {
        return Err(CallError::Arity {
            expected: gfn.parameters.len(),
            got: args.len(),
        });
    }
    let ir_func = ir
        .functions
        .values()
        .find(|f| f.name == name)
        .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
    let param_count = ir_func.param_count as usize;
    let mut flat: Vec<i32> = Vec::new();
    for (p, a) in gfn.parameters.iter().zip(args.iter()) {
        flat.extend(flatten_q32_arg(p, a)?);
    }
    if flat.len() != param_count {
        return Err(CallError::Unsupported(format!(
            "flattened argument count {} does not match IR param_count {}",
            flat.len(),
            param_count
        )));
    }
    let vmctx_word = DEFAULT_SHARED_START as i32;
    let mut full: Vec<i32> = Vec::with_capacity(1 + flat.len());
    full.push(vmctx_word);
    full.extend_from_slice(&flat);
    let isa = riscv32_reference_isa().map_err(|e| CallError::Unsupported(format!("{e}")))?;
    let sig = signature_for_ir_func(
        ir_func,
        CallConv::SystemV,
        options.float_mode,
        isa.pointer_type(),
        &*isa,
    );
    let n_ret = ir_func.return_types.len();
    let entry = *load.symbol_map.get(name).ok_or_else(|| {
        CallError::Unsupported(format!("symbol `{name}` not in linked RV32 image"))
    })?;
    let data_args: Vec<DataValue> = full.iter().copied().map(DataValue::I32).collect();
    let shared = shared_backing_for_call();
    let mut emu = emulator_with_shared(load, shared);
    let has_sr = sig
        .params
        .iter()
        .any(|p| p.purpose == ArgumentPurpose::StructReturn);
    let ret = if has_sr {
        emu.call_function_with_struct_return(entry, &data_args, &sig, n_ret * 4)
            .map_err(|e| CallError::Unsupported(format!("emulator: {e:?}")))?
    } else {
        emu.call_function(entry, &data_args, &sig)
            .map_err(|e| CallError::Unsupported(format!("emulator: {e:?}")))?
    };
    let mut words = Vec::with_capacity(ret.len());
    for dv in ret {
        match dv {
            DataValue::I32(w) => words.push(w),
            other => {
                return Err(CallError::Unsupported(format!(
                    "unexpected emulator return value: {other:?}"
                )));
            }
        }
    }
    if words.len() < n_ret {
        return Err(CallError::Unsupported(format!(
            "emulator returned {} words, signature expects {}",
            words.len(),
            n_ret
        )));
    }
    words.truncate(n_ret);
    if gfn.return_type == LpsType::Void {
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
    ir: &LpirModule,
    options: &CompileOptions,
    func_name: &str,
    args: &[i32],
) -> Result<i32, CompilerError> {
    let object = lpvm_cranelift::object_bytes_from_ir(ir, options)?;
    let load = lpvm_cranelift::link_object_with_builtins(&object)?;
    run_loaded_function_i32(&load, ir, options, func_name, args)
}

/// Resolve `func_name` in `load_info` and invoke with the given Cranelift `Signature`.
pub fn run_loaded_function_i32(
    load_info: &ElfLoadInfo,
    ir: &LpirModule,
    options: &CompileOptions,
    func_name: &str,
    args: &[i32],
) -> Result<i32, CompilerError> {
    let f = ir
        .functions
        .values()
        .find(|f| f.name == func_name)
        .ok_or_else(|| {
            CompilerError::Codegen(CompileError::unsupported(format!(
                "no IR function `{func_name}`"
            )))
        })?;
    let isa = riscv32_reference_isa()?;
    let sig = signature_for_ir_func(
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
        CompilerError::Codegen(CompileError::cranelift(format!(
            "symbol `{func_name}` not in linked image"
        )))
    })?;

    let has_sr = signature
        .params
        .iter()
        .any(|p| p.purpose == ArgumentPurpose::StructReturn);
    let expected_args = if has_sr {
        signature.params.len().saturating_sub(2)
    } else {
        signature.params.len().saturating_sub(1)
    };
    if args.len() != expected_args {
        return Err(CompilerError::Codegen(CompileError::unsupported(format!(
            "`{func_name}`: expected {} args, got {}",
            expected_args,
            args.len()
        ))));
    }

    let vmctx_word = DEFAULT_SHARED_START as i32;
    let mut data_args: Vec<DataValue> = Vec::with_capacity(1 + args.len());
    data_args.push(DataValue::I32(vmctx_word));
    data_args.extend(args.iter().copied().map(DataValue::I32));

    let shared = shared_backing_for_call();
    let mut emu = emulator_with_shared(load_info, shared);

    let ret = if has_sr {
        emu.call_function_with_struct_return(entry, &data_args, signature, 4)
            .map_err(|e| {
                CompilerError::Codegen(CompileError::cranelift(format!("emulator: {e:?}")))
            })?
    } else {
        emu.call_function(entry, &data_args, signature)
            .map_err(|e| {
                CompilerError::Codegen(CompileError::cranelift(format!("emulator: {e:?}")))
            })?
    };

    match ret.as_slice() {
        [DataValue::I32(v)] => Ok(*v),
        _ => Err(CompilerError::Codegen(CompileError::cranelift(format!(
            "expected single i32 return, got {ret:?}"
        )))),
    }
}
