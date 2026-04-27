//! Validate `TextureBindingSpec` maps against lowered module metadata (`Texture2D` uniforms).

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::String;

use crate::{LpsModuleSig, LpsType, TextureBindingSpec};

/// Every [`LpsType::Texture2D`] uniform in [`LpsModuleSig::uniforms_type`] must have a matching
/// spec entry; every spec key must name a declared sampler. Empty specs with no texture uniforms
/// succeeds.
///
/// Returns [`Err`] with a stable message when specs and uniforms disagree or when `uniforms_type`
/// is present but not a struct.
pub fn validate_texture_binding_specs_against_module(
    meta: &LpsModuleSig,
    specs: &BTreeMap<String, TextureBindingSpec>,
) -> Result<(), String> {
    let declared = declared_texture2d_names(meta)?;
    for name in &declared {
        if !specs.contains_key(name) {
            return Err(format!(
                "no texture binding spec for shader sampler '{name}'"
            ));
        }
    }
    for spec_name in specs.keys() {
        if !declared.contains(spec_name) {
            return Err(format!(
                "texture binding spec '{spec_name}' does not match any shader sampler2D uniform"
            ));
        }
    }
    Ok(())
}

fn declared_texture2d_names(meta: &LpsModuleSig) -> Result<BTreeSet<String>, String> {
    let Some(u) = meta.uniforms_type.as_ref() else {
        return Ok(BTreeSet::new());
    };
    let LpsType::Struct { members, .. } = u else {
        return Err(String::from(
            "uniforms metadata is not a struct (cannot validate texture bindings)",
        ));
    };
    let mut out = BTreeSet::new();
    for m in members {
        if m.ty != LpsType::Texture2D {
            continue;
        }
        let Some(name) = m.name.as_ref() else {
            return Err(String::from("texture uniform has no name"));
        };
        if name.is_empty() {
            return Err(String::from("texture uniform has no name"));
        }
        out.insert(name.clone());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StructMember;
    use alloc::vec;

    #[test]
    fn missing_spec_errors_with_sampler_name() {
        let meta = LpsModuleSig {
            functions: vec![],
            uniforms_type: Some(LpsType::Struct {
                name: None,
                members: vec![StructMember {
                    name: Some(String::from("inputColor")),
                    ty: LpsType::Texture2D,
                }],
            }),
            globals_type: None,
            ..Default::default()
        };
        let specs = BTreeMap::new();
        let err = validate_texture_binding_specs_against_module(&meta, &specs).unwrap_err();
        assert!(
            err.contains("inputColor") && err.contains("no texture binding spec"),
            "{err}"
        );
    }

    #[test]
    fn extra_spec_errors_with_spec_name() {
        let meta = LpsModuleSig {
            functions: vec![],
            uniforms_type: Some(LpsType::Struct {
                name: None,
                members: vec![],
            }),
            globals_type: None,
            ..Default::default()
        };
        let mut specs = BTreeMap::new();
        specs.insert(
            String::from("extraTex"),
            TextureBindingSpec {
                format: crate::TextureStorageFormat::Rgba16Unorm,
                filter: crate::TextureFilter::Nearest,
                wrap_x: crate::TextureWrap::ClampToEdge,
                wrap_y: crate::TextureWrap::ClampToEdge,
                shape_hint: crate::TextureShapeHint::General2D,
            },
        );
        let err = validate_texture_binding_specs_against_module(&meta, &specs).unwrap_err();
        assert!(
            err.contains("extraTex") && err.contains("does not match any shader sampler2D"),
            "{err}"
        );
    }

    #[test]
    fn matching_spec_succeeds() {
        let meta = LpsModuleSig {
            functions: vec![],
            uniforms_type: Some(LpsType::Struct {
                name: None,
                members: vec![StructMember {
                    name: Some(String::from("u_tex")),
                    ty: LpsType::Texture2D,
                }],
            }),
            globals_type: None,
            ..Default::default()
        };
        let mut specs = BTreeMap::new();
        specs.insert(
            String::from("u_tex"),
            TextureBindingSpec {
                format: crate::TextureStorageFormat::Rgba16Unorm,
                filter: crate::TextureFilter::Nearest,
                wrap_x: crate::TextureWrap::ClampToEdge,
                wrap_y: crate::TextureWrap::ClampToEdge,
                shape_hint: crate::TextureShapeHint::General2D,
            },
        );
        validate_texture_binding_specs_against_module(&meta, &specs).unwrap();
    }

    #[test]
    fn no_textures_and_no_specs_ok() {
        let meta = LpsModuleSig {
            functions: vec![],
            uniforms_type: None,
            globals_type: None,
            ..Default::default()
        };
        let specs = BTreeMap::new();
        validate_texture_binding_specs_against_module(&meta, &specs).unwrap();
    }

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
        let specs = BTreeMap::new();
        let err = validate_texture_binding_specs_against_module(&meta, &specs).unwrap_err();
        assert!(err.contains("no name"), "{err}");
    }
}
