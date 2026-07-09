//! Compile LPIR (+ module metadata) to WASM.

use alloc::{format, string::String, vec::Vec};

use lpir::LpirModule;
use lps_shared::{LpsModuleSig, LpsType};

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
    let export_names = export_fn_names(meta);
    let (wasm_bytes, shadow_stack_base, env_memory) =
        emit::emit_module(ir, &export_names, options).map_err(WasmError::emit)?;
    let inst_count = count_wasm_insts(&wasm_bytes)?;
    let exports = collect_exports(ir, meta, export_names, options);
    Ok(WasmArtifact {
        module: WasmModule {
            bytes: wasm_bytes,
            inst_count,
            exports,
            shadow_stack_base,
            env_memory,
        },
        signatures: meta.clone(),
    })
}

fn count_wasm_insts(wasm_bytes: &[u8]) -> Result<usize, WasmError> {
    let mut inst_count = 0usize;
    for payload in wasmparser::Parser::new(0).parse_all(wasm_bytes) {
        let payload = payload.map_err(|err| WasmError::emit(format!("{err}")))?;
        if let wasmparser::Payload::CodeSectionEntry(body) = payload {
            let operators = body
                .get_operators_reader()
                .map_err(|err| WasmError::emit(format!("{err}")))?;
            for op in operators {
                op.map_err(|err| WasmError::emit(format!("{err}")))?;
                inst_count = inst_count.saturating_add(1);
            }
        }
    }
    Ok(inst_count)
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

fn collect_exports(
    ir: &LpirModule,
    meta: &LpsModuleSig,
    export_names: Vec<String>,
    options: &WasmOptions,
) -> Vec<WasmExport> {
    ir.functions
        .values()
        .zip(meta.functions.iter())
        .zip(export_names)
        .map(|((ir_f, sig), name)| {
            let (params, results) = wasm_function_signature(ir_f, options.float_mode);
            let uses_sret = ir_f.sret_arg.is_some();
            WasmExport {
                name,
                params,
                results,
                return_type: sig.return_type.clone(),
                param_types: sig.parameters.iter().map(|p| p.ty.clone()).collect(),
                uses_sret,
            }
        })
        .collect()
}

/// One WASM export name per module function, in `meta.functions` order
/// (which [`validate_metadata`] guarantees matches `ir.functions`).
///
/// GLSL allows overloaded local functions, but WASM export names must be
/// unique or the module fails validation. Functions whose name is unique in
/// the module keep it verbatim; overloads get a GLSL-style signature suffix,
/// e.g. `pick` → `pick(vec3)`.
fn export_fn_names(meta: &LpsModuleSig) -> Vec<String> {
    let fns = &meta.functions;
    let mut names: Vec<String> = Vec::with_capacity(fns.len());
    for (i, f) in fns.iter().enumerate() {
        let overloaded = fns
            .iter()
            .enumerate()
            .any(|(j, other)| j != i && other.name == f.name);
        if !overloaded {
            names.push(f.name.clone());
            continue;
        }
        let mut name = f.name.clone();
        name.push('(');
        for (k, p) in f.parameters.iter().enumerate() {
            if k > 0 {
                name.push(',');
            }
            push_type_suffix(&mut name, &p.ty);
        }
        name.push(')');
        names.push(name);
    }
    // Backstop: the frontend rejects true redefinitions, but anonymous-struct
    // params (etc.) could still print identically — force uniqueness so the
    // module always validates.
    for i in 0..names.len() {
        if names[..i].contains(&names[i]) {
            names[i] = format!("{}#{i}", names[i]);
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use lps_shared::{FnParam, LpsFnSig, ParamQualifier};

    fn sig(name: &str, params: &[LpsType]) -> LpsFnSig {
        LpsFnSig {
            name: name.into(),
            return_type: LpsType::Float,
            parameters: params
                .iter()
                .map(|ty| FnParam {
                    name: "p".into(),
                    ty: ty.clone(),
                    qualifier: ParamQualifier::In,
                })
                .collect(),
            kind: Default::default(),
        }
    }

    fn names_for(functions: Vec<LpsFnSig>) -> Vec<String> {
        export_fn_names(&LpsModuleSig {
            functions,
            ..Default::default()
        })
    }

    #[test]
    fn unique_names_stay_verbatim() {
        let names = names_for(alloc::vec![
            sig("alpha", &[LpsType::Float]),
            sig("beta", &[LpsType::Float]),
        ]);
        assert_eq!(names, ["alpha", "beta"]);
    }

    #[test]
    fn overloads_get_signature_suffix() {
        let names = names_for(alloc::vec![
            sig("pick", &[LpsType::Float]),
            sig("pick", &[LpsType::Vec3]),
            sig("pick", &[LpsType::Float, LpsType::Int]),
            sig("solo", &[LpsType::Float]),
            sig(
                "arr",
                &[LpsType::Array {
                    element: alloc::boxed::Box::new(LpsType::Float),
                    len: 3,
                }],
            ),
            sig("arr", &[]),
        ]);
        assert_eq!(
            names,
            [
                "pick(float)",
                "pick(vec3)",
                "pick(float,int)",
                "solo",
                "arr(float[3])",
                "arr()",
            ]
        );
    }

    #[test]
    fn identical_printed_signatures_are_forced_unique() {
        let anon = || LpsType::Struct {
            name: None,
            members: alloc::vec::Vec::new(),
        };
        let names = names_for(alloc::vec![sig("f", &[anon()]), sig("f", &[anon()])]);
        assert_eq!(names, ["f(struct)", "f(struct)#1"]);
    }
}

/// GLSL-style spelling of `ty` for overload-disambiguated export names.
fn push_type_suffix(out: &mut String, ty: &LpsType) {
    let s = match ty {
        LpsType::Void => "void",
        LpsType::Float => "float",
        LpsType::Int => "int",
        LpsType::UInt => "uint",
        LpsType::Bool => "bool",
        LpsType::Vec2 => "vec2",
        LpsType::Vec3 => "vec3",
        LpsType::Vec4 => "vec4",
        LpsType::IVec2 => "ivec2",
        LpsType::IVec3 => "ivec3",
        LpsType::IVec4 => "ivec4",
        LpsType::UVec2 => "uvec2",
        LpsType::UVec3 => "uvec3",
        LpsType::UVec4 => "uvec4",
        LpsType::BVec2 => "bvec2",
        LpsType::BVec3 => "bvec3",
        LpsType::BVec4 => "bvec4",
        LpsType::Mat2 => "mat2",
        LpsType::Mat3 => "mat3",
        LpsType::Mat4 => "mat4",
        LpsType::Texture2D => "sampler2D",
        LpsType::Array { element, len } => {
            push_type_suffix(out, element);
            out.push_str(&format!("[{len}]"));
            return;
        }
        LpsType::Struct { name, .. } => name.as_deref().unwrap_or("struct"),
    };
    out.push_str(s);
}
