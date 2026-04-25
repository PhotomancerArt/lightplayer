//! Validate `CompilePxDesc` texture binding specs against shader metadata.

use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::String;

use lps_shared::{LpsModuleSig, LpsType};

use crate::compile_px_desc::TextureBindingSpecs;
use crate::error::LpsError;

/// Check that every declared `Texture2D` uniform has a spec and that every spec
/// names a declared sampler. Runs after lower, before `render` validation.
pub(crate) fn validate_texture_interface(
    meta: &LpsModuleSig,
    textures: &TextureBindingSpecs,
) -> Result<(), LpsError> {
    let declared = declared_texture2d_names(meta)?;
    for name in &declared {
        if !textures.contains_key(name) {
            return Err(LpsError::Validation(format!(
                "no texture binding spec for shader sampler `{name}`"
            )));
        }
    }
    for (spec_name, _) in textures {
        if !declared.contains(spec_name) {
            return Err(LpsError::Validation(format!(
                "texture binding spec `{spec_name}` does not match any shader sampler2D uniform"
            )));
        }
    }
    Ok(())
}

/// Texture2D members of `uniforms_type`, in deterministic sorted order.
fn declared_texture2d_names(meta: &LpsModuleSig) -> Result<BTreeSet<String>, LpsError> {
    let Some(u) = meta.uniforms_type.as_ref() else {
        return Ok(BTreeSet::new());
    };
    let LpsType::Struct { members, .. } = u else {
        return Err(LpsError::Validation(String::from(
            "uniforms metadata is not a struct (cannot validate texture bindings)",
        )));
    };
    let mut out = BTreeSet::new();
    for m in members {
        if m.ty != LpsType::Texture2D {
            continue;
        }
        let Some(name) = m.name.as_ref() else {
            return Err(LpsError::Validation(String::from(
                "texture uniform has no name",
            )));
        };
        if name.is_empty() {
            return Err(LpsError::Validation(String::from(
                "texture uniform has no name",
            )));
        }
        out.insert(name.clone());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lps_shared::StructMember;

    #[test]
    fn unnamed_texture2d_member_errors() {
        let meta = LpsModuleSig {
            functions: vec![],
            uniforms_type: Some(LpsType::Struct {
                name: None,
                members: vec![StructMember {
                    name: None,
                    ty: LpsType::Texture2D,
                }],
            }),
            globals_type: None,
        };
        let specs = TextureBindingSpecs::new();
        let err = validate_texture_interface(&meta, &specs).expect_err("expected name error");
        match err {
            LpsError::Validation(s) => assert!(s.contains("no name"), "{s}"),
            e => panic!("wrong err: {e:?}"),
        }
    }
}
