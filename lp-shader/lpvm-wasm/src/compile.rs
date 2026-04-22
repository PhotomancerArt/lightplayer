//! Compile LPIR (+ module metadata) to WASM.

use alloc::{collections::BTreeMap, format, vec::Vec};

use lpir::LpirModule;
use lps_shared::{LpsFnSig, LpsModuleSig};

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
    ir: &LpirModule,
    meta: &LpsModuleSig,
    options: &WasmOptions,
) -> Result<WasmArtifact, WasmError> {
    let mut ir_opt = ir.clone();
    let inline_result = lpir::inline_module(&mut ir_opt, &options.config.inline);
    if inline_result.call_sites_replaced > 0 {
        log::info!(
            "[wasm] inline: replaced {} call sites",
            inline_result.call_sites_replaced
        );
    }
    if !matches!(
        options.config.dead_func_elim.mode,
        lpir::DeadFuncElimMode::Never
    ) {
        let roots = lpir::roots_from_is_entry(&ir_opt);
        if !roots.is_empty() {
            let dfe = lpir::dead_func_elim(&mut ir_opt, &roots);
            if dfe.functions_removed > 0 {
                log::info!(
                    "[wasm] dead_func_elim: removed {} functions",
                    dfe.functions_removed
                );
            }
        }
    }

    validate_metadata(&ir_opt, meta)?;
    let (wasm_bytes, shadow_stack_base, env_memory) =
        emit::emit_module(&ir_opt, options).map_err(WasmError::emit)?;
    let exports = collect_exports(&ir_opt, meta, options);
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
    let sig_map: BTreeMap<&str, &LpsFnSig> = meta
        .functions
        .iter()
        .map(|s| (s.name.as_str(), s))
        .collect();
    for ir_f in ir.functions.values() {
        if !sig_map.contains_key(ir_f.name.as_str()) {
            return Err(WasmError::metadata_mismatch(format!(
                "IR function {:?} has no metadata entry",
                ir_f.name
            )));
        }
    }
    Ok(())
}

fn collect_exports(ir: &LpirModule, meta: &LpsModuleSig, options: &WasmOptions) -> Vec<WasmExport> {
    let sig_map: BTreeMap<&str, &LpsFnSig> = meta
        .functions
        .iter()
        .map(|s| (s.name.as_str(), s))
        .collect();
    ir.functions
        .values()
        .map(|ir_f| {
            let default_sig = LpsFnSig {
                name: ir_f.name.clone(),
                return_type: lps_shared::LpsType::Void,
                parameters: Vec::new(),
                kind: lps_shared::LpsFnKind::UserDefined,
            };
            let sig = sig_map
                .get(ir_f.name.as_str())
                .copied()
                .unwrap_or_else(|| &default_sig);
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
