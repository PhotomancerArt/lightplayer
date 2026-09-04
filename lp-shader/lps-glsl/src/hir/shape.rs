use lps_shared::layout::{round_up, type_alignment, type_size};
use lps_shared::{LayoutRules, LpsType};

use super::scalar::scalar_lane_count;

const RULES: LayoutRules = LayoutRules::Std430;

/// One struct member's placement, borrowed from the type it came from.
///
/// The single-field answer [`struct_field`] gives by walking the members
/// once, without materialising a per-type shape table.
pub(super) struct FieldPlacement<'a> {
    /// Position in the struct's member list.
    pub(super) index: usize,
    pub(super) ty: &'a LpsType,
    pub(super) lane_offset: usize,
    pub(super) lane_count: usize,
    pub(super) byte_offset: usize,
}

/// Resolves one struct member by name.
///
/// Member access asks this per swizzle, per field path segment and per place
/// segment, so it walks the members directly and stops at the match instead
/// of building a per-type shape table, which would clone the whole type first.
/// Offsets use the same std430 stepping, so the answers are identical.
pub(super) fn struct_field<'a>(ty: &'a LpsType, name: &str) -> Option<FieldPlacement<'a>> {
    let LpsType::Struct { members, .. } = ty else {
        return None;
    };
    let mut byte_offset = 0usize;
    let mut lane_offset = 0usize;
    for (index, member) in members.iter().enumerate() {
        let align = type_alignment(&member.ty, RULES);
        byte_offset = round_up(byte_offset, align);
        let lane_count = scalar_lane_count(&member.ty);
        if member_name_matches(member.name.as_deref(), index, name) {
            return Some(FieldPlacement {
                index,
                ty: &member.ty,
                lane_offset,
                lane_count,
                byte_offset,
            });
        }
        byte_offset += type_size(&member.ty, RULES);
        lane_offset += lane_count;
    }
    None
}

/// Whether member `index` answers to `name`.
///
/// Unnamed members are addressed as `_{index}` — the same spelling
/// the shape table used to format — matched here without allocating it.
fn member_name_matches(member_name: Option<&str>, index: usize, name: &str) -> bool {
    match member_name {
        Some(member_name) => member_name == name,
        None => {
            let Some(digits) = name.strip_prefix('_') else {
                return false;
            };
            // `format!("_{index}")` never emits a leading zero (except for
            // `_0` itself), so neither does a match.
            if digits.is_empty() || (digits.len() > 1 && digits.starts_with('0')) {
                return false;
            }
            digits.parse::<usize>() == Ok(index)
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::string::String;
    use alloc::vec;

    use lps_shared::StructMember;
    use lps_shared::layout::array_stride;

    use super::*;

    #[test]
    fn struct_fields_use_shared_std430_offsets_and_lane_offsets() {
        let ty = LpsType::Struct {
            name: Some(String::from("S")),
            members: vec![
                StructMember {
                    name: Some(String::from("a")),
                    ty: LpsType::Float,
                },
                StructMember {
                    name: Some(String::from("b")),
                    ty: LpsType::Vec2,
                },
                StructMember {
                    name: Some(String::from("c")),
                    ty: LpsType::Float,
                },
            ],
        };
        let field = |name: &str| struct_field(&ty, name).expect("member");
        assert_eq!(field("a").index, 0);
        assert_eq!(field("b").index, 1);
        assert_eq!(field("c").index, 2);
        assert_eq!(field("a").byte_offset, 0);
        assert_eq!(field("b").byte_offset, 8);
        assert_eq!(field("c").byte_offset, 16);
        assert_eq!(field("a").lane_offset, 0);
        assert_eq!(field("b").lane_offset, 1);
        assert_eq!(field("c").lane_offset, 3);
        assert_eq!(field("c").byte_offset + type_size(field("c").ty, RULES), 20);
    }

    #[test]
    fn array_element_uses_shared_stride() {
        let ty = LpsType::Array {
            element: Box::new(LpsType::Vec3),
            len: 3,
        };
        let LpsType::Array { element, .. } = &ty else {
            unreachable!()
        };
        assert_eq!(array_stride(element, RULES), 12);
        assert_eq!(
            type_alignment(&ty, RULES),
            type_alignment(&LpsType::Vec3, RULES)
        );
    }
}
