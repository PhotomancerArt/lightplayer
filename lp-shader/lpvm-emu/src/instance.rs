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
use lps_shared::{LpsType, LpsValueQ32, ParamQualifier};
use lpvm::{
    AllocError, CallError, LpsValueF32, LpvmInstance, LpvmMemory, decode_q32_return,
    encode_uniform_write, encode_uniform_write_q32, flat_q32_words_from_f32_args,
    glsl_component_count, q32_to_lps_value_f32,
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
    /// Byte offset from vmctx base to globals region
    globals_offset: usize,
    /// Byte offset from vmctx base to snapshot region
    snapshot_offset: usize,
    /// Size of globals region in bytes
    globals_size: usize,
}

impl EmuInstance {
    pub(crate) fn new(module: EmuModule) -> Result<Self, InstanceError> {
        let align = 16usize;
        let total_size = module.meta.vmctx_buffer_size();
        let size = total_size.max(align);
        let buf = module
            .arena
            .alloc(size, align)
            .map_err(|e: AllocError| InstanceError::Alloc(e.to_string()))?;

        // Zero-initialize the entire buffer, then write the vmctx header
        unsafe {
            let slot = core::slice::from_raw_parts_mut(buf.native_ptr(), total_size);
            slot.fill(0);
            emu_run::write_guest_vmctx_header(&mut slot[..GUEST_VMCTX_BYTES]);
        }

        let globals_offset = module.meta.globals_offset();
        let snapshot_offset = module.meta.snapshot_offset();
        let globals_size = module.meta.globals_size();

        let mut instance = Self {
            module,
            vmctx_guest: buf.guest_base() as u32,
            last_debug: None,
            last_guest_instruction_count: None,
            globals_offset,
            snapshot_offset,
            globals_size,
        };

        // Auto-init globals: call __shader_init if it exists, then snapshot
        let _ = instance.init_globals();

        Ok(instance)
    }

    /// Initialize globals by calling `__shader_init` if it exists,
    /// then memcpy globals -> snapshot to capture the initialized state.
    pub fn init_globals(&mut self) -> Result<(), InstanceError> {
        // Call __shader_init if it exists
        if self.has_function("__shader_init") {
            self.invoke_flat("__shader_init", &[])?;
        }

        // Copy globals region to snapshot region
        self.snapshot_globals();
        Ok(())
    }

    /// Reset globals by memcpy snapshot -> globals.
    /// This is a no-op if globals_size == 0.
    pub fn reset_globals(&mut self) {
        if self.globals_size == 0 {
            return;
        }

        self.memcpy_guest(self.snapshot_offset, self.globals_offset, self.globals_size);
    }

    /// Copy globals region to snapshot region (for init).
    fn snapshot_globals(&mut self) {
        if self.globals_size == 0 {
            return;
        }

        self.memcpy_guest(self.globals_offset, self.snapshot_offset, self.globals_size);
    }

    /// Check if a function exists in the module.
    fn has_function(&self, name: &str) -> bool {
        self.module.ir.functions.values().any(|f| f.name == name)
    }

    /// Memcpy within the guest memory (via shared arena).
    /// src_offset and dst_offset are relative to vmctx_guest base.
    fn memcpy_guest(&self, src_offset: usize, dst_offset: usize, size: usize) {
        let shared_start = self.module.arena.shared_start() as usize;
        let vmctx_base = self.vmctx_guest as usize;

        let src_addr = vmctx_base + src_offset - shared_start;
        let dst_addr = vmctx_base + dst_offset - shared_start;

        let mut storage = self.module.arena.lock_storage();
        if src_addr + size <= storage.len() && dst_addr + size <= storage.len() {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    storage.as_ptr().add(src_addr),
                    storage.as_mut_ptr().add(dst_addr),
                    size,
                );
            }
        }
    }

    fn refresh_vmctx_header(&self) {
        let off =
            (u64::from(self.vmctx_guest) - u64::from(self.module.arena.shared_start())) as usize;
        let mut v = self.module.arena.lock_storage();
        if off + GUEST_VMCTX_BYTES <= v.len() {
            emu_run::write_guest_vmctx_header(&mut v[off..off + GUEST_VMCTX_BYTES]);
        }
    }

    fn vmctx_write_bytes(&mut self, offset: usize, data: &[u8]) -> Result<(), InstanceError> {
        let total = self.module.meta.vmctx_buffer_size();
        let end = offset.checked_add(data.len()).ok_or_else(|| {
            InstanceError::Call(CallError::Unsupported(String::from(
                "vmctx write: offset overflow",
            )))
        })?;
        if end > total {
            return Err(InstanceError::Call(CallError::Unsupported(alloc::format!(
                "vmctx write out of bounds: end {end} total {total}"
            ))));
        }
        let shared_start = self.module.arena.shared_start() as usize;
        let vmctx_base = self.vmctx_guest as usize;
        let dst_addr = vmctx_base
            .checked_add(offset)
            .and_then(|a| a.checked_sub(shared_start))
            .ok_or_else(|| {
                InstanceError::Call(CallError::Unsupported(String::from(
                    "vmctx write: address overflow",
                )))
            })?;
        let mut storage = self.module.arena.lock_storage();
        if dst_addr + data.len() > storage.len() {
            return Err(InstanceError::Call(CallError::Unsupported(String::from(
                "vmctx write: arena too small",
            ))));
        }
        storage[dst_addr..dst_addr + data.len()].copy_from_slice(data);
        Ok(())
    }
}

impl LpvmInstance for EmuInstance {
    type Error = InstanceError;

    fn call(&mut self, name: &str, args: &[LpsValueF32]) -> Result<LpsValueF32, Self::Error> {
        // Reset globals before each call to ensure fresh state
        self.reset_globals();

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
        let ir_func = self
            .module
            .ir
            .functions
            .values()
            .find(|f| f.name == name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
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
        // Reset globals before each call to ensure fresh state
        self.reset_globals();

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

        let ir_func = self
            .module
            .ir
            .functions
            .values()
            .find(|f| f.name == name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
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

    fn set_uniform(&mut self, path: &str, value: &LpsValueF32) -> Result<(), Self::Error> {
        let (off, bytes) = encode_uniform_write(
            &self.module.meta,
            path,
            value,
            self.module.options.float_mode,
        )
        .map_err(|e| InstanceError::Call(CallError::Unsupported(format!("set_uniform: {e}"))))?;
        self.vmctx_write_bytes(off, &bytes)
    }

    fn set_uniform_q32(&mut self, path: &str, value: &LpsValueQ32) -> Result<(), Self::Error> {
        let (off, bytes) =
            encode_uniform_write_q32(&self.module.meta, path, value).map_err(|e| {
                InstanceError::Call(CallError::Unsupported(format!("set_uniform_q32: {e}")))
            })?;
        self.vmctx_write_bytes(off, &bytes)
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

        let ir_func = self
            .module
            .ir
            .functions
            .values()
            .find(|f| f.name == name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;

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
