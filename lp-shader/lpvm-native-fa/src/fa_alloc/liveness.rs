//! Recursive liveness analysis for region tree.
//! Uses RegSet (fixed-size bitset, no heap).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::region::{Region, RegionId, RegionTree, REGION_ID_NONE};
use crate::regset::RegSet;
use crate::vinst::{VInst, VReg};

/// Liveness result for a region.
#[derive(Debug, Clone)]
pub struct Liveness {
    pub live_in: RegSet,
    pub live_out: RegSet,
}

/// Analyze liveness recursively on region tree.
/// M4: Only handles Linear regions. IfThenElse/Loop are conservative (empty).
pub fn analyze_liveness(
    tree: &RegionTree,
    region_id: RegionId,
    vinsts: &[VInst],
    pool: &[VReg],
) -> Liveness {
    if region_id == REGION_ID_NONE {
        return Liveness {
            live_in: RegSet::new(),
            live_out: RegSet::new(),
        };
    }

    match &tree.nodes[region_id as usize] {
        Region::Linear { start, end } => {
            let mut live = RegSet::new();

            // Walk instructions backward
            for i in (*start..*end).rev() {
                let vinst = &vinsts[i as usize];
                // Remove defs (they're dead before this point)
                vinst.for_each_def(pool, |d| {
                    live.remove(d);
                });
                // Add uses (they're live before this point)
                vinst.for_each_use(pool, |u| {
                    live.insert(u);
                });
            }

            Liveness {
                live_in: live,
                live_out: RegSet::new(),
            }
        }

        // M5: Implement IfThenElse, Loop, Seq
        _ => Liveness {
            live_in: RegSet::new(),
            live_out: RegSet::new(),
        }
    }
}

/// Format liveness for debug output.
pub fn format_liveness(liveness: &Liveness) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push("=== Liveness ===".into());

    let live_in: Vec<String> = liveness.live_in.iter().map(|v| format!("v{}", v.0)).collect();
    lines.push(format!("  live_in:  [{}]", live_in.join(", ")));

    let live_out: Vec<String> = liveness.live_out.iter().map(|v| format!("v{}", v.0)).collect();
    lines.push(format!("  live_out: [{}]", live_out.join(", ")));

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::{Region, RegionTree};
    use crate::vinst::{VInst, VReg, SRC_OP_NONE};
    use alloc::vec::Vec;

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
        // v0, v1 are defined then used in Add32; no external uses
        // After backward walk: defs kill before uses add, so live_in empty
        assert!(liveness.live_in.is_empty());
    }

    #[test]
    fn liveness_external_use() {
        // v0 is used but never defined in this region
        let vinsts = vec![VInst::Add32 {
            dst: VReg(1),
            src1: VReg(0),
            src2: VReg(0),
            src_op: SRC_OP_NONE,
        }];

        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: 1 });
        tree.root = root;

        let liveness = analyze_liveness(&tree, root, &vinsts, &[]);
        assert!(liveness.live_in.contains(VReg(0)));
        assert!(!liveness.live_in.contains(VReg(1)));
    }

    #[test]
    fn format_liveness_output() {
        let vinsts = vec![VInst::IConst32 {
            dst: VReg(0),
            val: 42,
            src_op: SRC_OP_NONE,
        }];

        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: 1 });
        tree.root = root;

        let liveness = analyze_liveness(&tree, root, &vinsts, &[]);
        let output = format_liveness(&liveness);

        assert!(output.contains("=== Liveness ==="));
        assert!(output.contains("live_in"));
    }
}
