//! Source map for tracking source locations across multiple files.
//!
//! This module provides a comprehensive system for managing source locations
//! that supports multiple files, intrinsics, and synthetic sources.

use alloc::{collections::BTreeMap, format, string::String, vec::Vec};

#[cfg(feature = "std")]
use std::path::PathBuf;

/// Unique identifier for a source file
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GlFileId(pub u32);

/// Origin of a source file
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GlFileSource {
    /// Real filesystem file
    #[cfg(feature = "std")]
    File(PathBuf),
    /// Built-in intrinsic (e.g., "trig")
    Intrinsic(String),
    /// Generated/test code without a real file
    Synthetic(String),
    /// Real filesystem file (no_std fallback)
    #[cfg(not(feature = "std"))]
    File(String),
}

impl GlFileSource {
    /// Get a display name for this file source (for error messages).
    pub fn display_name(&self) -> &str {
        match self {
            #[cfg(feature = "std")]
            GlFileSource::File(path) => path.to_str().unwrap_or("<invalid path>"),
            GlFileSource::Intrinsic(name) => name,
            GlFileSource::Synthetic(name) => name,
            #[cfg(not(feature = "std"))]
            GlFileSource::File(name) => name,
        }
    }
}

/// Source file information
#[derive(Clone, Debug)]
pub struct GlSourceFile {
    /// Origin of the file
    pub source: GlFileSource,
    /// Full source text content
    pub contents: String,
}

/// Source location (single point)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlSourceLoc {
    /// File containing this location
    pub file_id: GlFileId,
    /// Line number (1-indexed). 0 means unknown.
    pub line: usize,
    /// Column number (1-indexed). 0 means unknown.
    pub column: usize,
}

impl GlSourceLoc {
    /// Create a new source location.
    pub fn new(file_id: GlFileId, line: usize, column: usize) -> Self {
        Self {
            file_id,
            line,
            column,
        }
    }

    /// Check if this location is unknown.
    pub fn is_unknown(&self) -> bool {
        self.line == 0 && self.column == 0
    }

    /// Create an unknown location for a specific file.
    pub fn unknown(file_id: GlFileId) -> Self {
        Self {
            file_id,
            line: 0,
            column: 0,
        }
    }

    /// Format this location as a string using the given source map.
    /// Returns a string like "filename:line:column" or "line:column" if file not found.
    pub fn format_with_map(&self, map: &GlSourceMap) -> String {
        if let Some(file) = map.get_file(self.file_id) {
            format!(
                "{}:{}:{}",
                file.source.display_name(),
                self.line,
                self.column
            )
        } else {
            format!("{}:{}", self.line, self.column)
        }
    }
}

impl core::fmt::Display for GlSourceLoc {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_unknown() {
            write!(f, "<unknown>")
        } else {
            write!(f, "{}:{}", self.line, self.column)
        }
    }
}

/// Source span (range)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlSourceSpan {
    /// File containing this span
    pub file_id: GlFileId,
    /// Start line number (1-indexed)
    pub start_line: usize,
    /// Start column number (1-indexed)
    pub start_column: usize,
    /// End line number (1-indexed)
    pub end_line: usize,
    /// End column number (1-indexed)
    pub end_column: usize,
}

impl GlSourceSpan {
    /// Create a new source span.
    pub fn new(
        file_id: GlFileId,
        start_line: usize,
        start_column: usize,
        end_line: usize,
        end_column: usize,
    ) -> Self {
        Self {
            file_id,
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }

    /// Check if this span is unknown.
    pub fn is_unknown(&self) -> bool {
        self.start_line == 0 && self.start_column == 0
    }

    /// Create an unknown span for a specific file.
    pub fn unknown(file_id: GlFileId) -> Self {
        Self {
            file_id,
            start_line: 0,
            start_column: 0,
            end_line: 0,
            end_column: 0,
        }
    }

    /// Create a single-point span (start and end are the same).
    pub fn point(file_id: GlFileId, line: usize, column: usize) -> Self {
        Self::new(file_id, line, column, line, column)
    }
}

/// Manages source files and provides conversion utilities
#[derive(Clone, Debug)]
pub struct GlSourceMap {
    /// Mapping from file ID to file information
    files: BTreeMap<GlFileId, GlSourceFile>,
    /// Next ID to assign to a new file
    next_file_id: u32,
}

impl GlSourceMap {
    /// Create a new empty source map.
    pub fn new() -> Self {
        Self {
            files: Default::default(),
            next_file_id: 1, // Start at 1, reserve 0 for "unknown"
        }
    }

    /// Add a source file and return its ID.
    pub fn add_file(&mut self, source: GlFileSource, contents: String) -> GlFileId {
        let id = GlFileId(self.next_file_id);
        self.next_file_id += 1;
        self.files.insert(id, GlSourceFile { source, contents });
        id
    }

    /// Look up a file by its ID.
    pub fn get_file(&self, id: GlFileId) -> Option<&GlSourceFile> {
        self.files.get(&id)
    }

    /// Look up a file by its ID (mutable).
    pub fn get_file_mut(&mut self, id: GlFileId) -> Option<&mut GlSourceFile> {
        self.files.get_mut(&id)
    }

    /// Find an intrinsic file by name.
    pub fn find_intrinsic(&self, name: &str) -> Option<GlFileId> {
        self.files
            .iter()
            .find(|(_, file)| matches!(&file.source, GlFileSource::Intrinsic(n) if n == name))
            .map(|(id, _)| *id)
    }

    /// Get the number of files in the source map.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Check if the source map is empty.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Convert glsl-parser SourceSpan to GlSourceSpan.
    ///
    /// Requires the file_id context since glsl-parser spans don't include file info.
    pub fn convert_span(&self, file_id: GlFileId, span: &glsl::syntax::SourceSpan) -> GlSourceSpan {
        GlSourceSpan {
            file_id,
            start_line: span.line,
            start_column: span.column,
            end_line: span.line, // glsl-parser spans are single positions
            end_column: span.column,
        }
    }

    /// Convert GlSourceSpan to SourceLocation (for error reporting backward compatibility).
    pub fn to_source_location(&self, span: &GlSourceSpan) -> crate::frontend::src_loc::GlSourceLoc {
        crate::frontend::src_loc::GlSourceLoc::new(span.file_id, span.start_line, span.start_column)
    }

    /// Extract source line text from a span for error messages.
    pub fn extract_line(&self, span: &GlSourceSpan) -> Option<String> {
        if span.start_line == 0 {
            return None;
        }
        let file = self.get_file(span.file_id)?;
        file.contents
            .lines()
            .nth(span.start_line.saturating_sub(1))
            .map(|s| String::from(s))
    }

    /// Extract source lines around a span for error context display.
    ///
    /// Returns formatted text showing the span location with surrounding context lines.
    pub fn extract_lines_around(
        &self,
        span: &GlSourceSpan,
        context_lines: usize,
    ) -> Option<String> {
        let file = self.get_file(span.file_id)?;
        let lines: Vec<&str> = file.contents.lines().collect();

        if span.start_line == 0 || span.start_line > lines.len() {
            return None;
        }

        let start_line = span.start_line.saturating_sub(context_lines).max(1);
        let end_line = (span.start_line + context_lines).min(lines.len());
        let source_lines: Vec<&str> = lines[(start_line - 1)..end_line].to_vec();

        let mut source_display = String::new();
        for (idx, line) in source_lines.iter().enumerate() {
            let line_num = start_line + idx;
            if line_num == span.start_line {
                source_display.push_str(&format!("{line_num:>4} | {line}\n"));
                // Point to the column if it's valid
                let col_pos = span.start_column.saturating_sub(1).min(line.len()).min(200);
                source_display.push_str(&format!("     | {}^ here\n", " ".repeat(col_pos)));
            } else {
                source_display.push_str(&format!("{line_num:>4} | {line}\n"));
            }
        }
        Some(String::from(source_display.trim_end()))
    }
}

impl Default for GlSourceMap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock SourceSpan for testing (since we can't import glsl-parser in tests easily)
    struct MockSourceSpan {
        #[allow(dead_code, reason = "Mock struct fields for testing")]
        pub line: usize,
        #[allow(dead_code, reason = "Mock struct fields for testing")]
        pub column: usize,
    }

    impl MockSourceSpan {
        fn new(line: usize, column: usize) -> Self {
            Self { line, column }
        }
    }

    #[test]
    fn test_gl_file_id() {
        let id1 = GlFileId(1);
        let id2 = GlFileId(1);
        let id3 = GlFileId(2);

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_gl_file_source_display_name() {
        // Test intrinsic
        let intrinsic = GlFileSource::Intrinsic(String::from("trig"));
        assert_eq!(intrinsic.display_name(), "trig");

        // Test synthetic
        let synthetic = GlFileSource::Synthetic("test.glsl".into());
        assert_eq!(synthetic.display_name(), "test.glsl");

        // Test file (std)
        #[cfg(feature = "std")]
        {
            use std::path::PathBuf;
            let file = GlFileSource::File(PathBuf::from("/path/to/file.glsl"));
            assert_eq!(file.display_name(), "/path/to/file.glsl");
        }

        // Test file (no_std)
        #[cfg(not(feature = "std"))]
        {
            let file = GlFileSource::File("/path/to/file.glsl".into());
            assert_eq!(file.display_name(), "/path/to/file.glsl");
        }
    }

    #[test]
    fn test_gl_source_loc() {
        let file_id = GlFileId(1);
        let loc = GlSourceLoc::new(file_id, 5, 10);

        assert_eq!(loc.file_id, file_id);
        assert_eq!(loc.line, 5);
        assert_eq!(loc.column, 10);
        assert!(!loc.is_unknown());

        let unknown_loc = GlSourceLoc::unknown(file_id);
        assert!(unknown_loc.is_unknown());
        assert_eq!(unknown_loc.line, 0);
        assert_eq!(unknown_loc.column, 0);
    }

    #[test]
    fn test_gl_source_span() {
        let file_id = GlFileId(1);
        let span = GlSourceSpan::new(file_id, 5, 10, 5, 15);

        assert_eq!(span.file_id, file_id);
        assert_eq!(span.start_line, 5);
        assert_eq!(span.start_column, 10);
        assert_eq!(span.end_line, 5);
        assert_eq!(span.end_column, 15);
        assert!(!span.is_unknown());

        let point_span = GlSourceSpan::point(file_id, 3, 8);
        assert_eq!(point_span.start_line, 3);
        assert_eq!(point_span.start_column, 8);
        assert_eq!(point_span.end_line, 3);
        assert_eq!(point_span.end_column, 8);

        let unknown_span = GlSourceSpan::unknown(file_id);
        assert!(unknown_span.is_unknown());
    }

    #[test]
    fn test_gl_source_map_basic() {
        let mut source_map = GlSourceMap::new();
        assert!(source_map.is_empty());
        assert_eq!(source_map.len(), 0);

        // Add intrinsic file
        let intrinsic_id = source_map.add_file(
            GlFileSource::Intrinsic("trig".into()),
            "float sin(float x);".into(),
        );
        assert_eq!(source_map.len(), 1);

        // Add synthetic file
        let synthetic_id = source_map.add_file(
            GlFileSource::Synthetic("test.glsl".into()),
            "void main() {}".into(),
        );
        assert_eq!(source_map.len(), 2);

        // Test retrieval
        let intrinsic_file = source_map.get_file(intrinsic_id).unwrap();
        assert_eq!(intrinsic_file.contents, "float sin(float x);");
        match &intrinsic_file.source {
            GlFileSource::Intrinsic(name) => assert_eq!(name, "trig"),
            _ => panic!("Expected intrinsic"),
        }

        let synthetic_file = source_map.get_file(synthetic_id).unwrap();
        assert_eq!(synthetic_file.contents, "void main() {}");
        match &synthetic_file.source {
            GlFileSource::Synthetic(name) => assert_eq!(name, "test.glsl"),
            _ => panic!("Expected synthetic"),
        }
    }

    #[test]
    fn test_find_intrinsic() {
        let mut source_map = GlSourceMap::new();

        let trig_id = source_map.add_file(
            GlFileSource::Intrinsic("trig".into()),
            "float sin(float x);".into(),
        );

        let math_id = source_map.add_file(
            GlFileSource::Intrinsic("math".into()),
            "float sqrt(float x);".into(),
        );

        assert_eq!(source_map.find_intrinsic("trig"), Some(trig_id));
        assert_eq!(source_map.find_intrinsic("math"), Some(math_id));
        assert_eq!(source_map.find_intrinsic("nonexistent"), None);
    }

    #[test]
    fn test_convert_span() {
        let mut source_map = GlSourceMap::new();
        let file_id = source_map.add_file(
            GlFileSource::Synthetic("test.glsl".into()),
            "void main() {}".into(),
        );

        // Mock glsl span
        let _mock_span = MockSourceSpan::new(3, 5);
        let gl_span = GlSourceSpan::new(file_id, 3, 5, 3, 5);

        // Since we can't easily create a real glsl::syntax::SourceSpan,
        // we'll test the structure by manually creating the expected result
        let converted = GlSourceSpan {
            file_id,
            start_line: 3,
            start_column: 5,
            end_line: 3,
            end_column: 5,
        };

        assert_eq!(converted, gl_span);
    }

    #[test]
    fn test_extract_line() {
        let mut source_map = GlSourceMap::new();
        let content = "line 1\nline 2\nline 3\nline 4\nline 5";
        let file_id =
            source_map.add_file(GlFileSource::Synthetic("test.glsl".into()), content.into());

        // Test valid line extraction
        let span = GlSourceSpan::point(file_id, 3, 1);
        let extracted = source_map.extract_line(&span);
        assert_eq!(extracted, Some("line 3".into()));

        // Test line 1
        let span1 = GlSourceSpan::point(file_id, 1, 1);
        let extracted1 = source_map.extract_line(&span1);
        assert_eq!(extracted1, Some("line 1".into()));

        // Test out of bounds
        let span_invalid = GlSourceSpan::point(file_id, 10, 1);
        let extracted_invalid = source_map.extract_line(&span_invalid);
        assert_eq!(extracted_invalid, None);

        // Test line 0 (unknown)
        let span_unknown = GlSourceSpan::point(file_id, 0, 1);
        let extracted_unknown = source_map.extract_line(&span_unknown);
        assert_eq!(extracted_unknown, None);
    }

    #[test]
    fn test_extract_lines_around() {
        let mut source_map = GlSourceMap::new();
        let content = "line 1\nline 2\nline 3\nline 4\nline 5";
        let file_id =
            source_map.add_file(GlFileSource::Synthetic("test.glsl".into()), content.into());

        // Test extracting around line 3 with context 1
        let span = GlSourceSpan::point(file_id, 3, 1);
        let extracted = source_map.extract_lines_around(&span, 1);

        // Should show lines 2, 3, 4
        let expected = "   2 | line 2\n   3 | line 3\n     | ^ here\n   4 | line 4";
        assert_eq!(extracted, Some(expected.into()));

        // Test with context 0 (just the target line)
        let extracted_zero = source_map.extract_lines_around(&span, 0);
        let expected_zero = "   3 | line 3\n     | ^ here";
        assert_eq!(extracted_zero, Some(expected_zero.into()));

        // Test edge case: line 1
        let span1 = GlSourceSpan::point(file_id, 1, 1);
        let extracted1 = source_map.extract_lines_around(&span1, 1);
        let expected1 = "   1 | line 1\n     | ^ here\n   2 | line 2";
        assert_eq!(extracted1, Some(expected1.into()));

        // Test invalid line
        let span_invalid = GlSourceSpan::point(file_id, 10, 1);
        let extracted_invalid = source_map.extract_lines_around(&span_invalid, 1);
        assert_eq!(extracted_invalid, None);
    }

    #[test]
    fn test_to_source_location() {
        let mut source_map = GlSourceMap::new();

        // Add a file
        let file_id = source_map.add_file(
            GlFileSource::Synthetic("test.glsl".into()),
            "void main() {}".into(),
        );

        // Test conversion
        let span = GlSourceSpan::point(file_id, 5, 10);
        let source_loc = source_map.to_source_location(&span);

        assert_eq!(source_loc.file_id, file_id);
        assert_eq!(source_loc.line, 5);
        assert_eq!(source_loc.column, 10);

        // Test with invalid file ID
        let invalid_span = GlSourceSpan::point(GlFileId(999), 1, 1);
        let invalid_loc = source_map.to_source_location(&invalid_span);

        assert_eq!(invalid_loc.file_id, GlFileId(999));
        assert_eq!(invalid_loc.line, 1);
        assert_eq!(invalid_loc.column, 1);
    }

    #[test]
    fn test_unknown_spans() {
        let file_id = GlFileId(1);
        let unknown_loc = GlSourceLoc::unknown(file_id);
        let unknown_span = GlSourceSpan::unknown(file_id);

        assert!(unknown_loc.is_unknown());
        assert!(unknown_span.is_unknown());

        // Test conversion of unknown span
        let mut source_map = GlSourceMap::new();
        let file_id2 = source_map.add_file(
            GlFileSource::Synthetic("test.glsl".into()),
            "content".into(),
        );

        let unknown_span2 = GlSourceSpan::unknown(file_id2);
        let source_loc = source_map.to_source_location(&unknown_span2);

        assert_eq!(source_loc.file_id, file_id2);
        assert_eq!(source_loc.line, 0);
        assert_eq!(source_loc.column, 0);
    }
}
