//! Semantic validation for GLSL shaders.
//!
//! This module validates function bodies, variable declarations, expressions,
//! and return statements to ensure they are semantically correct before codegen.

use crate::error::{
    ErrorCode, GlslError, add_span_text_to_error, extract_span_from_expr, source_span_to_location,
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

/// Validate a function body, checking all statements and expressions.
pub fn validate_function(
    func: &crate::frontend::semantic::TypedFunction,
    func_registry: &FunctionRegistry,
    source: &str,
) -> Result<(), GlslError> {
    let mut symbols = SymbolTable::new();

    // Add function parameters to symbol table
    for param in &func.parameters {
        symbols.declare_variable(param.name.clone(), param.ty.clone(), StorageClass::Local)?;
    }

    // Validate all statements
    for stmt in &func.body {
        validate_statement(stmt, &mut symbols, &func.return_type, func_registry, source)?;
    }

    Ok(())
}

/// Validate a statement and update the symbol table.
fn validate_statement(
    stmt: &Statement,
    symbols: &mut SymbolTable,
    return_type: &Type,
    func_registry: &FunctionRegistry,
    source: &str,
) -> Result<(), GlslError> {
    match stmt {
        Statement::Simple(simple) => {
            validate_simple_statement(simple, symbols, return_type, func_registry, source)
        }
        Statement::Compound(compound) => {
            symbols.push_scope();
            for stmt in &compound.statement_list {
                validate_statement(stmt, symbols, return_type, func_registry, source)?;
            }
            symbols.pop_scope();
            Ok(())
        }
    }
}

/// Validate a simple statement.
fn validate_simple_statement(
    stmt: &SimpleStatement,
    symbols: &mut SymbolTable,
    return_type: &Type,
    func_registry: &FunctionRegistry,
    source: &str,
) -> Result<(), GlslError> {
    use glsl::syntax::SimpleStatement;

    match stmt {
        SimpleStatement::Declaration(decl) => {
            validate_declaration(decl, symbols, func_registry, source)
        }
        SimpleStatement::Expression(Some(expr)) => {
            // Expression statement - just validate the expression
            let expr_span = extract_span_from_expr(expr);
            infer_expr_type_with_registry(expr, symbols, Some(func_registry)).map_err(|e| {
                if e.span_text.is_none() {
                    add_span_text_to_error(e, Some(source), &expr_span)
                } else {
                    e
                }
            })?;
            Ok(())
        }
        SimpleStatement::Expression(None) => Ok(()), // Empty statement
        SimpleStatement::Selection(selection) => {
            validate_selection(selection, symbols, return_type, func_registry, source)
        }
        SimpleStatement::Iteration(iteration) => {
            validate_iteration(iteration, symbols, return_type, func_registry, source)
        }
        SimpleStatement::Jump(jump) => {
            validate_jump(jump, symbols, return_type, func_registry, source)
        }
        _ => Err(GlslError::new(
            ErrorCode::E0400,
            format!("unsupported statement type in validation: {stmt:?}"),
        )),
    }
}

/// Validate a variable declaration.
fn validate_declaration(
    decl: &glsl::syntax::Declaration,
    symbols: &mut SymbolTable,
    func_registry: &FunctionRegistry,
    source: &str,
) -> Result<(), GlslError> {
    match decl {
        glsl::syntax::Declaration::InitDeclaratorList(list) => {
            // Get base type from type specifier (for tail declarations)
            let base_ty = type_resolver::parse_return_type(&list.head.ty, None)?;

            // Handle the head declaration
            if let Some(name) = &list.head.name {
                let name_span = name.span.clone();

                // Parse complete type including array specifier from SingleDeclaration
                let ty = type_resolver::parse_head_declarator_type(list, &name_span)?;

                symbols
                    .declare_variable(name.name.clone(), ty.clone(), StorageClass::Local)
                    .map_err(|e| e.with_location(source_span_to_location(&name_span)))?;

                // Validate initializer if present
                if let Some(init) = &list.head.initializer {
                    validate_initializer(init, &ty, symbols, func_registry, source)?;
                }
            }

            // Handle tail declarations (same type, different names)
            for declarator in &list.tail {
                let name_span = declarator.ident.ident.span.clone();

                // Parse complete type including array specifier from ArrayedIdentifier
                let declarator_ty =
                    type_resolver::parse_tail_declarator_type(&base_ty, declarator)?;

                symbols
                    .declare_variable(
                        declarator.ident.ident.name.clone(),
                        declarator_ty.clone(),
                        StorageClass::Local,
                    )
                    .map_err(|e| e.with_location(source_span_to_location(&name_span)))?;

                if let Some(init) = &declarator.initializer {
                    validate_initializer(init, &declarator_ty, symbols, func_registry, source)?;
                }
            }

            Ok(())
        }
        glsl::syntax::Declaration::Precision(_, _) => {
            // Precision qualifiers are ignored in our implementation
            Ok(())
        }
        glsl::syntax::Declaration::FunctionPrototype(_) => {
            // Function prototypes are handled separately
            Ok(())
        }
        glsl::syntax::Declaration::Block(_) => {
            // Block declarations not yet supported in validation
            Ok(())
        }
        glsl::syntax::Declaration::Global(_, _) => {
            // Global declarations not yet supported in validation
            Ok(())
        }
    }
}

/// Validate an initializer expression.
fn validate_initializer(
    init: &glsl::syntax::Initializer,
    declared_type: &Type,
    symbols: &SymbolTable,
    func_registry: &FunctionRegistry,
    source: &str,
) -> Result<(), GlslError> {
    use glsl::syntax::Initializer;

    match init {
        Initializer::Simple(expr) => {
            let init_span = extract_span_from_expr(expr.as_ref());
            let init_type =
                infer_expr_type_with_registry(expr.as_ref(), symbols, Some(func_registry))
                    .map_err(|e| {
                        if e.span_text.is_none() {
                            add_span_text_to_error(e, Some(source), &init_span)
                        } else {
                            e
                        }
                    })?;
            check_assignment_with_span(declared_type, &init_type, Some(init_span.clone()))
                .map_err(|e| add_span_text_to_error(e, Some(source), &init_span))?;
            Ok(())
        }
        _ => {
            // Complex initializers (arrays, etc.) not yet supported in validation
            Ok(())
        }
    }
}

/// Validate a selection statement (if/else).
fn validate_selection(
    selection: &glsl::syntax::SelectionStatement,
    symbols: &mut SymbolTable,
    return_type: &Type,
    func_registry: &FunctionRegistry,
    source: &str,
) -> Result<(), GlslError> {
    use glsl::syntax::SelectionRestStatement;

    // Validate condition
    let cond_span = extract_span_from_expr(&selection.cond);
    let cond_type = infer_expr_type_with_registry(&selection.cond, symbols, Some(func_registry))
        .map_err(|e| {
            if e.span_text.is_none() {
                add_span_text_to_error(e, Some(source), &cond_span)
            } else {
                e
            }
        })?;
    check_condition(&cond_type).map_err(|e| {
        let error = e.with_location(source_span_to_location(&cond_span));
        add_span_text_to_error(error, Some(source), &cond_span)
    })?;

    // Validate then/else branches
    match &selection.rest {
        SelectionRestStatement::Statement(then_stmt) => {
            validate_statement(then_stmt, symbols, return_type, func_registry, source)?;
        }
        SelectionRestStatement::Else(then_stmt, else_stmt) => {
            validate_statement(then_stmt, symbols, return_type, func_registry, source)?;
            validate_statement(else_stmt, symbols, return_type, func_registry, source)?;
        }
    }

    Ok(())
}

/// Validate an iteration statement (for/while).
fn validate_iteration(
    iteration: &glsl::syntax::IterationStatement,
    symbols: &mut SymbolTable,
    return_type: &Type,
    func_registry: &FunctionRegistry,
    source: &str,
) -> Result<(), GlslError> {
    use glsl::syntax::IterationStatement;

    match iteration {
        IterationStatement::While(condition, stmt) => {
            // Validate condition
            let cond_expr = match condition {
                glsl::syntax::Condition::Expr(expr) => expr.as_ref(),
                glsl::syntax::Condition::Assignment(_, _, _) => {
                    // Assignment in condition not yet supported in validation
                    return Ok(());
                }
            };
            let cond_span = extract_span_from_expr(cond_expr);
            let cond_type = infer_expr_type_with_registry(cond_expr, symbols, Some(func_registry))?;
            check_condition(&cond_type)
                .map_err(|e| e.with_location(source_span_to_location(&cond_span)))?;

            // Validate body
            symbols.push_scope();
            validate_statement(stmt, symbols, return_type, func_registry, source)?;
            symbols.pop_scope();
            Ok(())
        }
        IterationStatement::DoWhile(stmt, cond_expr) => {
            // Validate body first
            symbols.push_scope();
            validate_statement(stmt, symbols, return_type, func_registry, source)?;
            symbols.pop_scope();

            // Validate condition (DoWhile takes Expr directly)
            let cond_span = extract_span_from_expr(cond_expr.as_ref());
            let cond_type =
                infer_expr_type_with_registry(cond_expr.as_ref(), symbols, Some(func_registry))?;
            check_condition(&cond_type)
                .map_err(|e| e.with_location(source_span_to_location(&cond_span)))?;
            Ok(())
        }
        IterationStatement::For(init, rest, body) => {
            // Validate init (if present)
            symbols.push_scope();
            match init {
                glsl::syntax::ForInitStatement::Declaration(decl) => {
                    validate_declaration(decl, symbols, func_registry, source)?;
                }
                glsl::syntax::ForInitStatement::Expression(Some(expr)) => {
                    infer_expr_type_with_registry(expr, symbols, Some(func_registry))?;
                }
                glsl::syntax::ForInitStatement::Expression(None) => {
                    // Empty init
                }
            }

            // Validate condition (if present)
            if let Some(condition) = &rest.condition {
                let cond_expr = match condition {
                    glsl::syntax::Condition::Expr(expr) => expr,
                    glsl::syntax::Condition::Assignment(_, _, _) => {
                        // Assignment in condition not yet supported in validation
                        return Ok(());
                    }
                };
                let cond_span = extract_span_from_expr(cond_expr);
                let cond_type =
                    infer_expr_type_with_registry(cond_expr, symbols, Some(func_registry))?;
                check_condition(&cond_type)
                    .map_err(|e| e.with_location(source_span_to_location(&cond_span)))?;
            }

            // Validate update (if present)
            if let Some(update_expr) = &rest.post_expr {
                infer_expr_type_with_registry(update_expr, symbols, Some(func_registry))?;
            }

            // Validate body
            validate_statement(body, symbols, return_type, func_registry, source)?;
            symbols.pop_scope();
            Ok(())
        }
    }
}

/// Validate a jump statement (return/break/continue).
fn validate_jump(
    jump: &JumpStatement,
    symbols: &SymbolTable,
    return_type: &Type,
    func_registry: &FunctionRegistry,
    source: &str,
) -> Result<(), GlslError> {
    use crate::frontend::semantic::type_check::can_implicitly_convert;
    use glsl::syntax::JumpStatement;

    match jump {
        JumpStatement::Return(Some(expr)) => {
            // Validate return expression type matches function return type
            let expr_span = extract_span_from_expr(expr);
            let expr_type = infer_expr_type_with_registry(expr, symbols, Some(func_registry))
                .map_err(|e| {
                    if e.span_text.is_none() {
                        add_span_text_to_error(e, Some(source), &expr_span)
                    } else {
                        e
                    }
                })?;

            if !can_implicitly_convert(&expr_type, return_type) {
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
                return Err(add_span_text_to_error(error, Some(source), &expr_span));
            }
            Ok(())
        }
        JumpStatement::Return(None) => {
            // Validate that function return type is Void
            if *return_type != Type::Void {
                return Err(GlslError::new(
                    ErrorCode::E0116,
                    format!("return type mismatch: expected `{return_type:?}`, found `Void`"),
                ));
            }
            Ok(())
        }
        JumpStatement::Break | JumpStatement::Continue => {
            // Break/continue validation requires loop context, handled elsewhere
            Ok(())
        }
        JumpStatement::Discard => {
            // Discard is valid in fragment shaders
            Ok(())
        }
    }
}
