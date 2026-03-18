//! Vector component access and swizzle code generation.

use wasm_encoder::InstructionSink;

use crate::codegen::context::WasmCodegenContext;
use crate::codegen::rvalue::WasmRValue;
use crate::options::WasmOptions;
use lp_glsl_frontend::error::{GlslDiagnostics, GlslError};
use lp_glsl_frontend::semantic::types::Type;

/// Emit dot expression (component access / swizzle).
pub fn emit_dot(
    ctx: &WasmCodegenContext,
    sink: &mut InstructionSink,
    base_expr: &glsl::syntax::Expr,
    field: &glsl::syntax::Identifier,
    _options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    let name = match base_expr {
        glsl::syntax::Expr::Variable(ident, _) => &ident.name,
        _ => {
            return Err(GlslError::new(
                lp_glsl_frontend::error::ErrorCode::E0400,
                alloc::format!(
                    "component access on non-variable expr not yet supported: {:?}",
                    base_expr
                ),
            )
            .into());
        }
    };

    let info = ctx.lookup_local(name).ok_or_else(|| {
        GlslDiagnostics::from(GlslError::new(
            lp_glsl_frontend::error::ErrorCode::E0100,
            alloc::format!("undefined variable `{name}`"),
        ))
    })?;

    if !info.ty.is_vector() {
        return Err(GlslError::new(
            lp_glsl_frontend::error::ErrorCode::E0112,
            alloc::format!("component access on non-vector type: {:?}", info.ty),
        )
        .into());
    }

    let component_count = info.ty.component_count().unwrap();
    let base_ty = info.ty.vector_base_type().unwrap();
    let indices = parse_swizzle_indices(&field.name, component_count)?;

    if indices.len() == 1 {
        sink.local_get(info.base_index + indices[0] as u32);
        Ok(WasmRValue::scalar(base_ty))
    } else {
        for &idx in &indices {
            sink.local_get(info.base_index + idx as u32);
        }
        let result_ty = Type::vector_type(&base_ty, indices.len()).ok_or_else(|| {
            GlslDiagnostics::from(GlslError::new(
                lp_glsl_frontend::error::ErrorCode::E0400,
                alloc::format!("cannot create vector of size {}", indices.len()),
            ))
        })?;
        Ok(WasmRValue::from_type(result_ty))
    }
}

/// Parse swizzle string into component indices. Supports xyzw, rgba, stpq.
fn parse_swizzle_indices(
    swizzle: &str,
    max_components: usize,
) -> Result<alloc::vec::Vec<usize>, GlslDiagnostics> {
    let mut indices = alloc::vec::Vec::new();
    for ch in swizzle.chars() {
        let idx = match ch {
            'x' | 'r' | 's' => 0,
            'y' | 'g' | 't' => 1,
            'z' | 'b' | 'p' => 2,
            'w' | 'a' | 'q' => 3,
            _ => {
                return Err(GlslError::new(
                    lp_glsl_frontend::error::ErrorCode::E0113,
                    alloc::format!("invalid swizzle character: '{ch}'"),
                )
                .into());
            }
        };
        if idx >= max_components {
            return Err(GlslError::new(
                lp_glsl_frontend::error::ErrorCode::E0111,
                alloc::format!(
                    "component '{ch}' not valid for vector with {max_components} components"
                ),
            )
            .into());
        }
        indices.push(idx);
    }
    Ok(indices)
}
