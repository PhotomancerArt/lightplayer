//! GLSL → LPIR → [`lpvm::LpvmModule`], plus manifest input ↔ uniform validation.

use alloc::format;
use alloc::string::String;

use lpir::LpirModule;
use lps_shared::LpsModuleSig;
use lps_shared::path_resolve::LpsTypePathExt;
use lpvm::LpvmEngine;

use lpfx::FxManifest;

/// Fully compiled effect: runnable module, metadata, and IR snapshot.
pub struct CompiledEffect<M> {
    pub module: M,
    pub meta: LpsModuleSig,
    pub _ir: LpirModule,
}

/// Parse and lower GLSL, then compile with the given LPVM engine.
pub fn compile_glsl<E: LpvmEngine>(
    engine: &E,
    glsl: &str,
) -> Result<CompiledEffect<E::Module>, String> {
    let naga = lps_frontend::compile(glsl).map_err(|e| format!("GLSL parse: {e}"))?;
    let (ir, meta) = lps_frontend::lower(&naga).map_err(|e| format!("LPIR lower: {e}"))?;
    drop(naga);
    let module = engine
        .compile(&ir, &meta)
        .map_err(|e| format!("compile: {e}"))?;
    Ok(CompiledEffect {
        module,
        meta,
        _ir: ir,
    })
}

/// Ensures each `[input.X]` has a corresponding uniform field `input_X` in the shader metadata.
pub fn validate_inputs(manifest: &FxManifest, meta: &LpsModuleSig) -> Result<(), String> {
    let uniforms = meta.uniforms_type.as_ref();
    for (name, _def) in &manifest.inputs {
        let uniform_name = format!("input_{name}");
        if let Some(ut) = uniforms {
            if ut.type_at_path(&uniform_name).is_err() {
                return Err(format!(
                    "manifest input `{name}` has no matching uniform `{uniform_name}` in shader"
                ));
            }
        } else {
            return Err(format!(
                "shader has no uniforms but manifest declares input `{name}`"
            ));
        }
    }
    Ok(())
}
