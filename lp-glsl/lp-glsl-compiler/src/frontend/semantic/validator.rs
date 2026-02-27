//! Semantic validation for GLSL shaders.
//!
//! This module validates function bodies, variable declarations, expressions,
//! and return statements to ensure they are semantically correct before codegen.

use crate::error::{
    ErrorCode, GlslDiagnostics, GlslError, add_span_text_to_error, extract_span_from_expr,
    source_span_to_location,
};
use crate::frontend::semantic::functions::FunctionRegistry;
use crate::frontend::semantic::scope::{StorageClass, SymbolTable};
use crate::frontend::semantic::type_check::{
    check_assignment_with_span, check_condition, infer_expr_type_with_registry,
};
use crate::frontend::semantic::type_resolver;
use crate::frontend::semantic::types::Type;
use glsl::syntax::{JumpStatement, SimpleStatement, Statement};

use alloc::format;

/// Validate a function body, collecting errors into diagnostics.
pub fn validate_function(
    func: &crate::frontend::semantic::TypedFunction,
    func_registry: &FunctionRegistry,
    global_constants: &hashbrown::HashMap<
        alloc::string::String,
        crate::frontend::semantic::const_eval::ConstValue,
    >,
    source: &str,
    diagnostics: &mut GlslDiagnostics,
) {
    let mut symbols = SymbolTable::new();

    for (name, val) in global_constants {
        if let Err(e) = symbols.declare_variable(name.clone(), val.glsl_type(), StorageClass::Const)
        {
            if !diagnostics.push(e) {
                return;
            }
        }
    }

    for param in &func.parameters {
        if diagnostics.at_limit() {
            return;
        }
        if let Err(e) =
            symbols.declare_variable(param.name.clone(), param.ty.clone(), StorageClass::Local)
        {
            if !diagnostics.push(e) {
                return;
            }
        }
    }

    for stmt in &func.body {
        if diagnostics.at_limit() {
            return;
        }
        validate_statement(
            stmt,
            &mut symbols,
            &func.return_type,
            func_registry,
            source,
            diagnostics,
        );
    }
}

fn validate_statement(
    stmt: &Statement,
    symbols: &mut SymbolTable,
    return_type: &Type,
    func_registry: &FunctionRegistry,
    source: &str,
    diagnostics: &mut GlslDiagnostics,
) {
    match stmt {
        Statement::Simple(simple) => {
            validate_simple_statement(
                simple,
                symbols,
                return_type,
                func_registry,
                source,
                diagnostics,
            );
        }
        Statement::Compound(compound) => {
            symbols.push_scope();
            for stmt in &compound.statement_list {
                if diagnostics.at_limit() {
                    symbols.pop_scope();
                    return;
                }
                validate_statement(
                    stmt,
                    symbols,
                    return_type,
                    func_registry,
                    source,
                    diagnostics,
                );
            }
            symbols.pop_scope();
        }
    }
}

fn infer_or_error(
    expr: &glsl::syntax::Expr,
    symbols: &SymbolTable,
    func_registry: &FunctionRegistry,
    source: &str,
    span: &glsl::syntax::SourceSpan,
    diagnostics: &mut GlslDiagnostics,
) -> Type {
    match infer_expr_type_with_registry(expr, symbols, Some(func_registry)) {
        Ok(t) => t,
        Err(e) => {
            let e = if e.span_text.is_none() {
                add_span_text_to_error(e, Some(source), span)
            } else {
                e
            };
            let _ = diagnostics.push(e);
            Type::Error
        }
    }
}

fn validate_simple_statement(
    stmt: &SimpleStatement,
    symbols: &mut SymbolTable,
    return_type: &Type,
    func_registry: &FunctionRegistry,
    source: &str,
    diagnostics: &mut GlslDiagnostics,
) {
    use glsl::syntax::SimpleStatement;

    if diagnostics.at_limit() {
        return;
    }
    match stmt {
        SimpleStatement::Declaration(decl) => {
            validate_declaration(decl, symbols, func_registry, source, diagnostics);
        }
        SimpleStatement::Expression(Some(expr)) => {
            let expr_span = extract_span_from_expr(expr);
            let _ = infer_or_error(
                expr,
                symbols,
                func_registry,
                source,
                &expr_span,
                diagnostics,
            );
        }
        SimpleStatement::Expression(None) => {}
        SimpleStatement::Selection(selection) => {
            validate_selection(
                selection,
                symbols,
                return_type,
                func_registry,
                source,
                diagnostics,
            );
        }
        SimpleStatement::Iteration(iteration) => {
            validate_iteration(
                iteration,
                symbols,
                return_type,
                func_registry,
                source,
                diagnostics,
            );
        }
        SimpleStatement::Jump(jump) => {
            validate_jump(
                jump,
                symbols,
                return_type,
                func_registry,
                source,
                diagnostics,
            );
        }
        _ => {
            let _ = diagnostics.push(GlslError::new(
                ErrorCode::E0400,
                format!("unsupported statement type in validation: {stmt:?}"),
            ));
        }
    }
}

fn validate_declaration(
    decl: &glsl::syntax::Declaration,
    symbols: &mut SymbolTable,
    func_registry: &FunctionRegistry,
    source: &str,
    diagnostics: &mut GlslDiagnostics,
) {
    match decl {
        glsl::syntax::Declaration::InitDeclaratorList(list) => {
            let base_ty = match type_resolver::parse_return_type(&list.head.ty, None) {
                Ok(t) => t,
                Err(e) => {
                    let _ = diagnostics.push(e);
                    return;
                }
            };

            if let Some(name) = &list.head.name {
                let name_span = name.span.clone();
                let ty = match type_resolver::parse_head_declarator_type(list, &name_span) {
                    Ok(t) => t,
                    Err(e) => {
                        let _ = diagnostics.push(e);
                        return;
                    }
                };

                if let Err(e) =
                    symbols.declare_variable(name.name.clone(), ty.clone(), StorageClass::Local)
                {
                    let _ = diagnostics.push(e.with_location(source_span_to_location(&name_span)));
                    return;
                }

                if let Some(init) = &list.head.initializer {
                    validate_initializer(init, &ty, symbols, func_registry, source, diagnostics);
                }
            }

            for declarator in &list.tail {
                if diagnostics.at_limit() {
                    return;
                }
                let name_span = declarator.ident.ident.span.clone();
                let declarator_ty =
                    match type_resolver::parse_tail_declarator_type(&base_ty, declarator) {
                        Ok(t) => t,
                        Err(e) => {
                            let _ = diagnostics.push(e);
                            continue;
                        }
                    };

                if let Err(e) = symbols.declare_variable(
                    declarator.ident.ident.name.clone(),
                    declarator_ty.clone(),
                    StorageClass::Local,
                ) {
                    let _ = diagnostics.push(e.with_location(source_span_to_location(&name_span)));
                    continue;
                }

                if let Some(init) = &declarator.initializer {
                    validate_initializer(
                        init,
                        &declarator_ty,
                        symbols,
                        func_registry,
                        source,
                        diagnostics,
                    );
                }
            }
        }
        glsl::syntax::Declaration::Precision(_, _)
        | glsl::syntax::Declaration::FunctionPrototype(_)
        | glsl::syntax::Declaration::Block(_)
        | glsl::syntax::Declaration::Global(_, _) => {}
    }
}

fn validate_initializer(
    init: &glsl::syntax::Initializer,
    declared_type: &Type,
    symbols: &SymbolTable,
    func_registry: &FunctionRegistry,
    source: &str,
    diagnostics: &mut GlslDiagnostics,
) {
    use glsl::syntax::Initializer;

    match init {
        Initializer::Simple(expr) => {
            let init_span = extract_span_from_expr(expr.as_ref());
            let init_type = infer_or_error(
                expr.as_ref(),
                symbols,
                func_registry,
                source,
                &init_span,
                diagnostics,
            );
            if !init_type.is_error() {
                if let Err(e) =
                    check_assignment_with_span(declared_type, &init_type, Some(init_span.clone()))
                {
                    let _ = diagnostics.push(add_span_text_to_error(e, Some(source), &init_span));
                }
            }
        }
        _ => {}
    }
}

fn validate_selection(
    selection: &glsl::syntax::SelectionStatement,
    symbols: &mut SymbolTable,
    return_type: &Type,
    func_registry: &FunctionRegistry,
    source: &str,
    diagnostics: &mut GlslDiagnostics,
) {
    use glsl::syntax::SelectionRestStatement;

    let cond_span = extract_span_from_expr(&selection.cond);
    let cond_type = infer_or_error(
        &selection.cond,
        symbols,
        func_registry,
        source,
        &cond_span,
        diagnostics,
    );
    if !cond_type.is_error() {
        if let Err(e) = check_condition(&cond_type) {
            let error = e.with_location(source_span_to_location(&cond_span));
            let _ = diagnostics.push(add_span_text_to_error(error, Some(source), &cond_span));
        }
    }

    match &selection.rest {
        SelectionRestStatement::Statement(then_stmt) => {
            validate_statement(
                then_stmt,
                symbols,
                return_type,
                func_registry,
                source,
                diagnostics,
            );
        }
        SelectionRestStatement::Else(then_stmt, else_stmt) => {
            validate_statement(
                then_stmt,
                symbols,
                return_type,
                func_registry,
                source,
                diagnostics,
            );
            if !diagnostics.at_limit() {
                validate_statement(
                    else_stmt,
                    symbols,
                    return_type,
                    func_registry,
                    source,
                    diagnostics,
                );
            }
        }
    }
}

fn validate_iteration(
    iteration: &glsl::syntax::IterationStatement,
    symbols: &mut SymbolTable,
    return_type: &Type,
    func_registry: &FunctionRegistry,
    source: &str,
    diagnostics: &mut GlslDiagnostics,
) {
    use glsl::syntax::IterationStatement;

    match iteration {
        IterationStatement::While(condition, stmt) => {
            let cond_expr = match condition {
                glsl::syntax::Condition::Expr(expr) => expr.as_ref(),
                glsl::syntax::Condition::Assignment(_, _, _) => return,
            };
            let cond_span = extract_span_from_expr(cond_expr);
            let cond_type = infer_or_error(
                cond_expr,
                symbols,
                func_registry,
                source,
                &cond_span,
                diagnostics,
            );
            if !cond_type.is_error() {
                if let Err(e) = check_condition(&cond_type) {
                    let _ = diagnostics.push(add_span_text_to_error(
                        e.with_location(source_span_to_location(&cond_span)),
                        Some(source),
                        &cond_span,
                    ));
                }
            }
            symbols.push_scope();
            validate_statement(
                stmt,
                symbols,
                return_type,
                func_registry,
                source,
                diagnostics,
            );
            symbols.pop_scope();
        }
        IterationStatement::DoWhile(stmt, cond_expr) => {
            symbols.push_scope();
            validate_statement(
                stmt,
                symbols,
                return_type,
                func_registry,
                source,
                diagnostics,
            );
            symbols.pop_scope();
            let cond_span = extract_span_from_expr(cond_expr.as_ref());
            let cond_type = infer_or_error(
                cond_expr.as_ref(),
                symbols,
                func_registry,
                source,
                &cond_span,
                diagnostics,
            );
            if !cond_type.is_error() {
                if let Err(e) = check_condition(&cond_type) {
                    let _ = diagnostics.push(add_span_text_to_error(
                        e.with_location(source_span_to_location(&cond_span)),
                        Some(source),
                        &cond_span,
                    ));
                }
            }
        }
        IterationStatement::For(init, rest, body) => {
            symbols.push_scope();
            match init {
                glsl::syntax::ForInitStatement::Declaration(decl) => {
                    validate_declaration(decl, symbols, func_registry, source, diagnostics);
                }
                glsl::syntax::ForInitStatement::Expression(Some(expr)) => {
                    let span = extract_span_from_expr(expr);
                    let _ =
                        infer_or_error(expr, symbols, func_registry, source, &span, diagnostics);
                }
                glsl::syntax::ForInitStatement::Expression(None) => {}
            }

            if let Some(condition) = &rest.condition {
                let cond_expr = match condition {
                    glsl::syntax::Condition::Expr(expr) => expr,
                    glsl::syntax::Condition::Assignment(_, _, _) => {
                        symbols.pop_scope();
                        return;
                    }
                };
                let cond_span = extract_span_from_expr(cond_expr);
                let cond_type = infer_or_error(
                    cond_expr,
                    symbols,
                    func_registry,
                    source,
                    &cond_span,
                    diagnostics,
                );
                if !cond_type.is_error() {
                    if let Err(e) = check_condition(&cond_type) {
                        let _ = diagnostics.push(add_span_text_to_error(
                            e.with_location(source_span_to_location(&cond_span)),
                            Some(source),
                            &cond_span,
                        ));
                    }
                }
            }

            if let Some(update_expr) = &rest.post_expr {
                let span = extract_span_from_expr(update_expr);
                let _ = infer_or_error(
                    update_expr,
                    symbols,
                    func_registry,
                    source,
                    &span,
                    diagnostics,
                );
            }

            validate_statement(
                body,
                symbols,
                return_type,
                func_registry,
                source,
                diagnostics,
            );
            symbols.pop_scope();
        }
    }
}

fn validate_jump(
    jump: &JumpStatement,
    symbols: &SymbolTable,
    return_type: &Type,
    func_registry: &FunctionRegistry,
    source: &str,
    diagnostics: &mut GlslDiagnostics,
) {
    use crate::frontend::semantic::type_check::can_implicitly_convert;
    use glsl::syntax::JumpStatement;

    match jump {
        JumpStatement::Return(Some(expr)) => {
            let expr_span = extract_span_from_expr(expr);
            let expr_type = infer_or_error(
                expr,
                symbols,
                func_registry,
                source,
                &expr_span,
                diagnostics,
            );

            if !expr_type.is_error() && !can_implicitly_convert(&expr_type, return_type) {
                let error = GlslError::new(
                    ErrorCode::E0116,
                    format!(
                        "return type mismatch: expected `{return_type:?}`, found `{expr_type:?}`"
                    ),
                )
                .with_location(source_span_to_location(&expr_span))
                .with_note(format!(
                    "function returns `{return_type:?}` but expression has type `{expr_type:?}`"
                ));
                let _ = diagnostics.push(add_span_text_to_error(error, Some(source), &expr_span));
            }
        }
        JumpStatement::Return(None) => {
            if *return_type != Type::Void {
                let _ = diagnostics.push(GlslError::new(
                    ErrorCode::E0116,
                    format!("return type mismatch: expected `{return_type:?}`, found `Void`"),
                ));
            }
        }
        JumpStatement::Break | JumpStatement::Continue | JumpStatement::Discard => {}
    }
}
