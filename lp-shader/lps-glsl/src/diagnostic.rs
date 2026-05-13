use alloc::format;
use alloc::string::String;
use core::fmt;

use crate::{SourceMap, Span};

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

    pub fn render(&self, source: &str) -> String {
        let map = SourceMap::new(source);
        let Some((line, col)) = map.line_col(self.span.start) else {
            return format!("{self}");
        };
        let (line_start, line_end) = map.line_bounds(line).unwrap_or((0, source.len()));
        let line_text = &source[line_start..line_end];
        let underline_start = self.span.start.saturating_sub(line_start);
        let underline_end = self
            .span
            .end
            .min(line_end)
            .saturating_sub(line_start)
            .max(underline_start + 1);

        let mut marker = String::new();
        for _ in 0..underline_start {
            marker.push(' ');
        }
        for _ in underline_start..underline_end {
            marker.push('^');
        }

        format!(
            "error: {}\n --> <shader>:{line}:{col}\n  |\n{line:>2} | {line_text}\n  | {marker}",
            self.message
        )
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
