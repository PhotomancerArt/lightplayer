//! wasmtime store + instance; implements [`lpvm::LpvmInstance`].

use std::collections::HashMap;
use std::format;
use std::sync::Arc;

use lpir::FloatMode;
use lps_shared::{LpsModuleSig, LpsType, LpsValueQ32, ParamQualifier};
use lpvm::{
    DEFAULT_VMCTX_FUEL, LpsValueF32, LpvmBuffer, LpvmInstance, encode_uniform_write,
    encode_uniform_write_q32, validate_render_texture_sig_ir,
};
use wasmtime::{Instance, Val};

use super::WasmLpvmSharedRuntime;
use super::link;
use super::marshal::{
    build_wasm_args, build_wasm_args_q32_flat, wasm_vals_to_lps_value, wasm_vals_to_q32_words,
    zero_results_for_type,
};
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
            func.call(&mut *store, &wasm_args, &mut [])
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

    fn prepare_call(
        &self,
        store: &mut wasmtime::Store<()>,
        linear_memory: wasmtime::Memory,
    ) -> Result<(), WasmError> {
        // `__lp_get_fuel` reads `VmContext::fuel` as the first u64 in guest linear memory at the
        // vmctx pointer we pass as WASM param 0 (see `marshal` — currently 0). Keep that word in
        // sync with wasmtime execution fuel for filetests and host runs.
        let fuel_le = DEFAULT_VMCTX_FUEL.to_le_bytes();
        linear_memory
            .write(&mut *store, 0, &fuel_le)
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
        (*store)
            .set_fuel(DEFAULT_VMCTX_FUEL)
            .map_err(|e| WasmError::runtime(format!("failed to set fuel: {e}")))
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

    fn resolve_render_texture(&mut self, fn_name: &str) -> Result<wasmtime::Func, WasmError> {
        if let Some(entry) = &self.render_texture_cache {
            if entry.name == fn_name {
                return Ok(entry.func.clone());
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
        let func_ret = func.clone();
        self.render_texture_cache = Some(RenderTextureEntry {
            name: fn_name.into(),
            func,
        });
        Ok(func_ret)
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
        let wasm_args = build_wasm_args(
            &export.param_types,
            export.params.len(),
            args,
            self.float_mode,
        )?;

        let mut results = zero_results_for_type(&return_ty, self.float_mode);

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
        func.call(&mut *store, &wasm_args, &mut results)
            .map_err(|e| WasmError::runtime(format!("WASM trap: {e}")))?;

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
        let wasm_args = build_wasm_args_q32_flat(&export.param_types, export.params.len(), args)?;

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

        if matches!(return_ty, LpsType::Void) {
            let mut results: Vec<Val> = Vec::new();
            func.call(&mut *store, &wasm_args, &mut results)
                .map_err(|e| WasmError::runtime(format!("WASM trap: {e}")))?;
            return Ok(Vec::new());
        }

        let mut results = zero_results_for_type(&return_ty, self.float_mode);
        func.call(&mut *store, &wasm_args, &mut results)
            .map_err(|e| WasmError::runtime(format!("WASM trap: {e}")))?;

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
        func.call(&mut *store, &wasm_args, &mut [])
            .map_err(|e| WasmError::runtime(format!("WASM trap: {e}")))?;
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
}
