# Phase 3: Liveness Analysis

## Scope

Implement recursive liveness analysis for the region tree using `RegSet` (no-heap bitset).

## Implementation

### 1. Implement recursive liveness in `alloc/liveness.rs`

```rust
use crate::region::{Region, RegionId, RegionTree, REGION_ID_NONE};
use crate::regset::RegSet;
use crate::vinst::{VInst, VReg};

#[derive(Debug, Clone)]
pub struct Liveness {
    pub live_in: RegSet,
    pub live_out: RegSet,
}

/// Analyze liveness recursively on region tree.
/// M4: Linear and Seq regions. IfThenElse/Loop are conservative (empty).
pub fn analyze_liveness(
    tree: &RegionTree,
    region_id: RegionId,
    vinsts: &[VInst],
    pool: &[VReg],
) -> Liveness {
    if region_id == REGION_ID_NONE {
        return Liveness { live_in: RegSet::new(), live_out: RegSet::new() };
    }

    match &tree.nodes[region_id as usize] {
        Region::Linear { start, end } => {
            let mut live = RegSet::new();

            for i in (*start..*end).rev() {
                let vinst = &vinsts[i as usize];
                // Remove defs
                vinst.for_each_def(pool, |d| { live.remove(d); });
                // Add uses
                vinst.for_each_use(pool, |u| { live.insert(u); });
            }

            Liveness {
                live_in: live,
                live_out: RegSet::new(),
            }
        }

        Region::Seq { children_start, child_count } => {
            let start = *children_start as usize;
            let end = start + *child_count as usize;
            let mut combined = RegSet::new();

            for &child_id in &tree.seq_children[start..end] {
                let child_liveness = analyze_liveness(tree, child_id, vinsts, pool);
                combined = combined.union(&child_liveness.live_in);
            }

            Liveness {
                live_in: combined,
                live_out: RegSet::new(),
            }
        }

        // M5: IfThenElse, Loop handling
        _ => Liveness {
            live_in: RegSet::new(),
            live_out: RegSet::new(),
        }
    }
}

/// Format liveness for debug output.
pub fn format_liveness(liveness: &Liveness) -> String {
    use alloc::format;
    use alloc::string::String;
    use alloc::vec::Vec;

    let mut lines: Vec<String> = Vec::new();
    lines.push("=== Liveness ===".into());

    let live_in: Vec<String> = liveness.live_in.iter().map(|v| format!("v{}", v.0)).collect();
    lines.push(format!("  live_in:  [{}]", live_in.join(", ")));

    let live_out: Vec<String> = liveness.live_out.iter().map(|v| format!("v{}", v.0)).collect();
    lines.push(format!("  live_out: [{}]", live_out.join(", ")));

    lines.join("\n")
}
```

Key design point: uses `VInst::for_each_def` and `VInst::for_each_use` which already exist on `VInst` and handle all instruction variants correctly, including `VRegSlice`-based `Call`/`Ret` with the `pool` parameter.

### 2. Add unit tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::{Region, RegionTree};
    use crate::vinst::{VInst, VReg, SRC_OP_NONE};

    #[test]
    fn liveness_simple_linear() {
        let vinsts = vec![
            VInst::IConst32 { dst: VReg(0), val: 1, src_op: SRC_OP_NONE },
            VInst::IConst32 { dst: VReg(1), val: 2, src_op: SRC_OP_NONE },
            VInst::Add32 { dst: VReg(2), src1: VReg(0), src2: VReg(1), src_op: SRC_OP_NONE },
        ];

        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: 3 });
        tree.root = root;

        let liveness = analyze_liveness(&tree, root, &vinsts, &[]);
        // v0, v1 are used by Add32 but defined by IConst32 before that
        // After backward walk: defs kill, uses add. All defs precede uses → live_in empty
        assert!(liveness.live_in.is_empty());
    }

    #[test]
    fn liveness_external_use() {
        // v0 is used but never defined in this region
        let vinsts = vec![
            VInst::Add32 { dst: VReg(1), src1: VReg(0), src2: VReg(0), src_op: SRC_OP_NONE },
        ];

        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: 1 });
        tree.root = root;

        let liveness = analyze_liveness(&tree, root, &vinsts, &[]);
        assert!(liveness.live_in.contains(VReg(0)));
        assert!(!liveness.live_in.contains(VReg(1)));
    }
}
```

## Validate

```bash
cargo test -p lpvm-native --lib -- alloc::liveness
```
