//! Semantic analysis passes for processing GLSL AST

use crate::error::GlslDiagnostics;

/// A semantic analysis pass that processes the AST
pub trait SemanticPass {
    /// Execute the pass on a translation unit, collecting errors into diagnostics.
    /// Continues processing when possible; stops when diagnostics.at_limit().
    fn run(
        &mut self,
        shader: &glsl::syntax::TranslationUnit,
        source: &str,
        diagnostics: &mut GlslDiagnostics,
    );

    /// Pass name for debugging
    fn name(&self) -> &str;
}

pub mod function_extraction;
pub mod function_registry;
pub mod function_signature;
pub mod global_const_pass;
pub mod validation;
