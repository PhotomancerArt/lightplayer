//! Validate `CompilePxDesc` texture binding specs against shader metadata.

use lps_shared::{LpsModuleSig, validate_texture_binding_specs_against_module};

use crate::compile_px_desc::TextureBindingSpecs;
use crate::error::LpsError;

/// Check that every declared `Texture2D` uniform has a spec and that every spec
/// names a declared sampler. Runs after lower, before `render` validation.
pub(crate) fn validate_texture_interface(
    meta: &LpsModuleSig,
    textures: &TextureBindingSpecs,
) -> Result<(), LpsError> {
    validate_texture_binding_specs_against_module(meta, textures).map_err(LpsError::Validation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lps_shared::{LpsType, StructMember};

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
            ..Default::default()
        };
        let specs = TextureBindingSpecs::new();
        let err = validate_texture_interface(&meta, &specs).expect_err("expected name error");
        match err {
            LpsError::Validation(s) => assert!(s.contains("no name"), "{s}"),
            e => panic!("wrong err: {e:?}"),
        }
    }
}
