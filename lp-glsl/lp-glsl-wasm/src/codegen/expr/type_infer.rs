//! Expression type inference for WASM codegen (uses ctx.locals, no SymbolTable).

use crate::codegen::context::WasmCodegenContext;
use lp_glsl_frontend::error::{ErrorCode, GlslDiagnostics, GlslError};
use lp_glsl_frontend::semantic::builtins;
use lp_glsl_frontend::semantic::lpfx::lpfx_fn_registry;
use lp_glsl_frontend::semantic::type_check::{
    check_matrix_constructor, check_scalar_constructor_with_span,
    check_vector_constructor_with_span, infer_binary_result_type, is_matrix_type_name,
    is_scalar_type_name, is_vector_type_name, parse_swizzle_length,
};
use lp_glsl_frontend::semantic::types::Type;

/// Infer the GLSL type of an expression using codegen context (locals only).
/// Used for type-aware binary op dispatch. Returns Error for unsupported exprs.
pub fn infer_expr_type(
    ctx: &WasmCodegenContext,
    expr: &glsl::syntax::Expr,
) -> Result<Type, GlslDiagnostics> {
    use glsl::syntax::Expr;

    match expr {
        Expr::IntConst(_, _) => Ok(Type::Int),
        Expr::UIntConst(_, _) => Ok(Type::UInt),
        Expr::FloatConst(_, _) => Ok(Type::Float),
        Expr::BoolConst(_, _) => Ok(Type::Bool),
        Expr::Variable(ident, _) => {
            let info = ctx.lookup_local(&ident.name).ok_or_else(|| {
                lp_glsl_frontend::error::GlslDiagnostics::from(
                    lp_glsl_frontend::error::GlslError::new(
                        lp_glsl_frontend::error::ErrorCode::E0100,
                        alloc::format!("undefined variable `{}`", ident.name),
                    ),
                )
            })?;
            Ok(info.ty.clone())
        }
        Expr::Binary(op, lhs, rhs, span) => {
            let lhs_ty = infer_expr_type(ctx, lhs.as_ref())?;
            let rhs_ty = infer_expr_type(ctx, rhs.as_ref())?;
            infer_binary_result_type(op, &lhs_ty, &rhs_ty, span.clone())
                .map_err(lp_glsl_frontend::error::GlslDiagnostics::from)
        }
        Expr::Unary(op, operand, _span) => {
            use glsl::syntax::UnaryOp;
            let operand_ty = infer_expr_type(ctx, operand.as_ref())?;
            match op {
                UnaryOp::Minus => {
                    if operand_ty.is_numeric() {
                        Ok(operand_ty)
                    } else {
                        Err(lp_glsl_frontend::error::GlslError::new(
                            lp_glsl_frontend::error::ErrorCode::E0106,
                            alloc::format!("unary minus requires numeric operand"),
                        )
                        .into())
                    }
                }
                UnaryOp::Not => Ok(Type::Bool),
                _ => Err(lp_glsl_frontend::error::GlslError::new(
                    lp_glsl_frontend::error::ErrorCode::E0400,
                    alloc::format!("unary op {:?} not supported", op),
                )
                .into()),
            }
        }
        Expr::Assignment(lhs, _op, _rhs, _) => infer_expr_type(ctx, lhs.as_ref()),
        Expr::Ternary(_, then_expr, else_expr, _) => {
            let t = infer_expr_type(ctx, then_expr.as_ref())?;
            let e = infer_expr_type(ctx, else_expr.as_ref())?;
            if t == e {
                Ok(t)
            } else {
                Err(lp_glsl_frontend::error::GlslError::new(
                    lp_glsl_frontend::error::ErrorCode::E0102,
                    "ternary branches must have matching types",
                )
                .into())
            }
        }
        Expr::Dot(base_expr, field, _) => {
            let base_ty = infer_expr_type(ctx, base_expr.as_ref())?;
            if !base_ty.is_vector() {
                return Err(lp_glsl_frontend::error::GlslError::new(
                    lp_glsl_frontend::error::ErrorCode::E0112,
                    alloc::format!("component access on non-vector type: {:?}", base_ty),
                )
                .into());
            }
            let component_count = base_ty.component_count().unwrap();
            let swizzle_len = parse_swizzle_length(&field.name, component_count)
                .map_err(lp_glsl_frontend::error::GlslDiagnostics::from)?;
            let base_scalar = base_ty.vector_base_type().unwrap();
            if swizzle_len == 1 {
                Ok(base_scalar)
            } else {
                Type::vector_type(&base_scalar, swizzle_len).ok_or_else(|| {
                    lp_glsl_frontend::error::GlslError::new(
                        lp_glsl_frontend::error::ErrorCode::E0400,
                        alloc::format!("invalid swizzle length: {}", swizzle_len),
                    )
                    .into()
                })
            }
        }
        Expr::FunCall(func_ident, args, span) => {
            let name = match func_ident {
                glsl::syntax::FunIdentifier::Identifier(ident) => ident.name.as_str(),
                _ => "",
            };
            let arg_types: alloc::vec::Vec<Type> = args
                .iter()
                .map(|a| infer_expr_type(ctx, a))
                .collect::<Result<_, _>>()?;

            if is_vector_type_name(name) {
                return check_vector_constructor_with_span(name, &arg_types, Some(span.clone()))
                    .map_err(GlslDiagnostics::from);
            }
            if is_matrix_type_name(name) {
                return check_matrix_constructor(name, &arg_types).map_err(GlslDiagnostics::from);
            }
            if is_scalar_type_name(name) {
                return check_scalar_constructor_with_span(name, &arg_types, Some(span.clone()))
                    .map_err(GlslDiagnostics::from);
            }

            if builtins::is_builtin_function(name) {
                return builtins::check_builtin_call(name, &arg_types)
                    .map_err(|msg| GlslDiagnostics::from(GlslError::new(ErrorCode::E0114, msg)));
            }
            if lpfx_fn_registry::is_lpfx_fn(name) {
                return lpfx_fn_registry::check_lpfx_fn_call(name, &arg_types)
                    .map_err(|msg| GlslDiagnostics::from(GlslError::new(ErrorCode::E0114, msg)));
            }

            ctx.func_return_type.get(name).cloned().ok_or_else(|| {
                GlslError::new(
                    ErrorCode::E0400,
                    alloc::format!("type inference not supported for call `{name}`"),
                )
                .into()
            })
        }
        _ => Err(lp_glsl_frontend::error::GlslError::new(
            lp_glsl_frontend::error::ErrorCode::E0400,
            alloc::format!("type inference not supported for {:?}", expr),
        )
        .into()),
    }
}
