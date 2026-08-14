use alloc::format;

use lps_shared::LpsType;

use crate::body::BinaryOp;
use crate::{Diagnostic, Span};

pub(super) fn is_comparison(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge | BinaryOp::Eq | BinaryOp::Ne
    )
}

pub(super) fn is_logical(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::LogicalXor
    )
}

/// Source spelling of a binary operator, for diagnostics.
pub(super) fn binary_op_token(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Comma => ",",
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "%",
        BinaryOp::LogicalAnd => "&&",
        BinaryOp::LogicalOr => "||",
        BinaryOp::LogicalXor => "^^",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
    }
}

/// The LPFN signature spelling of a parameter type.
///
/// Static spellings: the caller matches on them and joins them into one
/// signature string, so handing back owned `String`s would allocate one per
/// argument per call for nothing.
pub(super) fn glsl_param_token(ty: &LpsType, span: Span) -> Result<&'static str, Diagnostic> {
    Ok(match ty {
        LpsType::Float => "Float",
        LpsType::Int => "Int",
        LpsType::UInt => "UInt",
        LpsType::Vec2 => "Vec2",
        LpsType::Vec3 => "Vec3",
        LpsType::Vec4 => "Vec4",
        LpsType::IVec2 => "IVec2",
        LpsType::IVec3 => "IVec3",
        LpsType::IVec4 => "IVec4",
        LpsType::UVec2 => "UVec2",
        LpsType::UVec3 => "UVec3",
        LpsType::UVec4 => "UVec4",
        LpsType::BVec2 => "BVec2",
        LpsType::BVec3 => "BVec3",
        LpsType::BVec4 => "BVec4",
        other => {
            return Err(Diagnostic::error(
                span,
                format!("unsupported LPFN parameter type {other:?}"),
            ));
        }
    })
}

pub fn scalar_lane_count(ty: &LpsType) -> usize {
    match ty {
        LpsType::Void => 0,
        LpsType::Float | LpsType::Int | LpsType::UInt | LpsType::Bool => 1,
        LpsType::Texture2D => 4,
        LpsType::Array { element, len } => scalar_lane_count(element).saturating_mul(*len as usize),
        LpsType::Struct { members, .. } => members
            .iter()
            .map(|member| scalar_lane_count(&member.ty))
            .sum(),
        _ => ty
            .component_count()
            .or_else(|| ty.matrix_element_count())
            .unwrap_or(0),
    }
}

pub fn scalar_base_type(ty: &LpsType) -> Option<LpsType> {
    if let LpsType::Array { element, .. } = ty {
        scalar_base_type(element)
    } else if ty.is_matrix() {
        Some(LpsType::Float)
    } else if ty.is_vector() {
        ty.vector_base_type()
    } else if ty.is_scalar() {
        Some(ty.clone())
    } else {
        None
    }
}

/// Scalarized IR lane types of a value type.
///
/// Sixteen inline lanes covers every vector and matrix shape (up to `mat4`)
/// plus small arrays and structs, so the per-call list stays off the heap on
/// the lowering hot path — this is queried per user call, per return, per
/// uniform/global load, per local declaration and per conditional.
pub type IrTypes = crate::small::InlineVec<lpir::IrType, 16>;

pub fn scalar_ir_types(ty: &LpsType) -> Result<IrTypes, Diagnostic> {
    let mut tys = IrTypes::new();
    push_scalar_ir_types(ty, &mut tys)?;
    Ok(tys)
}

fn push_scalar_ir_types(ty: &LpsType, tys: &mut IrTypes) -> Result<(), Diagnostic> {
    if *ty == LpsType::Void {
        return Ok(());
    }
    if *ty == LpsType::Texture2D {
        for _ in 0..4 {
            tys.push(lpir::IrType::I32);
        }
        return Ok(());
    }
    if let LpsType::Array { element, len } = ty {
        let mut element_tys = IrTypes::new();
        push_scalar_ir_types(element, &mut element_tys)?;
        for _ in 0..*len {
            tys.extend(element_tys.iter().copied());
        }
        return Ok(());
    }
    if let LpsType::Struct { members, .. } = ty {
        for member in members {
            push_scalar_ir_types(&member.ty, tys)?;
        }
        return Ok(());
    }
    let Some(base) = scalar_base_type(ty) else {
        return Err(Diagnostic::error(
            Span::new(0, 0),
            format!("cannot scalarize type {ty:?}"),
        ));
    };
    let lane = match base {
        LpsType::Float => lpir::IrType::F32,
        LpsType::Int | LpsType::UInt | LpsType::Bool => lpir::IrType::I32,
        _ => {
            return Err(Diagnostic::error(
                Span::new(0, 0),
                format!("cannot scalarize type {ty:?}"),
            ));
        }
    };
    for _ in 0..scalar_lane_count(ty) {
        tys.push(lane);
    }
    Ok(())
}
