//! Declaration statement code generation.

use glsl::syntax::Declaration;

use crate::codegen::context::WasmCodegenContext;
use crate::codegen::expr;
use crate::options::WasmOptions;
use lp_glsl_frontend::error::GlslDiagnostics;
use lp_glsl_frontend::semantic::type_resolver;

/// Allocate locals for a declaration (first pass).
pub fn allocate_local_from_decl(ctx: &mut WasmCodegenContext, decl: &Declaration) {
    if let Declaration::InitDeclaratorList(list) = decl {
        if let Some(ref name) = list.head.name {
            if let Ok(ty) = type_resolver::parse_head_declarator_type(list, &name.span, None) {
                if (ty.is_scalar() || ty.is_vector()) && !ty.is_error() {
                    let _ = ctx.add_local(name.name.clone(), ty);
                }
            }
        }
    }
}

/// Emit declaration statement (init + local.set).
pub fn emit_declaration(
    ctx: &mut WasmCodegenContext,
    f: &mut wasm_encoder::Function,
    decl: &Declaration,
    options: &WasmOptions,
) -> Result<(), GlslDiagnostics> {
    let mut instr = f.instructions();
    emit_declaration_to_sink(ctx, &mut instr, decl, options)
}

/// Emit declaration to instruction sink.
pub fn emit_declaration_to_sink(
    ctx: &mut WasmCodegenContext,
    instr: &mut wasm_encoder::InstructionSink,
    decl: &Declaration,
    options: &WasmOptions,
) -> Result<(), GlslDiagnostics> {
    if let Declaration::InitDeclaratorList(list) = decl {
        if let Some(ref name) = list.head.name {
            let ty = type_resolver::parse_head_declarator_type(list, &name.span, None)
                .map_err(|e| GlslDiagnostics::from(e))?;
            if (!ty.is_scalar() && !ty.is_vector()) || ty.is_error() {
                return Ok(());
            }
            let (base_index, component_count) = {
                let info = ctx.lookup_local(&name.name).ok_or_else(|| {
                    GlslDiagnostics::from(lp_glsl_frontend::error::GlslError::new(
                        lp_glsl_frontend::error::ErrorCode::E0400,
                        alloc::format!("variable {} not in scope", name.name),
                    ))
                })?;
                (info.base_index, info.component_count)
            };
            if let Some(ref init) = list.head.initializer {
                if let glsl::syntax::Initializer::Simple(expr) = init {
                    let _ = expr::emit_rvalue(ctx, instr, expr.as_ref(), options)?;
                    for i in (0..component_count).rev() {
                        instr.local_set(base_index + i);
                    }
                }
            }
        }
    }
    Ok(())
}
