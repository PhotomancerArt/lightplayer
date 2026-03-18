//! Expression type inference for WASM codegen (uses ctx.locals, no SymbolTable).

use crate::codegen::context::WasmCodegenContext;
use lp_glsl_frontend::error::GlslDiagnostics;
use lp_glsl_frontend::semantic::type_check::{infer_binary_result_type, parse_swizzle_length};
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
        Expr::FunCall(func_ident, _, _) => {
            let name = match func_ident {
                glsl::syntax::FunIdentifier::Identifier(ident) => ident.name.as_str(),
                _ => "",
            };
            match name {
                "bool" => Ok(Type::Bool),
                "int" => Ok(Type::Int),
                "uint" => Ok(Type::UInt),
                "float" => Ok(Type::Float),
                "vec2" => Ok(Type::Vec2),
                "vec3" => Ok(Type::Vec3),
                "vec4" => Ok(Type::Vec4),
                "ivec2" => Ok(Type::IVec2),
                "ivec3" => Ok(Type::IVec3),
                "ivec4" => Ok(Type::IVec4),
                "uvec2" => Ok(Type::UVec2),
                "uvec3" => Ok(Type::UVec3),
                "uvec4" => Ok(Type::UVec4),
                "bvec2" => Ok(Type::BVec2),
                "bvec3" => Ok(Type::BVec3),
                "bvec4" => Ok(Type::BVec4),
                _ => ctx.func_return_type.get(name).cloned().ok_or_else(|| {
                    lp_glsl_frontend::error::GlslError::new(
                        lp_glsl_frontend::error::ErrorCode::E0400,
                        alloc::format!("type inference not supported for call `{name}`"),
                    )
                    .into()
                }),
            }
        }
        _ => Err(lp_glsl_frontend::error::GlslError::new(
            lp_glsl_frontend::error::ErrorCode::E0400,
            alloc::format!("type inference not supported for {:?}", expr),
        )
        .into()),
    }
}
