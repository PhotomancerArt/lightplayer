//! wasmtime store + instance; implements [`lpvm::LpvmInstance`].

use std::collections::HashMap;
use std::format;
use std::sync::Arc;

use lpir::FloatMode;
use lps_shared::{LpsModuleSig, LpsType, ParamQualifier};
use lpvm::{DEFAULT_VMCTX_FUEL, LpsValueF32, LpvmInstance};
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

/// Runnable WASM instance (fuel, shadow stack, linked memory).
pub struct WasmLpvmInstance {
    runtime: Arc<WasmLpvmSharedRuntime>,
    instance: Instance,
    exports: HashMap<String, WasmExport>,
    signatures: LpsModuleSig,
    shadow_stack_base: Option<i32>,
    float_mode: FloatMode,
}

impl WasmLpvmInstance {
    pub(crate) fn new(module: &WasmLpvmModule) -> Result<Self, WasmError> {
        let instance = link::instantiate_wasm_module(
            &module.engine,
            module.runtime.as_ref(),
            &module.wasm_bytes,
        )?;
        Ok(Self {
            runtime: Arc::clone(&module.runtime),
            instance,
            exports: module.exports.clone(),
            signatures: module.signatures.clone(),
            shadow_stack_base: module.shadow_stack_base,
            float_mode: module.opts.float_mode,
        })
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
}
