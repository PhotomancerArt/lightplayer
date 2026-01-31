//! Semantic analysis passes for processing GLSL AST

/// A semantic analysis pass that processes the AST
pub trait SemanticPass {
    /// Execute the pass on a translation unit
    fn run(
        &mut self,
        shader: &glsl::syntax::TranslationUnit,
        source: &str,
    ) -> Result<(), crate::error::GlslError>;

    /// Pass name for debugging
    fn name(&self) -> &str;
}

pub mod function_extraction;
pub mod function_registry;
pub mod function_signature;
pub mod validation;
