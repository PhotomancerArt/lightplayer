//! Minimal source location types for errors (copied from `lp-glsl-frontend`).

use core::fmt;

/// Unique identifier for a source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GlFileId(pub u32);

/// Single point in source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlSourceLoc {
    pub file_id: GlFileId,
    /// 1-indexed; 0 = unknown.
    pub line: usize,
    /// 1-indexed; 0 = unknown.
    pub column: usize,
}

impl GlSourceLoc {
    pub fn new(file_id: GlFileId, line: usize, column: usize) -> Self {
        Self {
            file_id,
            line,
            column,
        }
    }

    pub fn is_unknown(&self) -> bool {
        self.line == 0 && self.column == 0
    }

    pub fn unknown(file_id: GlFileId) -> Self {
        Self {
            file_id,
            line: 0,
            column: 0,
        }
    }
}

impl fmt::Display for GlSourceLoc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_unknown() {
            write!(f, "<unknown>")
        } else {
            write!(f, "{}:{}", self.line, self.column)
        }
    }
}
