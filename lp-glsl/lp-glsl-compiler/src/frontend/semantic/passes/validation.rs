//! Pass for validating function bodies

use crate::error::GlslError;
use crate::frontend::semantic::TypedShader;
use crate::frontend::semantic::validator;

/// Validation pass that operates on a TypedShader
/// Note: This pass operates on TypedShader, not TranslationUnit, so it's a different phase
pub struct ValidationPass;

impl ValidationPass {
    /// Run validation on a TypedShader (different from SemanticPass trait)
    pub fn validate(&mut self, shader: &TypedShader, source: &str) -> Result<(), GlslError> {
        // Validate all functions (third pass logic)
        // Use the registry from the typed shader
        for func in &shader.user_functions {
            validator::validate_function(func, &shader.function_registry, source)?;
        }
        // Validate main function if present (optional for filetests)
        if let Some(ref main_function) = shader.main_function {
            validator::validate_function(main_function, &shader.function_registry, source)?;
        }
        Ok(())
    }
}

// Note: ValidationPass doesn't implement SemanticPass because it operates on TypedShader,
// not TranslationUnit. This is a design decision - validation happens after extraction.
