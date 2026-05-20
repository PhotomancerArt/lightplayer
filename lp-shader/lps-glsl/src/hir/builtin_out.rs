use alloc::format;
use lps_shared::LpsType;

use crate::body::ParsedExpr;
use crate::{Diagnostic, Span};

use super::arena::ExprId;
use super::typeck::TypeCtx;
use super::types::{BuiltinKind, HirExprKind, HirUserCallWriteback};
use super::typing::{coerce_arithmetic_pair, scalar_base_type};

impl<'a> TypeCtx<'a> {
    pub(super) fn type_builtin_out_call(
        &mut self,
        span: Span,
        kind: BuiltinKind,
        args: &[ParsedExpr],
    ) -> Result<ExprId, Diagnostic> {
        match kind {
            BuiltinKind::Modf => {
                if args.len() != 2 {
                    return Err(Diagnostic::error(span, "builtin expects 2 arguments"));
                }
                let value = self.type_expr(&args[0])?;
                let value_ty = self.arena.expr_ty(value).clone();
                if value_ty.is_matrix() || scalar_base_type(&value_ty) != Some(LpsType::Float) {
                    return Err(Diagnostic::error(
                        args[0].span,
                        "modf expects float scalar/vector lanes",
                    ));
                }
                let ty = value_ty;
                let integer = self.type_assign_target(&args[1])?;
                if self.arena.place(integer).ty != ty {
                    return Err(Diagnostic::error(
                        args[1].span,
                        "out argument type must match builtin argument type",
                    ));
                }

                let args = self.arena.push_expr_list([value]);
                Ok(self.arena.push_expr(
                    span,
                    ty.clone(),
                    HirExprKind::Builtin {
                        kind,
                        args,
                        writebacks: alloc::vec![HirUserCallWriteback {
                            arg_index: 1,
                            target: integer,
                            ty,
                            copy_in: false,
                        }],
                    },
                ))
            }
            BuiltinKind::UaddCarry | BuiltinKind::UsubBorrow => {
                if args.len() != 3 {
                    return Err(Diagnostic::error(span, "builtin expects 3 arguments"));
                }
                let lhs = self.type_expr(&args[0])?;
                let rhs = self.type_expr(&args[1])?;
                let (lhs, rhs, ty) = coerce_arithmetic_pair(&mut self.arena, span, lhs, rhs)?;
                require_integer_lane_type(span, kind, &ty, LpsType::UInt)?;

                let carry = self.type_assign_target(&args[2])?;
                if self.arena.place(carry).ty != ty {
                    return Err(Diagnostic::error(
                        args[2].span,
                        "out argument type must match builtin argument type",
                    ));
                }

                let args = self.arena.push_expr_list([lhs, rhs]);
                Ok(self.arena.push_expr(
                    span,
                    ty.clone(),
                    HirExprKind::Builtin {
                        kind,
                        args,
                        writebacks: alloc::vec![HirUserCallWriteback {
                            arg_index: 2,
                            target: carry,
                            ty,
                            copy_in: false,
                        }],
                    },
                ))
            }
            BuiltinKind::UmulExtended | BuiltinKind::ImulExtended => {
                if args.len() != 4 {
                    return Err(Diagnostic::error(span, "builtin expects 4 arguments"));
                }
                let lhs = self.type_expr(&args[0])?;
                let rhs = self.type_expr(&args[1])?;
                let (lhs, rhs, ty) = coerce_arithmetic_pair(&mut self.arena, span, lhs, rhs)?;
                let required = match kind {
                    BuiltinKind::UmulExtended => LpsType::UInt,
                    BuiltinKind::ImulExtended => LpsType::Int,
                    _ => unreachable!(),
                };
                require_integer_lane_type(span, kind, &ty, required)?;

                let msb = self.type_assign_target(&args[2])?;
                let lsb = self.type_assign_target(&args[3])?;
                if self.arena.place(msb).ty != ty || self.arena.place(lsb).ty != ty {
                    return Err(Diagnostic::error(
                        span,
                        "out argument types must match builtin argument type",
                    ));
                }

                let args = self.arena.push_expr_list([lhs, rhs]);
                Ok(self.arena.push_expr(
                    span,
                    LpsType::Void,
                    HirExprKind::Builtin {
                        kind,
                        args,
                        writebacks: alloc::vec![
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
                ))
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
