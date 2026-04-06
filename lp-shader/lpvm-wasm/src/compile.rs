//! Compile LPIR (+ module metadata) to WASM.

use alloc::{format, vec::Vec};

use lpir::IrModule;
use lps_shared::LpsModuleSig;

use crate::emit;
use crate::error::WasmError;
use crate::module::{WasmExport, WasmModule, WasmValType};
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
    ir: &IrModule,
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

fn validate_metadata(ir: &IrModule, meta: &LpsModuleSig) -> Result<(), WasmError> {
    if ir.functions.len() != meta.functions.len() {
        return Err(WasmError::metadata_mismatch(format!(
            "IR has {} functions but metadata has {}",
            ir.functions.len(),
            meta.functions.len()
        )));
    }
    for (ir_f, sig) in ir.functions.iter().zip(meta.functions.iter()) {
        if ir_f.name != sig.name {
            return Err(WasmError::metadata_mismatch(format!(
                "function name mismatch: IR {:?} vs metadata {:?}",
                ir_f.name, sig.name
            )));
        }
    }
    Ok(())
}

fn collect_exports(ir: &IrModule, meta: &LpsModuleSig, options: &WasmOptions) -> Vec<WasmExport> {
    ir.functions
        .iter()
        .zip(meta.functions.iter())
        .map(|(ir_f, sig)| {
            let mut params: Vec<_> = alloc::vec![WasmValType::I32];
            params.extend(sig.parameters.iter().flat_map(|p| {
                crate::module::glsl_type_to_wasm_components(&p.ty, options.float_mode)
            }));
            let results =
                crate::module::glsl_type_to_wasm_components(&sig.return_type, options.float_mode);
            WasmExport {
                name: ir_f.name.clone(),
                params,
                results,
                return_type: sig.return_type.clone(),
                param_types: sig.parameters.iter().map(|p| p.ty.clone()).collect(),
            }
        })
        .collect()
}
