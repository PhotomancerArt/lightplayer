//! Local peephole optimizations on a flattened [`crate::vinst::VInst`] sequence.

use alloc::vec::Vec;

use crate::vinst::VInst;

/// Apply local peephole optimizations to a lowered function.
///
/// Only runs when the region tree is a single Linear root (or empty); otherwise
/// removing VInsts would invalidate the region indices.
pub fn optimize_lowered(lowered: &mut crate::lower::LoweredFunction) {
    let is_linear_only = lowered.region_tree.nodes.len() <= 1;
    if !is_linear_only {
        return;
    }
    optimize(&mut lowered.vinsts);
    if let Some(root) = lowered.region_tree.nodes.first_mut() {
        if let crate::region::Region::Linear { end, .. } = root {
            *end = lowered.vinsts.len() as u16;
        }
    }
}

/// Apply local peephole optimizations in place (raw VInst vec).
///
/// Removes redundant unconditional branches whose target is the next instruction:
/// `Br { target: X }` followed by `Label(X, _)` -> keep only `Label(X, _)` (fall-through).
///
/// **Caution**: This invalidates any region tree indices. Use [`optimize_lowered`] instead
/// when a region tree exists.
pub fn optimize(vinsts: &mut Vec<VInst>) {
    let len = vinsts.len();
    let mut write = 0usize;
    let mut read = 0usize;

    while read < len {
        if read + 1 < len {
            if let (VInst::Br { target, .. }, VInst::Label(label_id, _)) =
                (&vinsts[read], &vinsts[read + 1])
            {
                if *target == *label_id {
                    vinsts[write] = vinsts[read + 1].clone();
                    write += 1;
                    read += 2;
                    continue;
                }
            }
        }
        if write != read {
            vinsts[write] = vinsts[read].clone();
        }
        write += 1;
        read += 1;
    }
    vinsts.truncate(write);
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;
    use crate::vinst::{SRC_OP_NONE, VInst, VReg, pack_src_op};

    #[test]
    fn removes_br_before_matching_label() {
        let mut v = vec![
            VInst::Br {
                target: 1,
                src_op: SRC_OP_NONE,
            },
            VInst::Label(1, SRC_OP_NONE),
        ];
        optimize(&mut v);
        assert_eq!(v.len(), 1);
        assert!(matches!(v[0], VInst::Label(1, SRC_OP_NONE)));
    }

    #[test]
    fn preserves_br_when_label_differs() {
        let mut v = vec![
            VInst::Br {
                target: 1,
                src_op: SRC_OP_NONE,
            },
            VInst::Label(2, SRC_OP_NONE),
        ];
        optimize(&mut v);
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn removes_br_in_middle_of_stream() {
        let mut v = vec![
            VInst::IConst32 {
                dst: VReg(0),
                val: 0,
                src_op: SRC_OP_NONE,
            },
            VInst::Br {
                target: 7,
                src_op: SRC_OP_NONE,
            },
            VInst::Label(7, pack_src_op(Some(0))),
            VInst::IConst32 {
                dst: VReg(1),
                val: 1,
                src_op: SRC_OP_NONE,
            },
        ];
        optimize(&mut v);
        assert_eq!(v.len(), 3);
        assert!(matches!(v[0], VInst::IConst32 { .. }));
        assert!(matches!(v[1], VInst::Label(7, po) if po == pack_src_op(Some(0))));
        assert!(matches!(v[2], VInst::IConst32 { .. }));
    }

    #[test]
    fn empty_vec() {
        let mut v: Vec<VInst> = vec![];
        optimize(&mut v);
        assert!(v.is_empty());
    }

    #[test]
    fn single_instruction_unchanged() {
        let mut v = vec![VInst::Label(0, SRC_OP_NONE)];
        optimize(&mut v);
        assert_eq!(v.len(), 1);
    }
}
