//! [`LpvmInstance`] implementation for emulated native RV32 execution.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use cranelift_codegen::data_value::DataValue;
use cranelift_codegen::ir::ArgumentPurpose;
use cranelift_codegen::isa::CallConv;
use lp_riscv_emu::{DEFAULT_SHARED_START, LogLevel, Memory, Riscv32Emulator};
use lpir::FloatMode;
use lps_shared::{LpsType, ParamQualifier, lps_value_f32::LpsValueF32};
use lpvm::{
    CallError, LpvmInstance, decode_q32_return, flat_q32_words_from_f32_args, glsl_component_count,
    q32_to_lps_value_f32,
};
use lpvm_cranelift::{CompileOptions, signature_for_ir_func};
use lpvm_emu::{GUEST_VMCTX_BYTES, riscv32_lpvm_reference_isa, write_guest_vmctx_header};

use crate::error::NativeError;

use super::NativeEmuModule;

/// Per-instance emulation state with VMContext.
pub struct NativeEmuInstance {
    pub(crate) module: NativeEmuModule,
    pub(crate) vmctx_guest: u32,
    pub(crate) last_debug: Option<String>,
    pub(crate) last_guest_instruction_count: Option<u64>,
}

impl NativeEmuInstance {
    fn refresh_vmctx_header(&self) {
        let off =
            (u64::from(self.vmctx_guest) - u64::from(self.module.arena.shared_start())) as usize;
        let mut v = self.module.arena.lock_storage();
        if off + GUEST_VMCTX_BYTES <= v.len() {
            write_guest_vmctx_header(&mut v[off..off + GUEST_VMCTX_BYTES]);
        }
    }

    fn cranelift_options(&self) -> CompileOptions {
        CompileOptions {
            float_mode: self.module.options.float_mode,
            ..Default::default()
        }
    }

    fn invoke_flat(&mut self, name: &str, flat: &[i32]) -> Result<Vec<i32>, NativeError> {
        self.last_guest_instruction_count = None;
        self.refresh_vmctx_header();

        let idx = self
            .module
            .ir
            .functions
            .iter()
            .position(|f| f.name == name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
        let ir_func = &self.module.ir.functions[idx];

        let mut full: Vec<i32> = Vec::with_capacity(1 + flat.len());
        full.push(self.vmctx_guest as i32);
        full.extend_from_slice(flat);

        let isa = riscv32_lpvm_reference_isa()
            .map_err(|e| NativeError::Call(CallError::Unsupported(format!("{e}"))))?;
        let opts = self.cranelift_options();
        let sig = signature_for_ir_func(
            ir_func,
            CallConv::SystemV,
            opts.float_mode,
            isa.pointer_type(),
            &*isa,
        );
        let n_ret = ir_func.return_types.len();
        let entry = *self.module.load.symbol_map.get(name).ok_or_else(|| {
            CallError::Unsupported(format!("symbol `{name}` not in linked RV32 image"))
        })?;

        let data_args: Vec<DataValue> = full.iter().copied().map(DataValue::I32).collect();
        let shared = self.module.arena.storage_arc();
        let mem = Memory::new_with_shared(
            self.module.load.code.clone(),
            self.module.load.ram.clone(),
            shared,
            0,
            DEFAULT_SHARED_START,
            lp_riscv_emu::DEFAULT_RAM_START,
        );
        let log_level = if self.module.options.emu_trace_instructions {
            LogLevel::Instructions
        } else {
            LogLevel::None
        };
        let mut emu = Riscv32Emulator::from_memory(mem, &[]).with_log_level(log_level);

        // The emulator handles sret natively: call_function_with_struct_return
        // allocates a buffer on the emulator stack, passes its address in a0
        // (shifting other args), executes, and reads results back as DataValues.
        let has_sr = sig
            .params
            .iter()
            .any(|p| p.purpose == ArgumentPurpose::StructReturn);
        let ret_result = if has_sr {
            emu.call_function_with_struct_return(entry, &data_args, &sig, n_ret * 4)
        } else {
            emu.call_function(entry, &data_args, &sig)
        };

        match ret_result {
            Ok(ret) => {
                let n_inst = emu.get_instruction_count();
                if self.module.options.emu_trace_instructions {
                    let mut debug_parts = Vec::new();
                    debug_parts.push(String::from("=== Debug Info ==="));
                    debug_parts.push(String::from("Execution completed normally."));
                    debug_parts.push(emu.dump_state());
                    debug_parts.push(emu.format_debug_info(
                        Some(emu.get_pc()),
                        lp_riscv_emu::config::INSTRUCTION_LOG_DISPLAY_COUNT,
                    ));
                    self.last_debug = Some(debug_parts.join("\n\n"));
                } else {
                    self.last_debug = None;
                }
                let mut words = Vec::with_capacity(ret.len());
                for dv in ret {
                    match dv {
                        DataValue::I32(w) => words.push(w),
                        other => {
                            return Err(NativeError::Call(CallError::Unsupported(format!(
                                "unexpected emulator return value: {other:?}"
                            ))));
                        }
                    }
                }
                if words.len() < n_ret {
                    return Err(NativeError::Call(CallError::Unsupported(format!(
                        "emulator returned {} words, signature expects {}",
                        words.len(),
                        n_ret
                    ))));
                }
                words.truncate(n_ret);
                self.last_guest_instruction_count = Some(n_inst);
                Ok(words)
            }
            Err(e) => {
                self.last_guest_instruction_count = None;
                // Capture full debug info including disassembly and instruction log
                let mut debug_parts = Vec::new();
                debug_parts.push(format!("=== Debug Info ==="));
                debug_parts.push(format!("Error: {e:?}"));
                debug_parts.push(emu.dump_state());
                debug_parts.push(emu.format_debug_info(
                    Some(emu.get_pc()),
                    lp_riscv_emu::config::INSTRUCTION_LOG_DISPLAY_COUNT,
                ));
                let debug_info = debug_parts.join("\n\n");
                self.last_debug = Some(debug_info);
                Err(NativeError::Call(CallError::Unsupported(format!(
                    "emulator: {e:?}"
                ))))
            }
        }
    }
}

impl LpvmInstance for NativeEmuInstance {
    type Error = NativeError;

    fn call(&mut self, name: &str, args: &[LpsValueF32]) -> Result<LpsValueF32, Self::Error> {
        self.last_debug = None;
        self.last_guest_instruction_count = None;
        if self.module.options.float_mode != FloatMode::Q32 {
            return Err(NativeError::Call(CallError::Unsupported(String::from(
                "NativeEmuInstance::call requires FloatMode::Q32",
            ))));
        }

        let gfn = self
            .module
            .meta
            .functions
            .iter()
            .find(|f| f.name == name)
            .cloned()
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;

        for p in &gfn.parameters {
            if matches!(p.qualifier, ParamQualifier::Out | ParamQualifier::InOut) {
                return Err(NativeError::Call(CallError::Unsupported(String::from(
                    "out/inout parameters are not supported for direct calling.",
                ))));
            }
        }

        if gfn.return_type == LpsType::Void {
            return Err(NativeError::Call(CallError::Unsupported(String::from(
                "void return is not represented as LpsValue; use a typed return",
            ))));
        }

        if gfn.parameters.len() != args.len() {
            return Err(NativeError::Call(CallError::Arity {
                expected: gfn.parameters.len(),
                got: args.len(),
            }));
        }

        let flat = flat_q32_words_from_f32_args(&gfn.parameters, args)?;
        let idx = self
            .module
            .ir
            .functions
            .iter()
            .position(|f| f.name == name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
        let ir_func = &self.module.ir.functions[idx];
        let param_count = ir_func.param_count as usize;
        if flat.len() != param_count {
            return Err(NativeError::Call(CallError::Unsupported(format!(
                "flattened argument count {} does not match IR param_count {}",
                flat.len(),
                param_count
            ))));
        }

        let words = self.invoke_flat(name, &flat)?;
        let gq = decode_q32_return(&gfn.return_type, &words)?;
        q32_to_lps_value_f32(&gfn.return_type, gq)
            .map_err(|e| NativeError::Call(CallError::TypeMismatch(e.to_string())))
    }

    fn call_q32(&mut self, name: &str, args: &[i32]) -> Result<Vec<i32>, Self::Error> {
        self.last_debug = None;
        self.last_guest_instruction_count = None;
        if self.module.options.float_mode != FloatMode::Q32 {
            return Err(NativeError::Call(CallError::Unsupported(String::from(
                "NativeEmuInstance::call_q32 requires FloatMode::Q32",
            ))));
        }

        let gfn = self
            .module
            .meta
            .functions
            .iter()
            .find(|f| f.name == name)
            .cloned()
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;

        for p in &gfn.parameters {
            if matches!(p.qualifier, ParamQualifier::Out | ParamQualifier::InOut) {
                return Err(NativeError::Call(CallError::Unsupported(String::from(
                    "out/inout parameters are not supported for direct calling.",
                ))));
            }
        }

        let idx = self
            .module
            .ir
            .functions
            .iter()
            .position(|f| f.name == name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
        let ir_func = &self.module.ir.functions[idx];
        let param_count = ir_func.param_count as usize;

        let expected_words: usize = gfn
            .parameters
            .iter()
            .map(|p| glsl_component_count(&p.ty))
            .sum();
        if args.len() != expected_words {
            return Err(NativeError::Call(CallError::Arity {
                expected: expected_words,
                got: args.len(),
            }));
        }
        if args.len() != param_count {
            return Err(NativeError::Call(CallError::Unsupported(format!(
                "flattened argument count {} does not match IR param_count {}",
                args.len(),
                param_count
            ))));
        }

        let words = self.invoke_flat(name, args)?;
        if gfn.return_type == LpsType::Void {
            return Ok(Vec::new());
        }
        Ok(words)
    }

    fn debug_state(&self) -> Option<String> {
        // Compile-time interleaved/disasm lives on [`NativeEmuModule::debug_info`]; filetests and
        // tooling print that once after compile. Here we only surface per-run emulator output.
        self.last_debug.clone()
    }

    fn last_guest_instruction_count(&self) -> Option<u64> {
        self.last_guest_instruction_count
    }
}
