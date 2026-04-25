//! GLSL `sampler2D` / `texture2D` → [`LpsType::Texture2D`] in [`LpsModuleSig::uniforms_type`].
//!
//! Naga’s GLSL-IN does not treat bare `sampler2D` as a type name; [`crate::parse`]
//! rewrites top-level `uniform sampler2D name;` to `layout(set=0, binding=n) uniform texture2D` (or
//! only the type if `layout(…)` is already present). See `parse.rs` for limitations.
//!
//! Naga then emits a sampled 2D `Image` in `AddressSpace::Handle` for the separate-texture form.

use alloc::string::String;
use alloc::vec;

use lps_shared::LayoutRules;
use lps_shared::layout::{type_alignment, type_size};

use crate::CompileError;
use crate::LpsModuleSig;
use crate::LpsType;
use crate::compile;
use crate::lower;
use crate::naga_types::naga_type_handle_to_lps;

/// Milestone public surface: `uniform sampler2D` (rewritten before Naga; see `parse.rs`).
#[test]
fn uniform_sampler2d_includes_texture2d_in_uniforms_type() {
    let glsl = r#"
uniform sampler2D inputColor;
vec4 render(vec2 pos) { return vec4(pos, 0.0, 1.0); }
"#;
    let naga_module = compile(glsl).expect("parse");
    let (_lpir, sig) = lower(&naga_module).expect("lower");
    let u = sig.uniforms_type.expect("uniforms_type");
    let LpsType::Struct { name: _, members } = u else {
        panic!("expected struct, got {u:?}");
    };
    let input = members
        .iter()
        .find(|m| m.name.as_deref() == Some("inputColor"))
        .expect("inputColor member");
    assert_eq!(input.ty, LpsType::Texture2D);
}

/// Explicit Naga-recognized `texture2D` + layout (no `sampler2D` token).
#[test]
fn layout_uniform_texture2d_includes_in_uniforms_type() {
    let glsl = r#"
layout(set = 0, binding = 0) uniform texture2D albedo;
vec4 render(vec2 pos) { return vec4(pos, 0.0, 1.0); }
"#;
    let naga_module = compile(glsl).expect("parse");
    let (_lpir, sig) = lower(&naga_module).expect("lower");
    let u = sig.uniforms_type.expect("uniforms_type");
    let LpsType::Struct { members, .. } = u else {
        panic!("expected struct");
    };
    let f = members
        .iter()
        .find(|m| m.name.as_deref() == Some("albedo"))
        .expect("albedo");
    assert_eq!(f.ty, LpsType::Texture2D);
}

#[test]
fn texture2d_plus_scalar_uniforms_stable_metadata() {
    let glsl = r#"
layout(set = 0, binding = 0) uniform float scale;
layout(set = 0, binding = 1) uniform sampler2D inputColor;
vec4 render(vec2 pos) { return vec4(pos * scale, 0.0, 1.0); }
"#;
    let naga_module = compile(glsl).expect("parse");
    let (_lpir, sig) = lower(&naga_module).expect("lower");
    let u = sig.uniforms_type.expect("uniforms_type");
    let LpsType::Struct { members, .. } = u else {
        panic!("expected struct");
    };
    assert_eq!(members.len(), 2);
    assert_eq!(members[0].name, Some(String::from("scale")));
    assert_eq!(members[0].ty, LpsType::Float);
    assert_eq!(members[1].name, Some(String::from("inputColor")));
    assert_eq!(members[1].ty, LpsType::Texture2D);
}

#[test]
fn texture3d_uniform_rejects_with_clear_error() {
    let glsl = r#"
layout(set = 0, binding = 0) uniform texture3D t;
vec4 render(vec2 pos) { return vec4(0.0); }
"#;
    let naga_module = compile(glsl).expect("parse");
    let e = lower(&naga_module).expect_err("expected unsupported 3D texture");
    let s = alloc::format!("{e}");
    assert!(
        s.contains("3D") || s.contains("texture"),
        "unexpected error: {s}"
    );
}

fn module_sig_for_glsl(glsl: &str) -> LpsModuleSig {
    let naga_module = compile(glsl).expect("compile");
    lower(&naga_module).expect("lower").1
}

/// Texture + scalar: walk members with std430 size/alignment; matches [`lps_shared::layout`].
#[test]
fn texture2d_std430_size_align_matches_layout_helper() {
    let glsl = r#"
layout(set = 0, binding = 0) uniform float a;
layout(set = 0, binding = 1) uniform sampler2D t;
vec4 render(vec2 p) { return vec4(a) + vec4(0.0); }
"#;
    let sig = module_sig_for_glsl(glsl);
    let u = sig.uniforms_type.expect("uniforms");
    let LpsType::Struct { members, .. } = u else {
        panic!("expected struct");
    };
    let mut off = 0u32;
    for m in &members {
        let al = type_alignment(&m.ty, LayoutRules::Std430) as u32;
        off = ((off + al - 1) / al) * al;
        off += type_size(&m.ty, LayoutRules::Std430) as u32;
    }
    assert_eq!(off, 4 + 16, "float + texture2D std430 back-to-back");
}

/// GLSL-IN `sampler2D` constructor form: two-member struct (image + sampler) → [`LpsType::Texture2D`].
#[test]
fn naga_type_maps_combined_sampler2d_struct_to_texture2d() {
    use naga::{ImageClass, ImageDimension, Module, ScalarKind, Type, TypeInner};

    let mut module = Module::default();
    let image_ty = module.types.insert(
        Type {
            name: None,
            inner: TypeInner::Image {
                dim: ImageDimension::D2,
                arrayed: false,
                class: ImageClass::Sampled {
                    kind: ScalarKind::Float,
                    multi: false,
                },
            },
        },
        naga::Span::UNDEFINED,
    );
    let sampler_ty = module.types.insert(
        Type {
            name: None,
            inner: TypeInner::Sampler { comparison: false },
        },
        naga::Span::UNDEFINED,
    );
    let struct_ty = module.types.insert(
        Type {
            name: None,
            inner: TypeInner::Struct {
                members: vec![
                    naga::StructMember {
                        name: None,
                        ty: image_ty,
                        binding: None,
                        offset: 0,
                    },
                    naga::StructMember {
                        name: None,
                        ty: sampler_ty,
                        binding: None,
                        offset: 0,
                    },
                ],
                span: 0,
            },
        },
        naga::Span::UNDEFINED,
    );
    let t = naga_type_handle_to_lps(&module, struct_ty).expect("map combined sampler struct");
    assert_eq!(t, LpsType::Texture2D);
}

/// Error substrings (for later compile validation): 3D sampled image.
#[test]
fn naga_image_3d_unsupported_type_message() {
    use naga::{ImageClass, ImageDimension, Module, ScalarKind, Type, TypeInner};

    let mut m = Module::default();
    let t3d = m.types.insert(
        Type {
            name: None,
            inner: TypeInner::Image {
                dim: ImageDimension::D3,
                arrayed: false,
                class: ImageClass::Sampled {
                    kind: ScalarKind::Float,
                    multi: false,
                },
            },
        },
        naga::Span::UNDEFINED,
    );
    let err = naga_type_handle_to_lps(&m, t3d).unwrap_err();
    let CompileError::UnsupportedType(msg) = err else {
        panic!("{err:?}");
    };
    assert!(msg.contains("3D"), "{msg}");
}
