//! Expression type inference for GLSL
//! Implements GLSL spec type rules for Phase 3

use crate::error::{
    ErrorCode, GlslError, extract_span_from_expr, extract_span_from_identifier,
    source_span_to_location,
};
use crate::frontend::semantic::functions::FunctionRegistry;
use crate::frontend::semantic::scope::SymbolTable;
use crate::frontend::semantic::types::Type;
use glsl::syntax::Expr;

use alloc::{format, vec::Vec};

use super::constructors::{
    check_matrix_constructor, check_scalar_constructor_with_span,
    check_vector_constructor_with_span, is_matrix_type_name, is_scalar_type_name,
    is_vector_type_name,
};
use super::conversion::can_implicitly_convert;
use super::operators::{
    infer_binary_result_type, infer_postdec_result_type, infer_postinc_result_type,
    infer_unary_result_type,
};
use super::swizzle::parse_swizzle_length;

/// Infer the result type of an expression
pub fn infer_expr_type(expr: &Expr, symbols: &SymbolTable) -> Result<Type, GlslError> {
    infer_expr_type_with_registry(expr, symbols, None)
}

/// Infer the result type of an expression with optional function registry
pub fn infer_expr_type_with_registry(
    expr: &Expr,
    symbols: &SymbolTable,
    func_registry: Option<&FunctionRegistry>,
) -> Result<Type, GlslError> {
    match expr {
        Expr::IntConst(_, _) => Ok(Type::Int),
        Expr::UIntConst(_, _) => Ok(Type::UInt),
        Expr::FloatConst(_, _) => Ok(Type::Float),
        Expr::BoolConst(_, _) => Ok(Type::Bool),
        Expr::DoubleConst(_, _) => Ok(Type::Float), // Treat as float for now

        Expr::Variable(ident, _span) => {
            let span = extract_span_from_identifier(ident);
            let var = symbols.lookup_variable(&ident.name).ok_or_else(|| {
                GlslError::undefined_variable(&ident.name)
                    .with_location(source_span_to_location(&span))
                    .with_note(format!(
                        "variable `{}` is not defined in this scope",
                        ident.name
                    ))
            })?;
            Ok(var.ty.clone())
        }

        Expr::Binary(op, lhs, rhs, span) => {
            let lhs_ty = infer_expr_type_with_registry(lhs, symbols, func_registry)?;
            let rhs_ty = infer_expr_type_with_registry(rhs, symbols, func_registry)?;
            infer_binary_result_type(op, &lhs_ty, &rhs_ty, span.clone())
        }

        Expr::PostInc(operand, span) => {
            let operand_ty = infer_expr_type_with_registry(operand, symbols, func_registry)?;
            infer_postinc_result_type(&operand_ty, span.clone())
        }

        Expr::PostDec(operand, span) => {
            let operand_ty = infer_expr_type_with_registry(operand, symbols, func_registry)?;
            infer_postdec_result_type(&operand_ty, span.clone())
        }

        Expr::Unary(op, expr, span) => {
            let expr_ty = infer_expr_type_with_registry(expr, symbols, func_registry)?;
            infer_unary_result_type(op, &expr_ty, span.clone())
        }

        Expr::Assignment(lhs, _op, _rhs, _span) => {
            // Assignment result has same type as LHS
            infer_expr_type_with_registry(lhs, symbols, func_registry)
        }

        Expr::Dot(expr, field, dot_span) => {
            // Component access (swizzle) - infer type of base expression
            let base_ty = infer_expr_type_with_registry(expr, symbols, func_registry)?;

            if !base_ty.is_vector() {
                let span = extract_span_from_expr(expr);
                return Err(GlslError::new(
                    ErrorCode::E0112,
                    format!("component access on non-vector type: {base_ty:?}"),
                )
                .with_location(source_span_to_location(&span)));
            }

            // Parse swizzle to determine result type
            let component_count = base_ty.component_count().unwrap();
            let dot_span_clone = dot_span.clone();
            let swizzle_len =
                parse_swizzle_length(&field.name, component_count).map_err(|mut e| {
                    // Add location if not already present
                    if e.location.is_none() {
                        e = e.with_location(source_span_to_location(&dot_span_clone));
                    }
                    e
                })?;
            let base_scalar_ty = base_ty.vector_base_type().unwrap();

            if swizzle_len == 1 {
                // Single component: return scalar
                Ok(base_scalar_ty)
            } else {
                // Multiple components: return vector
                Type::vector_type(&base_scalar_ty, swizzle_len).ok_or_else(|| {
                    let mut error = GlslError::new(
                        ErrorCode::E0113,
                        format!("invalid swizzle length: {swizzle_len}"),
                    );
                    if error.location.is_none() {
                        error = error.with_location(source_span_to_location(&dot_span));
                    }
                    error
                })
            }
        }

        Expr::FunCall(func_ident, args, span) => {
            // Extract function name
            let func_name = match func_ident {
                glsl::syntax::FunIdentifier::Identifier(ident) => &ident.name,
                _ => {
                    let span = extract_span_from_expr(expr);
                    return Err(GlslError::new(
                        ErrorCode::E0112,
                        "complex function identifiers not yet supported",
                    )
                    .with_location(source_span_to_location(&span)));
                }
            };

            // Infer argument types
            let mut arg_types = Vec::new();
            for arg in args {
                arg_types.push(infer_expr_type_with_registry(arg, symbols, func_registry)?);
            }

            // Check if it's a type constructor (must come before function lookup)
            if is_vector_type_name(func_name) {
                return check_vector_constructor_with_span(
                    func_name,
                    &arg_types,
                    Some(span.clone()),
                );
            }

            if is_matrix_type_name(func_name) {
                return check_matrix_constructor(func_name, &arg_types);
            }

            // Check for scalar constructors
            if is_scalar_type_name(func_name) {
                return check_scalar_constructor_with_span(
                    func_name,
                    &arg_types,
                    Some(span.clone()),
                );
            }

            // Check if it's a built-in function
            if crate::frontend::semantic::builtins::is_builtin_function(func_name) {
                match crate::frontend::semantic::builtins::check_builtin_call(func_name, &arg_types)
                {
                    Ok(return_type) => Ok(return_type),
                    Err(err_msg) => Err(GlslError::new(ErrorCode::E0114, err_msg)
                        .with_location(source_span_to_location(span))),
                }
            }
            // Check if it's an LPFX function
            else if crate::frontend::semantic::lpfx::lpfx_fn_registry::is_lpfx_fn(func_name) {
                match crate::frontend::semantic::lpfx::lpfx_fn_registry::check_lpfx_fn_call(
                    func_name, &arg_types,
                ) {
                    Ok(return_type) => Ok(return_type),
                    Err(err_msg) => Err(GlslError::new(ErrorCode::E0114, err_msg)
                        .with_location(source_span_to_location(span))),
                }
            } else if let Some(registry) = func_registry {
                // User-defined function
                let span_clone = span.clone();
                let func_sig =
                    registry
                        .lookup_function(func_name, &arg_types)
                        .map_err(|mut e| {
                            // Add location if not already present
                            if e.location.is_none() {
                                e = e.with_location(source_span_to_location(&span_clone));
                            }
                            e
                        })?;
                Ok(func_sig.return_type.clone())
            } else {
                // No function registry available - can't validate function calls
                let span = extract_span_from_expr(expr);
                Err(GlslError::new(
                    ErrorCode::E0112,
                    format!(
                        "cannot infer type for function call `{func_name}` without function registry"
                    ),
                )
                .with_location(source_span_to_location(&span)))
            }
        }

        Expr::Bracket(array_expr, array_spec, span) => {
            // Array/matrix/vector indexing: arr[i], mat[col], or vec[index]
            let array_ty = infer_expr_type_with_registry(array_expr, symbols, func_registry)?;

            use glsl::syntax::ArraySpecifierDimension;
            if array_spec.dimensions.0.is_empty() {
                return Err(
                    GlslError::new(ErrorCode::E0400, "indexing requires explicit index")
                        .with_location(source_span_to_location(span)),
                );
            }

            // Handle arrays first (before matrix/vector check)
            if array_ty.is_array() {
                // For Phase 1, only support 1D arrays
                // Check that index is int
                let index_expr = match &array_spec.dimensions.0[0] {
                    ArraySpecifierDimension::ExplicitlySized(expr) => expr,
                    ArraySpecifierDimension::Unsized => {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            "indexing requires explicit index",
                        )
                        .with_location(source_span_to_location(span)));
                    }
                };

                let index_ty = infer_expr_type_with_registry(index_expr, symbols, func_registry)?;
                if index_ty != Type::Int {
                    return Err(GlslError::new(ErrorCode::E0106, "index must be int")
                        .with_location(source_span_to_location(span)));
                }

                let element_ty = array_ty.array_element_type().unwrap();

                // If there are more dimensions and the element is a matrix/vector, continue processing
                if array_spec.dimensions.0.len() > 1
                    && (element_ty.is_matrix() || element_ty.is_vector())
                {
                    // Continue processing remaining dimensions
                    let mut current_ty = element_ty;

                    for dimension in array_spec.dimensions.0.iter().skip(1) {
                        let index_expr = match dimension {
                            ArraySpecifierDimension::ExplicitlySized(expr) => expr,
                            ArraySpecifierDimension::Unsized => {
                                return Err(GlslError::new(
                                    ErrorCode::E0400,
                                    "indexing requires explicit index",
                                )
                                .with_location(source_span_to_location(span)));
                            }
                        };

                        // Check that index is int
                        let index_ty =
                            infer_expr_type_with_registry(index_expr, symbols, func_registry)?;
                        if index_ty != Type::Int {
                            return Err(GlslError::new(ErrorCode::E0106, "index must be int")
                                .with_location(source_span_to_location(span)));
                        }

                        if current_ty.is_matrix() {
                            // Matrix indexing: mat[col] returns column vector
                            current_ty = current_ty.matrix_column_type().unwrap();
                        } else if current_ty.is_vector() {
                            // Vector indexing: vec[index] returns scalar component
                            current_ty = current_ty.vector_base_type().unwrap();
                        } else {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "cannot index into {current_ty:?} (only matrices and vectors can be indexed after array)"
                                ),
                            )
                            .with_location(source_span_to_location(span)));
                        }
                    }

                    return Ok(current_ty);
                } else {
                    // No more dimensions or element is scalar - return element type
                    return Ok(element_ty);
                }
            }

            if !array_ty.is_matrix() && !array_ty.is_vector() {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    "indexing only supported for arrays, matrices and vectors",
                )
                .with_location(source_span_to_location(span)));
            }

            // Process dimensions one at a time (for matrices/vectors)
            let mut current_ty = array_ty;

            for dimension in &array_spec.dimensions.0 {
                let index_expr = match dimension {
                    ArraySpecifierDimension::ExplicitlySized(expr) => expr,
                    ArraySpecifierDimension::Unsized => {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            "indexing requires explicit index",
                        )
                        .with_location(source_span_to_location(span)));
                    }
                };

                // Check that index is int (but don't need to evaluate it for type inference)
                let index_ty = infer_expr_type_with_registry(index_expr, symbols, func_registry)?;
                if index_ty != Type::Int {
                    return Err(GlslError::new(ErrorCode::E0106, "index must be int")
                        .with_location(source_span_to_location(span)));
                }

                if current_ty.is_matrix() {
                    // Matrix indexing: mat[col] returns column vector
                    current_ty = current_ty.matrix_column_type().unwrap();
                } else if current_ty.is_vector() {
                    // Vector indexing: vec[index] returns scalar component
                    current_ty = current_ty.vector_base_type().unwrap();
                } else {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        format!(
                            "cannot index into {current_ty:?} (only arrays, matrices and vectors can be indexed)"
                        ),
                    )
                    .with_location(source_span_to_location(span)));
                }
            }

            Ok(current_ty)
        }

        Expr::Ternary(cond, true_expr, false_expr, span) => {
            // Validate condition is scalar bool
            let cond_ty = infer_expr_type_with_registry(cond, symbols, func_registry)?;
            if cond_ty != Type::Bool {
                return Err(GlslError::new(
                    ErrorCode::E0107,
                    "ternary condition must be scalar bool type",
                )
                .with_location(source_span_to_location(span))
                .with_note(format!("condition has type `{cond_ty:?}`, expected `Bool`")));
            }

            // Infer types of both branches
            let true_ty = infer_expr_type_with_registry(true_expr, symbols, func_registry)?;
            let false_ty = infer_expr_type_with_registry(false_expr, symbols, func_registry)?;

            // Determine result type: exact match or implicit conversion
            if true_ty == false_ty {
                // Exact match
                Ok(true_ty)
            } else if can_implicitly_convert(&true_ty, &false_ty) {
                // true_ty can convert to false_ty, use false_ty as result
                Ok(false_ty)
            } else if can_implicitly_convert(&false_ty, &true_ty) {
                // false_ty can convert to true_ty, use true_ty as result
                Ok(true_ty)
            } else {
                // No conversion possible
                Err(GlslError::new(
                    ErrorCode::E0106,
                    "ternary operator branches have incompatible types",
                )
                .with_location(source_span_to_location(span))
                .with_note(format!(
                    "true branch has type `{true_ty:?}`, false branch has type `{false_ty:?}`"
                ))
                .with_note("branches must have matching types or allow implicit conversion"))
            }
        }

        _ => {
            let span = extract_span_from_expr(expr);
            Err(GlslError::new(
                ErrorCode::E0112,
                format!("cannot infer type for expression: {expr:?}"),
            )
            .with_location(source_span_to_location(&span)))
        }
    }
}

/// Extract the return expression from a parsed shader
/// Helper function similar to the one in glsl_value.rs
fn extract_return_expr_from_shader(shader: &glsl::syntax::TranslationUnit) -> Option<&Expr> {
    for decl in &shader.0 {
        if let glsl::syntax::ExternalDeclaration::FunctionDefinition(func) = decl {
            use glsl::syntax::{JumpStatement, SimpleStatement, Statement};
            for stmt in &func.statement.statement_list {
                if let Statement::Simple(simple_stmt) = stmt {
                    if let SimpleStatement::Jump(JumpStatement::Return(Some(ref expr))) =
                        **simple_stmt
                    {
                        return Some(expr);
                    }
                }
            }
        }
    }
    None
}

/// Infer type of expression within a program context
/// Parses an expression string and infers its type using the provided function registry
pub fn infer_expr_type_in_context(
    expr_str: &str,
    function_registry: &FunctionRegistry,
) -> Result<Type, GlslError> {
    // Wrap the expression in a minimal function to parse it
    // We'll try different return types to see which one parses successfully
    let wrappers = [
        format!("int main() {{ return {expr_str}; }}"),
        format!("uint main() {{ return {expr_str}; }}"),
        format!("float main() {{ return {expr_str}; }}"),
        format!("bool main() {{ return {expr_str}; }}"),
    ];

    // Use empty symbol table since we're only parsing expressions (no variables)
    let symbols = SymbolTable::new();

    for wrapper in &wrappers {
        if let Ok(shader) = glsl::parser::Parse::parse(wrapper) {
            // Extract the return statement expression and infer type immediately
            if let Some(expr) = extract_return_expr_from_shader(&shader) {
                // Infer type using the function registry
                return infer_expr_type_with_registry(expr, &symbols, Some(function_registry));
            }
        }
    }

    Err(GlslError::new(
        ErrorCode::E0001,
        format!("failed to parse expression: `{expr_str}`"),
    ))
}
