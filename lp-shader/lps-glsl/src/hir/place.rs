use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use lps_shared::LpsType;

use crate::{Diagnostic, Span};

use super::scalar::{scalar_base_type, scalar_lane_count};
use super::shape::TypeShape;
use super::types::HirExpr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "call-actual places land when aggregate out/inout lowering moves here"
)]
pub(crate) enum AccessMode {
    Read,
    Write,
    CallActual,
}

#[derive(Debug, Clone)]
pub(crate) struct HirPlace {
    pub(crate) root: PlaceRoot,
    pub(crate) segments: Vec<PlaceSegment>,
    pub(crate) ty: LpsType,
    pub(crate) lanes: Option<Vec<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PlaceRoot {
    Local {
        local: usize,
        ty: LpsType,
    },
    Param {
        param: usize,
        ty: LpsType,
    },
    Uniform {
        name: String,
        byte_offset: u32,
        ty: LpsType,
    },
}

#[derive(Debug, Clone)]
#[allow(
    dead_code,
    reason = "place paths intentionally carry layout metadata before slot-backed lowering consumes it"
)]
pub(crate) enum PlaceSegment {
    Field {
        name: String,
        ty: LpsType,
        lane_offset: usize,
        lane_count: usize,
        byte_offset: usize,
    },
    Swizzle {
        fields: String,
        lanes: Vec<usize>,
        ty: LpsType,
    },
    Index {
        index: Box<HirExpr>,
        ty: LpsType,
    },
}

impl HirPlace {
    pub(super) fn local(local: usize, ty: LpsType) -> Self {
        let lanes = (0..scalar_lane_count(&ty)).collect();
        Self {
            root: PlaceRoot::Local {
                local,
                ty: ty.clone(),
            },
            segments: Vec::new(),
            ty,
            lanes: Some(lanes),
        }
    }

    pub(super) fn param(param: usize, ty: LpsType) -> Self {
        let lanes = (0..scalar_lane_count(&ty)).collect();
        Self {
            root: PlaceRoot::Param {
                param,
                ty: ty.clone(),
            },
            segments: Vec::new(),
            ty,
            lanes: Some(lanes),
        }
    }

    pub(super) fn uniform(name: String, byte_offset: u32, ty: LpsType) -> Self {
        let lanes = (0..scalar_lane_count(&ty)).collect();
        Self {
            root: PlaceRoot::Uniform {
                name,
                byte_offset,
                ty: ty.clone(),
            },
            segments: Vec::new(),
            ty,
            lanes: Some(lanes),
        }
    }

    pub(super) fn push_field(&mut self, span: Span, name: &str) -> Result<(), Diagnostic> {
        let shape = TypeShape::new(&self.ty);
        if let Some(field) = shape.field(name) {
            self.ty = field.ty.clone();
            self.segments.push(PlaceSegment::Field {
                name: String::from(name),
                ty: field.ty.clone(),
                lane_offset: field.lane_offset,
                lane_count: field.lane_count,
                byte_offset: field.byte_offset,
            });
            if let Some(lanes) = &self.lanes {
                let Some(projected) =
                    lanes.get(field.lane_offset..field.lane_offset + field.lane_count)
                else {
                    return Err(Diagnostic::error(span, "field lane out of range"));
                };
                self.lanes = Some(projected.to_vec());
            }
            return Ok(());
        }
        let (relative_lanes, ty) = swizzle_lanes(span, &self.ty, name)?;
        self.ty = ty.clone();
        self.segments.push(PlaceSegment::Swizzle {
            fields: String::from(name),
            lanes: relative_lanes.clone(),
            ty,
        });
        if let Some(lanes) = &self.lanes {
            let projected = relative_lanes
                .iter()
                .map(|lane| {
                    lanes
                        .get(*lane)
                        .copied()
                        .ok_or_else(|| Diagnostic::error(span, "swizzle lane out of range"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            self.lanes = Some(projected);
        }
        Ok(())
    }

    pub(super) fn push_index(&mut self, index: HirExpr) -> Result<(), Diagnostic> {
        let span = index.span;
        let shape = TypeShape::new(&self.ty);
        if let Some(column_ty) = shape.matrix_column().cloned() {
            self.ty = column_ty.clone();
            self.lanes = None;
            self.segments.push(PlaceSegment::Index {
                index: Box::new(index),
                ty: column_ty,
            });
            return Ok(());
        }
        if let Some((element, _, _)) = shape.array_element() {
            let ty = element.clone();
            self.ty = ty.clone();
            self.lanes = None;
            self.segments.push(PlaceSegment::Index {
                index: Box::new(index),
                ty,
            });
            return Ok(());
        }
        if let Some(base) = scalar_base_type(&self.ty) {
            self.ty = base.clone();
            self.lanes = None;
            self.segments.push(PlaceSegment::Index {
                index: Box::new(index),
                ty: base,
            });
            return Ok(());
        }
        Err(Diagnostic::error(
            span,
            "index base must be vector, matrix, or array",
        ))
    }

    pub(crate) fn single_root_lane_path(&self) -> Option<Vec<usize>> {
        self.lanes.clone()
    }
}

impl PlaceRoot {
    pub(crate) fn is_writable(&self) -> bool {
        !matches!(self, PlaceRoot::Uniform { .. })
    }
}

pub(super) fn access_lanes(
    span: Span,
    ty: &LpsType,
    fields: &str,
) -> Result<(Vec<usize>, LpsType), Diagnostic> {
    let shape = TypeShape::new(ty);
    if let Some(field) = shape.field(fields) {
        return Ok((
            (field.lane_offset..field.lane_offset + field.lane_count).collect(),
            field.ty.clone(),
        ));
    }
    swizzle_lanes(span, ty, fields)
}

fn swizzle_lanes(
    span: Span,
    ty: &LpsType,
    fields: &str,
) -> Result<(Vec<usize>, LpsType), Diagnostic> {
    let count = scalar_lane_count(ty);
    if count < 2 {
        return Err(Diagnostic::error(span, "swizzle requires vector base"));
    }
    let mut lanes = Vec::new();
    for ch in fields.chars() {
        let lane = match ch {
            'x' | 'r' | 's' => 0,
            'y' | 'g' | 't' => 1,
            'z' | 'b' | 'p' => 2,
            'w' | 'a' | 'q' => 3,
            _ => return Err(Diagnostic::error(span, "unsupported swizzle field")),
        };
        if lane >= count {
            return Err(Diagnostic::error(span, "swizzle lane out of range"));
        }
        lanes.push(lane);
    }
    let base = scalar_base_type(ty).ok_or_else(|| Diagnostic::error(span, "swizzle base type"))?;
    let out_ty = if lanes.len() == 1 {
        base
    } else {
        LpsType::vector_type(&base, lanes.len())
            .ok_or_else(|| Diagnostic::error(span, "unsupported swizzle width"))?
    };
    Ok((lanes, out_ty))
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::string::String;
    use alloc::vec;

    use lps_shared::StructMember;

    use super::*;
    use crate::hir::{HirExpr, HirExprKind};

    #[test]
    fn place_struct_field_keeps_lane_and_byte_metadata() {
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
            ],
        };
        let mut place = local_place(0, ty);
        place.push_field(Span::new(0, 1), "b").unwrap();
        assert_eq!(place.ty, LpsType::Vec2);
        assert_eq!(place.lanes, Some(vec![1, 2]));
        let [
            PlaceSegment::Field {
                byte_offset,
                lane_offset,
                lane_count,
                ..
            },
        ] = place.segments.as_slice()
        else {
            panic!("expected one field segment");
        };
        assert_eq!((*byte_offset, *lane_offset, *lane_count), (8, 1, 2));
    }

    #[test]
    fn place_swizzle_projects_root_lanes() {
        let mut place = local_place(0, LpsType::Vec4);
        place.push_field(Span::new(0, 2), "zy").unwrap();
        assert_eq!(place.ty, LpsType::Vec2);
        assert_eq!(place.lanes, Some(vec![2, 1]));
    }

    #[test]
    fn place_array_index_switches_to_dynamic_path() {
        let ty = LpsType::Array {
            element: Box::new(LpsType::Vec3),
            len: 2,
        };
        let mut place = local_place(0, ty);
        place.push_index(int_expr(1)).unwrap();
        assert_eq!(place.ty, LpsType::Vec3);
        assert_eq!(place.lanes, None);
        assert_eq!(place.segments.len(), 1);
    }

    fn local_place(local: usize, ty: LpsType) -> HirPlace {
        HirPlace::local(local, ty)
    }

    fn int_expr(value: i32) -> HirExpr {
        HirExpr {
            span: Span::new(0, 1),
            ty: LpsType::Int,
            kind: HirExprKind::IntLiteral(value),
        }
    }
}
