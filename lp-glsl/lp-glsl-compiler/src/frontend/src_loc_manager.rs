//! Source location manager for mapping Cranelift SourceLoc to GLSL source positions.
//!
//! Cranelift's SourceLoc is an opaque u32, so we maintain our own mapping
//! from SourceLoc -> (line, column) in the original GLSL source.

use cranelift_codegen::ir::SourceLoc;
use hashbrown::HashMap;

/// Manages the mapping from Cranelift SourceLoc to GLSL source positions.
///
/// SourceLoc values are opaque u32 identifiers. This manager creates
/// SourceLoc values and maintains a mapping back to the original GLSL
/// source line and column.
#[derive(Clone, Debug)]
pub struct SourceLocManager {
    /// Next ID to assign to a new SourceLoc
    next_id: u32,
    /// Mapping from SourceLoc -> (line, column)
    mapping: HashMap<SourceLoc, (usize, usize)>,
}

impl SourceLocManager {
    /// Create a new SourceLocManager.
    pub fn new() -> Self {
        Self {
            next_id: 1, // Start at 1, 0 is reserved for default SourceLoc
            mapping: HashMap::new(),
        }
    }

    /// Create a SourceLoc from a GLSL SourceSpan and store the mapping.
    ///
    /// Returns the SourceLoc that should be used with Cranelift instructions.
    pub fn create_srcloc(&mut self, span: &glsl::syntax::SourceSpan) -> SourceLoc {
        // Skip if span is unknown
        if span.is_unknown() {
            return SourceLoc::default();
        }

        let id = self.next_id;
        self.next_id += 1;
        let srcloc = SourceLoc::new(id);
        self.mapping.insert(srcloc, (span.line, span.column));
        srcloc
    }

    /// Look up the line and column for a given SourceLoc.
    ///
    /// Returns None if the SourceLoc is not found or is the default SourceLoc.
    pub fn lookup_srcloc(&self, srcloc: SourceLoc) -> Option<(usize, usize)> {
        if srcloc.is_default() {
            return None;
        }
        self.mapping.get(&srcloc).copied()
    }

    /// Get all mappings (for debugging/testing).
    #[cfg(test)]
    pub fn all_mappings(&self) -> &HashMap<SourceLoc, (usize, usize)> {
        &self.mapping
    }

    /// Merge mappings from another SourceLocManager into this one.
    /// This is used to combine SourceLocManagers from multiple function compilations.
    pub fn merge_from(&mut self, other: &SourceLocManager) {
        // Update next_id to be the maximum of both
        self.next_id = self.next_id.max(other.next_id);
        // Merge mappings (other takes precedence if there are conflicts)
        for (srcloc, pos) in &other.mapping {
            self.mapping.insert(*srcloc, *pos);
        }
    }
}

impl Default for SourceLocManager {
    fn default() -> Self {
        Self::new()
    }
}
