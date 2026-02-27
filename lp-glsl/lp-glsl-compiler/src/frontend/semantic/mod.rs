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

        // Pass 1: Collect function signatures
        let mut registry_pass = passes::function_registry::FunctionRegistryPass::new();
        registry_pass.run(shader, source, &mut diagnostics);
        let registry = registry_pass.into_registry();

        // Pass 2: Extract function bodies
        let mut extraction_pass = passes::function_extraction::FunctionExtractionPass::new();
        extraction_pass.run(shader, source, &mut diagnostics);
        let (main_func, user_functions) = extraction_pass.into_results();

        // Pass 2b: Collect global const declarations
        let mut global_const_pass = passes::global_const_pass::GlobalConstPass::new();
        global_const_pass.run(shader, source, &mut diagnostics);
        let global_const_result = global_const_pass.into_result();

        // Pass 3: Validate
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
