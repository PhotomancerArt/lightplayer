//! [`uniform_leaves`]: the declared-uniform enumeration for the params diff.
//!
//! Walks [`LpsModuleSig::uniforms_type`] the same way the render path's
//! `apply_uniform_fields` does (`lp-shader/src/px_shader.rs`): struct
//! members recurse with dotted paths, `Texture2D` members are excluded
//! (textures bind through [`lps_shared::TextureBindingSpec`]s, not param
//! records), and everything else is a leaf. `outputSize` is the engine's
//! synthesized/reserved uniform — it stays in the list (the shader really
//! declares it) but is flagged so drift diffs can skip it.

use alloc::string::String;
use alloc::vec::Vec;

use lps_shared::{LpsModuleSig, LpsType, StructMember, glsl_type_name};

/// The engine-managed uniform name: written by the host every frame, never
/// backed by a def param record.
pub const RESERVED_UNIFORM: &str = "outputSize";

/// One non-texture uniform leaf of a compiled shader signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UniformLeaf {
    /// Dotted path from the uniforms root (`time`, `cfg.speed`).
    pub path: String,
    /// GLSL-style type spelling ([`glsl_type_name`]).
    pub glsl_type: String,
    /// True for the synthesized/reserved `outputSize` uniform — present in
    /// the signature, but excluded from def-record drift diffs.
    pub reserved: bool,
}

/// Enumerate the non-texture uniform leaves of `sig`, in declaration order.
pub fn uniform_leaves(sig: &LpsModuleSig) -> Vec<UniformLeaf> {
    let mut leaves = Vec::new();
    if let Some(LpsType::Struct { members, .. }) = &sig.uniforms_type {
        walk_members(members, "", &mut leaves);
    }
    leaves
}

/// The TOP-LEVEL uniform names of `sig`, in declaration order — struct and
/// texture uniforms included (each is one name, however many leaves it
/// carries). This is the declared side of the def-record drift diff: def
/// `consumed` records are keyed by top-level uniform name, not leaf path.
pub fn top_level_uniform_names(sig: &LpsModuleSig) -> Vec<String> {
    let Some(LpsType::Struct { members, .. }) = &sig.uniforms_type else {
        return Vec::new();
    };
    members
        .iter()
        .filter_map(|member| member.name.clone())
        .collect()
}

fn walk_members(members: &[StructMember], prefix: &str, out: &mut Vec<UniformLeaf>) {
    for member in members {
        let Some(name) = member.name.as_deref() else {
            continue;
        };
        let path = if prefix.is_empty() {
            String::from(name)
        } else {
            alloc::format!("{prefix}.{name}")
        };
        match &member.ty {
            LpsType::Struct {
                members: sub_members,
                ..
            } => walk_members(sub_members, &path, out),
            LpsType::Texture2D => {}
            ty => out.push(UniformLeaf {
                reserved: prefix.is_empty() && name == RESERVED_UNIFORM,
                glsl_type: glsl_type_name(ty),
                path,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;
    use alloc::vec;

    use super::*;

    fn member(name: &str, ty: LpsType) -> StructMember {
        StructMember {
            name: Some(name.to_string()),
            ty,
        }
    }

    fn sig_with(members: Vec<StructMember>) -> LpsModuleSig {
        LpsModuleSig {
            uniforms_type: Some(LpsType::Struct {
                name: Some("__uniforms".to_string()),
                members,
            }),
            ..LpsModuleSig::default()
        }
    }

    #[test]
    fn no_uniforms_yields_no_leaves() {
        assert!(uniform_leaves(&LpsModuleSig::default()).is_empty());
    }

    #[test]
    fn nested_structs_flatten_to_dotted_paths_preserving_instance_names() {
        let sig = sig_with(vec![
            member("time", LpsType::Float),
            member(
                "cfg",
                LpsType::Struct {
                    name: Some("Config".to_string()),
                    members: vec![
                        member("speed", LpsType::Float),
                        member(
                            "tint",
                            LpsType::Struct {
                                name: None,
                                members: vec![member("color", LpsType::Vec3)],
                            },
                        ),
                    ],
                },
            ),
        ]);
        let leaves = uniform_leaves(&sig);
        assert_eq!(
            leaves
                .iter()
                .map(|leaf| (leaf.path.as_str(), leaf.glsl_type.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("time", "float"),
                // The INSTANCE name (`cfg`), not the struct type name.
                ("cfg.speed", "float"),
                ("cfg.tint.color", "vec3"),
            ]
        );
        assert!(leaves.iter().all(|leaf| !leaf.reserved));
    }

    #[test]
    fn texture_members_are_excluded() {
        let sig = sig_with(vec![
            member("tex", LpsType::Texture2D),
            member("mix_amount", LpsType::Float),
        ]);
        let leaves = uniform_leaves(&sig);
        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].path, "mix_amount");
    }

    #[test]
    fn top_level_names_keep_struct_and_texture_uniforms_whole() {
        let sig = sig_with(vec![
            member("time", LpsType::Float),
            member(
                "cfg",
                LpsType::Struct {
                    name: None,
                    members: vec![member("speed", LpsType::Float)],
                },
            ),
            member("tex", LpsType::Texture2D),
        ]);
        assert_eq!(top_level_uniform_names(&sig), vec!["time", "cfg", "tex"]);
    }

    #[test]
    fn output_size_is_present_but_flagged_reserved() {
        let sig = sig_with(vec![
            member("outputSize", LpsType::Vec2),
            member(
                "cfg",
                LpsType::Struct {
                    name: None,
                    // A NESTED `outputSize` is an ordinary member.
                    members: vec![member("outputSize", LpsType::Vec2)],
                },
            ),
        ]);
        let leaves = uniform_leaves(&sig);
        assert_eq!(leaves.len(), 2);
        assert!(leaves[0].reserved, "top-level outputSize is reserved");
        assert_eq!(leaves[0].glsl_type, "vec2");
        assert!(!leaves[1].reserved, "nested outputSize is not");
    }
}
