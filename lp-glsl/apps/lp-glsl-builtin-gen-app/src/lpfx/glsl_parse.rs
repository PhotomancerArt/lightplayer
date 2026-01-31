//! Parse GLSL signature strings into FunctionSignature

use crate::lpfx::errors::LpfxCodegenError;
use glsl::parser::Parse;
use glsl::syntax::{ExternalDeclaration, TranslationUnit};
use lp_glsl_compiler::frontend::semantic::functions::FunctionSignature;
use lp_glsl_compiler::frontend::semantic::passes::function_signature::extract_function_signature;

/// Parse a GLSL function signature string into a FunctionSignature
pub fn parse_glsl_signature(
    sig_str: &str,
    function_name: &str,
    file_path: &str,
) -> Result<FunctionSignature, LpfxCodegenError> {
    // Parse the function signature as a function definition with empty body
    // Format: "float func(float x) {}"
    let func_def_str = format!("{} {{}}", sig_str);
    let shader: TranslationUnit = Parse::parse(&func_def_str).map_err(|e| {
        let error_msg = e
            .info
            .lines()
            .find(|line| {
                let trimmed = line.trim();
                trimmed.contains("expected") || trimmed.contains("found")
            })
            .map(|line| line.trim().to_string())
            .unwrap_or_else(|| format!("GLSL parse error: {}", e));

        LpfxCodegenError::InvalidSignature {
            function_name: function_name.to_string(),
            file_path: file_path.to_string(),
            signature: sig_str.to_string(),
            error: error_msg,
        }
    })?;

    // Find the function definition in the shader
    for decl in &shader.0 {
        if let ExternalDeclaration::FunctionDefinition(func_def) = decl {
            return extract_function_signature(&func_def.prototype).map_err(|e| {
                LpfxCodegenError::InvalidSignature {
                    function_name: function_name.to_string(),
                    file_path: file_path.to_string(),
                    signature: sig_str.to_string(),
                    error: format!("Failed to extract function signature: {}", e),
                }
            });
        }
    }

    Err(LpfxCodegenError::InvalidSignature {
        function_name: function_name.to_string(),
        file_path: file_path.to_string(),
        signature: sig_str.to_string(),
        error: "No function definition found in parsed GLSL".to_string(),
    })
}
