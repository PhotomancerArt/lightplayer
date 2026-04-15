# Phase 5: Backward Walk Shell

## Scope

Implement backward walk with stubbed decisions that logs to trace. The walk operates on the region tree.

## Implementation

### 1. Implement walk shell in `alloc/walk.rs`

```rust
use crate::region::{Region, RegionId, RegionTree, REGION_ID_NONE};
use crate::vinst::{ModuleSymbols, VInst, VReg};
use super::trace::{AllocTrace, TraceEntry, stub_entry};

/// Walk a region backward, recording stubbed decisions to trace.
/// M4: Handles Linear and Seq regions.
pub fn walk_region_stub(
    tree: &RegionTree,
    region_id: RegionId,
    vinsts: &[VInst],
    pool: &[VReg],
    trace: &mut AllocTrace,
) {
    if region_id == REGION_ID_NONE {
        return;
    }

    match &tree.nodes[region_id as usize] {
        Region::Linear { start, end } => {
            for i in (*start..*end).rev() {
                let vinst = &vinsts[i as usize];
                let detail = stub_detail(vinst, pool);
                trace.push(stub_entry(i as usize, vinst.mnemonic(), &detail));
            }
        }

        Region::Seq { children_start, child_count } => {
            let start = *children_start as usize;
            let end = start + *child_count as usize;
            // Walk children in reverse order
            for &child_id in tree.seq_children[start..end].iter().rev() {
                walk_region_stub(tree, child_id, vinsts, pool, trace);
            }
        }

        Region::IfThenElse { head, then_body, else_body } => {
            // M4 stub: walk each branch, note the structure
            walk_region_stub(tree, *else_body, vinsts, pool, trace);
            walk_region_stub(tree, *then_body, vinsts, pool, trace);
            walk_region_stub(tree, *head, vinsts, pool, trace);
        }

        Region::Loop { header, body } => {
            // M4 stub: walk body then header
            walk_region_stub(tree, *body, vinsts, pool, trace);
            walk_region_stub(tree, *header, vinsts, pool, trace);
        }
    }
}

fn stub_detail(vinst: &VInst, pool: &[VReg]) -> &'static str {
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
```

### 2. Add unit tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::{Region, RegionTree};
    use crate::vinst::{VInst, VReg, SRC_OP_NONE};
    use super::super::trace::AllocTrace;

    fn test_vinsts() -> Vec<VInst> {
        vec![
            VInst::IConst32 { dst: VReg(0), val: 1, src_op: SRC_OP_NONE },
            VInst::IConst32 { dst: VReg(1), val: 2, src_op: SRC_OP_NONE },
            VInst::Add32 { dst: VReg(2), src1: VReg(0), src2: VReg(1), src_op: SRC_OP_NONE },
        ];
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
}
```

## Validate

```bash
cargo test -p lpvm-native --lib -- alloc::walk
```
