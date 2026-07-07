use std::collections::{BTreeMap, BTreeSet};

use lpc_model::{NodeId, SlotData, SlotShapeLookup, SlotShapeView, TreePath};
use lpc_view::{ProjectView, SlotMirrorView, TreeEntryView};
use lpc_wire::{NodeRuntimeStatus, WireEntryState};

use crate::app::project::slot::SlotEditJoin;
use crate::{
    ControllerId, DirtySummary, NodeRevertOp, ProjectController, ProjectEditorOp,
    ProjectEditorTarget, ProjectNodeAddress, ProjectNodeStatusTone, ProjectNodeStatusView,
    ProjectNodeTarget, ProjectSlotAddress, ProjectSlotRoot, SlotController, UiAction, UiConfigSlot,
    UiNodeChild, UiNodeHeader, UiNodeSection, UiNodeTab, UiNodeView, UiPaneAction,
    UiProductPreview, UiProductRef, UiProductTrackingState, UiStatus,
};

/// User/controller intent for product subscriptions owned by a node.
///
/// M2a does not implement product subscription transport. This state exists so
/// reconciliation has a durable place to preserve that future intent.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProjectProductSubscriptionIntent {
    #[default]
    Default,
    Subscribed,
    Unsubscribed,
}

/// Local Studio state owned by a project node controller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeControllerState {
    pub collapsed: bool,
    pub focused: bool,
    pub product_subscription_intent: ProjectProductSubscriptionIntent,
}

impl NodeControllerState {
    /// Default expanded, unfocused node state.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for NodeControllerState {
    fn default() -> Self {
        Self {
            collapsed: false,
            focused: false,
            product_subscription_intent: ProjectProductSubscriptionIntent::Default,
        }
    }
}

/// UI-framework agnostic controller for one project node.
///
/// Node controllers form an owned tree under `ProjectController`. Each node
/// owns its child node controllers and its root slot controllers.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeController {
    address: ProjectNodeAddress,
    target: ProjectNodeTarget,
    parent: Option<ProjectNodeAddress>,
    child_addresses: Vec<ProjectNodeAddress>,
    label: String,
    kind: String,
    status: ProjectNodeStatusView,
    issues: Vec<String>,
    state: NodeControllerState,
    children: Vec<NodeController>,
    slots: Vec<SlotController>,
}

impl NodeController {
    pub(in crate::app::project) fn from_tree_entry(
        entry: &TreeEntryView,
        view: &ProjectView,
    ) -> Self {
        let address = ProjectNodeAddress::new(entry.path.clone());
        let target = ProjectNodeTarget::new(address.clone(), entry.id);
        let mut controller = Self {
            address,
            target,
            parent: None,
            child_addresses: Vec::new(),
            label: String::new(),
            kind: String::new(),
            status: ProjectNodeStatusView::new("Unknown", None, ProjectNodeStatusTone::Neutral),
            issues: Vec::new(),
            state: NodeControllerState::new(),
            children: Vec::new(),
            slots: Vec::new(),
        };
        controller.apply_tree_entry(entry, view);
        controller
    }

    /// Stable node address used as the controller key.
    pub fn address(&self) -> &ProjectNodeAddress {
        &self.address
    }

    /// Current action target for this node.
    pub fn target(&self) -> &ProjectNodeTarget {
        &self.target
    }

    /// Stable parent node address, if this node currently has one.
    pub fn parent(&self) -> Option<&ProjectNodeAddress> {
        self.parent.as_ref()
    }

    /// Stable child node addresses in mirror order.
    pub fn child_addresses(&self) -> &[ProjectNodeAddress] {
        &self.child_addresses
    }

    /// Human-readable node label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Human-readable node kind.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Current node status.
    pub fn status(&self) -> &ProjectNodeStatusView {
        &self.status
    }

    /// Mirror/application issues attached to this node controller.
    pub fn issues(&self) -> &[String] {
        &self.issues
    }

    /// Local node controller state.
    pub fn state(&self) -> &NodeControllerState {
        &self.state
    }

    /// Mutable local node controller state.
    pub fn state_mut(&mut self) -> &mut NodeControllerState {
        &mut self.state
    }

    /// Child node controllers in mirror order.
    pub fn children(&self) -> &[NodeController] {
        &self.children
    }

    /// Mutable child node controllers in mirror order.
    pub(in crate::app::project) fn children_mut(&mut self) -> &mut [NodeController] {
        &mut self.children
    }

    /// Root slot controllers in mirror order.
    pub fn slots(&self) -> &[SlotController] {
        &self.slots
    }

    /// Project this controller and its slot controllers into the node-pane DTO.
    pub fn ui_node(&self) -> UiNodeView {
        self.ui_node_with_product_previews(&|_| None, &SlotEditJoin::empty(), &|_| Vec::new())
    }

    /// Project this controller into a node-pane DTO with product preview state.
    ///
    /// The dirty aggregation rides the same walk: child DTOs are built first
    /// (each carrying its subtree summary), and the header summary merges the
    /// node's own slot summaries with the children's — no second traversal.
    pub(in crate::app::project) fn ui_node_with_product_previews(
        &self,
        product_preview: &impl Fn(&UiProductRef) -> Option<UiProductPreview>,
        edits: &SlotEditJoin<'_>,
        extra_config: &impl Fn(NodeId) -> Vec<UiConfigSlot>,
    ) -> UiNodeView {
        let children = self.ui_children_with_product_previews(product_preview, edits, extra_config);
        let dirty = self.own_slots_dirty_summary(edits)
            + children
                .iter()
                .map(|child| child.dirty)
                .sum::<DirtySummary>();
        let header = UiNodeHeader::new(
            self.label.clone(),
            self.kind.clone(),
            self.address.to_string(),
        )
        .with_status(self.ui_status())
        .with_dirty(dirty);

        let mut view = UiNodeView::new(
            header,
            vec![UiNodeTab::main(self.ui_sections_with_product_previews(
                product_preview,
                edits,
                extra_config,
            ))],
        )
        .with_node_id(self.address.to_string())
        .with_header_actions(node_header_actions(&self.address, &dirty))
        .with_children(children);
        view.focused = self.state.focused;
        view.action = Some(node_focus_action(self));
        view.collapsed = self.state.collapsed;
        view.issues = self.issues.clone();
        view
    }

    /// True when any of this node's slot roots carries a top-level field
    /// named `name` (used to detect wiring with no backing row).
    pub(in crate::app::project) fn has_slot_root_field(&self, name: &str) -> bool {
        self.slots.iter().any(|slot| slot.has_root_field(name))
    }

    /// Find a descendant node controller by stable address.
    pub fn node(&self, address: &ProjectNodeAddress) -> Option<&NodeController> {
        if self.address() == address {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.node(address))
    }

    /// Find a mutable descendant node controller by stable address.
    pub fn node_mut(&mut self, address: &ProjectNodeAddress) -> Option<&mut NodeController> {
        if self.address() == address {
            return Some(self);
        }
        self.children
            .iter_mut()
            .find_map(|child| child.node_mut(address))
    }

    /// Find a mutable slot controller by address.
    pub fn slot_mut(&mut self, address: &ProjectSlotAddress) -> Option<&mut SlotController> {
        self.slots.iter_mut().find_map(|slot| {
            if slot.address() == address {
                Some(slot)
            } else {
                slot.slot_mut(address)
            }
        })
    }

    pub(in crate::app::project) fn apply_tree_entry(
        &mut self,
        entry: &TreeEntryView,
        view: &ProjectView,
    ) {
        self.address = ProjectNodeAddress::new(entry.path.clone());
        self.target = ProjectNodeTarget::new(self.address.clone(), entry.id);
        self.label = node_label(entry);
        self.kind = node_kind_label(&entry.path);
        self.status = node_status_view(entry);
        self.issues.clear();
        self.parent = parent_address(entry, view, &mut self.issues);

        let desired_children = child_entries(entry, view, &mut self.issues);
        self.child_addresses = desired_children
            .iter()
            .map(|child| ProjectNodeAddress::new(child.path.clone()))
            .collect();
        self.reconcile_children(desired_children, view);
        self.reconcile_slots(
            root_slot_applies(entry, &self.address, &view.slots),
            &view.slots,
        );
        self.apply_binding_facts();
    }

    /// Distribute authored binding facts from the def root onto the slots
    /// they name: consumed/config slots on the def root and produced slots
    /// on the state root (bindings live at node-def roots since M0).
    fn apply_binding_facts(&mut self) {
        let facts = self
            .slots
            .iter()
            .find(|slot| matches!(slot.address().root, ProjectSlotRoot::Def))
            .map(SlotController::binding_facts)
            .unwrap_or_default();
        for slot in &mut self.slots {
            slot.apply_binding_facts(&facts);
        }
    }

    fn reconcile_children(&mut self, children: Vec<&TreeEntryView>, view: &ProjectView) {
        let mut previous = self
            .children
            .drain(..)
            .map(|child| (child.address().clone(), child))
            .collect::<BTreeMap<_, _>>();

        self.children = children
            .into_iter()
            .map(|entry| {
                let address = ProjectNodeAddress::new(entry.path.clone());
                if let Some(mut controller) = previous.remove(&address) {
                    controller.apply_tree_entry(entry, view);
                    controller
                } else {
                    Self::from_tree_entry(entry, view)
                }
            })
            .collect();
    }

    fn reconcile_slots(&mut self, slots: Vec<RootSlotApply<'_>>, mirror: &SlotMirrorView) {
        let mut previous = self
            .slots
            .drain(..)
            .map(|slot| (slot.address().clone(), slot))
            .collect::<BTreeMap<_, _>>();

        self.slots = slots
            .into_iter()
            .map(|slot| {
                let address = slot.address().clone();
                if let Some(mut controller) = previous.remove(&address) {
                    apply_root_slot(&mut controller, slot, mirror);
                    controller
                } else {
                    root_slot_controller(slot, mirror)
                }
            })
            .collect();
    }

    /// Collect produced product identities emitted by this node.
    pub(in crate::app::project) fn collect_produced_product_refs(
        &self,
        products: &mut Vec<UiProductRef>,
    ) {
        for slot in &self.slots {
            if matches!(slot.address().root, ProjectSlotRoot::State) {
                slot.collect_produced_product_refs(products);
            }
        }
    }

    fn ui_sections_with_product_previews(
        &self,
        product_preview: &impl Fn(&UiProductRef) -> Option<UiProductPreview>,
        edits: &SlotEditJoin<'_>,
        extra_config: &impl Fn(NodeId) -> Vec<UiConfigSlot>,
    ) -> Vec<UiNodeSection> {
        let mut products = Vec::new();
        let mut produced_values = Vec::new();
        let mut config_slots = Vec::new();
        let mut asset_slots = Vec::new();

        for slot in &self.slots {
            match slot.address().root {
                ProjectSlotRoot::State => {
                    slot.collect_produced(&mut products, &mut produced_values);
                }
                ProjectSlotRoot::Def | ProjectSlotRoot::Other(_) => {
                    slot.collect_config(edits, &mut config_slots, &mut asset_slots);
                }
            }
        }
        // Binding-derived rows: wiring on slots with no backing row —
        // implicit runtime consumed slots like `fixture.input` (roadmap M3).
        config_slots.extend(extra_config(self.target.node_id));

        let mut sections = Vec::new();
        if !products.is_empty() {
            let base_tracking = self.product_tracking_state();
            for product in &mut products {
                let mut has_cached_preview = false;
                if let Some(product_ref) = product.product
                    && let Some(preview) = product_preview(&product_ref)
                {
                    product.preview = preview;
                    has_cached_preview = true;
                }
                product.tracking =
                    if base_tracking == UiProductTrackingState::Untracked && has_cached_preview {
                        UiProductTrackingState::Paused
                    } else {
                        base_tracking
                    }
            }
            sections.push(UiNodeSection::ProducedProducts(products));
        }
        if !produced_values.is_empty() {
            sections.push(UiNodeSection::ProducedValues(produced_values));
        }
        if !asset_slots.is_empty() {
            sections.push(UiNodeSection::AssetSlots(asset_slots));
        }
        if !config_slots.is_empty() {
            sections.push(UiNodeSection::ConfigSlots(config_slots));
        }
        sections
    }

    /// Config and asset rows for this node's **own** slots (children
    /// excluded), in section order (config, then assets). Feeds the project
    /// popup's settings section for the workspace root, whose card the
    /// flat-root workspace no longer renders.
    pub(in crate::app::project) fn ui_config_slots(
        &self,
        edits: &SlotEditJoin<'_>,
    ) -> Vec<UiConfigSlot> {
        let mut config_slots = Vec::new();
        let mut asset_slots = Vec::new();
        for slot in &self.slots {
            match slot.address().root {
                ProjectSlotRoot::State => {}
                ProjectSlotRoot::Def | ProjectSlotRoot::Other(_) => {
                    slot.collect_config(edits, &mut config_slots, &mut asset_slots);
                }
            }
        }
        config_slots.extend(asset_slots);
        config_slots
    }

    fn ui_children_with_product_previews(
        &self,
        product_preview: &impl Fn(&UiProductRef) -> Option<UiProductPreview>,
        edits: &SlotEditJoin<'_>,
        extra_config: &impl Fn(NodeId) -> Vec<UiConfigSlot>,
    ) -> Vec<UiNodeChild> {
        self.children
            .iter()
            .map(|child| {
                let mut view = UiNodeChild::new(
                    child.label.clone(),
                    child.kind.clone(),
                    child.address.to_string(),
                );
                view.status = child.ui_status();
                view.summary = child.status.detail.clone();
                view.focused = child.state.focused;
                view.action = Some(node_focus_action(child));
                view.sections =
                    child.ui_sections_with_product_previews(product_preview, edits, extra_config);
                view.children =
                    child.ui_children_with_product_previews(product_preview, edits, extra_config);
                view.dirty = child.own_slots_dirty_summary(edits)
                    + view
                        .children
                        .iter()
                        .map(|nested| nested.dirty)
                        .sum::<DirtySummary>();
                view.header_actions = node_header_actions(&child.address, &view.dirty);
                view
            })
            .collect()
    }

    /// Aggregate dirty-edit summary for this node's subtree: own edits plus
    /// every descendant node's, counted by the join's single per-entry rule
    /// (`SlotEditJoin::dirty_summary_for_node`).
    pub(in crate::app::project) fn dirty_summary(&self, edits: &SlotEditJoin<'_>) -> DirtySummary {
        let mut summary = self.own_slots_dirty_summary(edits);
        for child in &self.children {
            summary += child.dirty_summary(edits);
        }
        summary
    }

    /// Aggregate dirty-edit summary for the edits addressed to this node
    /// (child nodes excluded). Counted per **edit entry**, not per slot row
    /// (`SlotEditJoin::dirty_summary_for_node`), so edits at paths with no
    /// surviving row — removed map entries — still count exactly once.
    /// Callers merging bottom-up (DTO and tree-item walks) combine this with
    /// already-computed child summaries.
    pub(in crate::app::project) fn own_slots_dirty_summary(
        &self,
        edits: &SlotEditJoin<'_>,
    ) -> DirtySummary {
        edits.dirty_summary_for_node(&self.address)
    }

    fn ui_status(&self) -> UiStatus {
        UiStatus::new(self.status.label.clone(), self.status.tone.ui_status_kind())
    }

    fn product_tracking_state(&self) -> UiProductTrackingState {
        match self.state.product_subscription_intent {
            ProjectProductSubscriptionIntent::Default if self.state.focused => {
                UiProductTrackingState::Tracking
            }
            ProjectProductSubscriptionIntent::Default => UiProductTrackingState::Untracked,
            ProjectProductSubscriptionIntent::Subscribed => UiProductTrackingState::Tracking,
            ProjectProductSubscriptionIntent::Unsubscribed => UiProductTrackingState::Paused,
        }
    }
}

/// Contextual node-header actions (pane grammar actions slot, M3 UX gate
/// feedback): the subtree batch revert ([`NodeRevertOp`]) with the same
/// "revert" icon token as the project header's Revert-to-saved, present only
/// while the header's subtree [`DirtySummary`] announces pending edits.
fn node_header_actions(node: &ProjectNodeAddress, dirty: &DirtySummary) -> Vec<UiPaneAction> {
    if dirty.is_clean() {
        return Vec::new();
    }
    vec![UiPaneAction::new(
        "revert",
        UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            NodeRevertOp { node: node.clone() },
        ),
    )]
}

fn node_focus_action(node: &NodeController) -> UiAction {
    UiAction::from_op(
        ProjectEditorTarget::addressed_node(node.target().clone()).node_id(),
        ProjectEditorOp::Focus,
    )
    .with_label(format!("Focus {}", node.label()))
    .with_summary(format!("Focus node {}.", node.address()))
}

enum RootSlotApply<'a> {
    Data {
        address: ProjectSlotAddress,
        label: String,
        data: &'a SlotData,
        shape: SlotShapeView<'a>,
    },
    Issue {
        address: ProjectSlotAddress,
        label: String,
        message: String,
    },
}

impl RootSlotApply<'_> {
    fn address(&self) -> &ProjectSlotAddress {
        match self {
            Self::Data { address, .. } | Self::Issue { address, .. } => address,
        }
    }
}

fn apply_root_slot(
    controller: &mut SlotController,
    slot: RootSlotApply<'_>,
    mirror: &SlotMirrorView,
) {
    match slot {
        RootSlotApply::Data {
            address,
            label,
            data,
            shape,
        } => {
            controller.apply_root_data(address, label, data, shape, &mirror.registry);
        }
        RootSlotApply::Issue {
            address,
            label,
            message,
        } => {
            controller.apply_root_issue(address, label, message);
        }
    }
}

fn root_slot_controller(slot: RootSlotApply<'_>, mirror: &SlotMirrorView) -> SlotController {
    match slot {
        RootSlotApply::Data {
            address,
            label,
            data,
            shape,
        } => SlotController::from_slot_data(address, label, data, shape, &mirror.registry),
        RootSlotApply::Issue {
            address,
            label,
            message,
        } => SlotController::issue(address, label, message),
    }
}

fn root_slot_applies<'a>(
    entry: &TreeEntryView,
    node: &ProjectNodeAddress,
    slots: &'a SlotMirrorView,
) -> Vec<RootSlotApply<'a>> {
    root_slot_names(entry.id, slots)
        .into_iter()
        .map(|root_name| root_slot_apply(entry.id, node, slots, root_name))
        .collect()
}

fn root_slot_apply<'a>(
    node_id: NodeId,
    node: &ProjectNodeAddress,
    slots: &'a SlotMirrorView,
    root_name: String,
) -> RootSlotApply<'a> {
    let key = root_slot_key(node_id, &root_name);
    let root = ProjectSlotRoot::from_name(&root_name);
    let address = ProjectSlotAddress::root(node.clone(), root);
    let label = human_label(&root_name);
    let Some(shape_id) = slots.root_shapes.get(&key).copied() else {
        return RootSlotApply::Issue {
            address,
            label,
            message: format!("{key} shape is missing"),
        };
    };
    let Some(data) = slots.roots.get(&key) else {
        return RootSlotApply::Issue {
            address,
            label,
            message: format!("{key} data is missing"),
        };
    };
    let Some(shape) = slots.registry.get_shape(shape_id) else {
        return RootSlotApply::Issue {
            address,
            label,
            message: format!("shape {shape_id} is missing"),
        };
    };
    RootSlotApply::Data {
        address,
        label,
        data,
        shape,
    }
}

fn root_slot_names(node_id: NodeId, slots: &SlotMirrorView) -> Vec<String> {
    let prefix = format!("node.{node_id}.");
    let mut names = BTreeSet::new();
    for key in slots.root_shapes.keys().chain(slots.roots.keys()) {
        if let Some(root_name) = key.strip_prefix(&prefix) {
            names.insert(root_name.to_string());
        }
    }
    let mut names = names.into_iter().collect::<Vec<_>>();
    names.sort_by(|left, right| root_name_sort_key(left).cmp(&root_name_sort_key(right)));
    names
}

fn root_name_sort_key(name: &str) -> (u8, &str) {
    match name {
        "def" => (0, name),
        "state" => (1, name),
        _ => (2, name),
    }
}

/// Key of a node's slot root in the mirror's `root_shapes`/`roots` maps.
pub(in crate::app::project) fn root_slot_key(node_id: NodeId, root_name: &str) -> String {
    format!("node.{node_id}.{root_name}")
}

fn parent_address(
    entry: &TreeEntryView,
    view: &ProjectView,
    issues: &mut Vec<String>,
) -> Option<ProjectNodeAddress> {
    let parent_id = entry.parent?;
    match view.tree.get(parent_id) {
        Some(parent) => Some(ProjectNodeAddress::new(parent.path.clone())),
        None => {
            issues.push(format!("parent node {parent_id} is missing"));
            None
        }
    }
}

fn child_entries<'a>(
    entry: &TreeEntryView,
    view: &'a ProjectView,
    issues: &mut Vec<String>,
) -> Vec<&'a TreeEntryView> {
    entry
        .children
        .iter()
        .filter_map(|child_id| match view.tree.get(*child_id) {
            Some(child) => Some(child),
            None => {
                issues.push(format!("child node {child_id} is missing"));
                None
            }
        })
        .collect()
}

fn node_label(entry: &TreeEntryView) -> String {
    entry
        .path
        .0
        .last()
        .map(|segment| human_label(segment.name.as_str()))
        .unwrap_or_else(|| format!("Node {}", entry.id))
}

fn node_kind_label(path: &TreePath) -> String {
    let Some(segment) = path.0.last() else {
        return "Node".to_string();
    };
    match segment.ty.as_str() {
        "project" | "show" => "Project".to_string(),
        "vis" | "visual" => "Visual".to_string(),
        "shader" | "shader_node" => "Shader".to_string(),
        "compute" | "compute_shader" => "Compute".to_string(),
        "fixture" => "Fixture".to_string(),
        "output" => "Output".to_string(),
        "clock" => "Clock".to_string(),
        "playlist" => "Playlist".to_string(),
        other => human_label(other),
    }
}

fn node_status_view(entry: &TreeEntryView) -> ProjectNodeStatusView {
    match &entry.state {
        WireEntryState::Failed { reason } => {
            ProjectNodeStatusView::new("Failed", Some(reason.clone()), ProjectNodeStatusTone::Error)
        }
        WireEntryState::Pending => {
            ProjectNodeStatusView::new("Pending", None, ProjectNodeStatusTone::Neutral)
        }
        WireEntryState::Alive => match &entry.status {
            NodeRuntimeStatus::Created => {
                ProjectNodeStatusView::new("Created", None, ProjectNodeStatusTone::Neutral)
            }
            NodeRuntimeStatus::Ok => {
                ProjectNodeStatusView::new("Running", None, ProjectNodeStatusTone::Good)
            }
            NodeRuntimeStatus::Warn(message) => ProjectNodeStatusView::new(
                "Warning",
                Some(message.clone()),
                ProjectNodeStatusTone::Warning,
            ),
            NodeRuntimeStatus::InitError(message) => ProjectNodeStatusView::new(
                "Init error",
                Some(message.clone()),
                ProjectNodeStatusTone::Error,
            ),
            NodeRuntimeStatus::Error(message) => ProjectNodeStatusView::new(
                "Error",
                Some(message.clone()),
                ProjectNodeStatusTone::Error,
            ),
        },
    }
}

fn human_label(raw: &str) -> String {
    let normalized = raw.replace(['_', '-'], " ");
    let mut chars = normalized.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first.to_uppercase().collect::<String>() + chars.as_str()
}
