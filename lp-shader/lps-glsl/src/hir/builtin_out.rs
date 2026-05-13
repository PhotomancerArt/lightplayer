use alloc::format;
use alloc::vec;

use lps_shared::LpsType;

use crate::body::ParsedExpr;
use crate::{Diagnostic, Span};

use super::typeck::TypeCtx;
use super::types::{BuiltinKind, HirExpr, HirExprKind, HirUserCallWriteback};
use super::typing::{coerce_arithmetic_pair, scalar_base_type};

impl<'a> TypeCtx<'a> {
    pub(super) fn type_builtin_out_call(
        &mut self,
        span: Span,
        kind: BuiltinKind,
        args: &[ParsedExpr],
    ) -> Result<HirExpr, Diagnostic> {
        match kind {
            BuiltinKind::UaddCarry | BuiltinKind::UsubBorrow => {
                if args.len() != 3 {
                    return Err(Diagnostic::error(span, "builtin expects 3 arguments"));
                }
                let lhs = self.type_expr(&args[0])?;
                let rhs = self.type_expr(&args[1])?;
                let (lhs, rhs, ty) = coerce_arithmetic_pair(span, lhs, rhs)?;
                require_integer_lane_type(span, kind, &ty, LpsType::UInt)?;

                let carry = self.type_assign_target(&args[2])?;
                if carry.ty() != &ty {
                    return Err(Diagnostic::error(
                        args[2].span,
                        "out argument type must match builtin argument type",
                    ));
                }

                Ok(HirExpr {
                    span,
                    ty: ty.clone(),
                    kind: HirExprKind::Builtin {
                        kind,
                        args: vec![lhs, rhs],
                        writebacks: vec![HirUserCallWriteback {
                            arg_index: 2,
                            target: carry,
                            ty,
                            copy_in: false,
                        }],
                    },
                })
            }
            BuiltinKind::UmulExtended | BuiltinKind::ImulExtended => {
                if args.len() != 4 {
                    return Err(Diagnostic::error(span, "builtin expects 4 arguments"));
                }
                let lhs = self.type_expr(&args[0])?;
                let rhs = self.type_expr(&args[1])?;
                let (lhs, rhs, ty) = coerce_arithmetic_pair(span, lhs, rhs)?;
                let required = match kind {
                    BuiltinKind::UmulExtended => LpsType::UInt,
                    BuiltinKind::ImulExtended => LpsType::Int,
                    _ => unreachable!(),
                };
                require_integer_lane_type(span, kind, &ty, required)?;

                let msb = self.type_assign_target(&args[2])?;
                let lsb = self.type_assign_target(&args[3])?;
                if msb.ty() != &ty || lsb.ty() != &ty {
                    return Err(Diagnostic::error(
                        span,
                        "out argument types must match builtin argument type",
                    ));
                }

                Ok(HirExpr {
                    span,
                    ty: LpsType::Void,
                    kind: HirExprKind::Builtin {
                        kind,
                        args: vec![lhs, rhs],
                        writebacks: vec![
                            HirUserCallWriteback {
                                arg_index: 2,
                                target: msb,
                                ty: ty.clone(),
                                copy_in: false,
                            },
                            HirUserCallWriteback {
                                arg_index: 3,
                                target: lsb,
                                ty,
                                copy_in: false,
                            },
                        ],
                    },
                })
            }
            _ => Err(Diagnostic::error(
                span,
                "internal builtin out-call typing mismatch",
            )),
        }
    }
}

fn require_integer_lane_type(
    span: Span,
    kind: BuiltinKind,
    ty: &LpsType,
    required: LpsType,
) -> Result<(), Diagnostic> {
    if ty.is_matrix() || scalar_base_type(ty) != Some(required.clone()) {
        return Err(Diagnostic::error(
            span,
            format!("{kind:?} expects matching {required:?} scalar/vector lanes"),
        ));
    }
    Ok(())
}
