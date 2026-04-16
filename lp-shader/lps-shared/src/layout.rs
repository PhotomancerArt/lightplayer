//! Memory layout for [`LpsType`] (std430 only for now).
//!
//! Rules match GLSL `std430` for transparent types. See `docs/design/glsl-layout.md`.

use crate::{LayoutRules, LpsType, StructMember};

/// Size of the VMContext header in bytes (16 bytes on 32-bit targets).
/// This is the offset where uniforms start in the VMContext buffer.
pub const VMCTX_HEADER_SIZE: usize = 16;

/// Round `size` up to a multiple of `alignment` (must be > 0).
pub fn round_up(size: usize, alignment: usize) -> usize {
    debug_assert!(alignment > 0);
    ((size + alignment - 1) / alignment) * alignment
}

/// Byte size of `ty` under `rules`.
pub fn type_size(ty: &LpsType, rules: LayoutRules) -> usize {
    match rules {
        LayoutRules::Std430 => std430_size(ty),
        LayoutRules::Std140 => panic!("std140 layout is not implemented yet"),
    }
}

/// Alignment of `ty` under `rules`.
pub fn type_alignment(ty: &LpsType, rules: LayoutRules) -> usize {
    match rules {
        LayoutRules::Std430 => std430_alignment(ty),
        LayoutRules::Std140 => panic!("std140 layout is not implemented yet"),
    }
}

/// Stride between array elements (size rounded up to element alignment).
pub fn array_stride(element: &LpsType, rules: LayoutRules) -> usize {
    let s = type_size(element, rules);
    let a = type_alignment(element, rules);
    round_up(s, a)
}

fn std430_size(ty: &LpsType) -> usize {
    use LpsType::*;
    match ty {
        Void => 0,
        Float | Int | UInt | Bool => 4,
        Vec2 | IVec2 | UVec2 | BVec2 => 8,
        Vec3 | IVec3 | UVec3 | BVec3 => 12,
        Vec4 | IVec4 | UVec4 | BVec4 => 16,
        Mat2 => 2 * std430_size(&Vec2),
        Mat3 => 3 * std430_size(&Vec3),
        Mat4 => 4 * std430_size(&Vec4),
        Array { element, len } => array_stride(element, LayoutRules::Std430) * (*len as usize),
        Struct { members, .. } => struct_data_size(members, LayoutRules::Std430),
    }
}

fn std430_alignment(ty: &LpsType) -> usize {
    use LpsType::*;
    match ty {
        Void => 1,
        Float | Int | UInt | Bool => 4,
        Vec2 | IVec2 | UVec2 | BVec2 => 8,
        Vec3 | IVec3 | UVec3 | BVec3 => 4,
        Vec4 | IVec4 | UVec4 | BVec4 => 16,
        Mat2 => std430_alignment(&Vec2),
        Mat3 => std430_alignment(&Vec3),
        Mat4 => std430_alignment(&Vec4),
        Array { element, .. } => std430_alignment(element),
        Struct { members, .. } => struct_alignment(members, LayoutRules::Std430),
    }
}

fn struct_alignment(members: &[StructMember], rules: LayoutRules) -> usize {
    members
        .iter()
        .map(|m| type_alignment(&m.ty, rules))
        .max()
        .unwrap_or(1)
}

fn struct_data_size(members: &[StructMember], rules: LayoutRules) -> usize {
    let mut offset = 0usize;
    for m in members {
        let a = type_alignment(&m.ty, rules);
        offset = round_up(offset, a);
        offset += type_size(&m.ty, rules);
    }
    let align = struct_alignment(members, rules);
    round_up(offset, align)
}
