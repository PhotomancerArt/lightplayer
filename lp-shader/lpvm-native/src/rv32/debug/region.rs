//! Region tree display for debug output.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::region::{REGION_ID_NONE, Region, RegionId, RegionTree};
use crate::vinst::{ModuleSymbols, VInst, VReg};

/// Format the region tree as indented text.
pub fn format_region_tree(
    tree: &RegionTree,
    region_id: RegionId,
    vinsts: &[VInst],
    pool: &[VReg],
    symbols: &ModuleSymbols,
    indent: usize,
) -> String {
    if region_id == REGION_ID_NONE {
        return "(empty)".into();
    }

    let prefix = "  ".repeat(indent);
    let region = &tree.nodes[region_id as usize];
    let mut lines = Vec::new();

    match region {
        Region::Linear { start, end } => {
            lines.push(format!("{}Linear [{}..{})", prefix, start, end));
            for i in *start..*end {
                let v = &vinsts[i as usize];
                lines.push(format!(
                    "{}  {}: {} {}",
                    prefix,
                    i,
                    v.mnemonic(),
                    v.format_alloc_trace_detail(pool, symbols),
                ));
            }
        }

        Region::IfThenElse {
            head,
            then_body,
            else_body,
            else_label,
            merge_label,
        } => {
            lines.push(format!(
                "{}IfThenElse (else={} merge={})",
                prefix, else_label, merge_label
            ));
            lines.push(format!("{}  head:", prefix));
            lines.push(format_region_tree(
                tree,
                *head,
                vinsts,
                pool,
                symbols,
                indent + 2,
            ));
            lines.push(format!("{}  then:", prefix));
            lines.push(format_region_tree(
                tree,
                *then_body,
                vinsts,
                pool,
                symbols,
                indent + 2,
            ));
            lines.push(format!("{}  else:", prefix));
            lines.push(format_region_tree(
                tree,
                *else_body,
                vinsts,
                pool,
                symbols,
                indent + 2,
            ));
        }

        Region::Loop {
            header,
            body,
            header_label,
            exit_label,
        } => {
            lines.push(format!(
                "{}Loop (header={} exit={})",
                prefix, header_label, exit_label
            ));
            lines.push(format!("{}  header:", prefix));
            lines.push(format_region_tree(
                tree,
                *header,
                vinsts,
                pool,
                symbols,
                indent + 2,
            ));
            lines.push(format!("{}  body:", prefix));
            lines.push(format_region_tree(
                tree,
                *body,
                vinsts,
                pool,
                symbols,
                indent + 2,
            ));
        }

        Region::Seq {
            children_start,
            child_count,
        } => {
            lines.push(format!("{}Seq ({})", prefix, child_count));
            let start = *children_start as usize;
            let end = start + *child_count as usize;
            for &child_id in &tree.seq_children[start..end] {
                lines.push(format_region_tree(
                    tree,
                    child_id,
                    vinsts,
                    pool,
                    symbols,
                    indent + 1,
                ));
            }
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::{Region, RegionTree};
    use crate::vinst::{ModuleSymbols, VInst, VReg};

    #[test]
    fn format_linear_region() {
        let mut tree = RegionTree::new();
        let vinsts = vec![VInst::IConst32 {
            dst: VReg(0),
            val: 42,
            src_op: 0xFFFF,
        }];
        let root = tree.push(Region::Linear { start: 0, end: 1 });
        tree.root = root;

        let output = format_region_tree(&tree, root, &vinsts, &[], &ModuleSymbols::default(), 0);
        assert!(output.contains("Linear [0..1)"));
        assert!(output.contains("IConst32"));
    }

    #[test]
    fn format_empty_region() {
        let tree = RegionTree::new();
        let vinsts: Vec<VInst> = vec![];

        let output = format_region_tree(
            &tree,
            REGION_ID_NONE,
            &vinsts,
            &[],
            &ModuleSymbols::default(),
            0,
        );
        assert_eq!(output, "(empty)");
    }

    #[test]
    fn format_nested_regions() {
        let mut tree = RegionTree::new();
        let vinsts = vec![
            VInst::IConst32 {
                dst: VReg(0),
                val: 1,
                src_op: 0,
            },
            VInst::IConst32 {
                dst: VReg(1),
                val: 2,
                src_op: 1,
            },
        ];

        let inner1 = tree.push(Region::Linear { start: 0, end: 1 });
        let inner2 = tree.push(Region::Linear { start: 1, end: 2 });
        let root = tree.push_seq(&[inner1, inner2]);
        tree.root = root;

        let output = format_region_tree(&tree, root, &vinsts, &[], &ModuleSymbols::default(), 0);
        assert!(output.contains("Seq (2)"));
        assert!(output.contains("Linear [0..1)"));
        assert!(output.contains("Linear [1..2)"));
    }
}
