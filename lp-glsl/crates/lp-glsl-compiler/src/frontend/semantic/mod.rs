use crate::error::GlslError;
use glsl::syntax::TranslationUnit;
use passes::SemanticPass;

use alloc::vec::Vec;

use alloc::string::String;
pub mod builtins;
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
    ) -> Result<TypedShader, GlslError> {
        // Pass 1: Collect function signatures
        let mut registry_pass = passes::function_registry::FunctionRegistryPass::new();
        registry_pass.run(shader, source)?;
        let registry = registry_pass.into_registry();

        // Pass 2: Extract function bodies
        let mut extraction_pass = passes::function_extraction::FunctionExtractionPass::new();
        extraction_pass.run(shader, source)?;
        let (main_func, user_functions) = extraction_pass.into_results();

        // Pass 3: Validate
        // Main function is optional for filetests (functions can be called directly)
        // For backward compatibility, we still allow requiring main, but don't enforce it here
        let typed_shader = TypedShader {
            main_function: main_func,
            user_functions,
            function_registry: registry,
        };

        // Pass 3 (continued): Validate (using reference to registry from typed_shader)
        let mut validation_pass = passes::validation::ValidationPass;
        validation_pass.validate(&typed_shader, source)?;

        Ok(typed_shader)
    }
}

impl Default for SemanticAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Analyze GLSL shader and produce typed AST
pub fn analyze(shader: &TranslationUnit) -> Result<TypedShader, GlslError> {
    analyze_with_source(shader, "")
}

/// Analyze GLSL shader with source text for better error messages
/// This function maintains backward compatibility with the old API
pub fn analyze_with_source(
    shader: &TranslationUnit,
    source: &str,
) -> Result<TypedShader, GlslError> {
    SemanticAnalyzer::new().analyze(shader, source)
}
