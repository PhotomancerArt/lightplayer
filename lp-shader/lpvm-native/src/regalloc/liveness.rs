//! Recursive liveness analysis for region tree.
//! Uses RegSet (fixed-size bitset, no heap).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::region::{REGION_ID_NONE, Region, RegionId, RegionTree};
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

        Region::Seq {
            children_start,
            child_count,
        } => {
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

        Region::IfThenElse {
            head,
            then_body,
            else_body,
            ..
        } => {
            // live_in = head_live_in ∪ then_live_in ∪ else_live_in
            let head_liveness = analyze_liveness(tree, *head, vinsts, pool);
            let then_liveness = analyze_liveness(tree, *then_body, vinsts, pool);
            let else_liveness = if *else_body != REGION_ID_NONE {
                analyze_liveness(tree, *else_body, vinsts, pool)
            } else {
                Liveness {
                    live_in: RegSet::new(),
                    live_out: RegSet::new(),
                }
            };

            let mut combined = head_liveness.live_in;
            combined = combined.union(&then_liveness.live_in);
            combined = combined.union(&else_liveness.live_in);

            Liveness {
                live_in: combined,
                live_out: RegSet::new(),
            }
        }

        Region::Loop { header, body, .. } => {
            // Simple approximation: union of header and body live_in
            // With spill-at-boundary, over-approximation is safe (just more spills)
            let header_liveness = analyze_liveness(tree, *header, vinsts, pool);
            let body_liveness = analyze_liveness(tree, *body, vinsts, pool);

            let mut combined = header_liveness.live_in;
            combined = combined.union(&body_liveness.live_in);

            Liveness {
                live_in: combined,
                live_out: RegSet::new(),
            }
        }

        Region::Block { body, .. } => analyze_liveness(tree, *body, vinsts, pool),
    }
}

/// Every [`VReg`] defined by some instruction inside `region_id` (including nested regions).
///
/// Used to distinguish **loop-carried** values (redefined each iteration and live across the
/// back-edge) from **loop-invariant** inputs such as function parameters: the latter appear in
/// `body.live_in` but must not get spill-slot preassignment, or the first reload reads
/// uninitialized stack before any def has stored there.
pub fn defs_in_region(
    tree: &RegionTree,
    region_id: RegionId,
    vinsts: &[VInst],
    vreg_pool: &[VReg],
) -> RegSet {
    let mut out = RegSet::new();
    if region_id == REGION_ID_NONE {
        return out;
    }
    match &tree.nodes[region_id as usize] {
        Region::Linear { start, end } => {
            for i in *start..*end {
                vinsts[i as usize].for_each_def(vreg_pool, |d| out.insert(d));
            }
        }
        Region::Seq {
            children_start,
            child_count,
        } => {
            let s = *children_start as usize;
            let e = s + *child_count as usize;
            for &child in &tree.seq_children[s..e] {
                out = out.union(&defs_in_region(tree, child, vinsts, vreg_pool));
            }
        }
        Region::IfThenElse {
            head,
            then_body,
            else_body,
            ..
        } => {
            out = out.union(&defs_in_region(tree, *head, vinsts, vreg_pool));
            out = out.union(&defs_in_region(tree, *then_body, vinsts, vreg_pool));
            if *else_body != REGION_ID_NONE {
                out = out.union(&defs_in_region(tree, *else_body, vinsts, vreg_pool));
            }
        }
        Region::Loop { header, body, .. } => {
            out = out.union(&defs_in_region(tree, *header, vinsts, vreg_pool));
            out = out.union(&defs_in_region(tree, *body, vinsts, vreg_pool));
        }
        Region::Block { body, .. } => {
            out = out.union(&defs_in_region(tree, *body, vinsts, vreg_pool));
        }
    }
    out
}

/// Format liveness for debug output.
pub fn format_liveness(liveness: &Liveness) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push("=== Liveness ===".into());

    let live_in: Vec<String> = liveness
        .live_in
        .iter()
        .map(|v| format!("v{}", v.0))
        .collect();
    lines.push(format!("  live_in:  [{}]", live_in.join(", ")));

    let live_out: Vec<String> = liveness
        .live_out
        .iter()
        .map(|v| format!("v{}", v.0))
        .collect();
    lines.push(format!("  live_out: [{}]", live_out.join(", ")));

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::{Region, RegionTree};
    use crate::vinst::{AluOp, SRC_OP_NONE, VInst, VReg};

    #[test]
    fn liveness_simple_linear() {
        let vinsts = vec![
            VInst::IConst32 {
                dst: VReg(0),
                val: 1,
                src_op: SRC_OP_NONE,
            },
            VInst::IConst32 {
                dst: VReg(1),
                val: 2,
                src_op: SRC_OP_NONE,
            },
            VInst::AluRRR {
                op: AluOp::Add,
                dst: VReg(2),
                src1: VReg(0),
                src2: VReg(1),
                src_op: SRC_OP_NONE,
            },
        ];

        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: 3 });
        tree.root = root;

        let liveness = analyze_liveness(&tree, root, &vinsts, &[]);
        // v0, v1 are defined then used in AluRRR/Add; no external uses
        // After backward walk: defs kill before uses add, so live_in empty
        assert!(liveness.live_in.is_empty());
    }

    #[test]
    fn liveness_external_use() {
        // v0 is used but never defined in this region
        let vinsts = vec![VInst::AluRRR {
            op: AluOp::Add,
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

    #[test]
    fn defs_in_linear_region() {
        let vinsts = vec![
            VInst::IConst32 {
                dst: VReg(0),
                val: 1,
                src_op: SRC_OP_NONE,
            },
            VInst::IConst32 {
                dst: VReg(1),
                val: 2,
                src_op: SRC_OP_NONE,
            },
        ];
        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: 2 });
        tree.root = root;
        let d = defs_in_region(&tree, root, &vinsts, &[]);
        assert!(d.contains(VReg(0)));
        assert!(d.contains(VReg(1)));
        assert!(!d.contains(VReg(2)));
    }

    #[test]
    fn liveness_seq_combines_children() {
        // Region 0: defines v0
        // Region 1: uses v0
        let vinsts = vec![
            VInst::IConst32 {
                dst: VReg(0),
                val: 1,
                src_op: SRC_OP_NONE,
            },
            VInst::Neg {
                dst: VReg(1),
                src: VReg(0),
                src_op: SRC_OP_NONE,
            },
        ];

        let mut tree = RegionTree::new();
        let r1 = tree.push(Region::Linear { start: 0, end: 1 }); // defines v0
        let r2 = tree.push(Region::Linear { start: 1, end: 2 }); // uses v0
        let root = tree.push_seq(&[r1, r2]);
        tree.root = root;

        let liveness = analyze_liveness(&tree, root, &vinsts, &[]);
        // r1 live_in = {} (IConst32 defines v0, no uses)
        // r2 live_in = {v0} (Neg uses v0)
        // Combined = {} ∪ {v0} = {v0}
        assert!(liveness.live_in.contains(VReg(0)));
    }
}
