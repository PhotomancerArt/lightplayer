//! Error codes and `GlslError` (subset copied from `lp-glsl-frontend`).

use alloc::{format, string::String, vec::Vec};
use core::fmt;

use crate::DEFAULT_MAX_ERRORS;
use crate::source_loc::GlSourceLoc;

/// Error codes for GLSL compilation errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    E0001,
    E0100,
    E0101,
    E0102,
    E0103,
    E0104,
    E0105,
    E0106,
    E0107,
    E0108,
    E0109,
    E0110,
    E0111,
    E0112,
    E0113,
    E0114,
    E0115,
    E0116,
    E0300,
    E0301,
    E0400,
    E0401,
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::E0001 => "E0001",
            ErrorCode::E0100 => "E0100",
            ErrorCode::E0101 => "E0101",
            ErrorCode::E0102 => "E0102",
            ErrorCode::E0103 => "E0103",
            ErrorCode::E0104 => "E0104",
            ErrorCode::E0105 => "E0105",
            ErrorCode::E0106 => "E0106",
            ErrorCode::E0107 => "E0107",
            ErrorCode::E0108 => "E0108",
            ErrorCode::E0109 => "E0109",
            ErrorCode::E0110 => "E0110",
            ErrorCode::E0111 => "E0111",
            ErrorCode::E0112 => "E0112",
            ErrorCode::E0113 => "E0113",
            ErrorCode::E0114 => "E0114",
            ErrorCode::E0115 => "E0115",
            ErrorCode::E0116 => "E0116",
            ErrorCode::E0300 => "E0300",
            ErrorCode::E0301 => "E0301",
            ErrorCode::E0400 => "E0400",
            ErrorCode::E0401 => "E0401",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            ErrorCode::E0001 => "parse error",
            ErrorCode::E0100 => "undefined variable",
            ErrorCode::E0101 => "undefined function",
            ErrorCode::E0102 => "type mismatch",
            ErrorCode::E0103 => "cannot implicitly convert",
            ErrorCode::E0104 => "wrong argument count",
            ErrorCode::E0105 => "wrong argument type",
            ErrorCode::E0106 => "incompatible types for operator",
            ErrorCode::E0107 => "condition must be bool",
            ErrorCode::E0108 => "no main function",
            ErrorCode::E0109 => "unsupported type",
            ErrorCode::E0110 => "invalid vector constructor",
            ErrorCode::E0111 => "component out of range",
            ErrorCode::E0112 => "invalid component access",
            ErrorCode::E0113 => "invalid swizzle",
            ErrorCode::E0114 => "no matching function",
            ErrorCode::E0115 => "cannot assign",
            ErrorCode::E0116 => "return type mismatch",
            ErrorCode::E0300 => "transformation error",
            ErrorCode::E0301 => "verification failed",
            ErrorCode::E0400 => "codegen error",
            ErrorCode::E0401 => "verification error",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Clone, Debug)]
pub struct GlslError {
    pub code: ErrorCode,
    pub message: String,
    pub location: Option<GlSourceLoc>,
    pub span_text: Option<String>,
    pub notes: Vec<String>,
    pub spec_ref: Option<String>,
}

impl GlslError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            location: None,
            span_text: None,
            notes: Vec::new(),
            spec_ref: None,
        }
    }

    pub fn with_location(mut self, location: GlSourceLoc) -> Self {
        self.location = Some(location);
        self
    }

    pub fn with_span_text(mut self, text: impl Into<String>) -> Self {
        self.span_text = Some(text.into());
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn with_spec_ref(mut self, spec_ref: impl Into<String>) -> Self {
        self.spec_ref = Some(spec_ref.into());
        self
    }

    pub fn to_simple_string(&self) -> String {
        if let Some(ref loc) = self.location {
            format!("{}: {}", loc, self.message)
        } else {
            self.message.clone()
        }
    }

    pub fn parse_error(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::E0001, message)
    }
}

impl fmt::Display for GlslError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "error[{}]: {}", self.code, self.message)?;
        if let Some(ref loc) = self.location {
            if !loc.is_unknown() {
                write!(f, "\n --> {loc}")?;
            }
        }
        if let Some(ref text) = self.span_text {
            write!(f, "\n{text}\n")?;
        } else if let Some(ref loc) = self.location {
            if !loc.is_unknown() {
                writeln!(f, "\n --> {loc}")?;
            }
        }
        let notes_to_show = if self.span_text.is_some() && !self.notes.is_empty() {
            &self.notes[1..]
        } else {
            &self.notes[..]
        };
        for note in notes_to_show {
            write!(f, "\nnote: {note}")?;
        }
        if let Some(ref spec_ref) = self.spec_ref {
            write!(f, "\n  = spec: {spec_ref}")?;
        }
        Ok(())
    }
}

impl core::error::Error for GlslError {}

#[derive(Clone, Debug)]
pub struct GlslDiagnostics {
    pub errors: Vec<GlslError>,
    pub limit: usize,
}

impl GlslDiagnostics {
    pub fn new(limit: usize) -> Self {
        Self {
            errors: Vec::new(),
            limit,
        }
    }

    pub fn push(&mut self, e: GlslError) -> bool {
        if self.errors.len() < self.limit {
            self.errors.push(e);
            true
        } else {
            false
        }
    }

    pub fn at_limit(&self) -> bool {
        self.errors.len() >= self.limit
    }

    pub fn single(e: GlslError, limit: usize) -> Self {
        Self {
            errors: {
                let mut v = Vec::new();
                v.push(e);
                v
            },
            limit,
        }
    }
}

impl fmt::Display for GlslDiagnostics {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (i, err) in self.errors.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{err}")?;
        }
        if self.at_limit() && !self.errors.is_empty() {
            writeln!(
                f,
                "\nnote: further errors suppressed (limit {})",
                self.limit
            )?;
        }
        Ok(())
    }
}

impl core::error::Error for GlslDiagnostics {}

impl From<GlslError> for GlslDiagnostics {
    fn from(e: GlslError) -> Self {
        GlslDiagnostics::single(e, DEFAULT_MAX_ERRORS)
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use super::*;
    use crate::source_loc::GlFileId;

    #[test]
    fn error_code_display() {
        assert_eq!(ErrorCode::E0100.as_str(), "E0100");
    }

    #[test]
    fn glsl_error_with_location() {
        let err = GlslError::new(ErrorCode::E0100, "bad").with_location(GlSourceLoc::new(
            GlFileId(1),
            5,
            10,
        ));
        let s = err.to_string();
        assert!(s.contains("E0100"));
        assert!(s.contains("5:10"));
    }
}
