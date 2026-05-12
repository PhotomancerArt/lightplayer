use alloc::format;
use alloc::string::String;
use core::fmt;

use crate::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub span: Span,
    pub message: String,
}

impl Diagnostic {
    pub fn error(span: Span, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            span,
            message: message.into(),
        }
    }

    pub fn expected(span: Span, expected: &str, found: &str) -> Self {
        Self::error(span, format!("expected {expected}, found {found}"))
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "error: {} at bytes {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}
