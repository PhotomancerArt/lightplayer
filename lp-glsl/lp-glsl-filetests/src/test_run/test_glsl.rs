//! Generating isolated test GLSL code (includes function filtering).

use anyhow::Result;
use glsl::syntax::{CompoundStatement, Expr, SimpleStatement, Statement};
use lp_glsl_compiler::frontend::CompilationPipeline;
use std::collections::{HashMap, HashSet, VecDeque};

/// Result of test GLSL code generation.
pub struct TestGlslResult {
    /// The generated test GLSL source code (function definitions only, no main wrapper).
    pub source: String,
}

/// Known GLSL return types that can be inferred.
const KNOWN_TYPES: &[&str] = &[
    "void", "float", "int", "uint", "bool", "vec2", "vec3", "vec4", "ivec2", "ivec3", "ivec4",
    "bvec2", "bvec3", "bvec4", "uvec2", "uvec3", "uvec4", "mat2", "mat3", "mat4",
];

/// Generate test GLSL code with only the function under test and its call graph dependencies.
/// No main() wrapper is generated - functions are called directly.
pub fn generate_test_glsl(
    file_lines: &[String],
    directive_line_number: usize,
    expression_str: &str,
) -> Result<TestGlslResult> {
    // Extract all GLSL code that appears before the directive line
    let full_function_code = extract_code_before_directive(file_lines, directive_line_number)?;

    // Extract function name from expression
    let func_name = extract_function_name(expression_str)?;

    // Try to filter functions using call graph analysis
    // If parsing/filtering fails, fall back to including all functions
    let function_code = match CompilationPipeline::parse(&full_function_code) {
        Ok(parse_result) => {
            match glsl_for_fn_graph(&parse_result.shader, &full_function_code, &func_name) {
                Ok(filtered) if !filtered.is_empty() => filtered,
                _ => full_function_code.clone(), // Fallback to all functions
            }
        }
        Err(_) => full_function_code.clone(), // Fallback to all functions if parsing fails
    };

    // Return only the filtered function code (no main wrapper)
    Ok(TestGlslResult {
        source: function_code,
    })
}

/// Extract all GLSL code from the file.
/// This includes all function definitions and code, excluding directive lines.
fn extract_code_before_directive(
    file_lines: &[String],
    _directive_line_number: usize,
) -> Result<String> {
    let mut glsl_code = String::new();

    // Extract all lines from the file
    for line in file_lines.iter() {
        let trimmed = line.trim();

        // Skip directive lines (test, target, run directives)
        if trimmed.starts_with("// test")
            || trimmed.starts_with("// target")
            || trimmed.starts_with("// #run:")
            || trimmed.starts_with("// run:")
        {
            continue;
        }

        // Include all other lines (GLSL code)
        glsl_code.push_str(line);
        glsl_code.push('\n');
    }

    Ok(glsl_code.trim_end().to_string())
}

/// Infer the return type of an expression by looking up the called function's return type.
pub fn infer_return_type_from_expression(source: &str, expression: &str) -> Result<String> {
    // Extract function name from expression (e.g., "add_float(0.0, 0.0)" -> "add_float")
    let func_name = extract_function_name(expression)?;

    // First try: Search for function definition in source
    if let Some(return_type) = find_return_type_in_source(source, &func_name)? {
        return Ok(return_type);
    }

    // Fallback: Use heuristics based on function name or expression content
    Ok(infer_type_from_heuristics(&func_name, expression))
}

/// Extract function name from an expression string.
fn extract_function_name(expression: &str) -> Result<String> {
    if !expression.contains('(') {
        return Err(anyhow::anyhow!(
            "invalid expression: no opening parenthesis"
        ));
    }
    expression
        .split('(')
        .next()
        .map(|s| s.trim().to_string())
        .ok_or_else(|| anyhow::anyhow!("invalid expression: no opening parenthesis"))
}

/// Search for function definition in source and extract its return type.
/// Returns an error if the return type is not in KNOWN_TYPES.
fn find_return_type_in_source(source: &str, func_name: &str) -> Result<Option<String>> {
    let pattern = format!("{func_name}(");
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(pos) = trimmed.find(&pattern) {
            // Extract the part before the function name
            let before_func = &trimmed[..pos].trim();
            // The return type should be the last word before the function name
            if let Some(return_type) = before_func.split_whitespace().last() {
                // Validate it's a known type
                if KNOWN_TYPES.contains(&return_type) {
                    return Ok(Some(return_type.to_string()));
                } else {
                    // Error if we found a return type that's not in KNOWN_TYPES
                    return Err(anyhow::anyhow!(
                        "unknown return type '{return_type}' for function '{func_name}'. Known types: {KNOWN_TYPES:?}. \
                         Please add '{return_type}' to KNOWN_TYPES in test_glsl.rs"
                    ));
                }
            }
        }
    }
    Ok(None)
}

/// Infer return type using heuristics based on function name or expression content.
fn infer_type_from_heuristics(func_name: &str, expression: &str) -> String {
    // Check for float indicators
    if func_name.contains("float") || expression.contains('.') {
        return "float".to_string();
    }

    // Check for uint indicators
    if func_name.contains("uint") {
        return "uint".to_string();
    }

    // Check for vector/matrix types in function name
    if func_name.contains("vec") || func_name.contains("mat") {
        // Check for boolean vectors first (bvec2, bvec3, bvec4)
        if func_name.contains("bvec2") {
            return "bvec2".to_string();
        } else if func_name.contains("bvec3") {
            return "bvec3".to_string();
        } else if func_name.contains("bvec4") {
            return "bvec4".to_string();
        }
        // Check for unsigned integer vectors (uvec2, uvec3, uvec4)
        else if func_name.contains("uvec2") {
            return "uvec2".to_string();
        } else if func_name.contains("uvec3") {
            return "uvec3".to_string();
        } else if func_name.contains("uvec4") {
            return "uvec4".to_string();
        }
        // Check for signed integer vectors (ivec2, ivec3, ivec4)
        else if func_name.contains("ivec2") {
            return "ivec2".to_string();
        } else if func_name.contains("ivec3") {
            return "ivec3".to_string();
        } else if func_name.contains("ivec4") {
            return "ivec4".to_string();
        }
        // Check for float vectors (vec2, vec3, vec4)
        else if func_name.contains("vec2") {
            return "vec2".to_string();
        } else if func_name.contains("vec3") {
            return "vec3".to_string();
        } else if func_name.contains("vec4") {
            return "vec4".to_string();
        }
        // Check for matrices
        else if func_name.contains("mat2") {
            return "mat2".to_string();
        } else if func_name.contains("mat3") {
            return "mat3".to_string();
        } else if func_name.contains("mat4") {
            return "mat4".to_string();
        }
        // Default to float for generic vec/mat without specific dimension
        return "float".to_string();
    }

    // Default to int
    "int".to_string()
}

// Function filtering code (merged from function_filter.rs)

/// Extract GLSL source containing only the specified function and its call graph dependencies.
///
/// Given a parsed GLSL AST and source string, this function:
/// 1. Builds a call graph by traversing all function bodies
/// 2. Finds all functions reachable from the starting function
/// 3. Extracts and returns only those function definitions from the source
///
/// # Arguments
/// * `ast` - The parsed GLSL translation unit
/// * `source` - The original GLSL source code
/// * `fn_name` - The name of the function to start the call graph traversal from
///
/// # Returns
/// Filtered GLSL source containing only the reachable functions
fn glsl_for_fn_graph(
    ast: &glsl::syntax::TranslationUnit,
    source: &str,
    fn_name: &str,
) -> Result<String> {
    // Step 1: Extract all function definitions and build call graph
    let mut function_defs: HashMap<String, &glsl::syntax::FunctionDefinition> = HashMap::new();
    let mut call_graph: HashMap<String, HashSet<String>> = HashMap::new();

    for decl in &ast.0 {
        if let glsl::syntax::ExternalDeclaration::FunctionDefinition(func) = decl {
            let func_name = func.prototype.name.name.clone();
            function_defs.insert(func_name.clone(), func);
            call_graph.insert(func_name.clone(), HashSet::new());
        }
    }

    // Step 2: Build call graph by traversing function bodies
    for (func_name, func_def) in &function_defs {
        // Function body is a CompoundStatement, wrap it in Statement::Compound
        let stmt = glsl::syntax::Statement::Compound(Box::new(func_def.statement.clone()));
        let callees = extract_function_calls(&stmt);
        if let Some(calls) = call_graph.get_mut(func_name) {
            *calls = callees;
        }
    }

    // Step 3: Find all reachable functions from the starting function
    let reachable = find_reachable_functions(fn_name, &call_graph, &function_defs);

    // Step 4: Extract function definitions from source using AST spans
    extract_functions_from_source_using_ast(ast, source, &reachable)
}

/// Recursively extract all function calls from a statement.
fn extract_function_calls(stmt: &Statement) -> HashSet<String> {
    let mut calls = HashSet::new();
    extract_function_calls_from_stmt(stmt, &mut calls);
    calls
}

/// Recursively traverse statements to find function calls.
fn extract_function_calls_from_stmt(stmt: &Statement, calls: &mut HashSet<String>) {
    match stmt {
        Statement::Simple(simple) => {
            extract_function_calls_from_simple_stmt(simple, calls);
        }
        Statement::Compound(compound) => {
            extract_function_calls_from_compound(compound, calls);
        }
    }
}

/// Extract function calls from a simple statement.
fn extract_function_calls_from_simple_stmt(stmt: &SimpleStatement, calls: &mut HashSet<String>) {
    match stmt {
        SimpleStatement::Expression(Some(expr)) => {
            extract_function_calls_from_expr(expr, calls);
        }
        SimpleStatement::Selection(selection) => {
            extract_function_calls_from_expr(&selection.cond, calls);
            match &selection.rest {
                glsl::syntax::SelectionRestStatement::Statement(then_stmt) => {
                    extract_function_calls_from_stmt(then_stmt, calls);
                }
                glsl::syntax::SelectionRestStatement::Else(then_stmt, else_stmt) => {
                    extract_function_calls_from_stmt(then_stmt, calls);
                    extract_function_calls_from_stmt(else_stmt, calls);
                }
            }
        }
        SimpleStatement::Iteration(iteration) => match iteration {
            glsl::syntax::IterationStatement::While(condition, body) => {
                extract_function_calls_from_condition(condition, calls);
                extract_function_calls_from_stmt(body, calls);
            }
            glsl::syntax::IterationStatement::DoWhile(body, expr) => {
                extract_function_calls_from_stmt(body, calls);
                extract_function_calls_from_expr(expr, calls);
            }
            glsl::syntax::IterationStatement::For(init, rest, body) => {
                extract_function_calls_from_for_init(init, calls);
                extract_function_calls_from_for_rest(rest, calls);
                extract_function_calls_from_stmt(body, calls);
            }
        },
        SimpleStatement::Jump(jump) => {
            if let glsl::syntax::JumpStatement::Return(Some(expr)) = jump {
                extract_function_calls_from_expr(expr, calls);
            }
        }
        SimpleStatement::Declaration(decl) => {
            // Check for initializers in declarations
            // decl is Box<Declaration>, dereference it
            match decl {
                glsl::syntax::Declaration::InitDeclaratorList(list) => {
                    if let Some(init) = &list.head.initializer {
                        extract_function_calls_from_initializer(init, calls);
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
}

/// Extract function calls from a compound statement.
fn extract_function_calls_from_compound(compound: &CompoundStatement, calls: &mut HashSet<String>) {
    for stmt in &compound.statement_list {
        extract_function_calls_from_stmt(stmt, calls);
    }
}

/// Recursively extract function calls from an expression.
fn extract_function_calls_from_expr(expr: &Expr, calls: &mut HashSet<String>) {
    match expr {
        Expr::FunCall(func_ident, args, _) => {
            // Extract function name
            if let glsl::syntax::FunIdentifier::Identifier(ident) = func_ident {
                let func_name = ident.name.clone();
                // Filter out built-ins and type constructors (simple heuristic)
                if !is_builtin_or_constructor(&func_name) {
                    calls.insert(func_name);
                }
            }
            // Also check arguments for nested function calls
            for arg in args {
                extract_function_calls_from_expr(arg, calls);
            }
        }
        Expr::Binary(_, left, right, _) => {
            extract_function_calls_from_expr(left, calls);
            extract_function_calls_from_expr(right, calls);
        }
        Expr::Unary(_, operand, _) => {
            extract_function_calls_from_expr(operand, calls);
        }
        Expr::Ternary(cond, then_expr, else_expr, _) => {
            extract_function_calls_from_expr(cond, calls);
            extract_function_calls_from_expr(then_expr, calls);
            extract_function_calls_from_expr(else_expr, calls);
        }
        Expr::Assignment(_, _lvalue, rvalue, _) => {
            extract_function_calls_from_expr(rvalue, calls);
        }
        Expr::Bracket(expr, _, _) => {
            extract_function_calls_from_expr(expr, calls);
            // ArraySpecifier doesn't contain function calls, skip it
        }
        Expr::Dot(expr, _, _) => {
            extract_function_calls_from_expr(expr, calls);
        }
        Expr::PostInc(expr, _) | Expr::PostDec(expr, _) => {
            extract_function_calls_from_expr(expr, calls);
        }
        _ => {}
    }
}

/// Extract function calls from an initializer.
fn extract_function_calls_from_initializer(
    init: &glsl::syntax::Initializer,
    calls: &mut HashSet<String>,
) {
    match init {
        glsl::syntax::Initializer::Simple(expr) => {
            extract_function_calls_from_expr(expr, calls);
        }
        glsl::syntax::Initializer::List(list) => {
            for item in list {
                extract_function_calls_from_initializer(item, calls);
            }
        }
    }
}

/// Extract function calls from a condition.
fn extract_function_calls_from_condition(
    condition: &glsl::syntax::Condition,
    calls: &mut HashSet<String>,
) {
    match condition {
        glsl::syntax::Condition::Expr(expr) => {
            extract_function_calls_from_expr(expr, calls);
        }
        glsl::syntax::Condition::Assignment(_, _, initializer) => {
            extract_function_calls_from_initializer(initializer, calls);
        }
    }
}

/// Extract function calls from a for init statement.
fn extract_function_calls_from_for_init(
    init: &glsl::syntax::ForInitStatement,
    calls: &mut HashSet<String>,
) {
    match init {
        glsl::syntax::ForInitStatement::Expression(Some(expr)) => {
            extract_function_calls_from_expr(expr, calls);
        }
        glsl::syntax::ForInitStatement::Declaration(decl) => {
            // decl is Box<Declaration>, dereference it
            match &**decl {
                glsl::syntax::Declaration::InitDeclaratorList(list) => {
                    if let Some(init) = &list.head.initializer {
                        extract_function_calls_from_initializer(init, calls);
                    }
                }
                _ => {}
            }
        }
        glsl::syntax::ForInitStatement::Expression(None) => {}
    }
}

/// Extract function calls from a for rest statement.
fn extract_function_calls_from_for_rest(
    rest: &glsl::syntax::ForRestStatement,
    calls: &mut HashSet<String>,
) {
    if let Some(ref cond) = rest.condition {
        extract_function_calls_from_condition(cond, calls);
    }
    if let Some(ref post_expr) = rest.post_expr {
        extract_function_calls_from_expr(post_expr, calls);
    }
}

/// Check if a function name is a built-in or type constructor.
fn is_builtin_or_constructor(name: &str) -> bool {
    // Type constructors
    matches!(
        name,
        "vec2"
            | "vec3"
            | "vec4"
            | "ivec2"
            | "ivec3"
            | "ivec4"
            | "uvec2"
            | "uvec3"
            | "uvec4"
            | "bvec2"
            | "bvec3"
            | "bvec4"
            | "mat2"
            | "mat3"
            | "mat4"
            | "float"
            | "int"
            | "uint"
            | "bool"
    ) || name.starts_with("gl_") // Built-in GLSL functions/variables
}

/// Find all functions reachable from a starting function using BFS.
fn find_reachable_functions(
    start_func: &str,
    call_graph: &HashMap<String, HashSet<String>>,
    function_defs: &HashMap<String, &glsl::syntax::FunctionDefinition>,
) -> HashSet<String> {
    let mut reachable = HashSet::new();

    // If the starting function doesn't exist, return empty set
    if !function_defs.contains_key(start_func) {
        return reachable;
    }

    let mut queue = VecDeque::new();
    queue.push_back(start_func.to_string());
    reachable.insert(start_func.to_string());

    while let Some(current) = queue.pop_front() {
        if let Some(callees) = call_graph.get(&current) {
            for callee in callees {
                if !reachable.contains(callee) && function_defs.contains_key(callee) {
                    reachable.insert(callee.clone());
                    queue.push_back(callee.clone());
                }
            }
        }
    }

    reachable
}

/// Extract function definitions from source using AST spans.
/// This preserves comments and whitespace perfectly.
fn extract_functions_from_source_using_ast(
    ast: &glsl::syntax::TranslationUnit,
    source: &str,
    function_names: &HashSet<String>,
) -> Result<String> {
    if function_names.is_empty() {
        return Ok(String::new());
    }

    let source_lines: Vec<&str> = source.lines().collect();
    let mut function_ranges: Vec<(usize, usize)> = Vec::new(); // (start_line, end_line) 1-indexed

    // Extract line ranges for each function we want to keep using spans
    // Iterate through AST to preserve order
    for decl in &ast.0 {
        if let glsl::syntax::ExternalDeclaration::FunctionDefinition(func_def) = decl {
            let func_name = func_def.prototype.name.name.clone();

            if !function_names.contains(&func_name) {
                continue;
            }

            let span = &func_def.span;

            // Skip if span is unknown
            if span.is_unknown() {
                continue;
            }

            // Calculate end byte position: offset + len
            let end_offset = span.offset + span.len;

            // Convert byte offsets to line numbers
            let start_line = byte_offset_to_line(source, span.offset)?;
            let end_line = byte_offset_to_line(source, end_offset.saturating_sub(1))?;

            function_ranges.push((start_line, end_line));
        }
    }

    // Sort by start line to maintain order
    function_ranges.sort_by_key(|(start, _)| *start);

    // Extract the functions from source
    let mut result = String::new();
    let mut extracted_lines = HashSet::new();

    for (start, end) in &function_ranges {
        // Convert from 1-indexed to 0-indexed
        let start_idx = start.saturating_sub(1);
        let end_idx = end.saturating_sub(1);

        for i in start_idx..=end_idx.min(source_lines.len().saturating_sub(1)) {
            extracted_lines.insert(i);
        }
    }

    // Build result, preserving original formatting including comments and whitespace
    for (i, line) in source_lines.iter().enumerate() {
        if extracted_lines.contains(&i) {
            result.push_str(line);
            result.push('\n');
        }
    }

    Ok(result.trim().to_string())
}

/// Convert a byte offset in source to a line number (1-indexed).
fn byte_offset_to_line(source: &str, offset: usize) -> Result<usize> {
    if offset >= source.len() {
        // If offset is at or past end, return the last line
        return Ok(source.lines().count().max(1));
    }

    // Count newlines before this offset
    let before_offset = &source[..offset];
    let line_num = before_offset.chars().filter(|&c| c == '\n').count() + 1;
    Ok(line_num)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_function_name() {
        assert_eq!(extract_function_name("add(1, 2)").unwrap(), "add");
        assert_eq!(extract_function_name("test()").unwrap(), "test");
        assert_eq!(
            extract_function_name("nested(vec2(1.0, 2.0))").unwrap(),
            "nested"
        );
    }

    #[test]
    fn test_extract_function_name_invalid() {
        assert!(extract_function_name("no_parens").is_err());
        assert!(extract_function_name("").is_err());
    }

    #[test]
    fn test_simple_call_graph() {
        let source = r#"
float add(float a, float b) {
    return a + b;
}

float multiply(float a, float b) {
    return a * b;
}

float test() {
    return multiply(add(1.0, 2.0), 3.0);
}
"#;

        let parse_result = CompilationPipeline::parse(source).unwrap();
        let filtered = glsl_for_fn_graph(&parse_result.shader, source, "test").unwrap();

        // Should include test, multiply, and add
        assert!(filtered.contains("test"));
        assert!(filtered.contains("multiply"));
        assert!(filtered.contains("add"));
    }

    #[test]
    fn test_nested_calls() {
        let source = r#"
float a() { return 1.0; }
float b() { return a() + 1.0; }
float c() { return b() + 1.0; }
float d() { return 4.0; }
float test() { return c(); }
"#;

        let parse_result = CompilationPipeline::parse(source).unwrap();
        let filtered = glsl_for_fn_graph(&parse_result.shader, source, "test").unwrap();

        // Should include test, c, b, a (but not d)
        assert!(filtered.contains("test"));
        assert!(filtered.contains("c"));
        assert!(filtered.contains("b"));
        assert!(filtered.contains("a"));
        assert!(!filtered.contains("d"));
    }

    #[test]
    fn test_missing_function() {
        let source = r#"
float test() { return 1.0; }
"#;

        let parse_result = CompilationPipeline::parse(source).unwrap();
        let filtered = glsl_for_fn_graph(&parse_result.shader, source, "nonexistent").unwrap();

        // Should return empty or just the function itself
        assert!(filtered.is_empty() || !filtered.contains("test"));
    }
}
