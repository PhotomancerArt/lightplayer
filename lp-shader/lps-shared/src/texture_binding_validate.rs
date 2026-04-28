//! Validate `TextureBindingSpec` maps against lowered module metadata (`Texture2D` uniforms).

use crate::path::parse_path;
use crate::path_resolve::LpsTypePathExt;
use crate::{LpsModuleSig, LpsType, StructMember, TextureBindingSpec};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::String;

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
    let declared = declared_texture2d_paths(meta)?;
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

fn declared_texture2d_paths(meta: &LpsModuleSig) -> Result<BTreeSet<String>, String> {
    let Some(u) = meta.uniforms_type.as_ref() else {
        return Ok(BTreeSet::new());
    };
    let LpsType::Struct { members, .. } = u else {
        return Err(String::from(
            "uniforms metadata is not a struct (cannot validate texture bindings)",
        ));
    };
    let mut out = BTreeSet::new();
    collect_texture2d_paths_from_members(u, members, &[], &mut out)?;
    Ok(out)
}

/// Recursively collect canonical dotted paths for every `Texture2D` leaf under a struct member
/// list. `prefix` holds field names from the uniforms root (empty at top level).
fn collect_texture2d_paths_from_members(
    uniforms_root: &LpsType,
    members: &[StructMember],
    prefix: &[String],
    out: &mut BTreeSet<String>,
) -> Result<(), String> {
    for m in members {
        let Some(name) = m.name.as_ref() else {
            return Err(String::from("uniform struct member has no name"));
        };
        if name.is_empty() {
            return Err(String::from("uniform struct member has no name"));
        }
        let mut path = prefix.to_vec();
        path.push(name.clone());
        match &m.ty {
            LpsType::Texture2D => {
                let key = canonical_texture_binding_path(uniforms_root, &path)?;
                out.insert(key);
            }
            LpsType::Struct { members: sub, .. } => {
                collect_texture2d_paths_from_members(uniforms_root, sub, &path, out)?;
            }
            LpsType::Array { element, .. } => {
                if type_contains_texture2d_leaf(element.as_ref()) {
                    let p = dotted_path_join(&path);
                    return Err(format!(
                        "texture bindings in uniform arrays are not supported (near '{p}')"
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// `Texture2D` or any aggregate that contains it (used to reject sampler arrays).
fn type_contains_texture2d_leaf(ty: &LpsType) -> bool {
    match ty {
        LpsType::Texture2D => true,
        LpsType::Struct { members, .. } => {
            members.iter().any(|m| type_contains_texture2d_leaf(&m.ty))
        }
        LpsType::Array { element, .. } => type_contains_texture2d_leaf(element.as_ref()),
        _ => false,
    }
}

fn dotted_path_join(parts: &[String]) -> String {
    parts.join(".")
}

/// Build the public binding key string, [`parse_path`]-check it, and confirm [`LpsTypePathExt::type_at_path`].
fn canonical_texture_binding_path(
    uniforms_root: &LpsType,
    parts: &[String],
) -> Result<String, String> {
    if parts.is_empty() {
        return Err(String::from(
            "internal error: empty texture uniform path segments",
        ));
    }
    let key = dotted_path_join(parts);
    parse_path(&key).map_err(|e| format!("invalid canonical texture binding path `{key}`: {e}"))?;
    match uniforms_root.type_at_path(&key) {
        Ok(LpsType::Texture2D) => Ok(key),
        Ok(leaf) => Err(format!(
            "uniform path `{key}` resolves to `{leaf:?}`, expected Texture2D"
        )),
        Err(e) => Err(format!(
            "uniform path `{key}` is not reachable in uniforms metadata ({e})"
        )),
    }
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
    fn nested_params_gradient_matching_spec_succeeds() {
        let meta = LpsModuleSig {
            functions: vec![],
            uniforms_type: Some(LpsType::Struct {
                name: None,
                members: vec![StructMember {
                    name: Some(String::from("params")),
                    ty: LpsType::Struct {
                        name: Some(String::from("Params")),
                        members: vec![
                            StructMember {
                                name: Some(String::from("amount")),
                                ty: LpsType::Float,
                            },
                            StructMember {
                                name: Some(String::from("gradient")),
                                ty: LpsType::Texture2D,
                            },
                        ],
                    },
                }],
            }),
            globals_type: None,
            ..Default::default()
        };
        let mut specs = BTreeMap::new();
        specs.insert(
            String::from("params.gradient"),
            TextureBindingSpec {
                format: crate::TextureStorageFormat::Rgba16Unorm,
                filter: crate::TextureFilter::Nearest,
                wrap_x: crate::TextureWrap::ClampToEdge,
                wrap_y: crate::TextureWrap::ClampToEdge,
                shape_hint: crate::TextureShapeHint::HeightOne,
            },
        );
        validate_texture_binding_specs_against_module(&meta, &specs).unwrap();
    }

    #[test]
    fn missing_nested_spec_errors_with_dotted_key() {
        let meta = LpsModuleSig {
            functions: vec![],
            uniforms_type: Some(LpsType::Struct {
                name: None,
                members: vec![StructMember {
                    name: Some(String::from("params")),
                    ty: LpsType::Struct {
                        name: None,
                        members: vec![StructMember {
                            name: Some(String::from("gradient")),
                            ty: LpsType::Texture2D,
                        }],
                    },
                }],
            }),
            globals_type: None,
            ..Default::default()
        };
        let specs = BTreeMap::new();
        let err = validate_texture_binding_specs_against_module(&meta, &specs).unwrap_err();
        assert!(
            err.contains("params.gradient") && err.contains("no texture binding spec"),
            "{err}"
        );
    }

    #[test]
    fn extra_nested_spec_errors_with_extra_key() {
        let meta = LpsModuleSig {
            functions: vec![],
            uniforms_type: Some(LpsType::Struct {
                name: None,
                members: vec![StructMember {
                    name: Some(String::from("params")),
                    ty: LpsType::Struct {
                        name: None,
                        members: vec![StructMember {
                            name: Some(String::from("gradient")),
                            ty: LpsType::Texture2D,
                        }],
                    },
                }],
            }),
            globals_type: None,
            ..Default::default()
        };
        let mut specs = BTreeMap::new();
        specs.insert(
            String::from("params.gradient"),
            TextureBindingSpec {
                format: crate::TextureStorageFormat::Rgba16Unorm,
                filter: crate::TextureFilter::Nearest,
                wrap_x: crate::TextureWrap::ClampToEdge,
                wrap_y: crate::TextureWrap::ClampToEdge,
                shape_hint: crate::TextureShapeHint::General2D,
            },
        );
        specs.insert(
            String::from("params.other"),
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
            err.contains("params.other") && err.contains("does not match any shader sampler2D"),
            "{err}"
        );
    }

    #[test]
    fn top_level_texture_plus_nested_matching_spec_succeeds() {
        let meta = LpsModuleSig {
            functions: vec![],
            uniforms_type: Some(LpsType::Struct {
                name: None,
                members: vec![
                    StructMember {
                        name: Some(String::from("inputColor")),
                        ty: LpsType::Texture2D,
                    },
                    StructMember {
                        name: Some(String::from("params")),
                        ty: LpsType::Struct {
                            name: None,
                            members: vec![StructMember {
                                name: Some(String::from("gradient")),
                                ty: LpsType::Texture2D,
                            }],
                        },
                    },
                ],
            }),
            globals_type: None,
            ..Default::default()
        };
        let mut specs = BTreeMap::new();
        for path in ["inputColor", "params.gradient"] {
            specs.insert(
                String::from(path),
                TextureBindingSpec {
                    format: crate::TextureStorageFormat::Rgba16Unorm,
                    filter: crate::TextureFilter::Nearest,
                    wrap_x: crate::TextureWrap::ClampToEdge,
                    wrap_y: crate::TextureWrap::ClampToEdge,
                    shape_hint: crate::TextureShapeHint::General2D,
                },
            );
        }
        validate_texture_binding_specs_against_module(&meta, &specs).unwrap();
    }

    #[test]
    fn nested_struct_only_scalar_fields_requires_no_texture_specs() {
        let meta = LpsModuleSig {
            functions: vec![],
            uniforms_type: Some(LpsType::Struct {
                name: None,
                members: vec![StructMember {
                    name: Some(String::from("params")),
                    ty: LpsType::Struct {
                        name: None,
                        members: vec![
                            StructMember {
                                name: Some(String::from("amount")),
                                ty: LpsType::Float,
                            },
                            StructMember {
                                name: Some(String::from("bias")),
                                ty: LpsType::Vec2,
                            },
                        ],
                    },
                }],
            }),
            globals_type: None,
            ..Default::default()
        };
        validate_texture_binding_specs_against_module(&meta, &BTreeMap::new()).unwrap();
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

    #[test]
    fn texture_uniform_array_errors() {
        let meta = LpsModuleSig {
            functions: vec![],
            uniforms_type: Some(LpsType::Struct {
                name: None,
                members: vec![StructMember {
                    name: Some(String::from("gradients")),
                    ty: LpsType::Array {
                        element: alloc::boxed::Box::new(LpsType::Texture2D),
                        len: 2,
                    },
                }],
            }),
            globals_type: None,
            ..Default::default()
        };
        let mut specs = BTreeMap::new();
        specs.insert(
            String::from("ignored"),
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
            err.contains("uniform arrays")
                && err.contains("not supported")
                && err.contains("gradients"),
            "{err}"
        );
    }

    #[test]
    fn texture_inside_struct_array_errors() {
        let meta = LpsModuleSig {
            functions: vec![],
            uniforms_type: Some(LpsType::Struct {
                name: None,
                members: vec![StructMember {
                    name: Some(String::from("layers")),
                    ty: LpsType::Array {
                        element: alloc::boxed::Box::new(LpsType::Struct {
                            name: None,
                            members: vec![StructMember {
                                name: Some(String::from("tex")),
                                ty: LpsType::Texture2D,
                            }],
                        }),
                        len: 2,
                    },
                }],
            }),
            globals_type: None,
            ..Default::default()
        };
        let specs = BTreeMap::new();
        let err = validate_texture_binding_specs_against_module(&meta, &specs).unwrap_err();
        assert!(
            err.contains("uniform arrays") && err.contains("layers"),
            "{err}"
        );
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
}
