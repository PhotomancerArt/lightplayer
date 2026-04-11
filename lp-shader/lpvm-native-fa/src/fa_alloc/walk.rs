//! Backward walk allocator shell with stubbed decisions.

use crate::region::{Region, RegionId, RegionTree, REGION_ID_NONE};
use crate::vinst::{VInst, VReg};
use super::trace::{AllocTrace, stub_entry};

/// Walk a region backward, recording stubbed decisions to trace.
/// M4: Handles Linear and Seq regions. IfThenElse/Loop are stubbed.
pub fn walk_region_stub(
    tree: &RegionTree,
    region_id: RegionId,
    vinsts: &[VInst],
    _pool: &[VReg],
    trace: &mut AllocTrace,
) {
    if region_id == REGION_ID_NONE {
        return;
    }

    match &tree.nodes[region_id as usize] {
        Region::Linear { start, end } => {
            // Walk instructions in reverse order
            for i in (*start..*end).rev() {
                let vinst = &vinsts[i as usize];
                let detail = stub_detail(vinst);
                trace.push(stub_entry(i as usize, vinst.mnemonic(), detail));
            }
        }

        Region::Seq { children_start, child_count } => {
            let start = *children_start as usize;
            let end = start + *child_count as usize;
            // Walk children in reverse order
            for &child_id in tree.seq_children[start..end].iter().rev() {
                walk_region_stub(tree, child_id, vinsts, _pool, trace);
            }
        }

        Region::IfThenElse { head, then_body, else_body } => {
            // M4 stub: walk each branch, note the structure
            walk_region_stub(tree, *else_body, vinsts, _pool, trace);
            walk_region_stub(tree, *then_body, vinsts, _pool, trace);
            walk_region_stub(tree, *head, vinsts, _pool, trace);
        }

        Region::Loop { header, body } => {
            // M4 stub: walk body then header
            walk_region_stub(tree, *body, vinsts, _pool, trace);
            walk_region_stub(tree, *header, vinsts, _pool, trace);
        }
    }
}

fn stub_detail(vinst: &VInst) -> &'static str {
    match vinst {
        VInst::IConst32 { .. } => "def (remat candidate)",
        VInst::Add32 { .. }
        | VInst::Sub32 { .. }
        | VInst::Mul32 { .. }
        | VInst::And32 { .. }
        | VInst::Or32 { .. }
        | VInst::Xor32 { .. }
        | VInst::Shl32 { .. }
        | VInst::ShrS32 { .. }
        | VInst::ShrU32 { .. }
        | VInst::DivS32 { .. }
        | VInst::DivU32 { .. }
        | VInst::RemS32 { .. }
        | VInst::RemU32 { .. } => "binop def+use",
        VInst::Neg32 { .. } | VInst::Bnot32 { .. } => "unop def+use",
        VInst::Mov32 { .. } => "copy",
        VInst::Icmp32 { .. } | VInst::IeqImm32 { .. } => "cmp def+use",
        VInst::Select32 { .. } => "select def+3use",
        VInst::Load32 { .. } => "load def+use",
        VInst::Store32 { .. } => "store 2use",
        VInst::SlotAddr { .. } => "slot_addr def",
        VInst::MemcpyWords { .. } => "memcpy 2use",
        VInst::Call { .. } => "call (clobber caller-saved)",
        VInst::Ret { .. } => "ret",
        VInst::Br { .. } => "branch",
        VInst::BrIf { .. } => "cond_branch use",
        VInst::Label(..) => "label",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::{Region, RegionTree};
    use crate::vinst::{VInst, VReg, SRC_OP_NONE};
    use super::super::trace::AllocTrace;
    use alloc::vec::Vec;

    fn test_vinsts() -> Vec<VInst> {
        vec![
            VInst::IConst32 { dst: VReg(0), val: 1, src_op: SRC_OP_NONE },
            VInst::IConst32 { dst: VReg(1), val: 2, src_op: SRC_OP_NONE },
            VInst::Add32 { dst: VReg(2), src1: VReg(0), src2: VReg(1), src_op: SRC_OP_NONE },
        ]
    }

    #[test]
    fn walk_linear_backward() {
        let vinsts = test_vinsts();
        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: 3 });
        tree.root = root;

        let mut trace = AllocTrace::new();
        walk_region_stub(&tree, root, &vinsts, &[], &mut trace);

        assert_eq!(trace.entries.len(), 3);
        // Walked backward: last instruction first
        assert_eq!(trace.entries[0].vinst_idx, 2);
        assert_eq!(trace.entries[2].vinst_idx, 0);
    }

    #[test]
    fn walk_then_reverse_gives_forward_order() {
        let vinsts = test_vinsts();
        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: 3 });
        tree.root = root;

        let mut trace = AllocTrace::new();
        walk_region_stub(&tree, root, &vinsts, &[], &mut trace);
        trace.reverse();

        assert_eq!(trace.entries[0].vinst_idx, 0);
        assert_eq!(trace.entries[2].vinst_idx, 2);
    }

    #[test]
    fn walk_empty_region() {
        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: 0 });
        tree.root = root;

        let mut trace = AllocTrace::new();
        walk_region_stub(&tree, root, &[], &[], &mut trace);
        assert!(trace.is_empty());
    }

    #[test]
    fn walk_if_then_else() {
        // Structure: BrIf (head), then block, else block
        let vinsts = vec![
            VInst::BrIf { cond: VReg(0), target: 1, invert: false, src_op: SRC_OP_NONE }, // 0: head
            VInst::IConst32 { dst: VReg(1), val: 1, src_op: SRC_OP_NONE },              // 1: then
            VInst::Br { target: 2, src_op: SRC_OP_NONE },                                // 2: branch to merge
            VInst::IConst32 { dst: VReg(1), val: 2, src_op: SRC_OP_NONE },              // 3: else
        ];

        let mut tree = RegionTree::new();
        let head = tree.push(Region::Linear { start: 0, end: 1 });
        let then_body = tree.push(Region::Linear { start: 1, end: 3 });
        let else_body = tree.push(Region::Linear { start: 3, end: 4 });
        let root = tree.push(Region::IfThenElse { head, then_body, else_body });
        tree.root = root;

        let mut trace = AllocTrace::new();
        walk_region_stub(&tree, root, &vinsts, &[], &mut trace);

        // Walk order: else (backward) -> then (backward) -> head (backward)
        // else: [3] (backward)
        // then: [2, 1] (backward)
        // head: [0] (backward)
        // Total: [3, 2, 1, 0]
        assert_eq!(trace.entries.len(), 4);
        assert_eq!(trace.entries[0].vinst_idx, 3); // else
        assert_eq!(trace.entries[1].vinst_idx, 2); // then end
        assert_eq!(trace.entries[2].vinst_idx, 1); // then start
        assert_eq!(trace.entries[3].vinst_idx, 0); // head
    }

    #[test]
    fn walk_loop() {
        // Structure: header label, body
        let vinsts = vec![
            VInst::Label(0, SRC_OP_NONE),                                               // 0: header
            VInst::IConst32 { dst: VReg(0), val: 0, src_op: SRC_OP_NONE },              // 1: body start
            VInst::Add32 { dst: VReg(0), src1: VReg(0), src2: VReg(0), src_op: SRC_OP_NONE }, // 2: body end
        ];

        let mut tree = RegionTree::new();
        let header = tree.push(Region::Linear { start: 0, end: 1 });
        let body = tree.push(Region::Linear { start: 1, end: 3 });
        let root = tree.push(Region::Loop { header, body });
        tree.root = root;

        let mut trace = AllocTrace::new();
        walk_region_stub(&tree, root, &vinsts, &[], &mut trace);

        // Walk order: body -> header (reverse of execution)
        assert_eq!(trace.entries.len(), 3);
        assert_eq!(trace.entries[0].vinst_idx, 2); // body end
        assert_eq!(trace.entries[2].vinst_idx, 0); // header
    }
}
