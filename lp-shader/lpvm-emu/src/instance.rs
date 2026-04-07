//! [`EmuInstance`]: per-instance VMContext slot in shared memory + emulated [`LpvmInstance::call`].

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use cranelift_codegen::data_value::DataValue;
use cranelift_codegen::ir::ArgumentPurpose;
use cranelift_codegen::isa::CallConv;
use lp_riscv_emu::{LogLevel, Memory, Riscv32Emulator, DEFAULT_SHARED_START};
use lpir::FloatMode;
use lps_shared::lps_value_f64_convert::{glsl_f64_to_lps_value, lps_value_to_f64};
use lps_shared::{LpsType, ParamQualifier};
use lpvm::{AllocError, LpsValue, LpvmInstance, LpvmMemory};
use lpvm_cranelift::{decode_q32_return, flatten_q32_arg, signature_for_ir_func, CallError};

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

    fn call(&mut self, name: &str, args: &[LpsValue]) -> Result<LpsValue, Self::Error> {
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

        let idx = self
            .module
            .ir
            .functions
            .iter()
            .position(|f| f.name == name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
        let ir_func = &self.module.ir.functions[idx];
        let param_count = ir_func.param_count as usize;

        let mut flat: Vec<i32> = Vec::new();
        for (p, a) in gfn.parameters.iter().zip(args.iter()) {
            let q = lps_value_to_f64(&p.ty, a)?;
            flat.extend(flatten_q32_arg(p, &q)?);
        }
        if flat.len() != param_count {
            return Err(CallError::Unsupported(format!(
                "flattened argument count {} does not match IR param_count {}",
                flat.len(),
                param_count
            ))
            .into());
        }

        self.refresh_vmctx_header();

        let mut full: Vec<i32> = Vec::with_capacity(1 + flat.len());
        full.push(self.vmctx_guest as i32);
        full.extend_from_slice(&flat);

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
        let mut emu = Riscv32Emulator::from_memory(mem, &[]).with_log_level(LogLevel::None);

        let has_sr = sig
            .params
            .iter()
            .any(|p| p.purpose == ArgumentPurpose::StructReturn);
        let ret = if has_sr {
            emu.call_function_with_struct_return(entry, &data_args, &sig, n_ret * 4)
                .map_err(|e| {
                    InstanceError::Call(CallError::Unsupported(format!("emulator: {e:?}")))
                })?
        } else {
            emu.call_function(entry, &data_args, &sig).map_err(|e| {
                InstanceError::Call(CallError::Unsupported(format!("emulator: {e:?}")))
            })?
        };

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

        let gq = decode_q32_return(&gfn.return_type, &words)?;
        glsl_f64_to_lps_value(&gfn.return_type, gq).map_err(|e| InstanceError::Call(e))
    }
}
