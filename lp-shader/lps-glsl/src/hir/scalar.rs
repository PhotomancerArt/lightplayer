use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

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

pub(super) fn glsl_param_token(ty: &LpsType, span: Span) -> Result<String, Diagnostic> {
    Ok(match ty {
        LpsType::Float => String::from("Float"),
        LpsType::Int => String::from("Int"),
        LpsType::UInt => String::from("UInt"),
        LpsType::Vec2 => String::from("Vec2"),
        LpsType::Vec3 => String::from("Vec3"),
        LpsType::Vec4 => String::from("Vec4"),
        LpsType::IVec2 => String::from("IVec2"),
        LpsType::IVec3 => String::from("IVec3"),
        LpsType::IVec4 => String::from("IVec4"),
        LpsType::UVec2 => String::from("UVec2"),
        LpsType::UVec3 => String::from("UVec3"),
        LpsType::UVec4 => String::from("UVec4"),
        LpsType::BVec2 => String::from("BVec2"),
        LpsType::BVec3 => String::from("BVec3"),
        LpsType::BVec4 => String::from("BVec4"),
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

pub fn scalar_ir_types(ty: &LpsType) -> Result<Vec<lpir::IrType>, Diagnostic> {
    if *ty == LpsType::Void {
        return Ok(Vec::new());
    }
    if *ty == LpsType::Texture2D {
        return Ok(alloc::vec![lpir::IrType::I32; 4]);
    }
    if let LpsType::Array { element, len } = ty {
        let element_tys = scalar_ir_types(element)?;
        let mut tys = Vec::new();
        for _ in 0..*len {
            tys.extend(element_tys.iter().copied());
        }
        return Ok(tys);
    }
    if let LpsType::Struct { members, .. } = ty {
        let mut tys = Vec::new();
        for member in members {
            tys.extend(scalar_ir_types(&member.ty)?);
        }
        return Ok(tys);
    }
    let Some(base) = scalar_base_type(ty) else {
        return Err(Diagnostic::error(
            Span::new(0, 0),
            format!("M3 lps-glsl cannot scalarize type {ty:?}"),
        ));
    };
    let lane = match base {
        LpsType::Float => lpir::IrType::F32,
        LpsType::Int | LpsType::UInt | LpsType::Bool => lpir::IrType::I32,
        _ => {
            return Err(Diagnostic::error(
                Span::new(0, 0),
                format!("M3 lps-glsl cannot scalarize type {ty:?}"),
            ));
        }
    };
    Ok(alloc::vec![lane; scalar_lane_count(ty)])
}
