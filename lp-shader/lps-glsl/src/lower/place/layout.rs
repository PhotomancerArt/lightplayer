use crate::hir::{HirExpr, HirExprKind};

const RULES: lps_shared::LayoutRules = lps_shared::LayoutRules::Std430;

pub(super) fn constant_index(expr: &HirExpr) -> Option<usize> {
    match expr.kind {
        HirExprKind::IntLiteral(value) => usize::try_from(value).ok(),
        HirExprKind::UIntLiteral(value) => usize::try_from(value).ok(),
        _ => None,
    }
}

pub(super) fn scalar_lane_offsets(ty: &lps_shared::LpsType) -> alloc::vec::Vec<u32> {
    use alloc::vec::Vec;
    use lps_shared::LpsType;
    use lps_shared::layout::{array_stride, round_up, type_alignment, type_size};

    match ty {
        LpsType::Void => Vec::new(),
        LpsType::Float
        | LpsType::Int
        | LpsType::UInt
        | LpsType::Bool
        | LpsType::Texture2D
        | LpsType::Vec2
        | LpsType::Vec3
        | LpsType::Vec4
        | LpsType::IVec2
        | LpsType::IVec3
        | LpsType::IVec4
        | LpsType::UVec2
        | LpsType::UVec3
        | LpsType::UVec4
        | LpsType::BVec2
        | LpsType::BVec3
        | LpsType::BVec4
        | LpsType::Mat2
        | LpsType::Mat3
        | LpsType::Mat4 => (0..crate::hir::scalar_lane_count(ty))
            .map(|lane| (lane as u32).saturating_mul(4))
            .collect(),
        LpsType::Array { element, len } => {
            let element_offsets = scalar_lane_offsets(element);
            let stride = array_stride(element, RULES) as u32;
            let mut offsets = Vec::new();
            for index in 0..*len {
                let base = index.saturating_mul(stride);
                offsets.extend(
                    element_offsets
                        .iter()
                        .map(|offset| base.saturating_add(*offset)),
                );
            }
            offsets
        }
        LpsType::Struct { members, .. } => {
            let mut offsets = Vec::new();
            let mut byte_offset = 0usize;
            for member in members {
                byte_offset = round_up(byte_offset, type_alignment(&member.ty, RULES));
                offsets.extend(
                    scalar_lane_offsets(&member.ty)
                        .into_iter()
                        .map(|offset| (byte_offset as u32).saturating_add(offset)),
                );
                byte_offset = byte_offset.saturating_add(type_size(&member.ty, RULES));
            }
            offsets
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::string::String;
    use alloc::vec;
    use lps_shared::LpsType;
    use lps_shared::StructMember;

    use super::*;
    use crate::Span;

    #[test]
    fn constant_index_rejects_negative_values() {
        let expr = HirExpr {
            span: Span::new(0, 1),
            ty: LpsType::Int,
            kind: HirExprKind::IntLiteral(-1),
        };

        assert_eq!(constant_index(&expr), None);
    }

    #[test]
    fn constant_index_accepts_uint_values() {
        let expr = HirExpr {
            span: Span::new(0, 1),
            ty: LpsType::UInt,
            kind: HirExprKind::UIntLiteral(3),
        };

        assert_eq!(constant_index(&expr), Some(3));
    }

    #[test]
    fn scalar_lane_offsets_preserve_struct_padding() {
        let ty = LpsType::Struct {
            name: Some(String::from("Emitter")),
            members: vec![
                StructMember {
                    name: Some(String::from("id")),
                    ty: LpsType::UInt,
                },
                StructMember {
                    name: Some(String::from("pos")),
                    ty: LpsType::Vec2,
                },
                StructMember {
                    name: Some(String::from("radius")),
                    ty: LpsType::Float,
                },
            ],
        };

        assert_eq!(scalar_lane_offsets(&ty), vec![0, 8, 12, 16]);
    }

    #[test]
    fn scalar_lane_offsets_include_array_stride() {
        let ty = LpsType::Array {
            element: Box::new(LpsType::Vec2),
            len: 2,
        };

        assert_eq!(scalar_lane_offsets(&ty), vec![0, 4, 8, 12]);
    }
}
