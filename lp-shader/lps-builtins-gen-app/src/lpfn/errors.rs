//! Error types for LPFX codegen

use std::fmt;

/// Error type for LPFX codegen operations
#[derive(Debug, Clone)]
#[allow(dead_code)] // Some variants reserved for future use
pub enum LpfnCodegenError {
    /// Function is missing the #[lpfn_impl] attribute
    MissingAttribute {
        function_name: String,
        file_path: String,
    },
    /// Invalid GLSL signature syntax
    InvalidSignature {
        function_name: String,
        file_path: String,
        signature: String,
        error: String,
    },
    /// Decimal function missing f32 or q32 variant
    MissingPair {
        function_name: String,
        missing_variant: Variant,
        found_variants: Vec<Variant>,
    },
    /// f32 and q32 signatures don't match
    SignatureMismatch {
        function_name: String,
        f32_signature: String,
        q32_signature: String,
    },
    /// Attribute parsing error
    AttributeParseError {
        function_name: String,
        file_path: String,
        error: String,
    },
    /// Multiple functions with same GLSL name but different signatures
    DuplicateFunctionName {
        function_name: String,
        conflicting_files: Vec<String>,
    },
}

/// Decimal format variant
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Variant {
    F32,
    Q32,
}

impl fmt::Display for Variant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Variant::F32 => write!(f, "f32"),
            Variant::Q32 => write!(f, "q32"),
        }
    }
}

impl fmt::Display for LpfnCodegenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LpfnCodegenError::MissingAttribute {
                function_name,
                file_path,
            } => {
                write!(
                    f,
                    "Function '{}' in '{}' is missing #[lpfn_impl(...)] attribute",
                    function_name, file_path
                )
            }
            LpfnCodegenError::InvalidSignature {
                function_name,
                file_path,
                signature,
                error,
            } => {
                write!(
                    f,
                    "Invalid GLSL signature for function '{}' in '{}': '{}' - {}",
                    function_name, file_path, signature, error
                )
            }
            LpfnCodegenError::MissingPair {
                function_name,
                missing_variant,
                found_variants,
            } => {
                write!(
                    f,
                    "Decimal function '{}' is missing {} variant. Found variants: {:?}",
                    function_name, missing_variant, found_variants
                )
            }
            LpfnCodegenError::SignatureMismatch {
                function_name,
                f32_signature,
                q32_signature,
            } => {
                write!(
                    f,
                    "Function '{}' has mismatched signatures:\n  f32: {}\n  q32: {}",
                    function_name, f32_signature, q32_signature
                )
            }
            LpfnCodegenError::AttributeParseError {
                function_name,
                file_path,
                error,
            } => {
                write!(
                    f,
                    "Failed to parse #[lpfn_impl] attribute for function '{}' in '{}': {}",
                    function_name, file_path, error
                )
            }
            LpfnCodegenError::DuplicateFunctionName {
                function_name,
                conflicting_files,
            } => {
                write!(
                    f,
                    "Multiple functions with GLSL name '{}' found in files: {:?}",
                    function_name, conflicting_files
                )
            }
        }
    }
}

impl std::error::Error for LpfnCodegenError {}
