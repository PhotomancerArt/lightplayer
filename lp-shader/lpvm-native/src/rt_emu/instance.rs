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
    CallError, LpvmBuffer, LpvmInstance, decode_global_read, decode_q32_return,
    encode_global_write, encode_uniform_write, encode_uniform_write_q32,
    flat_q32_words_from_f32_args, global_data_span, glsl_component_count, q32_to_lps_value_f32,
    validate_compute_tick_sig, validate_render_samples_sig_ir, validate_render_texture_sig_ir,
};
use lpvm::{INVOCATION_INDEX_ARMED, TRAP_CODE_NONE, VMCTX_OFFSET_FUEL, VMCTX_OFFSET_TRAP};
use lpvm_cranelift::{CompileOptions, signature_for_ir_func, signature_uses_struct_return};
use lpvm_emu::{GUEST_VMCTX_BYTES, riscv32_lpvm_reference_isa};

use crate::error::NativeError;

use super::NativeEmuModule;

/// Per-call emulator instruction limit for emulated native execution.
///
/// Raised above lp-riscv-emu's 1M default so guest fuel traps
/// deterministically first (a flat call arms `DEFAULT_VMCTX_FUEL` = 1M
/// back-edges ≈ 6-10M instructions of spinning before the trap fires). It
/// remains a hard backstop when fuel emission is compiled off.
const EMU_CALL_INSTRUCTION_LIMIT: u64 = 64_000_000;

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
    /// Fuel budget written to the vmctx fuel low u32 when arming before each
    /// guest entry. Defaults to `DEFAULT_VMCTX_FUEL as u32`; render wrappers
    /// re-arm per pixel/sample with `DEFAULT_INVOCATION_FUEL` regardless.
    pub(crate) armed_fuel: u32,
    pub(crate) render_texture_cache: Option<RenderTextureEntry>,
    pub(crate) render_samples_cache: Option<RenderTextureEntry>,
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

    /// Arm the vmctx fuel/trap words before a guest entry: full tank in the
    /// fuel low u32, host-armed invocation index in the high u32, no trap.
    /// `metadata` (vmctx+12) is left untouched.
    fn refresh_vmctx_header(&self) {
        let off =
            (u64::from(self.vmctx_guest) - u64::from(self.module.arena.shared_start())) as usize;
        let mut v = self.module.arena.lock_storage();
        if off + GUEST_VMCTX_BYTES <= v.len() {
            let fuel = off + VMCTX_OFFSET_FUEL;
            v[fuel..fuel + 4].copy_from_slice(&self.armed_fuel.to_le_bytes());
            v[fuel + 4..fuel + 8].copy_from_slice(&INVOCATION_INDEX_ARMED.to_le_bytes());
            let trap = off + VMCTX_OFFSET_TRAP;
            v[trap..trap + 4].copy_from_slice(&TRAP_CODE_NONE.to_le_bytes());
        }
    }

    /// Read the trap slot after a guest entry; nonzero → typed error carrying
    /// the invocation index (fuel high u32). Return values from a trapped
    /// call are garbage — callers must discard them on `Err`.
    fn take_trap(&self) -> Result<(), NativeError> {
        let trap_bytes = self.vmctx_read_bytes(VMCTX_OFFSET_TRAP, 4)?;
        let trap = u32::from_le_bytes(trap_bytes[..4].try_into().expect("4 trap bytes"));
        if trap == TRAP_CODE_NONE {
            return Ok(());
        }
        let idx_bytes = self.vmctx_read_bytes(VMCTX_OFFSET_FUEL + 4, 4)?;
        let invocation = u32::from_le_bytes(idx_bytes[..4].try_into().expect("4 index bytes"));
        Err(NativeError::Trap {
            code: trap,
            invocation,
        })
    }

    /// Override the fuel budget armed before each guest entry (tests / perf
    /// comparison; production uses the default).
    pub fn set_armed_fuel(&mut self, fuel: u32) {
        self.armed_fuel = fuel;
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

    fn resolve_render_samples(&mut self, fn_name: &str) -> Result<u32, NativeError> {
        if let Some(entry) = &self.render_samples_cache {
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
            .ok_or_else(|| NativeError::Call(CallError::MissingMetadata(String::from(fn_name))))?;
        validate_render_samples_sig_ir(ir_fn).map_err(|e| {
            NativeError::Call(CallError::Unsupported(alloc::format!(
                "render-samples sig invalid: {e}"
            )))
        })?;

        let entry = *self.module.load.symbol_map.get(fn_name).ok_or_else(|| {
            CallError::Unsupported(format!("symbol `{fn_name}` not in linked RV32 image"))
        })?;
        self.render_samples_cache = Some(RenderTextureEntry {
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
        // Guest fuel (armed by `refresh_vmctx_header` before every entry)
        // bounds runaway shader code first: a full `DEFAULT_VMCTX_FUEL` tank
        // of loop back-edges costs ~6-10 guest instructions each (≲10M),
        // well under `EMU_CALL_INSTRUCTION_LIMIT`. Raising the emulator
        // limit is safe precisely because of that metering; the limit stays
        // as the hard host-side backstop for fuel-off compiles and codegen
        // bugs.
        let mut emu = Riscv32Emulator::from_memory(mem, &[])
            .with_log_level(log_level)
            .with_call_instruction_limit(EMU_CALL_INSTRUCTION_LIMIT);
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
                // A trapped guest exits cleanly (epilogue cascade), so the
                // trap slot — not the emulator result — is the signal.
                self.take_trap()?;
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

    fn vmctx_read_bytes(&self, offset: usize, len: usize) -> Result<Vec<u8>, NativeError> {
        let total = self.module.meta.vmctx_buffer_size();
        let end = offset.checked_add(len).ok_or_else(|| {
            NativeError::Call(CallError::Unsupported(String::from(
                "vmctx read: offset overflow",
            )))
        })?;
        if end > total {
            return Err(NativeError::Call(CallError::Unsupported(alloc::format!(
                "vmctx read out of bounds: end {end} total {total}"
            ))));
        }
        let shared_start = self.module.arena.shared_start() as usize;
        let vmctx_base = self.vmctx_guest as usize;
        let src_addr = vmctx_base
            .checked_add(offset)
            .and_then(|a| a.checked_sub(shared_start))
            .ok_or_else(|| {
                NativeError::Call(CallError::Unsupported(String::from(
                    "vmctx read: address overflow",
                )))
            })?;
        let storage = self.module.arena.lock_storage();
        if src_addr + len > storage.len() {
            return Err(NativeError::Call(CallError::Unsupported(String::from(
                "vmctx read: arena too small",
            ))));
        }
        Ok(storage[src_addr..src_addr + len].to_vec())
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

    fn call_render_samples(
        &mut self,
        fn_name: &str,
        points: &mut LpvmBuffer,
        out: &mut LpvmBuffer,
        count: u32,
    ) -> Result<(), Self::Error> {
        self.reset_globals();

        self.last_guest_instruction_count = None;
        self.last_guest_cycle_count = None;
        self.refresh_vmctx_header();

        if self.module.options.float_mode != FloatMode::Q32 {
            return Err(NativeError::Call(CallError::Unsupported(String::from(
                "NativeEmuInstance::call_render_samples requires FloatMode::Q32",
            ))));
        }

        let entry = self.resolve_render_samples(fn_name)?;
        let ir_func = self
            .module
            .ir
            .functions
            .values()
            .find(|f| f.name == fn_name)
            .ok_or_else(|| NativeError::Call(CallError::MissingMetadata(fn_name.into())))?
            .clone();

        let points_offset = i32::try_from(points.guest_base()).map_err(|_| {
            NativeError::Call(CallError::Unsupported(alloc::format!(
                "points guest base {:#x} exceeds i32 range",
                points.guest_base()
            )))
        })?;
        let out_offset = i32::try_from(out.guest_base()).map_err(|_| {
            NativeError::Call(CallError::Unsupported(alloc::format!(
                "sample output guest base {:#x} exceeds i32 range",
                out.guest_base()
            )))
        })?;
        let vmctx = self.vmctx_guest as i32;
        let full = [vmctx, points_offset, out_offset, count as i32];
        self.run_emulator_call(&ir_func, entry, &full, CycleModel::default(), None)?;
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

    fn set_global(&mut self, path: &str, value: &LpsValueF32) -> Result<(), Self::Error> {
        let (off, bytes) = encode_global_write(
            &self.module.meta,
            path,
            value,
            self.module.options.float_mode,
        )
        .map_err(|e| {
            NativeError::Call(CallError::Unsupported(alloc::format!("set_global: {e}")))
        })?;
        self.vmctx_write_bytes(off, &bytes)
    }

    fn get_global(&mut self, path: &str) -> Result<LpsValueF32, Self::Error> {
        let span = global_data_span(&self.module.meta, path).map_err(|e| {
            NativeError::Call(CallError::Unsupported(alloc::format!("get_global: {e}")))
        })?;
        let bytes = self.vmctx_read_bytes(span.offset, span.len)?;
        decode_global_read(&span.ty, &bytes, self.module.options.float_mode).map_err(|e| {
            NativeError::Call(CallError::Unsupported(alloc::format!("get_global: {e}")))
        })
    }

    fn call_compute_tick(&mut self, name: &str) -> Result<(), Self::Error> {
        let sig = self
            .module
            .meta
            .functions
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
        validate_compute_tick_sig(sig)
            .map_err(|e| NativeError::Call(CallError::Unsupported(format!("{name}: {e}"))))?;
        self.invoke_flat(name, &[], CycleModel::default())?;
        Ok(())
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

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    use lp_shader::synth::synthesise_render_texture;
    use lpir::builder::FunctionBuilder;
    use lpir::{FuncId, IrType, LpirModule, LpirOp};
    use lps_shared::{
        FnParam, LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier, TextureStorageFormat,
    };
    use lpvm::{
        DEFAULT_VMCTX_FUEL, INVOCATION_INDEX_ARMED, LpvmEngine, LpvmInstance, LpvmModule,
        TRAP_CODE_OUT_OF_FUEL, VMCTX_OFFSET_FUEL,
    };

    use crate::error::NativeError;
    use crate::native_options::NativeCompileOptions;
    use crate::rt_emu::NativeEmuEngine;

    const Q_ONE: i32 = 65536;
    const Q_HALF: i32 = 32768;

    /// An infinite loop exhausts its fuel tank and surfaces as the typed
    /// trap error; the same instance stays usable (arming resets the
    /// fuel and trap slots on the next entry).
    #[test]
    fn infinite_loop_traps_and_instance_stays_reusable() {
        let (ir, meta) = spin_and_ok_module();
        let engine = NativeEmuEngine::new(NativeCompileOptions::default());
        let module = engine.compile(&ir, &meta).expect("compile");
        let mut inst = module.instantiate().expect("instantiate");
        // Small tank keeps this test fast (~6 guest instructions per spin
        // iteration); the default 1M tank is exercised by
        // `default_tank_trap_fires_before_emulator_instruction_limit`.
        inst.set_armed_fuel(1_000);

        let err = inst
            .call_q32("spin", &[])
            .expect_err("infinite loop must trap");
        match &err {
            NativeError::Trap { code, invocation } => {
                assert_eq!(*code, TRAP_CODE_OUT_OF_FUEL);
                // No render wrapper ran, so the index is the host-armed marker.
                assert_eq!(*invocation, INVOCATION_INDEX_ARMED);
            }
            other => panic!("expected NativeError::Trap, got {other:?}"),
        }
        let msg = alloc::format!("{err}");
        assert!(
            msg.contains("trap") && msg.contains("fuel"),
            "trap message must mention trap + fuel: {msg}"
        );

        // Next call re-arms and succeeds.
        let words = inst
            .call_q32("ok", &[])
            .expect("instance reusable after trap");
        assert_eq!(words, vec![42]);
    }

    /// With the raised `EMU_CALL_INSTRUCTION_LIMIT`, a flat call armed with
    /// the DEFAULT 1M tank exhausts guest fuel (≈6M spin instructions)
    /// before the emulator instruction limit — the typed fuel trap, not
    /// `InstructionLimitExceeded`, is what surfaces.
    #[test]
    fn default_tank_trap_fires_before_emulator_instruction_limit() {
        let (ir, meta) = spin_and_ok_module();
        let engine = NativeEmuEngine::new(NativeCompileOptions::default());
        let module = engine.compile(&ir, &meta).expect("compile");
        let mut inst = module.instantiate().expect("instantiate");

        let err = inst
            .call_q32("spin", &[])
            .expect_err("infinite loop must trap on the default tank");
        match &err {
            NativeError::Trap { code, .. } => assert_eq!(*code, TRAP_CODE_OUT_OF_FUEL),
            other => panic!("expected NativeError::Trap, got {other:?}"),
        }
        let insts = inst
            .last_guest_instruction_count()
            .expect("instruction count recorded on the trap path");
        assert!(
            insts > DEFAULT_VMCTX_FUEL,
            "draining a 1M tank must cost more than 1M guest instructions \
             (would have died on the old emulator limit): {insts}"
        );
    }

    /// A loop of N iterations consumes exactly N fuel units: back-edge
    /// checks decrement by one, entry checks are check-only.
    #[test]
    fn counted_loop_consumes_one_fuel_unit_per_backedge() {
        let (ir, meta) = counted_loop_module();
        let engine = NativeEmuEngine::new(NativeCompileOptions::default());
        let module = engine.compile(&ir, &meta).expect("compile");
        let mut inst = module.instantiate().expect("instantiate");

        let n = 10;
        let words = inst.call_q32("count", &[n]).expect("call");
        assert_eq!(words, vec![n]);

        let fuel_bytes = inst
            .vmctx_read_bytes(VMCTX_OFFSET_FUEL, 4)
            .expect("read fuel low");
        let fuel = u32::from_le_bytes(fuel_bytes[..4].try_into().expect("4 bytes"));
        assert_eq!(
            fuel,
            DEFAULT_VMCTX_FUEL as u32 - n as u32,
            "expected armed - N after N back-edges"
        );
    }

    /// Render-texture path with the real synth wrapper: the pixel that
    /// loops forever traps with its linear invocation index; pixels before
    /// it are already written.
    #[test]
    fn render_texture_trap_reports_offending_pixel_index() {
        let k = 2u32; // pixel (2, 0) of a 4x1 texture
        let (mut ir, mut meta) = render_module_spinning_at(k as i32 * Q_ONE + Q_HALF);
        let name =
            synthesise_render_texture(&mut ir, &mut meta, 0, TextureStorageFormat::Rgba16Unorm)
                .expect("synth");

        let engine = NativeEmuEngine::new(NativeCompileOptions::default());
        let module = engine.compile(&ir, &meta).expect("compile");
        let mut inst = module.instantiate().expect("instantiate");
        let mut tex = engine.memory().alloc(4 * 8, 4).expect("alloc texture");

        let err = inst
            .call_render_texture(&name, &mut tex, 4, 1)
            .expect_err("pixel k must trap");
        match &err {
            NativeError::Trap { code, invocation } => {
                assert_eq!(*code, TRAP_CODE_OUT_OF_FUEL);
                assert_eq!(*invocation, k, "invocation index must be the pixel index");
            }
            other => panic!("expected NativeError::Trap, got {other:?}"),
        }

        // Pixels 0 and 1 rendered 1.0 per channel before the trap.
        let mut before = [0u8; 16];
        unsafe { tex.read(0, &mut before).expect("read pixels 0..2") };
        assert!(
            before.iter().all(|b| *b == 0xFF),
            "earlier pixels must be written: {before:?}"
        );

        // Headroom documentation for P3: the whole trapped frame (two good
        // pixels + one 100k-iteration tank) must fit under lp-riscv-emu's
        // 1M call_function instruction limit.
        let insts = inst
            .last_guest_instruction_count()
            .expect("instruction count recorded on the trap path");
        assert!(insts < 1_000_000, "observed {insts} guest instructions");
        eprintln!("render_texture fuel-trap frame: {insts} guest instructions");
    }

    /// A bounded shader renders byte-identically with fuel on and off.
    #[test]
    fn bounded_render_is_byte_identical_with_fuel_on_and_off() {
        let with_fuel = bounded_render_bytes(true);
        let without_fuel = bounded_render_bytes(false);
        assert_eq!(with_fuel, without_fuel);
        assert!(
            with_fuel.iter().any(|b| *b != 0),
            "sanity: rendered texture must not be all zero"
        );
    }

    fn bounded_render_bytes(fuel: bool) -> Vec<u8> {
        let (mut ir, mut meta) = bounded_render_module();
        let name =
            synthesise_render_texture(&mut ir, &mut meta, 0, TextureStorageFormat::Rgba16Unorm)
                .expect("synth");
        let options = NativeCompileOptions {
            fuel,
            ..NativeCompileOptions::default()
        };
        let engine = NativeEmuEngine::new(options);
        let module = engine.compile(&ir, &meta).expect("compile");
        let mut inst = module.instantiate().expect("instantiate");
        let mut tex = engine.memory().alloc(4 * 2 * 8, 4).expect("alloc texture");
        inst.call_render_texture(&name, &mut tex, 4, 2)
            .expect("bounded render");
        let mut bytes = vec![0u8; 4 * 2 * 8];
        unsafe { tex.read(0, &mut bytes).expect("read texture") };
        bytes
    }

    /// `spin`: void fn looping forever; `ok`: () -> 42.
    fn spin_and_ok_module() -> (LpirModule, LpsModuleSig) {
        let mut fb = FunctionBuilder::new("spin", &[]);
        let scratch = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::IconstI32 {
            dst: scratch,
            value: 0,
        });
        fb.push_loop();
        fb.push(LpirOp::IaddImm {
            dst: scratch,
            src: scratch,
            imm: 1,
        });
        fb.end_loop();
        fb.push_return(&[]);
        let spin = fb.finish();

        let mut fb = FunctionBuilder::new("ok", &[IrType::I32]);
        let r = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::IconstI32 { dst: r, value: 42 });
        fb.push_return(&[r]);
        let ok = fb.finish();

        let mut module = LpirModule::new();
        module.functions.insert(FuncId(0), spin);
        module.functions.insert(FuncId(1), ok);

        let meta = LpsModuleSig {
            functions: vec![
                void_sig("spin"),
                LpsFnSig {
                    name: String::from("ok"),
                    return_type: LpsType::Int,
                    parameters: vec![],
                    kind: LpsFnKind::UserDefined,
                },
            ],
            ..Default::default()
        };
        (module, meta)
    }

    /// `count(n)`: i = 0; loop { if i >= n break; i += 1 }; return i.
    fn counted_loop_module() -> (LpirModule, LpsModuleSig) {
        let mut fb = FunctionBuilder::new("count", &[IrType::I32]);
        let n = fb.add_param(IrType::I32);
        let i = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::IconstI32 { dst: i, value: 0 });
        fb.push_loop();
        {
            let done = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IgeS {
                dst: done,
                lhs: i,
                rhs: n,
            });
            fb.push_if(done);
            fb.push(LpirOp::Break);
            fb.end_if();
            fb.push(LpirOp::IaddImm {
                dst: i,
                src: i,
                imm: 1,
            });
        }
        fb.end_loop();
        fb.push_return(&[i]);

        let mut module = LpirModule::new();
        module.functions.insert(FuncId(0), fb.finish());

        let meta = LpsModuleSig {
            functions: vec![LpsFnSig {
                name: String::from("count"),
                return_type: LpsType::Int,
                parameters: vec![FnParam {
                    name: String::from("n"),
                    ty: LpsType::Int,
                    qualifier: ParamQualifier::In,
                }],
                kind: LpsFnKind::UserDefined,
            }],
            ..Default::default()
        };
        (module, meta)
    }

    /// `render(pos)`: loops forever iff `pos.x == k_center` (Q16.16 bits),
    /// else returns vec4(1.0).
    fn render_module_spinning_at(k_center_bits: i32) -> (LpirModule, LpsModuleSig) {
        let ret_tys = [IrType::F32; 4];
        let mut fb = FunctionBuilder::new("render", &ret_tys);
        let px = fb.add_param(IrType::F32);
        let _py = fb.add_param(IrType::F32);

        let k_bits = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::IconstI32 {
            dst: k_bits,
            value: k_center_bits,
        });
        let kf = fb.alloc_vreg(IrType::F32);
        fb.push(LpirOp::FfromI32Bits {
            dst: kf,
            src: k_bits,
        });
        let cond = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::Feq {
            dst: cond,
            lhs: px,
            rhs: kf,
        });
        fb.push_if(cond);
        {
            // Empty body: the cheapest possible spin (~9 guest instructions
            // per back-edge incl. the fuel check), so draining the 100k
            // tank stays under lp-riscv-emu's 1M call_function limit until
            // P3 makes that limit configurable.
            fb.push_loop();
            fb.end_loop();
        }
        fb.end_if();

        let one_bits = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::IconstI32 {
            dst: one_bits,
            value: Q_ONE,
        });
        let one = fb.alloc_vreg(IrType::F32);
        fb.push(LpirOp::FfromI32Bits {
            dst: one,
            src: one_bits,
        });
        fb.push_return(&[one, one, one, one]);

        let mut module = LpirModule::new();
        module.functions.insert(FuncId(0), fb.finish());
        (module, render_vec4_meta())
    }

    /// `render(pos)`: 3-iteration counted loop, then vec4(pos.x * 0.25).
    fn bounded_render_module() -> (LpirModule, LpsModuleSig) {
        let ret_tys = [IrType::F32; 4];
        let mut fb = FunctionBuilder::new("render", &ret_tys);
        let px = fb.add_param(IrType::F32);
        let _py = fb.add_param(IrType::F32);

        let i = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::IconstI32 { dst: i, value: 0 });
        let three = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::IconstI32 {
            dst: three,
            value: 3,
        });
        fb.push_loop();
        {
            let done = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IgeS {
                dst: done,
                lhs: i,
                rhs: three,
            });
            fb.push_if(done);
            fb.push(LpirOp::Break);
            fb.end_if();
            fb.push(LpirOp::IaddImm {
                dst: i,
                src: i,
                imm: 1,
            });
        }
        fb.end_loop();

        let quarter_bits = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::IconstI32 {
            dst: quarter_bits,
            value: Q_ONE / 4,
        });
        let quarter = fb.alloc_vreg(IrType::F32);
        fb.push(LpirOp::FfromI32Bits {
            dst: quarter,
            src: quarter_bits,
        });
        let c = fb.alloc_vreg(IrType::F32);
        fb.push(LpirOp::Fmul {
            dst: c,
            lhs: px,
            rhs: quarter,
        });
        fb.push_return(&[c, c, c, c]);

        let mut module = LpirModule::new();
        module.functions.insert(FuncId(0), fb.finish());
        (module, render_vec4_meta())
    }

    fn render_vec4_meta() -> LpsModuleSig {
        LpsModuleSig {
            functions: vec![LpsFnSig {
                name: String::from("render"),
                return_type: LpsType::Vec4,
                parameters: vec![FnParam {
                    name: String::from("pos"),
                    ty: LpsType::Vec2,
                    qualifier: ParamQualifier::In,
                }],
                kind: LpsFnKind::UserDefined,
            }],
            ..Default::default()
        }
    }

    fn void_sig(name: &str) -> LpsFnSig {
        LpsFnSig {
            name: String::from(name),
            return_type: LpsType::Void,
            parameters: vec![],
            kind: LpsFnKind::UserDefined,
        }
    }
}
