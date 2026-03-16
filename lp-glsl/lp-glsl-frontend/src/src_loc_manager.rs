//! Source location manager for mapping source location IDs to GLSL source positions.
//!
//! Uses an opaque SourceLocId (u32) so the frontend has no Cranelift dependency.
//! The Cranelift codegen converts SourceLocId <-> cranelift SourceLoc at the boundary.

use hashbrown::HashMap;

/// Opaque source location ID. Maps to (line, column) in GLSL source.
/// Conversion to/from cranelift_codegen::ir::SourceLoc happens in the codegen layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceLocId(pub u32);

impl SourceLocId {
    /// ID for default/unknown location (matches cranelift SourceLoc::default() bits).
    pub const DEFAULT: SourceLocId = SourceLocId(u32::MAX);

    /// Create from raw bits (e.g., from cranelift SourceLoc::bits()).
    pub fn from_bits(bits: u32) -> Self {
        SourceLocId(bits)
    }

    /// Get raw bits (e.g., for cranelift SourceLoc::new).
    pub fn bits(self) -> u32 {
        self.0
    }

    pub fn is_default(self) -> bool {
        self.0 == u32::MAX
    }
}

/// Manages the mapping from SourceLocId to GLSL source positions.
///
/// SourceLocId values are opaque u32 identifiers. This manager creates
/// SourceLocId values and maintains a mapping back to the original GLSL
/// source line and column.
#[derive(Clone, Debug)]
pub struct SourceLocManager {
    /// Next ID to assign to a new SourceLocId
    next_id: u32,
    /// Mapping from source loc ID -> (line, column)
    mapping: HashMap<u32, (usize, usize)>,
}

impl SourceLocManager {
    /// Create a new SourceLocManager.
    pub fn new() -> Self {
        Self {
            next_id: 1, // Start at 1, 0 is reserved for default in some systems
            mapping: HashMap::new(),
        }
    }

    /// Create a SourceLocId from a GLSL SourceSpan and store the mapping.
    ///
    /// Returns the SourceLocId. Convert to cranelift SourceLoc via
    /// codegen::srcloc::to_cranelift_srcloc() when passing to Cranelift.
    pub fn create_srcloc(&mut self, span: &glsl::syntax::SourceSpan) -> SourceLocId {
        // Skip if span is unknown
        if span.is_unknown() {
            return SourceLocId::DEFAULT;
        }

        let id = self.next_id;
        self.next_id += 1;
        self.mapping.insert(id, (span.line, span.column));
        SourceLocId(id)
    }

    /// Look up the line and column for a given SourceLocId.
    ///
    /// Returns None if the ID is not found or is the default.
    pub fn lookup_srcloc(&self, id: SourceLocId) -> Option<(usize, usize)> {
        if id.is_default() {
            return None;
        }
        self.mapping.get(&id.0).copied()
    }

    /// Get all mappings (for debugging/testing).
    #[cfg(test)]
    pub fn all_mappings(&self) -> &HashMap<u32, (usize, usize)> {
        &self.mapping
    }

    /// Merge mappings from another SourceLocManager into this one.
    /// This is used to combine SourceLocManagers from multiple function compilations.
    pub fn merge_from(&mut self, other: &SourceLocManager) {
        self.next_id = self.next_id.max(other.next_id);
        for (id, pos) in &other.mapping {
            self.mapping.insert(*id, *pos);
        }
    }
}

impl Default for SourceLocManager {
    fn default() -> Self {
        Self::new()
    }
}
