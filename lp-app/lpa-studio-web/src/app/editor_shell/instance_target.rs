//! One instance row's selection target — what the tree, the canvas's
//! sprite pick and the dive's first selection all name an instance by, so
//! every surface agrees about what was clicked.

use lpa_studio_core::{NodeId, UiPatchInstance, UiPatchTarget};

/// One instance's selection target: its path when it has one, else the
/// honest RANGE the same lamps occupy (an id-less strand displays and
/// selects, it just cannot be addressed — the grain-robustness ruling).
pub(crate) fn instance_target(node: NodeId, instance: &UiPatchInstance) -> UiPatchTarget {
    if instance.path.is_empty() {
        UiPatchTarget::Range {
            node,
            start: instance.start,
            count: Some(instance.lamps),
        }
    } else {
        UiPatchTarget::Instance {
            node,
            path: instance.path.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An id-less strand selects at range grain — the same target every
    /// surface builds for it, so they agree about what was clicked.
    #[test]
    fn instance_targets_follow_addressability() {
        let node = NodeId::new(2);
        assert_eq!(
            instance_target(node, &instance("sector@2", 30, 30)),
            UiPatchTarget::Instance {
                node,
                path: "sector@2".to_string(),
            }
        );
        assert_eq!(
            instance_target(node, &instance("", 30, 30)),
            UiPatchTarget::Range {
                node,
                start: 30,
                count: Some(30),
            }
        );
    }

    fn instance(path: &str, start: u32, lamps: u32) -> UiPatchInstance {
        UiPatchInstance {
            path: path.to_string(),
            label: path.to_string(),
            start,
            lamps,
            stride: lamps,
            placed: false,
        }
    }
}
