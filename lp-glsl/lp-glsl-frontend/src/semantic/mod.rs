use crate::error::GlslDiagnostics;
use glsl::syntax::TranslationUnit;
use passes::SemanticPass;

use alloc::string::String;
use alloc::vec::Vec;

pub mod builtins;
pub mod const_eval;
pub mod functions;
pub mod lpfx;
pub mod passes;
pub mod scope;
pub mod type_check;
pub mod type_resolver;
pub mod types;
pub mod validator;

/// Name of the main entry point function in GLSL shaders
pub const MAIN_FUNCTION_NAME: &str = "main";

pub struct TypedShader {
    pub main_function: Option<TypedFunction>,
    pub user_functions: Vec<TypedFunction>,
    pub function_registry: functions::FunctionRegistry,
    /// Global const declarations (name -> evaluated value).
    pub global_constants: hashbrown::HashMap<alloc::string::String, const_eval::ConstValue>,
}

pub struct TypedFunction {
    pub name: String,
    pub return_type: types::Type,
    pub parameters: Vec<functions::Parameter>,
    pub body: Vec<glsl::syntax::Statement>,
}

impl TypedFunction {
    /// Recursive count of AST statement nodes. Used as a heuristic for
    /// function size when ordering compilation (smallest first).
    pub fn ast_node_count(&self) -> usize {
        self.body.iter().map(count_statement_nodes).sum()
    }
}

/// Semantic analyzer that orchestrates semantic analysis passes
pub struct SemanticAnalyzer {
    #[allow(dead_code, reason = "Function registry stored for future use")]
    registry: functions::FunctionRegistry,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self {
            registry: functions::FunctionRegistry::new(),
        }
    }

    pub fn analyze(
        &mut self,
        shader: &TranslationUnit,
        source: &str,
        max_errors: usize,
    ) -> Result<TypedShader, GlslDiagnostics> {
        let mut diagnostics = GlslDiagnostics::new(max_errors);

        // Pass 1: Collect global const declarations (needed for param array sizes)
        let mut global_const_pass = passes::global_const_pass::GlobalConstPass::new();
        global_const_pass.run(shader, source, &mut diagnostics);
        let global_const_result = global_const_pass.into_result();
        let const_env = Some(&global_const_result.global_constants);

        // Pass 2: Collect function signatures (uses const_env for param array sizes)
        let mut registry_pass = passes::function_registry::FunctionRegistryPass::new();
        registry_pass.run_with_const_env(shader, source, &mut diagnostics, const_env);
        let registry = registry_pass.into_registry();

        // Pass 3: Extract function bodies (uses const_env for param array sizes)
        let mut extraction_pass = passes::function_extraction::FunctionExtractionPass::new();
        extraction_pass.run_with_const_env(shader, source, &mut diagnostics, const_env);
        let (main_func, user_functions) = extraction_pass.into_results();

        // Pass 4: Validate
        let typed_shader = TypedShader {
            main_function: main_func,
            user_functions,
            function_registry: registry,
            global_constants: global_const_result.global_constants,
        };

        let mut validation_pass = passes::validation::ValidationPass;
        validation_pass.validate(&typed_shader, source, &mut diagnostics);

        if diagnostics.errors.is_empty() {
            Ok(typed_shader)
        } else {
            Err(diagnostics)
        }
    }
}

impl Default for SemanticAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Analyze GLSL shader and produce typed AST
pub fn analyze(shader: &TranslationUnit) -> Result<TypedShader, GlslDiagnostics> {
    analyze_with_source(shader, "", crate::DEFAULT_MAX_ERRORS)
}

/// Analyze GLSL shader with source text for better error messages
pub fn analyze_with_source(
    shader: &TranslationUnit,
    source: &str,
    max_errors: usize,
) -> Result<TypedShader, GlslDiagnostics> {
    SemanticAnalyzer::new().analyze(shader, source, max_errors)
}

// ---------------------------------------------------------------------------
// AST node counting helpers (for streaming compilation order)
// ---------------------------------------------------------------------------

fn count_statement_nodes(stmt: &glsl::syntax::Statement) -> usize {
    match stmt {
        glsl::syntax::Statement::Simple(simple) => count_simple_statement_nodes(simple),
        glsl::syntax::Statement::Compound(compound) => {
            1 + compound
                .statement_list
                .iter()
                .map(count_statement_nodes)
                .sum::<usize>()
        }
    }
}

fn count_simple_statement_nodes(stmt: &glsl::syntax::SimpleStatement) -> usize {
    match stmt {
        glsl::syntax::SimpleStatement::Selection(sel) => 1 + count_selection_nodes(&sel.rest),
        glsl::syntax::SimpleStatement::Iteration(iter) => 1 + count_iteration_nodes(iter),
        _ => 1, // Declaration, Expression, Jump, etc.
    }
}

fn count_selection_nodes(rest: &glsl::syntax::SelectionRestStatement) -> usize {
    use glsl::syntax::SelectionRestStatement;
    match rest {
        SelectionRestStatement::Statement(then_stmt) => count_statement_nodes(then_stmt),
        SelectionRestStatement::Else(then_stmt, else_stmt) => {
            count_statement_nodes(then_stmt) + count_statement_nodes(else_stmt)
        }
    }
}

fn count_iteration_nodes(iteration: &glsl::syntax::IterationStatement) -> usize {
    use glsl::syntax::IterationStatement;
    match iteration {
        IterationStatement::While(_condition, stmt) => count_statement_nodes(stmt),
        IterationStatement::DoWhile(stmt, _cond_expr) => count_statement_nodes(stmt),
        IterationStatement::For(_init, _rest, body) => count_statement_nodes(body),
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports, reason = "Used only when std feature enabled")]
    use super::*;

    #[test]
    #[cfg(feature = "std")]
    fn test_ast_node_count_simple() {
        let func = TypedFunction {
            name: alloc::string::String::from("test"),
            return_type: types::Type::Void,
            parameters: Vec::new(),
            body: vec![glsl::syntax::Statement::Simple(alloc::boxed::Box::new(
                glsl::syntax::SimpleStatement::Jump(glsl::syntax::JumpStatement::Return(None)),
            ))],
        };
        assert_eq!(func.ast_node_count(), 1);
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_ast_node_count_compound() {
        // Compound with 2 simple statements inside: 1 (compound) + 2 (children) = 3
        let func = TypedFunction {
            name: alloc::string::String::from("test"),
            return_type: types::Type::Void,
            parameters: Vec::new(),
            body: vec![glsl::syntax::Statement::Compound(alloc::boxed::Box::new(
                glsl::syntax::CompoundStatement {
                    statement_list: vec![
                        glsl::syntax::Statement::Simple(alloc::boxed::Box::new(
                            glsl::syntax::SimpleStatement::Expression(None),
                        )),
                        glsl::syntax::Statement::Simple(alloc::boxed::Box::new(
                            glsl::syntax::SimpleStatement::Jump(
                                glsl::syntax::JumpStatement::Return(None),
                            ),
                        )),
                    ],
                },
            ))],
        };
        assert_eq!(func.ast_node_count(), 3);
    }
}
