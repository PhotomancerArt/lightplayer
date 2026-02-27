//! Pass for validating function bodies

use crate::error::GlslDiagnostics;
use crate::frontend::semantic::TypedShader;
use crate::frontend::semantic::validator;

/// Validation pass that operates on a TypedShader
/// Note: This pass operates on TypedShader, not TranslationUnit, so it's a different phase
pub struct ValidationPass;

impl ValidationPass {
    /// Run validation on a TypedShader, collecting errors into diagnostics.
    pub fn validate(
        &mut self,
        shader: &TypedShader,
        source: &str,
        diagnostics: &mut GlslDiagnostics,
    ) {
        for func in &shader.user_functions {
            if diagnostics.at_limit() {
                break;
            }
            validator::validate_function(func, &shader.function_registry, source, diagnostics);
        }
        if !diagnostics.at_limit() {
            if let Some(ref main_function) = shader.main_function {
                validator::validate_function(
                    main_function,
                    &shader.function_registry,
                    source,
                    diagnostics,
                );
            }
        }
    }
}

// Note: ValidationPass doesn't implement SemanticPass because it operates on TypedShader,
// not TranslationUnit. This is a design decision - validation happens after extraction.
