//! Reconciled Studio project editor controller tree.
//!
//! The editor tree is the Studio business-logic layer between the remote
//! `ProjectView` mirror and the render DTO tree. It owns local node/slot state
//! and reconciles that state against mirror-shaped descriptors without depending
//! on Dioxus or any web component.

use std::collections::BTreeMap;

use crate::{ProjectEditorTreeDescriptor, ProjectNodeAddress, ProjectNodeController};

/// UI-framework agnostic controller tree for the project editor.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectEditorTree {
    nodes: Vec<ProjectNodeController>,
}

impl ProjectEditorTree {
    /// Create an empty editor tree.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reconcile the controller tree against the latest desired descriptor.
    pub fn reconcile(&mut self, descriptor: ProjectEditorTreeDescriptor) {
        let mut previous = self
            .nodes
            .drain(..)
            .map(|node| (node.address().clone(), node))
            .collect::<BTreeMap<_, _>>();

        self.nodes = descriptor
            .nodes
            .into_iter()
            .map(|descriptor| {
                if let Some(mut controller) = previous.remove(descriptor.address()) {
                    controller.reconcile(descriptor);
                    controller
                } else {
                    ProjectNodeController::new(descriptor)
                }
            })
            .collect();
    }

    /// Reconciled node controllers in descriptor order.
    pub fn nodes(&self) -> &[ProjectNodeController] {
        &self.nodes
    }

    /// Number of node controllers.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True when no node controllers exist.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Find a node controller by stable address.
    pub fn node(&self, address: &ProjectNodeAddress) -> Option<&ProjectNodeController> {
        self.nodes.iter().find(|node| node.address() == address)
    }

    /// Find a mutable node controller by stable address.
    pub fn node_mut(&mut self, address: &ProjectNodeAddress) -> Option<&mut ProjectNodeController> {
        self.nodes.iter_mut().find(|node| node.address() == address)
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::{NodeId, Revision, SlotMapKey, SlotPath, SlotPathSegment};

    use crate::{
        ProjectEditorTree, ProjectEditorTreeDescriptor, ProjectNodeAddress, ProjectNodeDescriptor,
        ProjectNodeStatusTone, ProjectNodeStatusView, ProjectNodeTarget,
        ProjectProductSubscriptionIntent, ProjectSlotAddress, ProjectSlotDescriptor,
        ProjectSlotDescriptorKind, ProjectSlotRoot,
    };

    #[test]
    fn initial_reconcile_creates_nodes_in_descriptor_order() {
        let mut tree = ProjectEditorTree::new();

        tree.reconcile(ProjectEditorTreeDescriptor::new(vec![
            node_descriptor(1, "/demo.project/a.shader", "A", Vec::new()),
            node_descriptor(2, "/demo.project/b.shader", "B", Vec::new()),
        ]));

        assert_eq!(node_labels(&tree), vec!["A".to_string(), "B".to_string()]);
    }

    #[test]
    fn node_update_preserves_local_state_and_refreshes_runtime_id() {
        let address = node_address("/demo.project/a.shader");
        let mut tree = ProjectEditorTree::new();
        tree.reconcile(ProjectEditorTreeDescriptor::new(vec![node_descriptor(
            1,
            "/demo.project/a.shader",
            "A",
            Vec::new(),
        )]));
        let node = tree.node_mut(&address).unwrap();
        node.state_mut().collapsed = true;
        node.state_mut().focused = true;
        node.state_mut().product_subscription_intent = ProjectProductSubscriptionIntent::Subscribed;

        tree.reconcile(ProjectEditorTreeDescriptor::new(vec![node_descriptor(
            42,
            "/demo.project/a.shader",
            "A updated",
            Vec::new(),
        )]));

        let node = tree.node(&address).unwrap();
        assert_eq!(node.descriptor().label, "A updated");
        assert_eq!(node.descriptor().target.node_id, NodeId::new(42));
        assert!(node.state().collapsed);
        assert!(node.state().focused);
        assert_eq!(
            node.state().product_subscription_intent,
            ProjectProductSubscriptionIntent::Subscribed
        );
    }

    #[test]
    fn node_add_remove_and_reorder_follow_descriptor() {
        let mut tree = ProjectEditorTree::new();
        tree.reconcile(ProjectEditorTreeDescriptor::new(vec![
            node_descriptor(1, "/demo.project/a.shader", "A", Vec::new()),
            node_descriptor(2, "/demo.project/b.shader", "B", Vec::new()),
        ]));

        tree.reconcile(ProjectEditorTreeDescriptor::new(vec![
            node_descriptor(3, "/demo.project/c.shader", "C", Vec::new()),
            node_descriptor(1, "/demo.project/a.shader", "A", Vec::new()),
        ]));

        assert_eq!(node_labels(&tree), vec!["C".to_string(), "A".to_string()]);
        assert!(tree.node(&node_address("/demo.project/b.shader")).is_none());
    }

    #[test]
    fn slot_update_preserves_local_state() {
        let node = node_address("/demo.project/a.shader");
        let slot = slot_address(&node, "config.brightness");
        let mut tree = ProjectEditorTree::new();
        tree.reconcile(ProjectEditorTreeDescriptor::new(vec![node_descriptor(
            1,
            "/demo.project/a.shader",
            "A",
            vec![slot_descriptor(slot.clone(), "Brightness", 1)],
        )]));
        tree.node_mut(&node)
            .unwrap()
            .slot_mut(&slot)
            .unwrap()
            .state_mut()
            .expanded = true;

        tree.reconcile(ProjectEditorTreeDescriptor::new(vec![node_descriptor(
            1,
            "/demo.project/a.shader",
            "A",
            vec![slot_descriptor(slot.clone(), "Brightness", 2)],
        )]));

        let slot = tree.node_mut(&node).unwrap().slot_mut(&slot).unwrap();
        assert_eq!(slot.descriptor().revision, Some(Revision::new(2)));
        assert!(slot.state().expanded);
    }

    #[test]
    fn slot_add_remove_and_reorder_follow_descriptor() {
        let node = node_address("/demo.project/a.shader");
        let brightness = slot_address(&node, "config.brightness");
        let speed = slot_address(&node, "config.speed");
        let gain = slot_address(&node, "config.gain");
        let mut tree = ProjectEditorTree::new();
        tree.reconcile(ProjectEditorTreeDescriptor::new(vec![node_descriptor(
            1,
            "/demo.project/a.shader",
            "A",
            vec![
                slot_descriptor(brightness.clone(), "Brightness", 1),
                slot_descriptor(speed.clone(), "Speed", 1),
            ],
        )]));

        tree.reconcile(ProjectEditorTreeDescriptor::new(vec![node_descriptor(
            1,
            "/demo.project/a.shader",
            "A",
            vec![
                slot_descriptor(gain.clone(), "Gain", 1),
                slot_descriptor(brightness.clone(), "Brightness", 1),
            ],
        )]));

        let node = tree.node(&node).unwrap();
        assert_eq!(
            node.slots()
                .iter()
                .map(|slot| slot.descriptor().label.as_str())
                .collect::<Vec<_>>(),
            vec!["Gain", "Brightness"]
        );
        assert!(node.slots().iter().all(|slot| slot.address() != &speed));
    }

    #[test]
    fn scalar_to_record_shape_change_adds_children() {
        let node = node_address("/demo.project/a.shader");
        let root = slot_address(&node, "config");
        let child = slot_address(&node, "config.brightness");
        let mut tree = ProjectEditorTree::new();
        tree.reconcile(ProjectEditorTreeDescriptor::new(vec![node_descriptor(
            1,
            "/demo.project/a.shader",
            "A",
            vec![slot_descriptor(root.clone(), "Config", 1)],
        )]));

        tree.reconcile(ProjectEditorTreeDescriptor::new(vec![node_descriptor(
            1,
            "/demo.project/a.shader",
            "A",
            vec![record_descriptor(
                root,
                "Config",
                vec![slot_descriptor(child.clone(), "Brightness", 2)],
            )],
        )]));

        let root = &tree.node(&node).unwrap().slots()[0];
        assert_eq!(root.descriptor().kind, ProjectSlotDescriptorKind::Record);
        assert_eq!(root.children()[0].address(), &child);
    }

    #[test]
    fn record_to_scalar_shape_change_removes_stale_children() {
        let node = node_address("/demo.project/a.shader");
        let root = slot_address(&node, "config");
        let child = slot_address(&node, "config.brightness");
        let mut tree = ProjectEditorTree::new();
        tree.reconcile(ProjectEditorTreeDescriptor::new(vec![node_descriptor(
            1,
            "/demo.project/a.shader",
            "A",
            vec![record_descriptor(
                root.clone(),
                "Config",
                vec![slot_descriptor(child, "Brightness", 1)],
            )],
        )]));

        tree.reconcile(ProjectEditorTreeDescriptor::new(vec![node_descriptor(
            1,
            "/demo.project/a.shader",
            "A",
            vec![slot_descriptor(root, "Config", 2)],
        )]));

        assert!(tree.node(&node).unwrap().slots()[0].children().is_empty());
    }

    #[test]
    fn nested_child_state_survives_stable_updates() {
        let node = node_address("/demo.project/a.shader");
        let root = slot_address(&node, "config");
        let child = slot_address(&node, "config.brightness");
        let mut tree = ProjectEditorTree::new();
        tree.reconcile(ProjectEditorTreeDescriptor::new(vec![node_descriptor(
            1,
            "/demo.project/a.shader",
            "A",
            vec![record_descriptor(
                root.clone(),
                "Config",
                vec![slot_descriptor(child.clone(), "Brightness", 1)],
            )],
        )]));
        tree.node_mut(&node)
            .unwrap()
            .slot_mut(&child)
            .unwrap()
            .state_mut()
            .expanded = true;

        tree.reconcile(ProjectEditorTreeDescriptor::new(vec![node_descriptor(
            1,
            "/demo.project/a.shader",
            "A",
            vec![record_descriptor(
                root,
                "Config",
                vec![slot_descriptor(child.clone(), "Brightness", 2)],
            )],
        )]));

        assert!(
            tree.node_mut(&node)
                .unwrap()
                .slot_mut(&child)
                .unwrap()
                .state()
                .expanded
        );
    }

    #[test]
    fn map_entry_add_and_remove_reconciles_keyed_children() {
        let node = node_address("/demo.project/a.shader");
        let root = slot_address(&node, "params");
        let phase = keyed_slot_address(&node, "params", SlotMapKey::String("phase".to_string()));
        let offset = keyed_slot_address(&node, "params", SlotMapKey::String("offset".to_string()));
        let mut tree = ProjectEditorTree::new();
        tree.reconcile(ProjectEditorTreeDescriptor::new(vec![node_descriptor(
            1,
            "/demo.project/a.shader",
            "A",
            vec![map_descriptor(
                root.clone(),
                "Params",
                vec![slot_descriptor(phase.clone(), "phase", 1)],
            )],
        )]));

        tree.reconcile(ProjectEditorTreeDescriptor::new(vec![node_descriptor(
            1,
            "/demo.project/a.shader",
            "A",
            vec![map_descriptor(
                root,
                "Params",
                vec![slot_descriptor(offset.clone(), "offset", 1)],
            )],
        )]));

        let root = &tree.node(&node).unwrap().slots()[0];
        assert_eq!(root.children()[0].address(), &offset);
        assert!(
            !root
                .children()
                .iter()
                .any(|child| child.address() == &phase)
        );
    }

    fn node_labels(tree: &ProjectEditorTree) -> Vec<String> {
        tree.nodes()
            .iter()
            .map(|node| node.descriptor().label.clone())
            .collect()
    }

    fn node_descriptor(
        id: u32,
        path: &str,
        label: &str,
        slots: Vec<ProjectSlotDescriptor>,
    ) -> ProjectNodeDescriptor {
        ProjectNodeDescriptor::new(
            ProjectNodeTarget::new(node_address(path), NodeId::new(id)),
            label,
            "Shader",
            ProjectNodeStatusView::new("Running", None, ProjectNodeStatusTone::Good),
        )
        .with_slots(slots)
    }

    fn node_address(path: &str) -> ProjectNodeAddress {
        ProjectNodeAddress::parse(path).unwrap()
    }

    fn slot_address(node: &ProjectNodeAddress, path: &str) -> ProjectSlotAddress {
        ProjectSlotAddress::new(
            node.clone(),
            ProjectSlotRoot::def(),
            SlotPath::parse(path).unwrap(),
        )
    }

    fn keyed_slot_address(
        node: &ProjectNodeAddress,
        root: &str,
        key: SlotMapKey,
    ) -> ProjectSlotAddress {
        let path = SlotPath::parse(root)
            .unwrap()
            .child_segment(SlotPathSegment::Key(key));
        ProjectSlotAddress::new(node.clone(), ProjectSlotRoot::def(), path)
    }

    fn slot_descriptor(
        address: ProjectSlotAddress,
        label: &str,
        revision: i64,
    ) -> ProjectSlotDescriptor {
        ProjectSlotDescriptor::new(address, label, ProjectSlotDescriptorKind::Value)
            .with_revision(Revision::new(revision))
    }

    fn record_descriptor(
        address: ProjectSlotAddress,
        label: &str,
        children: Vec<ProjectSlotDescriptor>,
    ) -> ProjectSlotDescriptor {
        ProjectSlotDescriptor::new(address, label, ProjectSlotDescriptorKind::Record)
            .with_children(children)
    }

    fn map_descriptor(
        address: ProjectSlotAddress,
        label: &str,
        children: Vec<ProjectSlotDescriptor>,
    ) -> ProjectSlotDescriptor {
        ProjectSlotDescriptor::new(address, label, ProjectSlotDescriptorKind::Map)
            .with_children(children)
    }
}
