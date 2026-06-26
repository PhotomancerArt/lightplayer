use std::collections::BTreeMap;

use crate::{ProjectSlotAddress, ProjectSlotDescriptor};

/// Local Studio state owned by a project slot controller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSlotControllerState {
    pub expanded: bool,
}

impl ProjectSlotControllerState {
    /// Default collapsed slot state.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for ProjectSlotControllerState {
    fn default() -> Self {
        Self { expanded: false }
    }
}

/// UI-framework agnostic controller for a slot node.
///
/// Slot controllers are recursive. Containers and leaves both get controllers
/// so future editing, binding, validation, and expansion state have stable
/// addressable homes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSlotController {
    descriptor: ProjectSlotDescriptor,
    state: ProjectSlotControllerState,
    children: Vec<ProjectSlotController>,
}

impl ProjectSlotController {
    /// Create a controller from a desired slot descriptor.
    pub fn new(descriptor: ProjectSlotDescriptor) -> Self {
        let children = descriptor.children.iter().cloned().map(Self::new).collect();
        Self {
            descriptor,
            state: ProjectSlotControllerState::new(),
            children,
        }
    }

    /// Stable slot address used as the controller key.
    pub fn address(&self) -> &ProjectSlotAddress {
        self.descriptor.address()
    }

    /// Latest mirror-derived slot descriptor.
    pub fn descriptor(&self) -> &ProjectSlotDescriptor {
        &self.descriptor
    }

    /// Local slot controller state.
    pub fn state(&self) -> &ProjectSlotControllerState {
        &self.state
    }

    /// Mutable local slot controller state.
    pub fn state_mut(&mut self) -> &mut ProjectSlotControllerState {
        &mut self.state
    }

    /// Reconciled child slot controllers in descriptor order.
    pub fn children(&self) -> &[ProjectSlotController] {
        &self.children
    }

    /// Find a mutable descendant slot controller by address.
    pub fn slot_mut(&mut self, address: &ProjectSlotAddress) -> Option<&mut ProjectSlotController> {
        if self.address() == address {
            return Some(self);
        }
        self.children
            .iter_mut()
            .find_map(|child| child.slot_mut(address))
    }

    /// Reconcile this controller against the latest desired descriptor.
    pub fn reconcile(&mut self, descriptor: ProjectSlotDescriptor) {
        let desired_children = descriptor.children.clone();
        self.descriptor = descriptor;
        self.reconcile_children(desired_children);
    }

    fn reconcile_children(&mut self, descriptors: Vec<ProjectSlotDescriptor>) {
        let mut previous = self
            .children
            .drain(..)
            .map(|child| (child.address().clone(), child))
            .collect::<BTreeMap<_, _>>();

        self.children = descriptors
            .into_iter()
            .map(|descriptor| {
                if let Some(mut controller) = previous.remove(descriptor.address()) {
                    controller.reconcile(descriptor);
                    controller
                } else {
                    Self::new(descriptor)
                }
            })
            .collect();
    }
}
