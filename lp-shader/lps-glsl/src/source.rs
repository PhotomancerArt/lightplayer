use alloc::vec::Vec;

/// Byte span in the original source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// Source line index for diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMap {
    line_starts: Vec<usize>,
}

impl SourceMap {
    pub fn new(source: &str) -> Self {
        let mut line_starts = Vec::new();
        line_starts.push(0);
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self { line_starts }
    }

    /// Return 1-based `(line, column)` for a byte offset.
    pub fn line_col(&self, offset: usize) -> Option<(usize, usize)> {
        let line_idx = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(0) => return None,
            Err(i) => i - 1,
        };
        let col = offset.checked_sub(self.line_starts[line_idx])? + 1;
        Some((line_idx + 1, col))
    }
}
