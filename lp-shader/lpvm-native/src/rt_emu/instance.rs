//! [`LpvmInstance`] implementation for emulated native RV32 execution.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use cranelift_codegen::data_value::DataValue;
use cranelift_codegen::isa::CallConv;
use lp_riscv_emu::{CycleModel, DEFAULT_SHARED_START, LogLevel, Memory, Riscv32Emulator};
use lpir::FloatMode;
use lpir::lpir_module::IrFunction;
use lps_shared::{LayoutRules, LpsType, LpsValueQ32, ParamQualifier, lps_value_f32::LpsValueF32};
use lpvm::{
    CallError, LpvmBuffer, LpvmInstance, decode_q32_return, encode_uniform_write,
    encode_uniform_write_q32, flat_q32_words_from_f32_args, glsl_component_count,
    q32_to_lps_value_f32, validate_render_texture_sig_ir,
};
use lpvm_cranelift::{CompileOptions, signature_for_ir_func, signature_uses_struct_return};
use lpvm_emu::{GUEST_VMCTX_BYTES, riscv32_lpvm_reference_isa, write_guest_vmctx_header};

use crate::error::NativeError;

use super::NativeEmuModule;

pub(crate) struct RenderTextureEntry {
    name: String,
    entry_pc: u32,
}

/// Per-instance emulation state with VMContext.
pub struct NativeEmuInstance {
    pub(crate) module: NativeEmuModule,
    pub(crate) vmctx_guest: u32,
    pub(crate) last_debug: Option<String>,
    pub(crate) last_guest_instruction_count: Option<u64>,
    pub(crate) last_guest_cycle_count: Option<u64>,
    /// Byte offset from vmctx base to globals region
    pub(crate) globals_offset: usize,
    /// Byte offset from vmctx base to snapshot region
    pub(crate) snapshot_offset: usize,
    /// Size of globals region in bytes
    pub(crate) globals_size: usize,
    pub(crate) render_texture_cache: Option<RenderTextureEntry>,
}

impl NativeEmuInstance {
    /// Initialize globals by calling `__shader_init` if it exists,
    /// then memcpy globals -> snapshot to capture the initialized state.
    pub fn init_globals(&mut self) -> Result<(), NativeError> {
        // Call __shader_init if it exists
        if self.has_function("__shader_init") {
            self.invoke_flat("__shader_init", &[], CycleModel::default())?;
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
            write_guest_vmctx_header(&mut v[off..off + GUEST_VMCTX_BYTES]);
        }
    }

    fn cranelift_options(&self) -> CompileOptions {
        CompileOptions {
            float_mode: self.module.options.float_mode,
            config: self.module.options.config.clone(),
            ..Default::default()
        }
    }

    fn resolve_render_texture(&mut self, fn_name: &str) -> Result<u32, NativeError> {
        if let Some(entry) = &self.render_texture_cache {
            if entry.name == fn_name {
                return Ok(entry.entry_pc);
            }
        }

        let ir_fn = self
            .module
            .ir
            .functions
            .values()
            .find(|f| f.name == fn_name)
            .ok_or_else(|| NativeError::Call(CallError::MissingMetadata(fn_name.into())))?;
        validate_render_texture_sig_ir(ir_fn).map_err(|e| {
            NativeError::Call(CallError::Unsupported(alloc::format!(
                "render-texture sig invalid: {e}"
            )))
        })?;

        let entry = *self.module.load.symbol_map.get(fn_name).ok_or_else(|| {
            CallError::Unsupported(format!("symbol `{fn_name}` not in linked RV32 image"))
        })?;

        self.render_texture_cache = Some(RenderTextureEntry {
            name: fn_name.into(),
            entry_pc: entry,
        });
        Ok(entry)
    }

    /// Run emulator at `entry` with full arg words (including vmctx in slot 0).
    fn run_emulator_call(
        &mut self,
        ir_func: &IrFunction,
        entry: u32,
        full: &[i32],
        cycle_model: CycleModel,
        return_ty_for_sret: Option<&LpsType>,
    ) -> Result<Vec<i32>, NativeError> {
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
        let uses_sret = signature_uses_struct_return(&*isa, ir_func);
        let struct_size = if uses_sret {
            if ir_func.sret_arg.is_some() {
                let rt = return_ty_for_sret.ok_or_else(|| {
                    NativeError::Call(CallError::Unsupported(String::from(
                        "internal: LPIR sret without host return type for sizing",
                    )))
                })?;
                if matches!(rt, LpsType::Void) {
                    return Err(NativeError::Call(CallError::Unsupported(String::from(
                        "internal: sret_buffer sizing for void return",
                    ))));
                }
                lps_shared::type_size(rt, LayoutRules::Std430)
            } else {
                ir_func.return_types.len() * 4
            }
        } else {
            0usize
        };

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
        emu.set_cycle_model(cycle_model);

        let ret_result = if uses_sret {
            emu.call_function_with_struct_return(entry, &data_args, &sig, struct_size)
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
                if uses_sret {
                    let need_words = (struct_size + 3) / 4;
                    if words.len() < need_words {
                        return Err(NativeError::Call(CallError::Unsupported(format!(
                            "emulator returned {} words, sret needs {} words for {} bytes",
                            words.len(),
                            need_words,
                            struct_size
                        ))));
                    }
                    words.truncate(need_words);
                } else {
                    if words.len() < n_ret {
                        return Err(NativeError::Call(CallError::Unsupported(format!(
                            "emulator returned {} words, signature expects {}",
                            words.len(),
                            n_ret
                        ))));
                    }
                    words.truncate(n_ret);
                }
                self.last_guest_instruction_count = Some(n_inst);
                self.last_guest_cycle_count = Some(emu.get_cycle_count());
                Ok(words)
            }
            Err(e) => {
                self.last_guest_instruction_count = None;
                self.last_guest_cycle_count = None;
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

    fn invoke_flat(
        &mut self,
        name: &str,
        flat: &[i32],
        cycle_model: CycleModel,
    ) -> Result<Vec<i32>, NativeError> {
        self.last_guest_instruction_count = None;
        self.last_guest_cycle_count = None;
        self.refresh_vmctx_header();

        let ir_func = self
            .module
            .ir
            .functions
            .values()
            .find(|f| f.name == name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?
            .clone();

        let mut full: Vec<i32> = Vec::with_capacity(1 + flat.len());
        full.push(self.vmctx_guest as i32);
        full.extend_from_slice(flat);

        let entry = *self.module.load.symbol_map.get(name).ok_or_else(|| {
            CallError::Unsupported(format!("symbol `{name}` not in linked RV32 image"))
        })?;

        let return_ty_owned = if ir_func.sret_arg.is_some() {
            let gfn = self
                .module
                .meta
                .functions
                .iter()
                .find(|f| f.name == name)
                .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
            Some(gfn.return_type.clone())
        } else {
            None
        };

        self.run_emulator_call(
            &ir_func,
            entry,
            &full,
            cycle_model,
            return_ty_owned.as_ref(),
        )
    }

    fn vmctx_write_bytes(&mut self, offset: usize, data: &[u8]) -> Result<(), NativeError> {
        let total = self.module.meta.vmctx_buffer_size();
        let end = offset.checked_add(data.len()).ok_or_else(|| {
            NativeError::Call(CallError::Unsupported(String::from(
                "vmctx write: offset overflow",
            )))
        })?;
        if end > total {
            return Err(NativeError::Call(CallError::Unsupported(alloc::format!(
                "vmctx write out of bounds: end {end} total {total}"
            ))));
        }
        let shared_start = self.module.arena.shared_start() as usize;
        let vmctx_base = self.vmctx_guest as usize;
        let dst_addr = vmctx_base
            .checked_add(offset)
            .and_then(|a| a.checked_sub(shared_start))
            .ok_or_else(|| {
                NativeError::Call(CallError::Unsupported(String::from(
                    "vmctx write: address overflow",
                )))
            })?;
        let mut storage = self.module.arena.lock_storage();
        if dst_addr + data.len() > storage.len() {
            return Err(NativeError::Call(CallError::Unsupported(String::from(
                "vmctx write: arena too small",
            ))));
        }
        storage[dst_addr..dst_addr + data.len()].copy_from_slice(data);
        Ok(())
    }
}

impl LpvmInstance for NativeEmuInstance {
    type Error = NativeError;

    fn call(&mut self, name: &str, args: &[LpsValueF32]) -> Result<LpsValueF32, Self::Error> {
        // Reset globals before each call to ensure fresh state
        self.reset_globals();

        self.last_debug = None;
        self.last_guest_instruction_count = None;
        self.last_guest_cycle_count = None;
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
        let ir_func = self
            .module
            .ir
            .functions
            .values()
            .find(|f| f.name == name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
        let param_count = ir_func.param_count as usize;
        if flat.len() != param_count {
            return Err(NativeError::Call(CallError::Unsupported(format!(
                "flattened argument count {} does not match IR param_count {}",
                flat.len(),
                param_count
            ))));
        }

        let words = self.invoke_flat(name, &flat, CycleModel::default())?;
        let gq = decode_q32_return(&gfn.return_type, &words)?;
        q32_to_lps_value_f32(&gfn.return_type, gq)
            .map_err(|e| NativeError::Call(CallError::TypeMismatch(e.to_string())))
    }

    fn call_q32(&mut self, name: &str, args: &[i32]) -> Result<Vec<i32>, Self::Error> {
        self.call_q32_with_cycle_model(name, args, CycleModel::default())
    }

    fn call_render_texture(
        &mut self,
        fn_name: &str,
        texture: &mut LpvmBuffer,
        width: u32,
        height: u32,
    ) -> Result<(), Self::Error> {
        self.reset_globals();

        self.last_guest_instruction_count = None;
        self.last_guest_cycle_count = None;
        self.refresh_vmctx_header();

        if self.module.options.float_mode != FloatMode::Q32 {
            return Err(NativeError::Call(CallError::Unsupported(String::from(
                "NativeEmuInstance::call_render_texture requires FloatMode::Q32",
            ))));
        }

        let entry = self.resolve_render_texture(fn_name)?;
        let ir_func = self
            .module
            .ir
            .functions
            .values()
            .find(|f| f.name == fn_name)
            .ok_or_else(|| NativeError::Call(CallError::MissingMetadata(fn_name.into())))?
            .clone();

        let tex_offset = i32::try_from(texture.guest_base()).map_err(|_| {
            NativeError::Call(CallError::Unsupported(alloc::format!(
                "texture guest base {:#x} exceeds i32 range",
                texture.guest_base()
            )))
        })?;
        let vmctx = self.vmctx_guest as i32;
        let full = [vmctx, tex_offset, width as i32, height as i32];
        let return_ty_owned = if ir_func.sret_arg.is_some() {
            self.module
                .meta
                .functions
                .iter()
                .find(|f| f.name == fn_name)
                .map(|g| g.return_type.clone())
        } else {
            None
        };
        self.run_emulator_call(
            &ir_func,
            entry,
            &full,
            CycleModel::default(),
            return_ty_owned.as_ref(),
        )?;
        Ok(())
    }

    fn set_uniform(&mut self, path: &str, value: &LpsValueF32) -> Result<(), Self::Error> {
        let (off, bytes) = encode_uniform_write(
            &self.module.meta,
            path,
            value,
            self.module.options.float_mode,
        )
        .map_err(|e| {
            NativeError::Call(CallError::Unsupported(alloc::format!("set_uniform: {e}")))
        })?;
        self.vmctx_write_bytes(off, &bytes)
    }

    fn set_uniform_q32(&mut self, path: &str, value: &LpsValueQ32) -> Result<(), Self::Error> {
        let (off, bytes) =
            encode_uniform_write_q32(&self.module.meta, path, value).map_err(|e| {
                NativeError::Call(CallError::Unsupported(alloc::format!(
                    "set_uniform_q32: {e}"
                )))
            })?;
        self.vmctx_write_bytes(off, &bytes)
    }

    fn debug_state(&self) -> Option<String> {
        // Compile-time interleaved/disasm lives on [`NativeEmuModule::debug_info`]; filetests and
        // tooling print that once after compile. Here we only surface per-run emulator output.
        self.last_debug.clone()
    }

    fn last_guest_instruction_count(&self) -> Option<u64> {
        self.last_guest_instruction_count
    }

    fn last_guest_cycle_count(&self) -> Option<u64> {
        self.last_guest_cycle_count
    }
}

impl NativeEmuInstance {
    /// Like [`LpvmInstance::call_q32`], but selects the guest [`CycleModel`] for this invocation.
    pub fn call_q32_with_cycle_model(
        &mut self,
        name: &str,
        args: &[i32],
        cycle_model: CycleModel,
    ) -> Result<Vec<i32>, NativeError> {
        // Reset globals before each call to ensure fresh state
        self.reset_globals();

        self.last_debug = None;
        self.last_guest_instruction_count = None;
        self.last_guest_cycle_count = None;
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

        let words = self.invoke_flat(name, args, cycle_model)?;
        if gfn.return_type == LpsType::Void {
            return Ok(Vec::new());
        }
        Ok(words)
    }
}
