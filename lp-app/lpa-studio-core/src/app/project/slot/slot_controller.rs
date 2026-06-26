use std::collections::BTreeMap;

use lpc_model::{
    Revision, SlotData, SlotMapKey, SlotName, SlotShapeLookup, SlotShapeRegistry, SlotShapeView,
};

use crate::{ProjectSlotAddress, app::project::format_slot_map_key};

/// Compact structural family for a project slot controller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlotKind {
    Unit,
    Value,
    Record,
    Map,
    Enum,
    Option,
    Asset,
    Issue,
}

/// Local Studio state owned by a project slot controller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlotControllerState {
    pub expanded: bool,
}

impl SlotControllerState {
    /// Default collapsed slot state.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for SlotControllerState {
    fn default() -> Self {
        Self { expanded: false }
    }
}

/// UI-framework agnostic controller for one slot tree node.
///
/// Slot controllers are recursive. Containers and leaves both get controllers
/// so future editing, binding, validation, and expansion state have stable
/// addressable homes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlotController {
    address: ProjectSlotAddress,
    label: String,
    kind: SlotKind,
    revision: Option<Revision>,
    issues: Vec<String>,
    state: SlotControllerState,
    children: Vec<SlotController>,
}

impl SlotController {
    pub(in crate::app::project) fn from_slot_data(
        address: ProjectSlotAddress,
        label: String,
        data: &SlotData,
        shape: SlotShapeView<'_>,
        registry: &SlotShapeRegistry,
    ) -> Self {
        let mut controller = Self::empty(address, label);
        controller.apply_slot_data(data, shape, registry);
        controller
    }

    pub(in crate::app::project) fn issue(
        address: ProjectSlotAddress,
        label: impl Into<String>,
        issue: impl Into<String>,
    ) -> Self {
        let mut controller = Self::empty(address, label.into());
        controller.kind = SlotKind::Issue;
        controller.issues.push(issue.into());
        controller
    }

    pub(in crate::app::project) fn apply_root_data(
        &mut self,
        address: ProjectSlotAddress,
        label: String,
        data: &SlotData,
        shape: SlotShapeView<'_>,
        registry: &SlotShapeRegistry,
    ) {
        self.address = address;
        self.label = label;
        self.apply_slot_data(data, shape, registry);
    }

    pub(in crate::app::project) fn apply_root_issue(
        &mut self,
        address: ProjectSlotAddress,
        label: String,
        issue: String,
    ) {
        self.address = address;
        self.label = label;
        self.revision = None;
        self.apply_issue(issue);
    }

    fn empty(address: ProjectSlotAddress, label: String) -> Self {
        Self {
            address,
            label,
            kind: SlotKind::Issue,
            revision: None,
            issues: Vec::new(),
            state: SlotControllerState::new(),
            children: Vec::new(),
        }
    }

    /// Stable slot address used as the controller key.
    pub fn address(&self) -> &ProjectSlotAddress {
        &self.address
    }

    /// Human-readable slot label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Latest structural slot kind observed in the mirror.
    pub fn kind(&self) -> SlotKind {
        self.kind
    }

    /// Latest known revision for this slot, if the mirror supplied one.
    pub fn revision(&self) -> Option<Revision> {
        self.revision
    }

    /// Mirror/application issues attached to this slot controller.
    pub fn issues(&self) -> &[String] {
        &self.issues
    }

    /// Local slot controller state.
    pub fn state(&self) -> &SlotControllerState {
        &self.state
    }

    /// Mutable local slot controller state.
    pub fn state_mut(&mut self) -> &mut SlotControllerState {
        &mut self.state
    }

    /// Reconciled child slot controllers in mirror order.
    pub fn children(&self) -> &[SlotController] {
        &self.children
    }

    /// Find a mutable descendant slot controller by address.
    pub fn slot_mut(&mut self, address: &ProjectSlotAddress) -> Option<&mut SlotController> {
        if self.address() == address {
            return Some(self);
        }
        self.children
            .iter_mut()
            .find_map(|child| child.slot_mut(address))
    }

    pub(super) fn apply_slot_data(
        &mut self,
        data: &SlotData,
        shape: SlotShapeView<'_>,
        registry: &SlotShapeRegistry,
    ) {
        self.revision = data_revision(data);
        self.issues.clear();

        let Ok(shape) = resolve_shape(shape, registry) else {
            self.apply_issue("slot shape could not be resolved");
            return;
        };

        if shape.is_unit() {
            self.apply_unit(data);
        } else if shape.value_shape().is_some() {
            self.apply_value(data);
        } else if let Some(field_count) = shape.record_fields_len() {
            self.apply_record(data, shape, field_count, registry);
        } else if let Some(value_shape) = shape.map_value() {
            self.apply_map(data, value_shape, registry);
        } else if shape.is_enum() {
            self.apply_enum(data, shape, registry);
        } else if let Some(some_shape) = shape.option_some() {
            self.apply_option(data, some_shape, registry);
        } else {
            self.apply_issue("unsupported slot shape");
        }
    }

    fn apply_unit(&mut self, data: &SlotData) {
        match data {
            SlotData::Unit { .. } => {
                self.kind = SlotKind::Unit;
                self.children.clear();
            }
            _ => self.apply_issue("expected unit data"),
        }
    }

    fn apply_value(&mut self, data: &SlotData) {
        match data {
            SlotData::Value(_) => {
                self.kind = SlotKind::Value;
                self.children.clear();
            }
            _ => self.apply_issue("expected value data"),
        }
    }

    fn apply_record(
        &mut self,
        data: &SlotData,
        shape: SlotShapeView<'_>,
        field_count: usize,
        registry: &SlotShapeRegistry,
    ) {
        let SlotData::Record(record) = data else {
            self.apply_issue("expected record data");
            return;
        };

        self.kind = SlotKind::Record;
        let children = (0..field_count)
            .map(|index| {
                let Some(field) = shape.record_field(index) else {
                    return SlotChildApply::Issue {
                        address: self.address.clone(),
                        label: format!("field {index}"),
                        message: "field shape is missing".to_string(),
                    };
                };
                let label = human_label(field.name_str());
                let address = self.address_with_field(field.name_str());
                match record.fields.get(index) {
                    Some(data) => SlotChildApply::Data {
                        address,
                        label,
                        data,
                        shape: field.shape(),
                    },
                    None => SlotChildApply::Issue {
                        address,
                        label,
                        message: "field data is missing".to_string(),
                    },
                }
            })
            .collect();
        self.reconcile_children(children, registry);
    }

    fn apply_map(
        &mut self,
        data: &SlotData,
        value_shape: SlotShapeView<'_>,
        registry: &SlotShapeRegistry,
    ) {
        let SlotData::Map(map) = data else {
            self.apply_issue("expected map data");
            return;
        };

        self.kind = SlotKind::Map;
        let children = map
            .entries
            .iter()
            .map(|(key, data)| SlotChildApply::Data {
                address: ProjectSlotAddress::new(
                    self.address.node.clone(),
                    self.address.root.clone(),
                    self.address.path.child_key(key.clone()),
                ),
                label: map_key_label(key),
                data,
                shape: value_shape,
            })
            .collect();
        self.reconcile_children(children, registry);
    }

    fn apply_enum(
        &mut self,
        data: &SlotData,
        shape: SlotShapeView<'_>,
        registry: &SlotShapeRegistry,
    ) {
        let SlotData::Enum(value) = data else {
            self.apply_issue("expected enum data");
            return;
        };

        self.kind = SlotKind::Enum;
        let Some(variant_shape) = shape.enum_variant_by_name(&value.variant) else {
            self.apply_issue(format!(
                "enum variant {} is missing from shape",
                value.variant.as_str()
            ));
            return;
        };

        let children = vec![SlotChildApply::Data {
            address: ProjectSlotAddress::new(
                self.address.node.clone(),
                self.address.root.clone(),
                self.address.path.child(value.variant.clone()),
            ),
            label: human_label(value.variant.as_str()),
            data: &value.data,
            shape: variant_shape.shape(),
        }];
        self.reconcile_children(children, registry);
    }

    fn apply_option(
        &mut self,
        data: &SlotData,
        some_shape: SlotShapeView<'_>,
        registry: &SlotShapeRegistry,
    ) {
        let SlotData::Option(value) = data else {
            self.apply_issue("expected optional data");
            return;
        };

        self.kind = SlotKind::Option;
        let Some(data) = &value.data else {
            self.children.clear();
            return;
        };

        let children = vec![SlotChildApply::Data {
            address: ProjectSlotAddress::new(
                self.address.node.clone(),
                self.address.root.clone(),
                self.address
                    .path
                    .child(SlotName::parse("some").expect("valid slot name")),
            ),
            label: "Value".to_string(),
            data,
            shape: some_shape,
        }];
        self.reconcile_children(children, registry);
    }

    fn apply_issue(&mut self, issue: impl Into<String>) {
        self.kind = SlotKind::Issue;
        self.issues.clear();
        self.issues.push(issue.into());
        self.children.clear();
    }

    fn reconcile_children(
        &mut self,
        children: Vec<SlotChildApply<'_>>,
        registry: &SlotShapeRegistry,
    ) {
        let mut previous = self
            .children
            .drain(..)
            .map(|child| (child.address().clone(), child))
            .collect::<BTreeMap<_, _>>();

        self.children = children
            .into_iter()
            .map(|child| {
                let address = child.address().clone();
                if let Some(mut controller) = previous.remove(&address) {
                    controller.apply_child(child, registry);
                    controller
                } else {
                    Self::from_child(child, registry)
                }
            })
            .collect();
    }

    fn apply_child(&mut self, child: SlotChildApply<'_>, registry: &SlotShapeRegistry) {
        match child {
            SlotChildApply::Data {
                address,
                label,
                data,
                shape,
            } => {
                self.address = address;
                self.label = label;
                self.apply_slot_data(data, shape, registry);
            }
            SlotChildApply::Issue {
                address,
                label,
                message,
            } => {
                self.address = address;
                self.label = label;
                self.revision = None;
                self.apply_issue(message);
            }
        }
    }

    fn from_child(child: SlotChildApply<'_>, registry: &SlotShapeRegistry) -> Self {
        match child {
            SlotChildApply::Data {
                address,
                label,
                data,
                shape,
            } => Self::from_slot_data(address, label, data, shape, registry),
            SlotChildApply::Issue {
                address,
                label,
                message,
            } => Self::issue(address, label, message),
        }
    }

    fn address_with_field(&self, field_name: &str) -> ProjectSlotAddress {
        ProjectSlotAddress::new(
            self.address.node.clone(),
            self.address.root.clone(),
            self.address
                .path
                .child(SlotName::parse(field_name).expect("shape field name is valid")),
        )
    }
}

enum SlotChildApply<'a> {
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

impl SlotChildApply<'_> {
    fn address(&self) -> &ProjectSlotAddress {
        match self {
            Self::Data { address, .. } | Self::Issue { address, .. } => address,
        }
    }
}

fn resolve_shape<'a>(
    mut shape: SlotShapeView<'a>,
    registry: &'a SlotShapeRegistry,
) -> Result<SlotShapeView<'a>, ()> {
    for _ in 0..32 {
        if let Some(id) = shape.ref_id() {
            shape = registry.get_shape(id).ok_or(())?;
            continue;
        }
        if let Some(inner) = shape.custom_shape() {
            shape = inner;
            continue;
        }
        return Ok(shape);
    }
    Err(())
}

fn data_revision(data: &SlotData) -> Option<Revision> {
    match data {
        SlotData::Unit { revision } => Some(*revision),
        SlotData::Value(value) => Some(value.changed_at()),
        SlotData::Record(record) => Some(record.fields_revision),
        SlotData::Map(map) => Some(map.keys_revision),
        SlotData::Enum(value) => Some(value.variant_revision),
        SlotData::Option(value) => Some(value.presence_revision),
    }
}

fn map_key_label(key: &SlotMapKey) -> String {
    format_slot_map_key(key)
}

fn human_label(raw: &str) -> String {
    let normalized = raw.replace(['_', '-'], " ");
    let mut chars = normalized.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first.to_uppercase().collect::<String>() + chars.as_str()
}
