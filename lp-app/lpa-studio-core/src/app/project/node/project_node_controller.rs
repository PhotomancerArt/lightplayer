use std::collections::BTreeMap;

use crate::{ProjectNodeAddress, ProjectNodeDescriptor, ProjectSlotController};

/// User/controller intent for product subscriptions owned by a node.
///
/// M2 does not implement product subscription transport. This state exists so
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
pub struct ProjectNodeControllerState {
    pub collapsed: bool,
    pub focused: bool,
    pub product_subscription_intent: ProjectProductSubscriptionIntent,
}

impl ProjectNodeControllerState {
    /// Default expanded, unfocused node state.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for ProjectNodeControllerState {
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
/// The controller stores the latest mirror-derived descriptor separately from
/// local Studio state. Reconciliation replaces descriptor data and recursively
/// updates slots while preserving state for stable addresses.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectNodeController {
    descriptor: ProjectNodeDescriptor,
    state: ProjectNodeControllerState,
    slots: Vec<ProjectSlotController>,
}

impl ProjectNodeController {
    /// Create a controller from a desired node descriptor.
    pub fn new(descriptor: ProjectNodeDescriptor) -> Self {
        let slots = descriptor
            .slots
            .iter()
            .cloned()
            .map(ProjectSlotController::new)
            .collect();
        Self {
            descriptor,
            state: ProjectNodeControllerState::new(),
            slots,
        }
    }

    /// Stable node address used as the controller key.
    pub fn address(&self) -> &ProjectNodeAddress {
        self.descriptor.address()
    }

    /// Latest mirror-derived node descriptor.
    pub fn descriptor(&self) -> &ProjectNodeDescriptor {
        &self.descriptor
    }

    /// Local node controller state.
    pub fn state(&self) -> &ProjectNodeControllerState {
        &self.state
    }

    /// Mutable local node controller state.
    pub fn state_mut(&mut self) -> &mut ProjectNodeControllerState {
        &mut self.state
    }

    /// Reconciled slot controllers in descriptor order.
    pub fn slots(&self) -> &[ProjectSlotController] {
        &self.slots
    }

    /// Find a mutable slot controller by address.
    pub fn slot_mut(
        &mut self,
        address: &crate::ProjectSlotAddress,
    ) -> Option<&mut ProjectSlotController> {
        self.slots.iter_mut().find_map(|slot| {
            if slot.address() == address {
                Some(slot)
            } else {
                slot.slot_mut(address)
            }
        })
    }

    /// Reconcile this controller against the latest desired descriptor.
    pub fn reconcile(&mut self, descriptor: ProjectNodeDescriptor) {
        let desired_slots = descriptor.slots.clone();
        self.descriptor = descriptor;
        self.reconcile_slots(desired_slots);
    }

    fn reconcile_slots(&mut self, descriptors: Vec<crate::ProjectSlotDescriptor>) {
        let mut previous = self
            .slots
            .drain(..)
            .map(|slot| (slot.address().clone(), slot))
            .collect::<BTreeMap<_, _>>();

        self.slots = descriptors
            .into_iter()
            .map(|descriptor| {
                if let Some(mut controller) = previous.remove(descriptor.address()) {
                    controller.reconcile(descriptor);
                    controller
                } else {
                    ProjectSlotController::new(descriptor)
                }
            })
            .collect();
    }
}
