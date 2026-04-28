//! Compile LPIR (+ module metadata) to WASM.

use alloc::{format, vec::Vec};

use lpir::LpirModule;
use lps_shared::LpsModuleSig;

use crate::emit;
use crate::emit::func::wasm_function_signature;
use crate::error::WasmError;
use crate::module::{WasmExport, WasmModule};
use crate::options::WasmOptions;

/// Result of LPIR → WASM compilation: bytes, export layout, and the signature table.
#[derive(Debug, Clone)]
pub struct WasmArtifact {
    module: WasmModule,
    signatures: LpsModuleSig,
}

impl WasmArtifact {
    pub fn wasm_module(&self) -> &WasmModule {
        &self.module
    }

    pub fn bytes(&self) -> &[u8] {
        self.module.bytes()
    }

    pub fn signatures(&self) -> &LpsModuleSig {
        &self.signatures
    }

    pub fn into_parts(self) -> (WasmModule, LpsModuleSig) {
        (self.module, self.signatures)
    }
}

/// Compile `ir` using `meta` for export signatures (must match `ir.functions` order and names).
pub fn compile_lpir(
    ir: &LpirModule,
    meta: &LpsModuleSig,
    options: &WasmOptions,
) -> Result<WasmArtifact, WasmError> {
    validate_metadata(ir, meta)?;
    let (wasm_bytes, shadow_stack_base, env_memory) =
        emit::emit_module(ir, options).map_err(WasmError::emit)?;
    let exports = collect_exports(ir, meta, options);
    Ok(WasmArtifact {
        module: WasmModule {
            bytes: wasm_bytes,
            exports,
            shadow_stack_base,
            env_memory,
        },
        signatures: meta.clone(),
    })
}

fn validate_metadata(ir: &LpirModule, meta: &LpsModuleSig) -> Result<(), WasmError> {
    if ir.functions.len() != meta.functions.len() {
        return Err(WasmError::metadata_mismatch(format!(
            "IR has {} functions but metadata has {}",
            ir.functions.len(),
            meta.functions.len()
        )));
    }
    for (ir_f, sig) in ir.functions.values().zip(meta.functions.iter()) {
        if ir_f.name != sig.name {
            return Err(WasmError::metadata_mismatch(format!(
                "function name mismatch: IR {:?} vs metadata {:?}",
                ir_f.name, sig.name
            )));
        }
    }
    Ok(())
}

fn collect_exports(ir: &LpirModule, meta: &LpsModuleSig, options: &WasmOptions) -> Vec<WasmExport> {
    ir.functions
        .values()
        .zip(meta.functions.iter())
        .map(|(ir_f, sig)| {
            let (params, results) = wasm_function_signature(ir_f, options.float_mode);
            let uses_sret = ir_f.sret_arg.is_some();
            WasmExport {
                name: ir_f.name.clone(),
                params,
                results,
                return_type: sig.return_type.clone(),
                param_types: sig.parameters.iter().map(|p| p.ty.clone()).collect(),
                uses_sret,
            }
        })
        .collect()
}
