//! Pass for collecting function signatures from the AST

use super::SemanticPass;
use super::function_signature;
use crate::error::GlslDiagnostics;
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
        diagnostics: &mut GlslDiagnostics,
    ) {
        for decl in &shader.0 {
            if diagnostics.at_limit() {
                break;
            }
            if let glsl::syntax::ExternalDeclaration::FunctionDefinition(func) = decl {
                match function_signature::extract_function_signature(&func.prototype) {
                    Ok(sig) => {
                        let _ = self.registry.register_function(sig);
                    }
                    Err(e) => {
                        if !diagnostics.push(e) {
                            break;
                        }
                    }
                }
            }
        }
    }

    fn name(&self) -> &str {
        "function_registry"
    }
}
