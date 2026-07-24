//! wasmtime store + instance; implements [`lpvm::LpvmInstance`].

use std::collections::HashMap;
use std::format;
use std::sync::Arc;

use lpir::FloatMode;
use lps_shared::{LpsModuleSig, LpsType, LpsValueQ32, ParamQualifier};
use lpvm::{
    DEFAULT_VMCTX_FUEL, INVOCATION_INDEX_ARMED, LpsValueF32, LpvmBuffer, LpvmInstance,
    TRAP_CODE_NONE, VMCTX_OFFSET_FUEL, VMCTX_OFFSET_TRAP, decode_global_read, encode_global_write,
    encode_uniform_write, encode_uniform_write_q32, global_data_span, validate_compute_tick_sig,
    validate_render_samples_sig_ir, validate_render_texture_sig_ir,
};
use wasmtime::{Instance, Val};

use super::WasmLpvmSharedRuntime;
use super::link;
use super::marshal::{
    build_wasm_args_for_call, build_wasm_args_q32_for_call, build_wasm_args_q32_scalar_only,
    build_wasm_args_scalar_only, decode_sret_q32_return, shadow_stack_frame_close,
    shadow_stack_frame_open, wasm_vals_to_lps_value, wasm_vals_to_q32_words, zero_results_for_type,
};
use crate::aggregate_abi::{decode_aggregate_std430_bytes, export_needs_shadow_marshal};
use crate::error::WasmError;
use crate::module::{SHADOW_STACK_GLOBAL_EXPORT, WasmExport};

use super::WasmLpvmModule;
use lpir::LpirModule;

struct RenderTextureEntry {
    name: String,
    func: wasmtime::Func,
}

/// Runnable WASM instance (fuel, shadow stack, linked memory, globals lifecycle).
pub struct WasmLpvmInstance {
    runtime: Arc<WasmLpvmSharedRuntime>,
    instance: Instance,
    exports: HashMap<String, WasmExport>,
    signatures: LpsModuleSig,
    shadow_stack_base: Option<i32>,
    float_mode: FloatMode,
    /// Byte offset from vmctx base to globals region
    globals_offset: usize,
    /// Byte offset from vmctx base to snapshot region
    snapshot_offset: usize,
    /// Size of globals region in bytes
    globals_size: usize,
    lpir: LpirModule,
    render_texture_cache: Option<RenderTextureEntry>,
    render_samples_cache: Option<RenderTextureEntry>,
}

impl WasmLpvmInstance {
    pub(crate) fn new(module: &WasmLpvmModule) -> Result<Self, WasmError> {
        let instance = link::instantiate_wasm_module(
            &module.engine,
            module.runtime.as_ref(),
            &module.wasm_bytes,
        )?;

        let sigs = &module.signatures;
        let globals_offset = sigs.globals_offset();
        let snapshot_offset = sigs.snapshot_offset();
        let globals_size = sigs.globals_size();

        let mut inst = Self {
            runtime: Arc::clone(&module.runtime),
            instance,
            exports: module.exports.clone(),
            signatures: module.signatures.clone(),
            shadow_stack_base: module.shadow_stack_base,
            float_mode: module.opts.float_mode,
            globals_offset,
            snapshot_offset,
            globals_size,
            lpir: module.lpir.clone(),
            render_texture_cache: None,
            render_samples_cache: None,
        };

        // Auto-init globals: call __shader_init if it exists, then snapshot
        inst.init_globals()?;

        Ok(inst)
    }

    /// Initialize globals by calling `__shader_init` if it exists,
    /// then memcpy globals -> snapshot to capture the initialized state.
    pub fn init_globals(&mut self) -> Result<(), WasmError> {
        let mut guard = self.runtime.lock();

        // Call __shader_init if it exists (it may not be present if there are no globals with initializers)
        if self.exports.contains_key("__shader_init") {
            let mem = guard.memory;
            let store = &mut guard.store;
            let func = self
                .instance
                .get_func(&mut *store, "__shader_init")
                .ok_or_else(|| WasmError::runtime("__shader_init export not found"))?;

            self.prepare_call(store, mem)?;
            // Pass vmctx pointer (0) as first argument, same as other shader calls
            let wasm_args = vec![Val::I32(0)];
            let call_result = func.call(&mut *store, &wasm_args, &mut []);
            take_trap(store, mem)?;
            call_result
                .map_err(|e| WasmError::runtime(format!("WASM trap in __shader_init: {e}")))?;
        }

        // Copy globals region to snapshot region
        self.snapshot_globals_with_guard(&mut guard);
        Ok(())
    }

    /// Reset globals by memcpy snapshot -> globals using the provided guard.
    /// This is a no-op if globals_size == 0.
    fn reset_globals_with_guard(
        &self,
        guard: &mut super::shared_runtime::WasmLpvmSharedRuntimeInner,
    ) {
        if self.globals_size == 0 {
            return;
        }

        let mem = guard.memory;
        let store = &mut guard.store;

        let globals_start = self.globals_offset;
        let snapshot_start = self.snapshot_offset;
        let size = self.globals_size;

        // Copy snapshot -> globals
        let src = mem.data(&*store)[snapshot_start..snapshot_start + size].to_vec();
        let dst = &mut mem.data_mut(store)[globals_start..globals_start + size];
        dst.copy_from_slice(&src);
    }

    /// Copy globals region to snapshot region (for init) using the provided guard.
    fn snapshot_globals_with_guard(
        &self,
        guard: &mut super::shared_runtime::WasmLpvmSharedRuntimeInner,
    ) {
        if self.globals_size == 0 {
            return;
        }

        let mem = guard.memory;
        let store = &mut guard.store;

        let globals_start = self.globals_offset;
        let snapshot_start = self.snapshot_offset;
        let size = self.globals_size;

        // Copy globals -> snapshot
        let src = mem.data(&*store)[globals_start..globals_start + size].to_vec();
        let dst = &mut mem.data_mut(store)[snapshot_start..snapshot_start + size];
        dst.copy_from_slice(&src);
    }

    /// Arm the vmctx fuel/trap words before a guest entry: full tank in the
    /// fuel low u32, host-armed invocation index in the high u32, no trap.
    /// Render wrappers immediately re-arm per pixel/sample with
    /// `DEFAULT_INVOCATION_FUEL`; `metadata` (vmctx+12) is left untouched.
    /// The vmctx block sits at guest offset 0 (see `marshal` — the vmctx
    /// pointer passed as WASM param 0 is 0).
    fn prepare_call(
        &self,
        store: &mut wasmtime::Store<()>,
        linear_memory: wasmtime::Memory,
    ) -> Result<(), WasmError> {
        let mut header = [0u8; 12];
        header[0..4].copy_from_slice(&(DEFAULT_VMCTX_FUEL as u32).to_le_bytes());
        header[4..8].copy_from_slice(&INVOCATION_INDEX_ARMED.to_le_bytes());
        header[8..12].copy_from_slice(&TRAP_CODE_NONE.to_le_bytes());
        debug_assert_eq!(VMCTX_OFFSET_FUEL, 0);
        debug_assert_eq!(VMCTX_OFFSET_TRAP, 8);
        linear_memory
            .write(&mut *store, VMCTX_OFFSET_FUEL, &header)
            .map_err(|e| WasmError::runtime(format!("failed to write vmctx fuel header: {e}")))?;
        if let Some(base) = self.shadow_stack_base {
            let g = self
                .instance
                .get_global(&mut *store, SHADOW_STACK_GLOBAL_EXPORT)
                .ok_or_else(|| WasmError::runtime("missing shadow stack global export"))?;
            g.set(&mut *store, Val::I32(base)).map_err(|e| {
                WasmError::runtime(format!("failed to reset shadow stack pointer: {e}"))
            })?;
        }
        Ok(())
    }

    fn vmctx_write_bytes(&mut self, offset: usize, data: &[u8]) -> Result<(), WasmError> {
        let total = self.signatures.vmctx_buffer_size();
        let end = offset
            .checked_add(data.len())
            .ok_or_else(|| WasmError::runtime("vmctx write: offset overflow"))?;
        if end > total {
            return Err(WasmError::runtime(format!(
                "vmctx write out of bounds: end {end} total {total}"
            )));
        }
        let mut guard = self.runtime.lock();
        let mem = guard.memory;
        let store = &mut guard.store;
        mem.write(store, offset, data)
            .map_err(|e| WasmError::runtime(format!("vmctx write failed: {e}")))?;
        Ok(())
    }

    fn vmctx_read_bytes(&mut self, offset: usize, len: usize) -> Result<Vec<u8>, WasmError> {
        let total = self.signatures.vmctx_buffer_size();
        let end = offset
            .checked_add(len)
            .ok_or_else(|| WasmError::runtime("vmctx read: offset overflow"))?;
        if end > total {
            return Err(WasmError::runtime(format!(
                "vmctx read out of bounds: end {end} total {total}"
            )));
        }
        let mut guard = self.runtime.lock();
        let mem = guard.memory;
        let store = &mut guard.store;
        let mut bytes = vec![0u8; len];
        mem.read(store, offset, &mut bytes)
            .map_err(|e| WasmError::runtime(format!("vmctx read failed: {e}")))?;
        Ok(bytes)
    }

    fn resolve_render_texture(&mut self, fn_name: &str) -> Result<wasmtime::Func, WasmError> {
        if let Some(entry) = &self.render_texture_cache {
            if entry.name == fn_name {
                return Ok(entry.func);
            }
        }

        let ir_fn = self
            .lpir
            .functions
            .values()
            .find(|f| f.name == fn_name)
            .ok_or_else(|| WasmError::runtime(format!("function `{fn_name}` not in LPIR")))?;
        validate_render_texture_sig_ir(ir_fn)
            .map_err(|e| WasmError::runtime(format!("render-texture sig invalid: {e}")))?;

        let mut guard = self.runtime.lock();
        let store = &mut guard.store;
        let func = self
            .instance
            .get_func(store, fn_name)
            .ok_or_else(|| WasmError::runtime(format!("function `{fn_name}` not found")))?;
        let func_ret = func;
        self.render_texture_cache = Some(RenderTextureEntry {
            name: fn_name.into(),
            func,
        });
        Ok(func_ret)
    }

    fn resolve_render_samples(&mut self, fn_name: &str) -> Result<wasmtime::Func, WasmError> {
        if let Some(entry) = &self.render_samples_cache {
            if entry.name == fn_name {
                return Ok(entry.func);
            }
        }

        let ir_fn = self
            .lpir
            .functions
            .values()
            .find(|f| f.name == fn_name)
            .ok_or_else(|| WasmError::runtime(format!("function `{fn_name}` not in LPIR")))?;
        validate_render_samples_sig_ir(ir_fn)
            .map_err(|e| WasmError::runtime(format!("render-samples sig invalid: {e}")))?;

        let mut guard = self.runtime.lock();
        let store = &mut guard.store;
        let func = self
            .instance
            .get_func(store, fn_name)
            .ok_or_else(|| WasmError::runtime(format!("function `{fn_name}` not found")))?;
        let func_ret = func;
        self.render_samples_cache = Some(RenderTextureEntry {
            name: fn_name.into(),
            func,
        });
        Ok(func_ret)
    }
}

fn format_wasm_call_error(context: &str, error: wasmtime::Error) -> WasmError {
    WasmError::runtime(format!("WASM trap while {context}: {error}"))
}

/// Read the vmctx trap slot after a guest entry; nonzero → typed
/// [`WasmError::Trap`] carrying the invocation index (fuel high u32).
///
/// Must run on BOTH `Ok` and `Err` call returns: an emitted fuel check
/// aborts with `unreachable`, which arrives as a generic wasmtime trap
/// error — classification is by the slot, never the message. Return values
/// from a trapped call are garbage; callers must discard them.
fn take_trap(
    store: &mut wasmtime::Store<()>,
    linear_memory: wasmtime::Memory,
) -> Result<(), WasmError> {
    let mut words = [0u8; 8];
    linear_memory
        .read(&*store, VMCTX_OFFSET_FUEL + 4, &mut words)
        .map_err(|e| WasmError::runtime(format!("failed to read vmctx trap slot: {e}")))?;
    let invocation = u32::from_le_bytes(words[0..4].try_into().expect("4 bytes"));
    let trap = u32::from_le_bytes(words[4..8].try_into().expect("4 bytes"));
    if trap == TRAP_CODE_NONE {
        Ok(())
    } else {
        Err(WasmError::Trap {
            code: trap,
            invocation,
        })
    }
}

impl LpvmInstance for WasmLpvmInstance {
    type Error = WasmError;

    fn call(&mut self, name: &str, args: &[LpsValueF32]) -> Result<LpsValueF32, Self::Error> {
        let fn_sig = self
            .signatures
            .functions
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| WasmError::runtime(format!("function '{name}' not found")))?;

        for p in &fn_sig.parameters {
            if matches!(p.qualifier, ParamQualifier::Out | ParamQualifier::InOut) {
                return Err(WasmError::runtime(
                    "out/inout parameters are not supported for direct calling.",
                ));
            }
        }

        let export = self.exports.get(name).cloned().ok_or_else(|| {
            WasmError::runtime(format!("function '{name}' not found in WASM export table"))
        })?;

        if matches!(export.return_type, LpsType::Void) {
            return Err(WasmError::runtime(
                "void return is not represented as LpsValue; use a typed return",
            ));
        }

        let return_ty = export.return_type.clone();
        let needs_shadow = export_needs_shadow_marshal(&export);
        if needs_shadow && self.shadow_stack_base.is_none() {
            return Err(WasmError::runtime(
                "aggregate/sret calling convention requires an exported shadow stack global",
            ));
        }

        let mut guard = self.runtime.lock();
        // Reset globals before each shader call to ensure per-pixel isolation
        self.reset_globals_with_guard(&mut guard);

        let mem = guard.memory;
        let store = &mut guard.store;
        let func = self
            .instance
            .get_func(&mut *store, name)
            .ok_or_else(|| WasmError::runtime(format!("function '{name}' not found")))?;

        self.prepare_call(store, mem)?;

        let shadow_frame = if needs_shadow {
            Some(shadow_stack_frame_open(&self.instance, store)?)
        } else {
            None
        };

        let (wasm_args, sret_plan) = if needs_shadow {
            build_wasm_args_for_call(
                &self.instance,
                store,
                &mem,
                &export,
                args,
                self.float_mode,
                &return_ty,
            )?
        } else {
            (
                build_wasm_args_scalar_only(
                    &export.param_types,
                    export.params.len(),
                    args,
                    self.float_mode,
                )?,
                None,
            )
        };

        let mut results = if export.uses_sret {
            Vec::new()
        } else {
            zero_results_for_type(&return_ty, self.float_mode)
        };

        let call_result = func.call(&mut *store, &wasm_args, &mut results);
        take_trap(store, mem)?;
        call_result.map_err(|e| format_wasm_call_error(&format!("calling `{name}`"), e))?;

        if let Some(frame) = shadow_frame {
            shadow_stack_frame_close(&self.instance, store, frame)?;
        }

        if export.uses_sret {
            let plan = sret_plan.ok_or_else(|| {
                WasmError::runtime("internal: sret export without sret allocation plan")
            })?;
            let bytes = super::marshal::wasmtime_memory_read(&mem, store, plan.ptr, plan.size)?;
            return decode_aggregate_std430_bytes(&return_ty, &bytes, self.float_mode);
        }

        let (val, consumed) = wasm_vals_to_lps_value(&return_ty, &results, self.float_mode)?;
        if consumed != results.len() {
            return Err(WasmError::runtime(format!(
                "return slot count mismatch: decoded {consumed}, got {}",
                results.len()
            )));
        }
        Ok(val)
    }

    fn call_q32(&mut self, name: &str, args: &[i32]) -> Result<Vec<i32>, Self::Error> {
        if self.float_mode != FloatMode::Q32 {
            return Err(WasmError::runtime(
                "WasmLpvmInstance::call_q32 requires FloatMode::Q32",
            ));
        }

        let fn_sig = self
            .signatures
            .functions
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| WasmError::runtime(format!("function '{name}' not found")))?;

        for p in &fn_sig.parameters {
            if matches!(p.qualifier, ParamQualifier::Out | ParamQualifier::InOut) {
                return Err(WasmError::runtime(
                    "out/inout parameters are not supported for direct calling.",
                ));
            }
        }

        let export = self.exports.get(name).cloned().ok_or_else(|| {
            WasmError::runtime(format!("function '{name}' not found in WASM export table"))
        })?;

        let return_ty = export.return_type.clone();
        let needs_shadow = export_needs_shadow_marshal(&export);
        if needs_shadow && self.shadow_stack_base.is_none() {
            return Err(WasmError::runtime(
                "aggregate/sret calling convention requires an exported shadow stack global",
            ));
        }

        let mut guard = self.runtime.lock();
        // Reset globals before each shader call to ensure per-pixel isolation
        self.reset_globals_with_guard(&mut guard);

        let mem = guard.memory;
        let store = &mut guard.store;
        let func = self
            .instance
            .get_func(&mut *store, name)
            .ok_or_else(|| WasmError::runtime(format!("function '{name}' not found")))?;

        self.prepare_call(store, mem)?;

        let shadow_frame = if needs_shadow {
            Some(shadow_stack_frame_open(&self.instance, store)?)
        } else {
            None
        };

        let (wasm_args, sret_plan) = if needs_shadow {
            build_wasm_args_q32_for_call(&self.instance, store, &mem, &export, args, &return_ty)?
        } else {
            (
                build_wasm_args_q32_scalar_only(&export.param_types, export.params.len(), args)?,
                None,
            )
        };

        if matches!(return_ty, LpsType::Void) {
            let mut results: Vec<Val> = Vec::new();
            let call_result = func.call(&mut *store, &wasm_args, &mut results);
            take_trap(store, mem)?;
            call_result.map_err(|e| format_wasm_call_error(&format!("calling `{name}`"), e))?;
            if let Some(frame) = shadow_frame {
                shadow_stack_frame_close(&self.instance, store, frame)?;
            }
            return Ok(Vec::new());
        }

        let mut results = if export.uses_sret {
            Vec::new()
        } else {
            zero_results_for_type(&return_ty, self.float_mode)
        };
        let call_result = func.call(&mut *store, &wasm_args, &mut results);
        take_trap(store, mem)?;
        call_result.map_err(|e| format_wasm_call_error(&format!("calling `{name}`"), e))?;

        if let Some(frame) = shadow_frame {
            shadow_stack_frame_close(&self.instance, store, frame)?;
        }

        if export.uses_sret {
            let plan = sret_plan.ok_or_else(|| {
                WasmError::runtime("internal: sret export without sret allocation plan")
            })?;
            return decode_sret_q32_return(&mem, store, &plan, &return_ty);
        }

        wasm_vals_to_q32_words(&return_ty, &results, self.float_mode)
    }

    fn call_render_texture(
        &mut self,
        fn_name: &str,
        texture: &mut LpvmBuffer,
        width: u32,
        height: u32,
    ) -> Result<(), Self::Error> {
        if self.float_mode != FloatMode::Q32 {
            return Err(WasmError::runtime(
                "WasmLpvmInstance::call_render_texture requires FloatMode::Q32",
            ));
        }

        let func = self.resolve_render_texture(fn_name)?;
        let tex_offset = i32::try_from(texture.guest_base()).map_err(|_| {
            WasmError::runtime(format!(
                "texture guest base {:#x} exceeds i32 range",
                texture.guest_base()
            ))
        })?;

        let wasm_args = vec![
            Val::I32(0),
            Val::I32(tex_offset),
            Val::I32(width as i32),
            Val::I32(height as i32),
        ];

        let mut guard = self.runtime.lock();
        self.reset_globals_with_guard(&mut guard);

        let mem = guard.memory;
        let store = &mut guard.store;
        self.prepare_call(store, mem)?;
        let call_result = func.call(&mut *store, &wasm_args, &mut []);
        take_trap(store, mem)?;
        call_result.map_err(|e| {
            format_wasm_call_error(&format!("rendering texture via `{fn_name}`"), e)
        })?;
        Ok(())
    }

    fn call_render_samples(
        &mut self,
        fn_name: &str,
        points: &mut LpvmBuffer,
        out: &mut LpvmBuffer,
        count: u32,
    ) -> Result<(), Self::Error> {
        if self.float_mode != FloatMode::Q32 {
            return Err(WasmError::runtime(
                "WasmLpvmInstance::call_render_samples requires FloatMode::Q32",
            ));
        }

        let func = self.resolve_render_samples(fn_name)?;
        let points_offset = i32::try_from(points.guest_base()).map_err(|_| {
            WasmError::runtime(format!(
                "points guest base {:#x} exceeds i32 range",
                points.guest_base()
            ))
        })?;
        let out_offset = i32::try_from(out.guest_base()).map_err(|_| {
            WasmError::runtime(format!(
                "sample output guest base {:#x} exceeds i32 range",
                out.guest_base()
            ))
        })?;

        let wasm_args = vec![
            Val::I32(0),
            Val::I32(points_offset),
            Val::I32(out_offset),
            Val::I32(count as i32),
        ];

        let mut guard = self.runtime.lock();
        self.reset_globals_with_guard(&mut guard);

        let mem = guard.memory;
        let store = &mut guard.store;
        self.prepare_call(store, mem)?;
        let call_result = func.call(&mut *store, &wasm_args, &mut []);
        take_trap(store, mem)?;
        call_result.map_err(|e| {
            format_wasm_call_error(&format!("rendering samples via `{fn_name}`"), e)
        })?;
        Ok(())
    }

    fn set_uniform(&mut self, path: &str, value: &LpsValueF32) -> Result<(), Self::Error> {
        let (off, bytes) = encode_uniform_write(&self.signatures, path, value, self.float_mode)
            .map_err(|e| WasmError::runtime(format!("set_uniform: {e}")))?;
        self.vmctx_write_bytes(off, &bytes)
    }

    fn set_uniform_q32(&mut self, path: &str, value: &LpsValueQ32) -> Result<(), Self::Error> {
        let (off, bytes) = encode_uniform_write_q32(&self.signatures, path, value)
            .map_err(|e| WasmError::runtime(format!("set_uniform_q32: {e}")))?;
        self.vmctx_write_bytes(off, &bytes)
    }

    fn set_global(&mut self, path: &str, value: &LpsValueF32) -> Result<(), Self::Error> {
        let (off, bytes) = encode_global_write(&self.signatures, path, value, self.float_mode)
            .map_err(|e| WasmError::runtime(format!("set_global: {e}")))?;
        self.vmctx_write_bytes(off, &bytes)
    }

    fn get_global(&mut self, path: &str) -> Result<LpsValueF32, Self::Error> {
        let span = global_data_span(&self.signatures, path)
            .map_err(|e| WasmError::runtime(format!("get_global: {e}")))?;
        let bytes = self.vmctx_read_bytes(span.offset, span.len)?;
        decode_global_read(&span.ty, &bytes, self.float_mode)
            .map_err(|e| WasmError::runtime(format!("get_global: {e}")))
    }

    fn call_compute_tick(&mut self, name: &str) -> Result<(), Self::Error> {
        let fn_sig = self
            .signatures
            .functions
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| WasmError::runtime(format!("function '{name}' not found")))?;
        validate_compute_tick_sig(fn_sig)
            .map_err(|e| WasmError::runtime(format!("{name}: {e}")))?;

        let mut guard = self.runtime.lock();
        let mem = guard.memory;
        let store = &mut guard.store;
        let func = self
            .instance
            .get_func(&mut *store, name)
            .ok_or_else(|| WasmError::runtime(format!("function '{name}' not found")))?;
        self.prepare_call(store, mem)?;
        let call_result = func.call(&mut *store, &[Val::I32(0)], &mut []);
        take_trap(store, mem)?;
        call_result.map_err(|e| format_wasm_call_error(&format!("calling `{name}`"), e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Fuel-trap round trips on the wasmtime host, mirroring lpvm-native's
    //! rt_emu tests (`lpvm-native/src/rt_emu/instance.rs`): same LPIR
    //! modules, same header contract, same typed-trap expectations.

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

    use crate::error::WasmError;
    use crate::options::WasmOptions;
    use crate::rt_wasmtime::WasmLpvmEngine;

    const Q_ONE: i32 = 65536;
    const Q_HALF: i32 = 32768;

    /// An infinite loop exhausts the default 1M tank and surfaces as the
    /// typed trap error; the same instance stays usable (arming resets the
    /// fuel and trap slots on the next entry).
    #[test]
    fn infinite_loop_traps_and_instance_stays_reusable() {
        let (ir, meta) = spin_and_ok_module();
        let engine = WasmLpvmEngine::new(WasmOptions::default()).expect("engine");
        let module = engine.compile(&ir, &meta).expect("compile");
        let mut inst = module.instantiate().expect("instantiate");

        let err = inst
            .call_q32("spin", &[])
            .expect_err("infinite loop must trap");
        match &err {
            WasmError::Trap { code, invocation } => {
                assert_eq!(*code, TRAP_CODE_OUT_OF_FUEL);
                // No render wrapper ran, so the index is the host-armed marker.
                assert_eq!(*invocation, INVOCATION_INDEX_ARMED);
            }
            other => panic!("expected WasmError::Trap, got {other:?}"),
        }
        let msg = format!("{err}");
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

    /// A loop of N iterations consumes exactly N fuel units: back-edge
    /// checks decrement by one, entry checks are check-only.
    #[test]
    fn counted_loop_consumes_one_fuel_unit_per_backedge() {
        let (ir, meta) = counted_loop_module();
        let engine = WasmLpvmEngine::new(WasmOptions::default()).expect("engine");
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

        let engine = WasmLpvmEngine::new(WasmOptions::default()).expect("engine");
        let module = engine.compile(&ir, &meta).expect("compile");
        let mut inst = module.instantiate().expect("instantiate");
        let mut tex = engine.memory().alloc(4 * 8, 4).expect("alloc texture");

        let err = inst
            .call_render_texture(&name, &mut tex, 4, 1)
            .expect_err("pixel k must trap");
        match &err {
            WasmError::Trap { code, invocation } => {
                assert_eq!(*code, TRAP_CODE_OUT_OF_FUEL);
                assert_eq!(*invocation, k, "invocation index must be the pixel index");
            }
            other => panic!("expected WasmError::Trap, got {other:?}"),
        }

        // Pixels 0 and 1 rendered 1.0 per channel before the trap.
        let mut before = [0u8; 16];
        unsafe { tex.read(0, &mut before).expect("read pixels 0..2") };
        assert!(
            before.iter().all(|b| *b == 0xFF),
            "earlier pixels must be written: {before:?}"
        );
    }

    /// The legacy `render_frame` export (web-demo's entry point) is called
    /// raw — no host-side `prepare_call` arming — so with fuel on the
    /// wrapper must self-arm: a bounded shader renders even from a zeroed
    /// vmctx header (the per-pixel re-arm supplies the tank).
    #[test]
    fn raw_render_frame_export_self_arms_per_pixel() {
        let (ir, meta) = render_frame_module(None);
        let engine = WasmLpvmEngine::new(WasmOptions::default()).expect("engine");
        let module = engine.compile(&ir, &meta).expect("compile");
        let mut inst = module.instantiate().expect("instantiate");
        let out = engine.memory().alloc(4 * 4, 4).expect("alloc rgba8 out");
        let out_ptr = i32::try_from(out.guest_base()).expect("out ptr fits i32");

        // The raw path gets no arming; prove the wrapper works from zero.
        inst.vmctx_write_bytes(VMCTX_OFFSET_FUEL, &[0u8; 12])
            .expect("zero vmctx header");

        {
            let mut guard = inst.runtime.lock();
            let store = &mut guard.store;
            let func = inst
                .instance
                .get_func(&mut *store, "render_frame")
                .expect("render_frame export");
            func.call(
                &mut *store,
                &[
                    wasmtime::Val::I32(4),
                    wasmtime::Val::I32(1),
                    wasmtime::Val::I32(0),
                    wasmtime::Val::I32(out_ptr),
                ],
                &mut [],
            )
            .expect("raw render_frame call must self-arm");
        }

        let mut bytes = [0u8; 16];
        unsafe { out.read(0, &mut bytes).expect("read rgba8 out") };
        assert!(
            bytes.iter().all(|b| *b == 0xFF),
            "vec4(1.0) must convert to 0xFF per channel: {bytes:?}"
        );
    }

    /// Raw `render_frame` with an infinite pixel: the user render fn's
    /// emitted checks drain the wrapper-armed per-pixel tank, the trap
    /// unwinds the whole frame call, and the slot carries the offending
    /// linear pixel index. Earlier pixels are already written.
    #[test]
    fn raw_render_frame_export_traps_on_infinite_pixel() {
        let k = 2u32; // pixel (2, 0) of a 4x1 frame; wrapper passes x<<16
        let (ir, meta) = render_frame_module(Some(k as i32 * Q_ONE));
        let engine = WasmLpvmEngine::new(WasmOptions::default()).expect("engine");
        let module = engine.compile(&ir, &meta).expect("compile");
        let mut inst = module.instantiate().expect("instantiate");
        let out = engine.memory().alloc(4 * 4, 4).expect("alloc rgba8 out");
        let out_ptr = i32::try_from(out.guest_base()).expect("out ptr fits i32");

        {
            let mut guard = inst.runtime.lock();
            let store = &mut guard.store;
            let func = inst
                .instance
                .get_func(&mut *store, "render_frame")
                .expect("render_frame export");
            let result = func.call(
                &mut *store,
                &[
                    wasmtime::Val::I32(4),
                    wasmtime::Val::I32(1),
                    wasmtime::Val::I32(0),
                    wasmtime::Val::I32(out_ptr),
                ],
                &mut [],
            );
            assert!(result.is_err(), "infinite pixel must trap the raw call");
        }

        let words = inst
            .vmctx_read_bytes(VMCTX_OFFSET_FUEL + 4, 8)
            .expect("read invocation + trap slot");
        let invocation = u32::from_le_bytes(words[0..4].try_into().expect("4 bytes"));
        let trap = u32::from_le_bytes(words[4..8].try_into().expect("4 bytes"));
        assert_eq!(trap, TRAP_CODE_OUT_OF_FUEL);
        assert_eq!(invocation, k, "slot must carry the linear pixel index");

        let mut before = [0u8; 8];
        unsafe { out.read(0, &mut before).expect("read pixels 0..2") };
        assert!(
            before.iter().all(|b| *b == 0xFF),
            "earlier pixels must be written: {before:?}"
        );
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
        let options = WasmOptions {
            fuel,
            ..WasmOptions::default()
        };
        let engine = WasmLpvmEngine::new(options).expect("engine");
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
            // Empty body: the cheapest possible spin, draining the 100k
            // per-pixel tank in ~100k wasm loop iterations.
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

    /// `render(pos, size, time)` — the 5-scalar-param shape that makes the
    /// emitter synthesize the legacy `render_frame` wrapper export. Returns
    /// vec4(1.0); first spins forever iff `pos.x` equals `spin_at_bits`
    /// (raw Q16.16 bits — the wrapper passes `x << 16`).
    fn render_frame_module(spin_at_bits: Option<i32>) -> (LpirModule, LpsModuleSig) {
        let ret_tys = [IrType::F32; 4];
        let mut fb = FunctionBuilder::new("render", &ret_tys);
        let px = fb.add_param(IrType::F32);
        let _py = fb.add_param(IrType::F32);
        let _w = fb.add_param(IrType::F32);
        let _h = fb.add_param(IrType::F32);
        let _t = fb.add_param(IrType::F32);

        if let Some(bits) = spin_at_bits {
            let k_bits = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IconstI32 {
                dst: k_bits,
                value: bits,
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
                fb.push_loop();
                fb.end_loop();
            }
            fb.end_if();
        }

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

        let meta = LpsModuleSig {
            functions: vec![LpsFnSig {
                name: String::from("render"),
                return_type: LpsType::Vec4,
                parameters: vec![
                    FnParam {
                        name: String::from("pos"),
                        ty: LpsType::Vec2,
                        qualifier: ParamQualifier::In,
                    },
                    FnParam {
                        name: String::from("size"),
                        ty: LpsType::Vec2,
                        qualifier: ParamQualifier::In,
                    },
                    FnParam {
                        name: String::from("time"),
                        ty: LpsType::Float,
                        qualifier: ParamQualifier::In,
                    },
                ],
                kind: LpsFnKind::UserDefined,
            }],
            ..Default::default()
        };
        (module, meta)
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
