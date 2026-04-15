# Phase 8: Define RegionTree Structure

## Scope

Define the arena-based region tree types that will be built during lowering in M4. This phase just defines the types; actual construction happens in M4.

## Implementation

### 1. Create `region.rs`

```rust
//! Arena-based region tree for structured control flow.
//!
//! Built during lowering (M4) from LPIR's structured control flow.
//! Replaces Box<Region> with index-based arena for memory efficiency.

use alloc::vec::Vec;
use alloc::format;
use alloc::string::String;

/// Index into RegionTree.nodes.
pub type RegionId = u16;

/// Sentinel for invalid region.
pub const REGION_ID_NONE: RegionId = u16::MAX;

/// Region of VInsts with structured control flow.
/// Indices are into the flat VInst slice (no copies).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Region {
    /// Linear VInst range [start..end), no internal branches.
    Linear {
        start: u16,
        end: u16,
    },
    
    /// if (BrIf at head_end-1) { then_body } else { else_body }
    IfThenElse {
        head: RegionId,        // ends with BrIf
        then_body: RegionId,
        else_body: RegionId,   // may be REGION_ID_NONE if empty
    },
    
    /// loop { header; body; back-edge }
    Loop {
        header: RegionId,
        body: RegionId,
    },
    
    /// Sequential composition.
    /// Children are stored in RegionTree.seq_children at [start..start+count).
    Seq {
        children_start: u16,
        child_count: u16,
    },
}

/// Arena-based region tree.
/// All regions stored in a single Vec, no Box allocations.
#[derive(Clone, Debug)]
pub struct RegionTree {
    /// All regions in the tree.
    pub nodes: Vec<Region>,
    /// Storage for Seq children (shared across all Seq regions).
    pub seq_children: Vec<RegionId>,
    /// Root region id.
    pub root: RegionId,
}

impl RegionTree {
    /// Create empty tree.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            seq_children: Vec::new(),
            root: REGION_ID_NONE,
        }
    }
    
    /// Allocate a new region, return its id.
    pub fn alloc(&mut self, region: Region) -> RegionId {
        let id = self.nodes.len() as RegionId;
        self.nodes.push(region);
        id
    }
    
    /// Get region by id.
    pub fn get(&self, id: RegionId) -> Option<&Region> {
        self.nodes.get(id as usize)
    }
    
    /// Get region mutably.
    pub fn get_mut(&mut self, id: RegionId) -> Option<&mut Region> {
        self.nodes.get_mut(id as usize)
    }
    
    /// Append seq children, return start index.
    pub fn push_seq_children(&mut self, children: &[RegionId]) -> u16 {
        let start = self.seq_children.len() as u16;
        self.seq_children.extend_from_slice(children);
        start
    }
    
    /// Get slice of seq children.
    pub fn seq_children(&self, start: u16, count: u16) -> &[RegionId] {
        let end = start as usize + count as usize;
        &self.seq_children[start as usize..end]
    }
}

impl Default for RegionTree {
    fn default() -> Self {
        Self::new()
    }
}

/// Format region tree for debug output.
pub fn format_region(tree: &RegionTree, vinsts: &[VInst], indent: usize) -> String 
where 
    VInst: core::fmt::Debug, // placeholder constraint
{
    fn format_node(
        tree: &RegionTree,
        id: RegionId,
        vinsts: &[VInst],
        indent: usize,
        lines: &mut Vec<String>,
    ) {
        let prefix = "  ".repeat(indent);
        
        if let Some(region) = tree.get(id) {
            match region {
                Region::Linear { start, end } => {
                    lines.push(format!("{}Linear [{}..{}]", prefix, start, end));
                }
                Region::IfThenElse { head, then_body, else_body } => {
                    lines.push(format!("{}IfThenElse", prefix));
                    lines.push(format!("{}  head:", prefix));
                    format_node(tree, *head, vinsts, indent + 2, lines);
                    lines.push(format!("{}  then:", prefix));
                    format_node(tree, *then_body, vinsts, indent + 2, lines);
                    if *else_body != REGION_ID_NONE {
                        lines.push(format!("{}  else:", prefix));
                        format_node(tree, *else_body, vinsts, indent + 2, lines);
                    }
                }
                Region::Loop { header, body } => {
                    lines.push(format!("{}Loop", prefix));
                    lines.push(format!("{}  header:", prefix));
                    format_node(tree, *header, vinsts, indent + 2, lines);
                    lines.push(format!("{}  body:", prefix));
                    format_node(tree, *body, vinsts, indent + 2, lines);
                }
                Region::Seq { children_start, child_count } => {
                    lines.push(format!("{}Seq", prefix));
                    let children = tree.seq_children(*children_start, *child_count);
                    for child_id in children {
                        format_node(tree, *child_id, vinsts, indent + 1, lines);
                    }
                }
            }
        } else {
            lines.push(format!("{}<invalid region {}>", prefix, id));
        }
    }
    
    let mut lines = Vec::new();
    if tree.root != REGION_ID_NONE {
        format_node(tree, tree.root, vinsts, indent, &mut lines);
    } else {
        lines.push(format!("{}<empty tree>", "  ".repeat(indent)));
    }
    
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_region_tree_alloc() {
        let mut tree = RegionTree::new();
        let id = tree.alloc(Region::Linear { start: 0, end: 5 });
        assert_eq!(id, 0);
        assert_eq!(tree.nodes.len(), 1);
    }
    
    #[test]
    fn test_region_tree_seq() {
        let mut tree = RegionTree::new();
        
        // Allocate children
        let child1 = tree.alloc(Region::Linear { start: 0, end: 2 });
        let child2 = tree.alloc(Region::Linear { start: 2, end: 4 });
        
        // Create seq region with children
        let seq_start = tree.push_seq_children(&[child1, child2]);
        let seq = tree.alloc(Region::Seq { 
            children_start: seq_start, 
            child_count: 2 
        });
        
        tree.root = seq;
        
        // Verify
        let children = tree.seq_children(seq_start, 2);
        assert_eq!(children, &[child1, child2]);
    }
    
    #[test]
    fn test_region_tree_if_then_else() {
        let mut tree = RegionTree::new();
        
        let head = tree.alloc(Region::Linear { start: 0, end: 1 });
        let then_body = tree.alloc(Region::Linear { start: 1, end: 3 });
        let else_body = tree.alloc(Region::Linear { start: 3, end: 5 });
        
        let ifte = tree.alloc(Region::IfThenElse {
            head,
            then_body,
            else_body,
        });
        
        tree.root = ifte;
        
        // Just verify it doesn't panic
        let _ = format_region(&tree, &[], 0);
    }
    
    #[test]
    fn test_region_tree_size() {
        use core::mem::size_of;
        // Region should be compact (discriminant + 3×u16 or similar)
        assert!(size_of::<Region>() <= 16);
        assert_eq!(size_of::<RegionId>(), 2);
    }
}
```

### 2. Update `lib.rs`

Export the new types:

```rust
pub mod region;
pub use region::{Region, RegionTree, RegionId, REGION_ID_NONE};
```

### 3. Update `LoweredFunction` (placeholder for M4)

Add the region field to LoweredFunction (will be populated in M4):

```rust
pub struct LoweredFunction {
    pub vinsts: Vec<VInst>,
    pub vreg_pool: Vec<VReg>,
    pub regions: RegionTree,  // NEW - empty until M4
    pub loop_regions: Vec<LoopRegion>,
}
```

## Validate

```bash
cargo test -p lpvm-native --lib -- region
```

Tests should verify:
- Region allocation works
- Seq children storage works
- Formatting produces expected output
- Sizes are reasonable (< 16 bytes per Region)
