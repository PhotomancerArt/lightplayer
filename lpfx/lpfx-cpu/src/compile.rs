//! Manifest input ↔ shader uniform validation.

use alloc::format;
use alloc::string::String;

use lps_shared::LpsModuleSig;
use lps_shared::path_resolve::LpsTypePathExt;

use lpfx::FxManifest;

/// Ensures each `[input.X]` has a corresponding uniform field `input_X` in the shader metadata.
pub(crate) fn validate_inputs(manifest: &FxManifest, meta: &LpsModuleSig) -> Result<(), String> {
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
