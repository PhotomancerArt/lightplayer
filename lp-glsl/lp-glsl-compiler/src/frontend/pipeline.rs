//! Compilation pipeline for GLSL shaders.
//!
//! This module provides a unified compilation pipeline that can be used
//! by different backends (JIT, code generation, CLIF output).

use crate::error::{
    ErrorCode, GlslError, format_source_lines_around_span, source_span_to_location,
};
use crate::frontend::semantic::TypedShader;
use crate::frontend::semantic::functions::FunctionRegistry;

use alloc::{boxed::Box, string::ToString};

/// Result of parsing GLSL source
pub struct ParseResult<'a> {
    pub shader: glsl::syntax::TranslationUnit,
    pub source: &'a str,
}

/// Result of semantic analysis
pub struct SemanticResult<'a> {
    pub typed_ast: TypedShader,
    pub source: &'a str,
}

/// Result of compilation, ready for backend
pub struct CompiledShader {
    pub typed_shader: TypedShader,
    pub source: alloc::string::String,
}

impl<'a> From<SemanticResult<'a>> for CompiledShader {
    fn from(result: SemanticResult<'a>) -> Self {
        CompiledShader {
            typed_shader: result.typed_ast,
            #[cfg(not(feature = "std"))]
            source: alloc::string::ToString::to_string(result.source),
            #[cfg(feature = "std")]
            source: std::string::ToString::to_string(result.source),
        }
    }
}

/// A transformation pass that operates on semantic results
pub trait TransformationPass {
    /// Apply the transformation
    fn apply<'a>(&self, result: SemanticResult<'a>) -> Result<SemanticResult<'a>, GlslError>;

    /// Whether this transformation is enabled
    fn is_enabled(&self) -> bool;

    /// Name of the transformation
    fn name(&self) -> &str;
}

/// Common compilation pipeline steps
pub struct CompilationPipeline;

impl CompilationPipeline {
    /// Parse GLSL source into an AST
    pub fn parse<'a>(source: &'a str) -> Result<ParseResult<'a>, GlslError> {
        let shader = glsl::parser::Parse::parse(source).map_err(|e| {
            // Extract clean error message from ParseError.info field
            // The info field contains a formatted message like:
            // "0: at line 8:\n    float add_in(...) {\n    ^\nexpected '}', found f\n\n"
            // We want to extract just "expected '}', found f"
            let clean_message = e
                .info
                .lines()
                .find(|line| {
                    let trimmed = line.trim();
                    // Look for the actual error message (contains "expected" or "found")
                    (trimmed.starts_with("expected") || trimmed.contains("expected"))
                        && (trimmed.contains("found") || trimmed.contains("unexpected"))
                })
                .map(|line| line.trim().to_string())
                .unwrap_or_else(|| {
                    // Fallback: try to find any line with "expected" or "found"
                    e.info
                        .lines()
                        .find(|line| {
                            let trimmed = line.trim();
                            trimmed.contains("expected") || trimmed.contains("found")
                        })
                        .map(|line| line.trim().to_string())
                        .unwrap_or_else(|| "parse error".to_string())
                });

            let mut error = GlslError::new(ErrorCode::E0001, clean_message);
            // Try to extract span from parse error if available
            if let Some(ref span) = e.span {
                error = error.with_location(source_span_to_location(span));
                // For parse errors, add span_text but don't duplicate the message on the caret line
                // The main message is already shown, so we'll pass None to avoid duplication
                if let Some(span_text) = format_source_lines_around_span(source, span, None) {
                    error = error.with_span_text(span_text);
                }
            }
            error
        })?;

        Ok(ParseResult { shader, source })
    }

    /// Perform semantic analysis on parsed shader
    pub fn analyze<'a>(parse_result: ParseResult<'a>) -> Result<SemanticResult<'a>, GlslError> {
        let typed_ast = crate::frontend::semantic::analyze_with_source(
            &parse_result.shader,
            parse_result.source,
        )?;

        Ok(SemanticResult {
            typed_ast,
            source: parse_result.source,
        })
    }

    /// Apply transformations to semantic result
    pub fn transform<'a>(
        result: SemanticResult<'a>,
        transforms: &[Box<dyn TransformationPass>],
    ) -> Result<SemanticResult<'a>, GlslError> {
        let mut current = result;
        for transform in transforms {
            if transform.is_enabled() {
                current = transform.apply(current)?;
            }
        }
        Ok(current)
    }

    /// Parse, analyze, and transform in one step
    pub fn compile<'a>(
        source: &'a str,
        transforms: &[Box<dyn TransformationPass>],
    ) -> Result<SemanticResult<'a>, GlslError> {
        let parse_result = Self::parse(source)?;
        let semantic_result = Self::analyze(parse_result)?;
        Self::transform(semantic_result, transforms)
    }

    /// Parse and analyze in one step (backward compatibility)
    pub fn parse_and_analyze<'a>(source: &'a str) -> Result<SemanticResult<'a>, GlslError> {
        let parse_result = Self::parse(source)?;
        Self::analyze(parse_result)
    }
}

/// Parse and type-check a GLSL program, returning function registry
/// This function is useful for extracting function signatures without full compilation
pub fn parse_program_with_registry(source: &str) -> Result<FunctionRegistry, GlslError> {
    // Parse and analyze the program
    let semantic_result = CompilationPipeline::parse_and_analyze(source)?;

    // Extract function registry from the typed shader
    Ok(semantic_result.typed_ast.function_registry)
}

/// Backend that generates code from compiled shader
pub trait Backend {
    type Output;
    type Error;

    fn compile(&mut self, shader: CompiledShader) -> Result<Self::Output, Self::Error>;
}
