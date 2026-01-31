//! Pass for collecting function signatures from the AST

use super::SemanticPass;
use super::function_signature;
use crate::error::GlslError;
use crate::frontend::semantic::functions::FunctionRegistry;

pub struct FunctionRegistryPass {
    registry: FunctionRegistry,
}

impl FunctionRegistryPass {
    pub fn new() -> Self {
        Self {
            registry: FunctionRegistry::new(),
        }
    }

    pub fn into_registry(self) -> FunctionRegistry {
        self.registry
    }
}

impl SemanticPass for FunctionRegistryPass {
    fn run(
        &mut self,
        shader: &glsl::syntax::TranslationUnit,
        _source: &str,
    ) -> Result<(), GlslError> {
        // Extract function signatures (first pass logic)
        for decl in &shader.0 {
            if let glsl::syntax::ExternalDeclaration::FunctionDefinition(func) = decl {
                let sig = function_signature::extract_function_signature(&func.prototype)?;
                self.registry.register_function(sig)?;
            }
        }
        Ok(())
    }

    fn name(&self) -> &str {
        "function_registry"
    }
}
