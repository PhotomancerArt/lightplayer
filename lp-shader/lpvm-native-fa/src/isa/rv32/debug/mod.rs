//! Debug line table, annotated disassembly, and fastalloc PInst text.

pub mod disasm;
pub mod pinst;

use alloc::vec::Vec;

/// One mapping from a code offset to an LPIR op index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineEntry {
    /// Byte offset of the instruction word in the function's `.text` blob.
    pub offset: u32,
    /// Index into [`lpir::IrFunction::body`].
    pub src_op: u32,
}

/// Sorted by `offset`; supports PC → LPIR lookup for debuggers and core dumps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineTable {
    entries: Vec<LineEntry>,
}

impl LineTable {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Build from emitter `debug_lines` (offset, optional LPIR index).
    pub fn from_debug_lines(lines: &[(u32, Option<u32>)]) -> Self {
        let mut entries: Vec<LineEntry> = lines
            .iter()
            .filter_map(|(off, src)| {
                src.map(|s| LineEntry {
                    offset: *off,
                    src_op: s,
                })
            })
            .collect();
        entries.sort_by_key(|e| e.offset);
        Self { entries }
    }

    /// Largest entry with `offset <= pc`, or `None` if `pc` is before the first mapped instruction.
    ///
    /// Suitable for core dumps when `pc` may point inside a run of instructions from one LPIR op.
    /// For disassembly line-by-line, prefer [`Self::src_op_at_offset`].
    pub fn lookup(&self, pc: u32) -> Option<&LineEntry> {
        if self.entries.is_empty() {
            return None;
        }
        let i = self.entries.partition_point(|e| e.offset <= pc);
        if i == 0 {
            None
        } else {
            Some(&self.entries[i - 1])
        }
    }

    /// Exact match: RV32 word at `offset` was recorded with this LPIR op index (disassembly).
    pub fn src_op_at_offset(&self, offset: u32) -> Option<u32> {
        match self.entries.binary_search_by_key(&offset, |e| e.offset) {
            Ok(i) => Some(self.entries[i].src_op),
            Err(_) => None,
        }
    }

    pub fn entries(&self) -> &[LineEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn line_table_lookup_between() {
        let lines = vec![(0, Some(0u32)), (4, Some(1)), (8, Some(2))];
        let t = LineTable::from_debug_lines(&lines);
        assert_eq!(t.lookup(6).map(|e| e.src_op), Some(1));
    }

    #[test]
    fn line_table_lookup_before_first() {
        let lines = vec![(4, Some(0u32))];
        let t = LineTable::from_debug_lines(&lines);
        assert!(t.lookup(0).is_none());
    }

    #[test]
    fn line_table_lookup_after_last() {
        let lines = vec![(0, Some(0u32)), (4, Some(1))];
        let t = LineTable::from_debug_lines(&lines);
        assert_eq!(t.lookup(100).map(|e| e.src_op), Some(1));
    }

    #[test]
    fn src_op_at_offset_exact_only() {
        let lines = vec![(0, Some(0u32)), (4, Some(1))];
        let t = LineTable::from_debug_lines(&lines);
        assert_eq!(t.src_op_at_offset(4), Some(1));
        assert_eq!(t.src_op_at_offset(8), None);
    }
}
