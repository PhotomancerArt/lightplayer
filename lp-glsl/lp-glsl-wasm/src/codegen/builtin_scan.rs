//! Collect `BuiltinId`s referenced by the shader (Q32 import set).

use alloc::format;
use alloc::vec::Vec;

use hashbrown::{HashMap, HashSet};

use crate::codegen::context::WasmCodegenContext;
use crate::codegen::expr::infer_expr_type;
use crate::codegen::stmt;
use crate::options::WasmOptions;
use glsl::syntax::{
    Condition, Declaration, Expr, ForInitStatement, Initializer, JumpStatement,
    SelectionRestStatement, SimpleStatement, Statement,
};
use lp_glsl_builtin_ids::{
    BuiltinId, GlslParamKind, glsl_lpfx_q32_builtin_id, glsl_q32_math_builtin_id,
};
use lp_glsl_frontend::FloatMode;
use lp_glsl_frontend::error::{GlslDiagnostics, GlslError};
use lp_glsl_frontend::semantic::builtins;
use lp_glsl_frontend::semantic::lpfx::lpfx_fn_registry;
use lp_glsl_frontend::semantic::type_check::{
    is_matrix_type_name, is_scalar_type_name, is_vector_type_name,
};
use lp_glsl_frontend::semantic::types::Type;
use lp_glsl_frontend::semantic::{TypedFunction, TypedShader};

fn type_to_glsl_param_kind(ty: &Type) -> Result<GlslParamKind, GlslDiagnostics> {
    Ok(match ty {
        Type::Bool => GlslParamKind::Bool,
        Type::Int => GlslParamKind::Int,
        Type::UInt => GlslParamKind::UInt,
        Type::Float => GlslParamKind::Float,
        Type::Vec2 => GlslParamKind::Vec2,
        Type::Vec3 => GlslParamKind::Vec3,
        Type::Vec4 => GlslParamKind::Vec4,
        Type::IVec2 => GlslParamKind::IVec2,
        Type::IVec3 => GlslParamKind::IVec3,
        Type::IVec4 => GlslParamKind::IVec4,
        Type::UVec2 => GlslParamKind::UVec2,
        Type::UVec3 => GlslParamKind::UVec3,
        Type::UVec4 => GlslParamKind::UVec4,
        Type::BVec2 => GlslParamKind::BVec2,
        Type::BVec3 => GlslParamKind::BVec3,
        Type::BVec4 => GlslParamKind::BVec4,
        _ => {
            return Err(GlslError::new(
                lp_glsl_frontend::error::ErrorCode::E0400,
                format!("unsupported LPFX parameter type {:?}", ty),
            )
            .into());
        }
    })
}

fn walk_condition(
    ctx: &WasmCodegenContext,
    condition: &Condition,
    used: &mut HashSet<BuiltinId>,
    options: &WasmOptions,
) -> Result<(), GlslDiagnostics> {
    match condition {
        Condition::Expr(expr) => walk_expr(ctx, expr, used, options),
        Condition::Assignment(_, _, _) => Ok(()),
    }
}

fn walk_expr(
    ctx: &WasmCodegenContext,
    expr: &Expr,
    used: &mut HashSet<BuiltinId>,
    options: &WasmOptions,
) -> Result<(), GlslDiagnostics> {
    if options.float_mode != FloatMode::Q32 {
        return Ok(());
    }

    use Expr::*;
    match expr {
        IntConst(_, _)
        | UIntConst(_, _)
        | FloatConst(_, _)
        | BoolConst(_, _)
        | Expr::Variable(_, _) => Ok(()),
        Binary(_, lhs, rhs, _) => {
            walk_expr(ctx, lhs.as_ref(), used, options)?;
            walk_expr(ctx, rhs.as_ref(), used, options)
        }
        Unary(_, operand, _) => walk_expr(ctx, operand.as_ref(), used, options),
        Ternary(c, t, e, _) => {
            walk_expr(ctx, c.as_ref(), used, options)?;
            walk_expr(ctx, t.as_ref(), used, options)?;
            walk_expr(ctx, e.as_ref(), used, options)
        }
        Dot(base, _, _) => walk_expr(ctx, base.as_ref(), used, options),
        Assignment(lhs, _, rhs, _) => {
            walk_expr(ctx, lhs.as_ref(), used, options)?;
            walk_expr(ctx, rhs.as_ref(), used, options)
        }
        FunCall(func_ident, args, _) => {
            for a in args {
                walk_expr(ctx, a, used, options)?;
            }
            let name = match func_ident {
                glsl::syntax::FunIdentifier::Identifier(ident) => ident.name.as_str(),
                _ => return Ok(()),
            };

            if is_scalar_type_name(name) || is_vector_type_name(name) || is_matrix_type_name(name) {
                return Ok(());
            }
            if ctx.func_index_map.contains_key(name) {
                return Ok(());
            }

            if builtins::is_builtin_function(name) {
                if crate::codegen::expr::builtin_inline::q32_builtin_import_suppressed(
                    name,
                    args.len(),
                ) {
                    return Ok(());
                }
                if let Some(id) = glsl_q32_math_builtin_id(name, args.len()) {
                    used.insert(id);
                }
            } else if lpfx_fn_registry::is_lpfx_fn(name) {
                let arg_types: Vec<Type> = args
                    .iter()
                    .map(|a| infer_expr_type(ctx, a))
                    .collect::<Result<_, _>>()?;
                let kinds: Vec<GlslParamKind> = arg_types
                    .iter()
                    .map(type_to_glsl_param_kind)
                    .collect::<Result<_, _>>()?;
                if let Some(id) = glsl_lpfx_q32_builtin_id(name, &kinds) {
                    used.insert(id);
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn walk_declaration(
    ctx: &WasmCodegenContext,
    decl: &Declaration,
    used: &mut HashSet<BuiltinId>,
    options: &WasmOptions,
) -> Result<(), GlslDiagnostics> {
    if let Declaration::InitDeclaratorList(list) = decl {
        if let Some(ref init) = list.head.initializer {
            if let Initializer::Simple(e) = init {
                walk_expr(ctx, e.as_ref(), used, options)?;
            }
        }
        for tail in &list.tail {
            if let Some(ref init) = tail.initializer {
                if let Initializer::Simple(e) = init {
                    walk_expr(ctx, e.as_ref(), used, options)?;
                }
            }
        }
    }
    Ok(())
}

fn walk_simple_statement(
    ctx: &WasmCodegenContext,
    simple: &SimpleStatement,
    used: &mut HashSet<BuiltinId>,
    options: &WasmOptions,
) -> Result<(), GlslDiagnostics> {
    match simple {
        SimpleStatement::Declaration(decl) => walk_declaration(ctx, decl, used, options),
        SimpleStatement::Expression(Some(expr)) => walk_expr(ctx, expr, used, options),
        SimpleStatement::Expression(None) => Ok(()),
        SimpleStatement::Jump(jump) => match jump {
            JumpStatement::Return(Some(expr)) => walk_expr(ctx, expr, used, options),
            _ => Ok(()),
        },
        SimpleStatement::Selection(sel) => {
            walk_expr(ctx, &sel.cond, used, options)?;
            match &sel.rest {
                SelectionRestStatement::Statement(s) => walk_statement(ctx, s, used, options),
                SelectionRestStatement::Else(then_s, else_s) => {
                    walk_statement(ctx, then_s, used, options)?;
                    walk_statement(ctx, else_s, used, options)
                }
            }
        }
        SimpleStatement::Iteration(iter) => match iter {
            glsl::syntax::IterationStatement::While(cond, body) => {
                walk_condition(ctx, cond, used, options)?;
                walk_statement(ctx, body, used, options)
            }
            glsl::syntax::IterationStatement::DoWhile(body, cond) => {
                walk_statement(ctx, body, used, options)?;
                walk_expr(ctx, cond, used, options)
            }
            glsl::syntax::IterationStatement::For(init, rest, body) => {
                match init {
                    ForInitStatement::Declaration(decl) => {
                        walk_declaration(ctx, decl, used, options)?;
                    }
                    ForInitStatement::Expression(Some(e)) => {
                        walk_expr(ctx, e, used, options)?;
                    }
                    ForInitStatement::Expression(None) => {}
                }
                if let Some(c) = &rest.condition {
                    walk_condition(ctx, c, used, options)?;
                }
                if let Some(u) = &rest.post_expr {
                    walk_expr(ctx, u, used, options)?;
                }
                walk_statement(ctx, body, used, options)
            }
        },
        SimpleStatement::Switch(_) | SimpleStatement::CaseLabel(_) => Ok(()),
    }
}

fn walk_statement(
    ctx: &WasmCodegenContext,
    stmt: &Statement,
    used: &mut HashSet<BuiltinId>,
    options: &WasmOptions,
) -> Result<(), GlslDiagnostics> {
    match stmt {
        Statement::Simple(simple) => walk_simple_statement(ctx, simple, used, options),
        Statement::Compound(compound) => {
            for s in &compound.statement_list {
                walk_statement(ctx, s, used, options)?;
            }
            Ok(())
        }
    }
}

fn scan_function(
    func: &TypedFunction,
    options: &WasmOptions,
    func_index_map: &HashMap<alloc::string::String, u32>,
    func_return_type: &HashMap<alloc::string::String, Type>,
) -> Result<HashSet<BuiltinId>, GlslDiagnostics> {
    let mut used = HashSet::new();
    if options.float_mode != FloatMode::Q32 {
        return Ok(used);
    }

    let no_builtin_idx: HashMap<BuiltinId, u32> = HashMap::new();
    let mut ctx = WasmCodegenContext::new(
        &func.parameters,
        options,
        func_index_map,
        &no_builtin_idx,
        func_return_type,
    );
    for stmt in &func.body {
        stmt::walk_for_declarations(&mut ctx, stmt);
    }
    for stmt in &func.body {
        walk_statement(&ctx, stmt, &mut used, options)?;
    }
    Ok(used)
}

/// All Q32 builtins that must be imported when linking this module.
pub fn scan_shader_for_builtin_imports(
    shader: &TypedShader,
    options: &WasmOptions,
    func_index_map: &HashMap<alloc::string::String, u32>,
    func_return_type: &HashMap<alloc::string::String, Type>,
) -> Result<HashSet<BuiltinId>, GlslDiagnostics> {
    let mut all = HashSet::new();
    if options.float_mode != FloatMode::Q32 {
        return Ok(all);
    }
    if let Some(ref main) = shader.main_function {
        for id in scan_function(main, options, func_index_map, func_return_type)? {
            all.insert(id);
        }
    }
    for f in &shader.user_functions {
        for id in scan_function(f, options, func_index_map, func_return_type)? {
            all.insert(id);
        }
    }
    Ok(all)
}
