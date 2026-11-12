# Phase 3: LineTable Structure

## Scope

Create `LineTable` and `LineEntry` structures for binary-searchable PC-to-source mapping.

## Code Organization Reminders

- Place in new `debug/mod.rs` module
- Keep structure simple and extensible
- Sort by offset for binary search
- Add lookup method for PC → LineEntry resolution

## Implementation Details

### Create `isa/rv32/debug/mod.rs`

```rust
//! Debug information tracking for RV32 emission.
//!
//! Provides:
//! - LineTable: Maps instruction offsets to source LPIR operations
//! - Future: extension point for DWARF .debug_line generation

use alloc::vec::Vec;

/// Entry in the line table mapping code offset to source information.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineEntry {
    /// Instruction offset in the code section (bytes)
    pub offset: u32,
    /// Index of the originating LPIR operation
    pub src_op: u32,
}

/// Binary-searchable table mapping instruction offsets to source locations.
///
/// This structure is designed to support:
/// 1. Annotated disassembly (offset → src_op → LPIR display)
/// 2. Emulator debugging (PC → src_op lookup)
/// 3. Future DWARF .debug_line generation
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineTable {
    /// Sorted by offset for binary search
    entries: Vec<LineEntry>,
}

impl LineTable {
    /// Create an empty line table
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    /// Create from raw (offset, src_op) pairs
    ///
    /// # Arguments
    /// * `pairs` - Vector of (offset, src_op) from EmitContext
    ///
    /// Automatically sorts by offset and deduplicates.
    pub fn from_pairs(pairs: &[(u32, Option<u32>)]) -> Self {
        let mut entries: Vec<LineEntry> = pairs
            .iter()
            .filter_map(|(offset, src_op)| {
                src_op.map(|op| LineEntry {
                    offset: *offset,
                    src_op: op,
                })
            })
            .collect();
        
        // Sort by offset
        entries.sort_by_key(|e| e.offset);
        
        // Deduplicate: keep the first entry for each offset
        entries.dedup_by_key(|e| e.offset);
        
        Self { entries }
    }

    /// Look up the LineEntry for a given PC offset.
    ///
    /// Returns the entry with the largest offset <= pc.
    /// This gives the source location for the instruction at or before the PC.
    pub fn lookup(&self, pc: u32) -> Option<&LineEntry> {
        // Binary search for the largest offset <= pc
        match self.entries.binary_search_by_key(&pc, |e| e.offset) {
            Ok(idx) => Some(&self.entries[idx]),
            Err(0) => None, // All entries have offset > pc
            Err(idx) => Some(&self.entries[idx - 1]), // idx is first > pc, so idx-1 is <= pc
        }
    }

    /// Iterator over all entries
    pub fn entries(&self) -> &[LineEntry] {
        &self.entries
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

impl Default for LineTable {
    fn default() -> Self {
        Self::new()
    }
}
```

### Add debug module re-export

In `isa/rv32/mod.rs`:

```rust
pub mod debug;
```

In `lib.rs` (optional, feature-gated):

```rust
#[cfg(feature = "std")]
pub mod debug;
```

Or just let users access via `isa::rv32::debug`.

## Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_table_lookup_none() {
        let table = LineTable::new();
        assert!(table.lookup(0).is_none());
        assert!(table.lookup(100).is_none());
    }

    #[test]
    fn from_pairs_creates_sorted_table() {
        let pairs = vec![
            (8, Some(2)),
            (0, Some(0)),
            (4, Some(1)),
        ];
        let table = LineTable::from_pairs(&pairs);
        
        assert_eq!(table.len(), 3);
        assert_eq!(table.entries[0].offset, 0);
        assert_eq!(table.entries[0].src_op, 0);
        assert_eq!(table.entries[1].offset, 4);
        assert_eq!(table.entries[1].src_op, 1);
        assert_eq!(table.entries[2].offset, 8);
        assert_eq!(table.entries[2].src_op, 2);
    }

    #[test]
    fn from_pairs_filters_none() {
        let pairs = vec![
            (0, Some(0)),
            (4, None),        // Should be filtered out
            (8, Some(1)),
        ];
        let table = LineTable::from_pairs(&pairs);
        
        assert_eq!(table.len(), 2);
        assert_eq!(table.entries[0].offset, 0);
        assert_eq!(table.entries[1].offset, 8);
    }

    #[test]
    fn from_pairs_deduplicates() {
        let pairs = vec![
            (0, Some(0)),
            (0, Some(1)),  // Duplicate offset, should be deduped
            (4, Some(2)),
        ];
        let table = LineTable::from_pairs(&pairs);
        
        assert_eq!(table.len(), 2);
        assert_eq!(table.entries[0].offset, 0);
        assert_eq!(table.entries[0].src_op, 0); // First one kept
        assert_eq!(table.entries[1].offset, 4);
    }

    #[test]
    fn lookup_exact_match() {
        let pairs = vec![
            (0, Some(0)),
            (4, Some(1)),
            (8, Some(2)),
        ];
        let table = LineTable::from_pairs(&pairs);
        
        let entry = table.lookup(4).expect("found");
        assert_eq!(entry.offset, 4);
        assert_eq!(entry.src_op, 1);
    }

    #[test]
    fn lookup_between_entries() {
        let pairs = vec![
            (0, Some(0)),
            (4, Some(1)),
            (8, Some(2)),
        ];
        let table = LineTable::from_pairs(&pairs);
        
        // PC 6 is between entries at 4 and 8
        // Should return entry at 4 (largest <= 6)
        let entry = table.lookup(6).expect("found");
        assert_eq!(entry.offset, 4);
        assert_eq!(entry.src_op, 1);
    }

    #[test]
    fn lookup_before_first() {
        let pairs = vec![
            (4, Some(0)),
            (8, Some(1)),
        ];
        let table = LineTable::from_pairs(&pairs);
        
        // PC 2 is before first entry at 4
        assert!(table.lookup(2).is_none());
    }

    #[test]
    fn lookup_after_last() {
        let pairs = vec![
            (0, Some(0)),
            (4, Some(1)),
        ];
        let table = LineTable::from_pairs(&pairs);
        
        // PC 10 is after last entry at 4
        // Should return last entry (largest <= 10)
        let entry = table.lookup(10).expect("found");
        assert_eq!(entry.offset, 4);
        assert_eq!(entry.src_op, 1);
    }
}
```

## Validate

```bash
cargo check -p lpvm-native
cargo test -p lpvm-native --lib line_table
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf
```
