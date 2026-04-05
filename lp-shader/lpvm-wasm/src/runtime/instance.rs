//! wasmtime store + instance; implements [`lpvm::LpvmInstance`].

use std::collections::HashMap;
use std::format;

use lpir::FloatMode;
use lps_shared::{LpsModuleSig, LpsType, ParamQualifier};
use lpvm::{DEFAULT_VMCTX_FUEL, LpsValue, LpvmInstance};
use wasmtime::{Instance, Store, Val};

use crate::error::WasmError;
use crate::module::{SHADOW_STACK_GLOBAL_EXPORT, WasmExport};
use crate::runtime::link;
use crate::runtime::marshal::{build_wasm_args, wasm_vals_to_lps_value, zero_results_for_type};

use super::WasmLpvmModule;

/// Runnable WASM instance (fuel, shadow stack, linked memory).
pub struct WasmLpvmInstance {
    store: Store<()>,
    instance: Instance,
    exports: HashMap<String, WasmExport>,
    signatures: LpsModuleSig,
    shadow_stack_base: Option<i32>,
    float_mode: FloatMode,
}

impl WasmLpvmInstance {
    pub(crate) fn new(module: &WasmLpvmModule) -> Result<Self, WasmError> {
        let mut store = Store::new(&module.engine, ());
        let (instance, _) = link::instantiate_wasm_module(
            &module.engine,
            &mut store,
            &module.wasm_bytes,
            &module.builtins_wasm,
        )?;
        Ok(Self {
            store,
            instance,
            exports: module.exports.clone(),
            signatures: module.signatures.clone(),
            shadow_stack_base: module.shadow_stack_base,
            float_mode: module.opts.float_mode,
        })
    }

    fn prepare_call(&mut self) -> Result<(), WasmError> {
        if let Some(base) = self.shadow_stack_base {
            let g = self
                .instance
                .get_global(&mut self.store, SHADOW_STACK_GLOBAL_EXPORT)
                .ok_or_else(|| WasmError::runtime("missing shadow stack global export"))?;
            g.set(&mut self.store, Val::I32(base)).map_err(|e| {
                WasmError::runtime(format!("failed to reset shadow stack pointer: {e}"))
            })?;
        }
        self.store
            .set_fuel(DEFAULT_VMCTX_FUEL)
            .map_err(|e| WasmError::runtime(format!("failed to set fuel: {e}")))
    }
}

impl LpvmInstance for WasmLpvmInstance {
    type Error = WasmError;

    fn call(&mut self, name: &str, args: &[LpsValue]) -> Result<LpsValue, Self::Error> {
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

        let func = self
            .instance
            .get_func(&mut self.store, name)
            .ok_or_else(|| WasmError::runtime(format!("function '{name}' not found")))?;

        self.prepare_call()?;
        func.call(&mut self.store, &wasm_args, &mut results)
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
}
