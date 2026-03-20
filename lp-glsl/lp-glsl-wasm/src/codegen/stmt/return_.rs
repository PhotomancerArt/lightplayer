//! Return statement code generation.

use crate::codegen::context::WasmCodegenContext;
use crate::codegen::expr;
use crate::options::WasmOptions;
use lp_glsl_frontend::error::GlslDiagnostics;
use lp_glsl_frontend::semantic::TypedFunction;
use lp_glsl_frontend::semantic::functions::ParamQualifier;
use lp_glsl_frontend::semantic::types::Type;

/// Emit return statement (emit expr, then return).
pub fn emit_return(
    ctx: &mut WasmCodegenContext,
    f: &mut wasm_encoder::Function,
    expr: &glsl::syntax::Expr,
    options: &WasmOptions,
    return_type: &Type,
) -> Result<(), GlslDiagnostics> {
    let mut instr = f.instructions();
    emit_return_to_sink(ctx, &mut instr, expr, options, return_type)
}

/// Emit return to instruction sink.
pub fn emit_return_to_sink(
    ctx: &mut WasmCodegenContext,
    instr: &mut wasm_encoder::InstructionSink,
    expr: &glsl::syntax::Expr,
    options: &WasmOptions,
    return_type: &Type,
) -> Result<(), GlslDiagnostics> {
    if !matches!(return_type, Type::Void) {
        expr::emit_rvalue(ctx, instr, expr, options)?;
    }
    emit_fn_out_writebacks(ctx, instr);
    instr.return_();
    Ok(())
}

/// Push `inout`/`out` parameter values for the WASM multi-return ABI.
pub(crate) fn emit_fn_out_writebacks(
    ctx: &WasmCodegenContext,
    instr: &mut wasm_encoder::InstructionSink,
) {
    for p in ctx.fn_params {
        if matches!(p.qualifier, ParamQualifier::InOut | ParamQualifier::Out) {
            let info = ctx
                .lookup_local(&p.name)
                .expect("inout/out parameter must be a local");
            for c in 0..info.component_count {
                instr.local_get(info.base_index + c);
            }
        }
    }
}

/// Void functions that fall off the end must still return `inout`/`out` copies.
pub(crate) fn emit_implicit_tail_return(
    ctx: &WasmCodegenContext,
    instr: &mut wasm_encoder::InstructionSink,
    func: &TypedFunction,
) -> Result<(), GlslDiagnostics> {
    let has_primary = !matches!(func.return_type, Type::Void);
    let has_wb = func
        .parameters
        .iter()
        .any(|p| matches!(p.qualifier, ParamQualifier::InOut | ParamQualifier::Out));
    if has_primary {
        return Ok(());
    }
    if has_wb {
        emit_fn_out_writebacks(ctx, instr);
        instr.return_();
    }
    Ok(())
}
