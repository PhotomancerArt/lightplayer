//! [`EmuInstance`]: per-instance VMContext slot in shared memory + emulated [`LpvmInstance::call`].

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use cranelift_codegen::data_value::DataValue;
use cranelift_codegen::ir::ArgumentPurpose;
use cranelift_codegen::isa::CallConv;
use lp_riscv_emu::{DEFAULT_SHARED_START, LogLevel, Memory, Riscv32Emulator};
use lpir::FloatMode;
use lps_shared::{LpsType, ParamQualifier};
use lpvm::{
    AllocError, CallError, LpsValueF32, LpvmInstance, LpvmMemory, decode_q32_return,
    flat_q32_words_from_f32_args, glsl_component_count, q32_to_lps_value_f32,
};
use lpvm_cranelift::signature_for_ir_func;

use crate::emu_run::{self, GUEST_VMCTX_BYTES};
use crate::module::EmuModule;

/// Execution error for [`EmuInstance`].
#[derive(Debug)]
pub enum InstanceError {
    Call(CallError),
    Unsupported(&'static str),
    Alloc(String),
}

impl fmt::Display for InstanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstanceError::Call(e) => e.fmt(f),
            InstanceError::Unsupported(s) => write!(f, "{s}"),
            InstanceError::Alloc(s) => write!(f, "allocation error: {s}"),
        }
    }
}

impl From<CallError> for InstanceError {
    fn from(value: CallError) -> Self {
        InstanceError::Call(value)
    }
}

impl core::error::Error for InstanceError {}

/// One runnable instance: VMContext lives in the engine shared region at `vmctx_guest`.
pub struct EmuInstance {
    module: EmuModule,
    vmctx_guest: u32,
    last_debug: Option<String>,
    last_guest_instruction_count: Option<u64>,
}

impl EmuInstance {
    pub(crate) fn new(module: EmuModule) -> Result<Self, InstanceError> {
        let align = 16usize;
        let size = GUEST_VMCTX_BYTES.max(align);
        let buf = module
            .arena
            .alloc(size, align)
            .map_err(|e: AllocError| InstanceError::Alloc(e.to_string()))?;
        unsafe {
            let slot = core::slice::from_raw_parts_mut(buf.native_ptr(), GUEST_VMCTX_BYTES);
            emu_run::write_guest_vmctx_header(slot);
        }
        Ok(Self {
            module,
            vmctx_guest: buf.guest_base() as u32,
            last_debug: None,
            last_guest_instruction_count: None,
        })
    }

    fn refresh_vmctx_header(&self) {
        let off =
            (u64::from(self.vmctx_guest) - u64::from(self.module.arena.shared_start())) as usize;
        let mut v = self.module.arena.lock_storage();
        if off + GUEST_VMCTX_BYTES <= v.len() {
            emu_run::write_guest_vmctx_header(&mut v[off..off + GUEST_VMCTX_BYTES]);
        }
    }
}

impl LpvmInstance for EmuInstance {
    type Error = InstanceError;

    fn call(&mut self, name: &str, args: &[LpsValueF32]) -> Result<LpsValueF32, Self::Error> {
        self.last_debug = None;
        self.last_guest_instruction_count = None;
        if self.module.options.float_mode != FloatMode::Q32 {
            return Err(InstanceError::Unsupported(
                "EmuInstance::call requires FloatMode::Q32",
            ));
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
                return Err(CallError::Unsupported(String::from(
                    "out/inout parameters are not supported for direct calling.",
                ))
                .into());
            }
        }

        if gfn.return_type == LpsType::Void {
            return Err(InstanceError::Unsupported(
                "void return is not represented as LpsValue; use a typed return",
            ));
        }

        if gfn.parameters.len() != args.len() {
            return Err(CallError::Arity {
                expected: gfn.parameters.len(),
                got: args.len(),
            }
            .into());
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
            return Err(CallError::Unsupported(format!(
                "flattened argument count {} does not match IR param_count {}",
                flat.len(),
                param_count
            ))
            .into());
        }

        let words = self.invoke_flat(name, &flat)?;
        let gq = decode_q32_return(&gfn.return_type, &words)?;
        q32_to_lps_value_f32(&gfn.return_type, gq)
            .map_err(|e| InstanceError::Call(CallError::TypeMismatch(e.to_string())))
    }

    fn call_q32(&mut self, name: &str, args: &[i32]) -> Result<Vec<i32>, Self::Error> {
        self.last_debug = None;
        self.last_guest_instruction_count = None;
        if self.module.options.float_mode != FloatMode::Q32 {
            return Err(InstanceError::Unsupported(
                "EmuInstance::call_q32 requires FloatMode::Q32",
            ));
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
                return Err(CallError::Unsupported(String::from(
                    "out/inout parameters are not supported for direct calling.",
                ))
                .into());
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
            return Err(CallError::Arity {
                expected: expected_words,
                got: args.len(),
            }
            .into());
        }
        if args.len() != param_count {
            return Err(CallError::Unsupported(format!(
                "flattened argument count {} does not match IR param_count {}",
                args.len(),
                param_count
            ))
            .into());
        }

        let words = self.invoke_flat(name, args)?;
        if gfn.return_type == LpsType::Void {
            return Ok(Vec::new());
        }
        Ok(words)
    }

    fn debug_state(&self) -> Option<String> {
        self.last_debug.clone()
    }

    fn last_guest_instruction_count(&self) -> Option<u64> {
        self.last_guest_instruction_count
    }
}

impl EmuInstance {
    fn invoke_flat(&mut self, name: &str, flat: &[i32]) -> Result<Vec<i32>, InstanceError> {
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

        let isa = emu_run::riscv32_reference_isa()
            .map_err(|e| InstanceError::Call(CallError::Unsupported(format!("{e}"))))?;
        let sig = signature_for_ir_func(
            ir_func,
            CallConv::SystemV,
            self.module.options.float_mode,
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
                            return Err(InstanceError::Call(CallError::Unsupported(format!(
                                "unexpected emulator return value: {other:?}"
                            ))));
                        }
                    }
                }
                if words.len() < n_ret {
                    return Err(InstanceError::Call(CallError::Unsupported(format!(
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
                let mut debug_parts = Vec::new();
                debug_parts.push(String::from("=== Debug Info ==="));
                debug_parts.push(format!("Error: {e:?}"));
                debug_parts.push(emu.dump_state());
                debug_parts.push(emu.format_debug_info(
                    Some(emu.get_pc()),
                    lp_riscv_emu::config::INSTRUCTION_LOG_DISPLAY_COUNT,
                ));
                self.last_debug = Some(debug_parts.join("\n\n"));
                Err(InstanceError::Call(CallError::Unsupported(format!(
                    "emulator: {e:?}"
                ))))
            }
        }
    }
}
