use std::collections::BTreeMap;

use lpc_model::slot::{SlotFieldShapeView, SlotPersistence};
use lpc_model::{
    LpType, LpValue, ProductRef, Revision, SlotData, SlotDirection, SlotMapKey, SlotMapKeyShape,
    SlotName, SlotPathSegment, SlotPolicy, SlotSemantics, SlotShapeLookup, SlotShapeRegistry,
    SlotShapeView, SlotValueShape, SlotValueShapeView, ValueEditorHint,
};

use crate::{
    PendingEditPhase, ProjectSlotAddress, ProjectSlotRoot, UiAssetEditorKind, UiBindingEndpoint,
    UiConfigSlot, UiConfigSlotBody, UiNodeDirtyState, UiProducedBinding, UiProducedProduct,
    UiProducedValue, UiProductRef, UiSlotAsset, UiSlotComposite, UiSlotEditorHint,
    UiSlotEnumComposite, UiSlotFieldState, UiSlotMapComposite, UiSlotMapKeyKind, UiSlotOptionality,
    UiSlotRecord, UiSlotSourceState, UiSlotUnit, UiSlotValue, app::project::format_slot_map_key,
};

use super::{PrefixEditState, SlotBindingFact, SlotBindingFactKind, SlotEditJoin};

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

/// Latest render-relevant body facts for a project slot controller.
#[derive(Clone, Debug, PartialEq)]
enum SlotControllerBody {
    Empty,
    Value {
        value: LpValue,
    },
    Record,
    Map {
        key: SlotMapKeyShape,
    },
    Enum {
        variant: String,
        /// Declared variant idents (raw, in declaration order) from the shape.
        declared: Vec<String>,
    },
    Option {
        present: bool,
    },
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
/// addressable homes. Each controller also retains the latest mirror-derived
/// value, shape, semantics, and policy facts needed to project node DTOs without
/// walking the project mirror a second time.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotController {
    address: ProjectSlotAddress,
    label: String,
    kind: SlotKind,
    body: SlotControllerBody,
    revision: Option<Revision>,
    semantics: SlotSemantics,
    policy: SlotPolicy,
    value_shape: Option<SlotValueShape>,
    source: UiSlotSourceState,
    publish: Option<UiBindingEndpoint>,
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
        controller.apply_issue(issue);
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
        self.apply_context(SlotApplyContext::for_root(&self.address.root));
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
        self.apply_context(SlotApplyContext::for_root(&self.address.root));
        self.revision = None;
        self.apply_issue(issue);
    }

    fn empty(address: ProjectSlotAddress, label: String) -> Self {
        let context = SlotApplyContext::for_root(&address.root);
        Self {
            address,
            label,
            kind: SlotKind::Issue,
            body: SlotControllerBody::Issue,
            revision: None,
            semantics: context.semantics,
            policy: context.policy,
            value_shape: None,
            source: UiSlotSourceState::Direct,
            publish: None,
            issues: Vec::new(),
            state: SlotControllerState::new(),
            children: Vec::new(),
        }
    }

    /// Extract authored binding facts from this root's `bindings` child.
    ///
    /// Meaningful on the def root: since bindings live at node-def roots
    /// (M0), the `bindings` field is a map keyed by local slot name whose
    /// entries carry exactly one of `value`/`source`/`target`.
    pub(in crate::app::project) fn binding_facts(&self) -> Vec<SlotBindingFact> {
        let Some(bindings) = self
            .children
            .iter()
            .find(|child| child.root_field_name() == Some("bindings"))
        else {
            return Vec::new();
        };
        let mut facts = Vec::new();
        for entry in &bindings.children {
            let Some(slot) = entry.last_key_string() else {
                continue;
            };
            for field in &entry.children {
                let Some(endpoint) = field.binding_endpoint() else {
                    continue;
                };
                let kind = match field.last_field_name() {
                    Some("source") => SlotBindingFactKind::Source(endpoint),
                    Some("target") => SlotBindingFactKind::Target(endpoint),
                    Some("value") => SlotBindingFactKind::Literal(endpoint),
                    _ => continue,
                };
                facts.push(SlotBindingFact {
                    slot: slot.clone(),
                    kind,
                });
            }
        }
        facts
    }

    /// Apply authored binding facts to this root's top-level field slots.
    ///
    /// Resets binding state on every field first so removed bindings clear.
    /// `source`/`value` facts mark the named slot's value as bound; `target`
    /// facts mark it as publishing to the endpoint.
    pub(in crate::app::project) fn apply_binding_facts(&mut self, facts: &[SlotBindingFact]) {
        for child in &mut self.children {
            let Some(name) = child.root_field_name().map(str::to_string) else {
                continue;
            };
            child.source = UiSlotSourceState::Direct;
            child.publish = None;
            for fact in facts.iter().filter(|fact| fact.slot == name) {
                match &fact.kind {
                    SlotBindingFactKind::Source(endpoint)
                    | SlotBindingFactKind::Literal(endpoint) => {
                        child.source = UiSlotSourceState::Bound(endpoint.clone());
                    }
                    SlotBindingFactKind::Target(endpoint) => {
                        child.publish = Some(endpoint.clone());
                    }
                }
            }
        }
    }

    /// True when this root has a top-level field child named `name`.
    pub(in crate::app::project) fn has_root_field(&self, name: &str) -> bool {
        self.children
            .iter()
            .any(|child| child.root_field_name() == Some(name))
    }

    /// This slot's field name when it is a root-level field (`def.time`).
    fn root_field_name(&self) -> Option<&str> {
        match self.address.path.segments() {
            [SlotPathSegment::Field(name)] => Some(name.as_str()),
            _ => None,
        }
    }

    /// Trailing field-segment name, regardless of depth.
    fn last_field_name(&self) -> Option<&str> {
        match self.address.path.segments().last() {
            Some(SlotPathSegment::Field(name)) => Some(name.as_str()),
            _ => None,
        }
    }

    /// Trailing map-key segment as a display string.
    fn last_key_string(&self) -> Option<String> {
        match self.address.path.segments().last() {
            Some(SlotPathSegment::Key(key)) => Some(format_slot_map_key(key)),
            _ => None,
        }
    }

    /// Endpoint carried by a present binding option field (`source`/`target`
    /// hold an endpoint string; `value` holds an arbitrary literal).
    fn binding_endpoint(&self) -> Option<UiBindingEndpoint> {
        if !matches!(&self.body, SlotControllerBody::Option { present: true }) {
            return None;
        }
        let value = self.children.first()?.value()?;
        Some(match value {
            LpValue::String(endpoint) => UiBindingEndpoint::new(endpoint.clone()),
            other => UiBindingEndpoint::new(UiSlotValue::from_lp_value(other).display)
                .with_detail("literal value"),
        })
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

    /// Project this slot and its descendants as a config slot row.
    pub(in crate::app::project) fn ui_config_slot(&self, edits: &SlotEditJoin<'_>) -> UiConfigSlot {
        let mut slot = UiConfigSlot::new(
            self.ui_key(),
            self.label.clone(),
            self.ui_config_slot_body(edits),
        )
        .with_address(self.address.clone())
        .with_source(self.ui_source())
        .with_state(self.ui_field_state(edits));

        if let Some(publish) = &self.publish {
            slot = slot.with_publish(publish.clone());
        }
        if let Some(address) = self.edit_entry_address(edits) {
            slot = slot.with_edit_entry_address(address);
        }
        if let Some(detail) = self.ui_detail() {
            slot = slot.with_detail(detail);
        }
        if let Some(optionality) = self.ui_optionality() {
            slot = slot.with_optionality(optionality);
        }
        if let Some(composite) = self.ui_composite() {
            slot = slot.with_composite(composite);
        }
        for issue in &self.issues {
            slot = slot.with_issue(issue.clone());
        }
        slot
    }

    /// Project this slot as an asset row if it looks asset-like.
    pub(in crate::app::project) fn ui_asset_slot(
        &self,
        edits: &SlotEditJoin<'_>,
    ) -> Option<UiConfigSlot> {
        let asset = self.ui_slot_asset()?;
        let mut slot = UiConfigSlot::asset(self.ui_key(), self.label.clone(), asset)
            .with_address(self.address.clone())
            .with_source(self.ui_source())
            .with_state(self.ui_field_state(edits));
        if let Some(address) = self.edit_entry_address(edits) {
            slot = slot.with_edit_entry_address(address);
        }
        if let Some(detail) = self.ui_detail() {
            slot = slot.with_detail(detail);
        }
        if let Some(optionality) = self.ui_optionality() {
            slot = slot.with_optionality(optionality);
        }
        for issue in &self.issues {
            slot = slot.with_issue(issue.clone());
        }
        Some(slot)
    }

    /// Binding metadata for a produced slot (populated by the node's
    /// binding-facts pass).
    fn ui_produced_binding(&self) -> UiProducedBinding {
        let mut binding = UiProducedBinding::none();
        if let Some(endpoint) = &self.publish {
            if endpoint.label.starts_with("bus#") {
                binding.bindings.bus_target = Some(endpoint.clone());
            } else {
                binding.bindings.target_bindings.push(endpoint.clone());
            }
        }
        binding
    }

    /// Project this slot as a produced product if it carries product output.
    pub(in crate::app::project) fn ui_produced_product(&self) -> Option<UiProducedProduct> {
        if !self.is_produced_slot() {
            return None;
        }
        match self.value() {
            Some(LpValue::Product(ProductRef::Visual(product))) => {
                let product_ref = UiProductRef::from_visual_product(*product);
                Some(
                    UiProducedProduct::visual(self.label.clone())
                        .with_product(product_ref)
                        .with_detail(format!(
                            "node {} output {}",
                            product.node(),
                            product.output()
                        )),
                )
            }
            Some(LpValue::Product(ProductRef::Control(product))) => {
                let extent = product.preferred_extent();
                let product_ref = UiProductRef::from_control_product(*product);
                Some(
                    UiProducedProduct::control(self.label.clone())
                        .with_product(product_ref)
                        .with_detail(format!(
                            "node {} output {} {}x{}",
                            product.node(),
                            product.output(),
                            extent.rows,
                            extent.samples_per_row
                        )),
                )
            }
            Some(LpValue::Unset) if self.value_shape_is_product() => {
                Some(UiProducedProduct::empty(self.label.clone()))
            }
            None if self.value_shape_is_product() => {
                Some(UiProducedProduct::empty(self.label.clone()))
            }
            _ => None,
        }
        .map(|mut product| {
            product.binding = self.ui_produced_binding();
            product
        })
    }

    /// Collect concrete produced products under this slot.
    pub(in crate::app::project) fn collect_produced_product_refs(
        &self,
        products: &mut Vec<UiProductRef>,
    ) {
        if let Some(product) = self
            .ui_produced_product()
            .and_then(|product| product.product)
        {
            products.push(product);
            return;
        }
        for child in &self.children {
            child.collect_produced_product_refs(products);
        }
    }

    /// Project this slot as a compact produced value if it is produced but not a product.
    pub(in crate::app::project) fn ui_produced_value(&self) -> Option<UiProducedValue> {
        if !self.is_produced_slot() || self.ui_produced_product().is_some() {
            return None;
        }
        let value = self.value()?;
        let ui_value = UiSlotValue::from_lp_value(value);
        let mut produced = UiProducedValue::new(self.label.clone(), ui_value.display);
        produced.detail = Some(ui_value.kind.type_label().to_string());
        produced.unit = self.ui_unit();
        produced.binding = self.ui_produced_binding();
        Some(produced)
    }

    /// Collect produced section items under this slot.
    pub(in crate::app::project) fn collect_produced(
        &self,
        products: &mut Vec<UiProducedProduct>,
        values: &mut Vec<UiProducedValue>,
    ) {
        if let Some(product) = self.ui_produced_product() {
            products.push(product);
            return;
        }
        if let Some(value) = self.ui_produced_value() {
            values.push(value);
            return;
        }
        for child in &self.children {
            child.collect_produced(products, values);
        }
    }

    /// Collect config and asset rows under this slot.
    pub(in crate::app::project) fn collect_config(
        &self,
        edits: &SlotEditJoin<'_>,
        config_slots: &mut Vec<UiConfigSlot>,
        asset_slots: &mut Vec<UiConfigSlot>,
    ) {
        if self.is_internal_config_slot() {
            return;
        }
        if let Some(asset) = self.ui_asset_slot(edits) {
            asset_slots.push(asset);
            return;
        }
        if self.address.is_root() && self.children_are_top_level_rows() {
            for child in &self.children {
                child.collect_config(edits, config_slots, asset_slots);
            }
            return;
        }
        config_slots.push(self.ui_config_slot(edits));
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
        self.body = SlotControllerBody::Issue;
        self.value_shape = None;

        let Ok(shape) = resolve_shape(shape, registry) else {
            self.apply_issue("slot shape could not be resolved");
            return;
        };

        if shape.is_unit() {
            self.apply_unit(data);
        } else if let Some(value_shape) = shape.value_shape() {
            self.apply_value(data, value_shape);
        } else if let Some(field_count) = shape.record_fields_len() {
            self.apply_record(data, shape, field_count, registry);
        } else if let Some(value_shape) = shape.map_value() {
            let key = shape.map_key().unwrap_or(SlotMapKeyShape::String);
            self.apply_map(data, key, value_shape, registry);
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
                self.body = SlotControllerBody::Empty;
                self.children.clear();
            }
            _ => self.apply_issue("expected unit data"),
        }
    }

    fn apply_value(&mut self, data: &SlotData, shape: SlotValueShapeView<'_>) {
        match data {
            SlotData::Value(value) => {
                self.kind = SlotKind::Value;
                self.body = SlotControllerBody::Value {
                    value: value.get().clone(),
                };
                self.value_shape = Some(owned_value_shape(shape));
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
        self.body = SlotControllerBody::Record;
        let children = (0..field_count)
            .map(|index| {
                let Some(field) = shape.record_field(index) else {
                    return SlotChildApply::Issue {
                        address: self.address.clone(),
                        label: format!("field {index}"),
                        message: "field shape is missing".to_string(),
                        context: self.context(),
                    };
                };
                let label = human_label(field.name_str());
                let address = self.address_with_field(field.name_str());
                let context = self.field_context(field);
                match record.fields.get(index) {
                    Some(data) => SlotChildApply::Data {
                        address,
                        label,
                        data,
                        shape: field.shape(),
                        context,
                    },
                    None => SlotChildApply::Issue {
                        address,
                        label,
                        message: "field data is missing".to_string(),
                        context,
                    },
                }
            })
            .collect();
        self.reconcile_children(children, registry);
    }

    fn apply_map(
        &mut self,
        data: &SlotData,
        key: SlotMapKeyShape,
        value_shape: SlotShapeView<'_>,
        registry: &SlotShapeRegistry,
    ) {
        let SlotData::Map(map) = data else {
            self.apply_issue("expected map data");
            return;
        };

        self.kind = SlotKind::Map;
        self.body = SlotControllerBody::Map { key };
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
                context: self.context(),
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
        self.body = SlotControllerBody::Enum {
            variant: value.variant.as_str().to_string(),
            declared: (0..)
                .map_while(|index| shape.enum_variant(index))
                .map(|variant| variant.name_str().to_string())
                .collect(),
        };
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
            context: self.context(),
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
        self.body = SlotControllerBody::Option {
            present: value.data.is_some(),
        };
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
            context: self.context(),
        }];
        self.reconcile_children(children, registry);
    }

    fn apply_issue(&mut self, issue: impl Into<String>) {
        self.kind = SlotKind::Issue;
        self.body = SlotControllerBody::Issue;
        self.value_shape = None;
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
                context,
            } => {
                self.address = address;
                self.label = label;
                self.apply_context(context);
                self.apply_slot_data(data, shape, registry);
            }
            SlotChildApply::Issue {
                address,
                label,
                message,
                context,
            } => {
                self.address = address;
                self.label = label;
                self.apply_context(context);
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
                context,
            } => {
                let mut controller = Self::empty(address, label);
                controller.apply_context(context);
                controller.apply_slot_data(data, shape, registry);
                controller
            }
            SlotChildApply::Issue {
                address,
                label,
                message,
                context,
            } => {
                let mut controller = Self::empty(address, label);
                controller.apply_context(context);
                controller.apply_issue(message);
                controller
            }
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

    fn context(&self) -> SlotApplyContext {
        SlotApplyContext {
            semantics: self.semantics,
            policy: self.policy,
        }
    }

    fn apply_context(&mut self, context: SlotApplyContext) {
        self.semantics = context.semantics;
        self.policy = context.policy;
    }

    fn field_context(&self, field: SlotFieldShapeView<'_>) -> SlotApplyContext {
        let semantics = field.semantics();
        let policy = field.policy();
        let default_semantics = semantics == SlotSemantics::default();
        let default_policy = policy == SlotPolicy::default();
        let mut context =
            if self.address.root == ProjectSlotRoot::State && default_semantics && default_policy {
                self.context()
            } else {
                SlotApplyContext { semantics, policy }
            };
        if context.semantics.direction == SlotDirection::Produced && default_policy {
            context.policy = SlotPolicy::read_only_transient();
        }
        context
    }

    fn ui_config_slot_body(&self, edits: &SlotEditJoin<'_>) -> UiConfigSlotBody {
        match &self.body {
            SlotControllerBody::Empty => UiConfigSlotBody::Empty,
            SlotControllerBody::Value { value } => {
                // A buffered or overlay-pending edit shadows the synced value
                // (rubber-band protection: an older pulled value must not
                // fight the value the user asked for).
                let value = edits.value_shadow(&self.address).unwrap_or(value);
                UiConfigSlotBody::Value(self.ui_slot_value(value))
            }
            SlotControllerBody::Record
            | SlotControllerBody::Map { .. }
            | SlotControllerBody::Enum { .. } => UiConfigSlotBody::Record(UiSlotRecord::new(
                self.children
                    .iter()
                    .filter(|child| !child.is_internal_config_slot())
                    .map(|child| child.ui_config_slot(edits))
                    .collect(),
            )),
            SlotControllerBody::Option { present } if *present => {
                self.ui_present_option_body(edits)
            }
            SlotControllerBody::Option { .. } | SlotControllerBody::Issue => {
                UiConfigSlotBody::Empty
            }
        }
    }

    fn ui_present_option_body(&self, edits: &SlotEditJoin<'_>) -> UiConfigSlotBody {
        let Some(child) = self.children.first() else {
            return UiConfigSlotBody::Empty;
        };
        if let Some(asset) = child.ui_slot_asset() {
            return UiConfigSlotBody::Asset(asset);
        }
        child.ui_config_slot_body(edits)
    }

    fn ui_slot_value(&self, value: &LpValue) -> UiSlotValue {
        let mut value = UiSlotValue::from_lp_value(value);
        if let Some(shape) = &self.value_shape {
            value.editor = ui_editor_hint(&shape.editor);
            if let Some(description) = shape.meta.description.as_ref() {
                value = value.with_detail(description.clone());
            }
        }
        value
    }

    fn ui_slot_asset(&self) -> Option<UiSlotAsset> {
        if !self.is_asset_like() {
            return None;
        }
        let value = self.value()?;
        let (source, content) = match value {
            LpValue::String(value) if looks_like_inline_glsl(value) => {
                ("inline.glsl".to_string(), Some(value.clone()))
            }
            LpValue::String(value) if looks_like_inline_svg(value) => {
                ("inline.svg".to_string(), Some(value.clone()))
            }
            LpValue::String(value) => (value.clone(), None),
            LpValue::Resource(resource) => (
                format!("resource {:?}:{}", resource.domain, resource.id),
                None,
            ),
            other => (UiSlotValue::from_lp_value(other).display, None),
        };
        let editor = asset_editor_kind(&source, content.as_deref(), self.value_shape.as_ref());
        let mut asset = UiSlotAsset::new(source, editor);
        if let Some(content) = content {
            asset = asset.with_content(content);
        }
        Some(asset)
    }

    fn ui_key(&self) -> String {
        if self.address.path.is_root() {
            self.address.root.name().to_string()
        } else {
            self.address.path.to_string()
        }
    }

    fn ui_source(&self) -> UiSlotSourceState {
        // A binding supplies the value, so bound wins over an unset or
        // absent authored fallback.
        if self.source.is_bound() {
            return self.source.clone();
        }
        if matches!(&self.body, SlotControllerBody::Option { present: false }) {
            return UiSlotSourceState::Unset;
        }
        match self.value() {
            Some(LpValue::Unset) => UiSlotSourceState::Unset,
            _ => self.source.clone(),
        }
    }

    fn ui_optionality(&self) -> Option<UiSlotOptionality> {
        let SlotControllerBody::Option { present } = &self.body else {
            return None;
        };
        Some(if *present {
            UiSlotOptionality::included(self.policy.writable)
        } else {
            UiSlotOptionality::excluded(self.policy.writable)
        })
    }

    fn ui_unit(&self) -> Option<UiSlotUnit> {
        UiSlotUnit::from_known_label(&self.label)
    }

    /// Structural gesture facts for map and enum composite rows (M3 D1).
    fn ui_composite(&self) -> Option<UiSlotComposite> {
        match &self.body {
            SlotControllerBody::Map { key } => {
                let key_kind = match key {
                    SlotMapKeyShape::String => UiSlotMapKeyKind::String,
                    SlotMapKeyShape::I32 => UiSlotMapKeyKind::I32,
                    SlotMapKeyShape::U32 => UiSlotMapKeyKind::U32,
                };
                Some(UiSlotComposite::Map(UiSlotMapComposite {
                    key_kind,
                    suggested_key: self.suggested_map_key(key_kind),
                }))
            }
            SlotControllerBody::Enum { variant, declared } => {
                Some(UiSlotComposite::Enum(UiSlotEnumComposite {
                    active: variant.clone(),
                    variants: declared.clone(),
                }))
            }
            _ => None,
        }
    }

    /// Suggested key for the add-entry gesture (numeric maps add here
    /// immediately; the key input is the override): the **first free index**
    /// counting up from 0 over the effective entry keys, filling gaps left by
    /// removed entries — effective keys `{0, 2}` suggest `1`, so a deleted
    /// middle key can be refilled. Empty for string key maps (string keys
    /// cannot be guessed; the input stays the primary flow).
    fn suggested_map_key(&self, key_kind: UiSlotMapKeyKind) -> String {
        let entry_keys = || {
            self.children
                .iter()
                .filter_map(|child| match child.address.path.segments().last() {
                    Some(SlotPathSegment::Key(key)) => Some(key),
                    _ => None,
                })
        };
        match key_kind {
            UiSlotMapKeyKind::String => String::new(),
            UiSlotMapKeyKind::I32 => first_free_index(entry_keys().filter_map(|key| match key {
                SlotMapKey::I32(value) => u32::try_from(*value).ok(),
                _ => None,
            }))
            .to_string(),
            UiSlotMapKeyKind::U32 => first_free_index(entry_keys().filter_map(|key| match key {
                SlotMapKey::U32(value) => Some(*value),
                _ => None,
            }))
            .to_string(),
        }
    }

    fn ui_field_state(&self, edits: &SlotEditJoin<'_>) -> UiSlotFieldState {
        let mut state = if self.policy.writable {
            UiSlotFieldState::editable()
        } else {
            UiSlotFieldState::readonly()
        };
        state = state.with_live(self.policy.persistence == SlotPersistence::Transient);

        // Join order: edit buffer (Saving/Error + invalid reason), then the
        // overlay mirror (Dirty), then — for composite slots only — the
        // prefix-aware join over edits strictly under this path (D4), else
        // Clean. The prefix join is what surfaces a removed map entry (its
        // row is gone; the parent map reads Dirty) and a rejected gesture on
        // a not-yet-existing path (the dispatching composite reads Error).
        if let Some(edit) = edits.pending(&self.address) {
            state = match &edit.phase {
                // `AwaitingRefresh` keeps the Saving treatment: the server
                // normalized the edit away, but the synced view is stale
                // until the next applied read (the entry's value shadow keeps
                // the DTO on the acked value through that window).
                PendingEditPhase::Pending
                | PendingEditPhase::InFlight { .. }
                | PendingEditPhase::AwaitingRefresh => state.with_dirty(UiNodeDirtyState::Saving),
                PendingEditPhase::Failed { reason } => state
                    .with_dirty(UiNodeDirtyState::Error)
                    .with_invalid(reason.clone()),
            };
        } else if edits.overlay_dirty(&self.address) {
            state = state.with_dirty(UiNodeDirtyState::Dirty);
        } else if self.is_composite()
            && let Some(under) = edits.state_under(&self.address)
        {
            state = match under {
                PrefixEditState::Failed { reason } => state
                    .with_dirty(UiNodeDirtyState::Error)
                    .with_invalid(reason),
                PrefixEditState::Saving => state.with_dirty(UiNodeDirtyState::Saving),
                PrefixEditState::Dirty => state.with_dirty(UiNodeDirtyState::Dirty),
            };
        }

        if state.invalid.is_none()
            && let Some(issue) = self.issues.first()
        {
            state = state.with_invalid(issue.clone());
        }
        state
    }

    /// The address of this row's **own** edit entry, if the buffer or the
    /// overlay mirror holds one: the row's own address, or — for a present
    /// option row, whose interior value renders inline on the same row —
    /// the interior `.some` child address, or — for an enum row, whose
    /// variant-switch gesture stores its entry at the variant child path
    /// (`enum_path.Variant`, never the enum's own path) — the variant child
    /// address carrying an entry (the active variant first, so the enum row
    /// where the variant select lives offers the Revert that undoes the
    /// switch; the payload row's exact-match revert targets the same entry).
    /// `None` for rows whose dirty state is prefix-only (edits strictly under
    /// a composite): their per-entry revert lives in the save panel, and a
    /// row-level Revert at the composite's own path would be a no-op.
    fn edit_entry_address(&self, edits: &SlotEditJoin<'_>) -> Option<ProjectSlotAddress> {
        let has_entry = |address: &ProjectSlotAddress| {
            edits.pending(address).is_some() || edits.overlay_dirty(address)
        };
        if has_entry(&self.address) {
            return Some(self.address.clone());
        }
        match &self.body {
            SlotControllerBody::Option { present: true } => {
                let child = self.children.first()?;
                has_entry(&child.address).then(|| child.address.clone())
            }
            SlotControllerBody::Enum { variant, declared } => {
                // Active variant first (the steady state after the switch
                // round-trips); the other declared variants cover the
                // ack-to-refresh window, where the view's active variant
                // still lags the acked switch.
                core::iter::once(variant)
                    .chain(declared.iter().filter(|name| *name != variant))
                    .filter_map(|name| self.variant_child_address(name))
                    .find(|address| has_entry(address))
            }
            _ => None,
        }
    }

    /// The slot address of `variant`'s payload under this enum row, using the
    /// raw declared ident verbatim (D7). `None` when the ident is not a valid
    /// slot name (shape-declared idents always are).
    fn variant_child_address(&self, variant: &str) -> Option<ProjectSlotAddress> {
        Some(ProjectSlotAddress::new(
            self.address.node.clone(),
            self.address.root.clone(),
            self.address.path.child(SlotName::parse(variant).ok()?),
        ))
    }

    /// True for slot kinds whose dirty state includes descendant edit paths
    /// through the prefix-aware join. Value leaves stay exact-match only.
    fn is_composite(&self) -> bool {
        matches!(
            self.kind,
            SlotKind::Record | SlotKind::Map | SlotKind::Enum | SlotKind::Option
        )
    }

    fn ui_detail(&self) -> Option<String> {
        match &self.body {
            SlotControllerBody::Value { value } => Some(
                UiSlotValue::from_lp_value(value)
                    .kind
                    .type_label()
                    .to_string(),
            ),
            SlotControllerBody::Record => Some(child_count_detail(self.children.len(), "field")),
            SlotControllerBody::Map { .. } => {
                Some(child_count_detail(self.children.len(), "entry"))
            }
            SlotControllerBody::Enum { variant, .. } => Some(format!("variant {variant}")),
            SlotControllerBody::Option { present: true } => {
                self.children.first().and_then(|child| child.ui_detail())
            }
            SlotControllerBody::Option { present: false } => None,
            SlotControllerBody::Empty | SlotControllerBody::Issue => None,
        }
    }

    fn value(&self) -> Option<&LpValue> {
        match &self.body {
            SlotControllerBody::Value { value } => Some(value),
            _ => None,
        }
    }

    fn is_produced_slot(&self) -> bool {
        self.address.root == ProjectSlotRoot::State
            || self.semantics.direction == SlotDirection::Produced
    }

    fn value_shape_is_product(&self) -> bool {
        matches!(
            self.value_shape.as_ref().map(|shape| &shape.ty),
            Some(LpType::Product(_))
        ) || matches!(
            self.value_shape.as_ref().map(|shape| &shape.editor),
            Some(ValueEditorHint::VisualProduct | ValueEditorHint::ControlProduct)
        )
    }

    fn is_asset_like(&self) -> bool {
        if self.is_produced_slot() {
            return false;
        }
        if matches!(
            self.value_shape.as_ref().map(|shape| &shape.editor),
            Some(ValueEditorHint::Resource | ValueEditorHint::RuntimeBufferResource)
        ) {
            return true;
        }
        let key = self.ui_key().to_ascii_lowercase();
        if matches!(key.as_str(), "source" | "shader" | "glsl" | "svg")
            || key.ends_with(".source")
            || key.ends_with(".shader")
            || key.ends_with(".glsl")
            || key.ends_with(".svg")
        {
            return matches!(
                self.value(),
                Some(LpValue::String(_) | LpValue::Resource(_))
            );
        }
        matches!(
            self.value(),
            Some(LpValue::String(value))
                if value.ends_with(".glsl")
                    || value.ends_with(".svg")
                    || looks_like_inline_glsl(value)
                    || looks_like_inline_svg(value)
        )
    }

    fn is_internal_config_slot(&self) -> bool {
        matches!(
            self.address.path.segments().first(),
            Some(SlotPathSegment::Field(name)) if name.as_str() == "bindings"
        )
    }

    fn children_are_top_level_rows(&self) -> bool {
        matches!(
            self.body,
            SlotControllerBody::Record
                | SlotControllerBody::Map { .. }
                | SlotControllerBody::Enum { .. }
                | SlotControllerBody::Option { present: true }
        )
    }
}

enum SlotChildApply<'a> {
    Data {
        address: ProjectSlotAddress,
        label: String,
        data: &'a SlotData,
        shape: SlotShapeView<'a>,
        context: SlotApplyContext,
    },
    Issue {
        address: ProjectSlotAddress,
        label: String,
        message: String,
        context: SlotApplyContext,
    },
}

impl SlotChildApply<'_> {
    fn address(&self) -> &ProjectSlotAddress {
        match self {
            Self::Data { address, .. } | Self::Issue { address, .. } => address,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SlotApplyContext {
    semantics: SlotSemantics,
    policy: SlotPolicy,
}

impl SlotApplyContext {
    fn for_root(root: &ProjectSlotRoot) -> Self {
        match root {
            ProjectSlotRoot::Def => Self {
                semantics: SlotSemantics::local(),
                policy: SlotPolicy::writable_persisted(),
            },
            ProjectSlotRoot::State => Self {
                semantics: SlotSemantics::produced(),
                policy: SlotPolicy::read_only_transient(),
            },
            ProjectSlotRoot::Other(_) => Self {
                semantics: SlotSemantics::local(),
                policy: SlotPolicy::read_only_persisted(),
            },
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

/// The smallest index from 0 upward that `used` does not contain — the
/// gap-filling suggested key for numeric maps (negative i32 keys never block
/// a suggestion; they are filtered out before the scan).
fn first_free_index(used: impl Iterator<Item = u32>) -> u32 {
    let used: std::collections::BTreeSet<u32> = used.collect();
    (0..)
        .find(|candidate| !used.contains(candidate))
        .expect("a finite key set always leaves a free index")
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

fn owned_value_shape(shape: SlotValueShapeView<'_>) -> SlotValueShape {
    match shape {
        SlotValueShapeView::Static(shape) => shape.to_owned_value_shape(),
        SlotValueShapeView::Dynamic(shape) => shape.clone(),
    }
}

fn ui_editor_hint(editor: &ValueEditorHint) -> UiSlotEditorHint {
    match editor {
        ValueEditorHint::Plain
        | ValueEditorHint::NodeRef
        | ValueEditorHint::Path
        | ValueEditorHint::Dimensions
        | ValueEditorHint::Affine2d
        | ValueEditorHint::Resource
        | ValueEditorHint::RuntimeBufferResource
        | ValueEditorHint::VisualProduct
        | ValueEditorHint::ControlProduct => UiSlotEditorHint::Auto,
        ValueEditorHint::Number { min, max, step } => UiSlotEditorHint::Number {
            min: min.map(|value| value.0),
            max: max.map(|value| value.0),
            step: step.map(|value| value.0),
        },
        ValueEditorHint::Slider { min, max, step } => UiSlotEditorHint::Slider {
            min: min.0,
            max: max.0,
            step: step.map(|value| value.0),
        },
        ValueEditorHint::Xy => UiSlotEditorHint::Xy,
        ValueEditorHint::Dropdown { options } => UiSlotEditorHint::dropdown(
            options
                .iter()
                .map(|option| (option.value.clone(), option.label.clone())),
        ),
    }
}

fn asset_editor_kind(
    source: &str,
    content: Option<&str>,
    shape: Option<&SlotValueShape>,
) -> UiAssetEditorKind {
    let lower = source.to_ascii_lowercase();
    if lower.ends_with(".glsl") || content.is_some_and(looks_like_inline_glsl) {
        UiAssetEditorKind::Glsl
    } else if lower.ends_with(".svg") || content.is_some_and(looks_like_inline_svg) {
        UiAssetEditorKind::Svg
    } else if matches!(
        shape.map(|shape| &shape.editor),
        Some(ValueEditorHint::RuntimeBufferResource)
    ) {
        UiAssetEditorKind::Binary
    } else {
        UiAssetEditorKind::Text
    }
}

fn looks_like_inline_glsl(value: &str) -> bool {
    value.contains("#version")
        || value.contains("void main")
        || value.contains("void mainImage")
        || value.contains("gl_FragColor")
}

fn looks_like_inline_svg(value: &str) -> bool {
    value.trim_start().starts_with("<svg")
}

fn child_count_detail(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::{
        LpType, LpValue, SlotEnumEncoding, SlotMapDyn, SlotMeta, SlotShape, SlotShapeRegistry,
        SlotVariantShape, WithRevision,
    };

    use crate::{PendingEdit, ProjectNodeAddress, app::project::slot::SlotEditJoin};

    use super::*;

    fn slot_address(path: &str) -> ProjectSlotAddress {
        ProjectSlotAddress::new(
            ProjectNodeAddress::parse("/demo.project/pixels.fixture").unwrap(),
            ProjectSlotRoot::def(),
            lpc_model::SlotPath::parse(path).unwrap(),
        )
    }

    fn u32_map_data(keys: &[u32]) -> SlotData {
        let mut map = SlotMapDyn::with_revision(Revision::new(1), Default::default());
        for key in keys {
            map.entries.insert(
                SlotMapKey::U32(*key),
                SlotData::Value(WithRevision::new(Revision::new(1), LpValue::U32(*key))),
            );
        }
        SlotData::Map(map)
    }

    fn u32_map_shape() -> SlotShape {
        SlotShape::Map {
            meta: SlotMeta::empty(),
            key: SlotMapKeyShape::U32,
            value: Box::new(SlotShape::value(LpType::U32)),
        }
    }

    #[test]
    fn map_slot_suggests_the_first_free_index_filling_gaps() {
        let registry = SlotShapeRegistry::default();
        let shape = u32_map_shape();
        // A deleted middle key must be refillable: effective {0, 2} → 1.
        let controller = SlotController::from_slot_data(
            slot_address("ring_lamp_counts"),
            "Ring lamp counts".to_string(),
            &u32_map_data(&[0, 2]),
            SlotShapeView::Dynamic(&shape),
            &registry,
        );

        let slot = controller.ui_config_slot(&SlotEditJoin::empty());

        assert_eq!(
            slot.composite,
            Some(UiSlotComposite::Map(UiSlotMapComposite {
                key_kind: UiSlotMapKeyKind::U32,
                suggested_key: "1".to_string(),
            }))
        );
    }

    #[test]
    fn gapless_map_suggests_the_next_index() {
        let registry = SlotShapeRegistry::default();
        let shape = u32_map_shape();
        let controller = SlotController::from_slot_data(
            slot_address("ring_lamp_counts"),
            "Ring lamp counts".to_string(),
            &u32_map_data(&[0, 1, 2]),
            SlotShapeView::Dynamic(&shape),
            &registry,
        );

        let slot = controller.ui_config_slot(&SlotEditJoin::empty());

        assert_eq!(
            slot.composite,
            Some(UiSlotComposite::Map(UiSlotMapComposite {
                key_kind: UiSlotMapKeyKind::U32,
                suggested_key: "3".to_string(),
            }))
        );
    }

    #[test]
    fn empty_map_suggests_the_first_index() {
        let registry = SlotShapeRegistry::default();
        let shape = u32_map_shape();
        let controller = SlotController::from_slot_data(
            slot_address("paths"),
            "Paths".to_string(),
            &u32_map_data(&[]),
            SlotShapeView::Dynamic(&shape),
            &registry,
        );

        let slot = controller.ui_config_slot(&SlotEditJoin::empty());

        assert_eq!(
            slot.composite,
            Some(UiSlotComposite::Map(UiSlotMapComposite {
                key_kind: UiSlotMapKeyKind::U32,
                suggested_key: "0".to_string(),
            }))
        );
    }

    #[test]
    fn string_map_suggests_no_key() {
        let registry = SlotShapeRegistry::default();
        let shape = SlotShape::Map {
            meta: SlotMeta::empty(),
            key: SlotMapKeyShape::String,
            value: Box::new(SlotShape::value(LpType::F32)),
        };
        let mut map = SlotMapDyn::with_revision(Revision::new(1), Default::default());
        map.entries.insert(
            SlotMapKey::String("warm".to_string()),
            SlotData::Value(WithRevision::new(Revision::new(1), LpValue::F32(0.5))),
        );
        let controller = SlotController::from_slot_data(
            slot_address("presets"),
            "Presets".to_string(),
            &SlotData::Map(map),
            SlotShapeView::Dynamic(&shape),
            &registry,
        );

        let slot = controller.ui_config_slot(&SlotEditJoin::empty());

        assert_eq!(
            slot.composite,
            Some(UiSlotComposite::Map(UiSlotMapComposite {
                key_kind: UiSlotMapKeyKind::String,
                suggested_key: String::new(),
            }))
        );
    }

    #[test]
    fn enum_slot_projects_declared_variants_verbatim() {
        let registry = SlotShapeRegistry::default();
        let shape = SlotShape::Enum {
            meta: SlotMeta::empty(),
            encoding: SlotEnumEncoding::default(),
            variants: vec![
                SlotVariantShape::new(
                    "Unset",
                    SlotShape::Unit {
                        meta: SlotMeta::empty(),
                    },
                )
                .unwrap(),
                SlotVariantShape::new("PathPoints", SlotShape::value(LpType::F32)).unwrap(),
                SlotVariantShape::new("SvgPath", SlotShape::value(LpType::String)).unwrap(),
            ],
        };
        let data = SlotData::Enum(lpc_model::SlotEnum::with_version(
            Revision::new(1),
            SlotName::parse("PathPoints").unwrap(),
            SlotData::Value(WithRevision::new(Revision::new(1), LpValue::F32(0.0))),
        ));
        let controller = SlotController::from_slot_data(
            slot_address("mapping"),
            "Mapping".to_string(),
            &data,
            SlotShapeView::Dynamic(&shape),
            &registry,
        );

        let slot = controller.ui_config_slot(&SlotEditJoin::empty());

        assert_eq!(
            slot.composite,
            Some(UiSlotComposite::Enum(UiSlotEnumComposite {
                active: "PathPoints".to_string(),
                variants: vec![
                    "Unset".to_string(),
                    "PathPoints".to_string(),
                    "SvgPath".to_string(),
                ],
            }))
        );
    }

    fn overlay_join(
        entries: &[(&str, lpc_model::SlotEditOp)],
    ) -> (
        std::collections::BTreeMap<ProjectSlotAddress, PendingEdit>,
        std::collections::BTreeMap<ProjectSlotAddress, lpc_model::SlotEditOp>,
    ) {
        let overlay = entries
            .iter()
            .map(|(path, op)| (slot_address(path), op.clone()))
            .collect();
        (std::collections::BTreeMap::new(), overlay)
    }

    fn option_u32_shape() -> SlotShape {
        SlotShape::Option {
            meta: SlotMeta::empty(),
            some: Box::new(SlotShape::value(LpType::U32)),
        }
    }

    #[test]
    fn own_edit_entry_projects_the_revert_target() {
        let registry = SlotShapeRegistry::default();
        let shape = SlotShape::value(LpType::U32);
        let controller = SlotController::from_slot_data(
            slot_address("brightness"),
            "Brightness".to_string(),
            &SlotData::Value(WithRevision::new(Revision::new(1), LpValue::U32(255))),
            SlotShapeView::Dynamic(&shape),
            &registry,
        );
        let (buffer, overlay) = overlay_join(&[(
            "brightness",
            lpc_model::SlotEditOp::AssignValue(LpValue::U32(64)),
        )]);
        let join = SlotEditJoin::new(&buffer, overlay, Default::default());

        let slot = controller.ui_config_slot(&join);

        assert_eq!(slot.edit_entry_address, Some(slot_address("brightness")));
    }

    #[test]
    fn prefix_only_dirty_composite_projects_no_revert_target() {
        let registry = SlotShapeRegistry::default();
        let shape = u32_map_shape();
        let controller = SlotController::from_slot_data(
            slot_address("ring_lamp_counts"),
            "Ring lamp counts".to_string(),
            &u32_map_data(&[0]),
            SlotShapeView::Dynamic(&shape),
            &registry,
        );
        let (buffer, overlay) =
            overlay_join(&[("ring_lamp_counts[1]", lpc_model::SlotEditOp::Remove)]);
        let join = SlotEditJoin::new(&buffer, overlay, Default::default());

        let slot = controller.ui_config_slot(&join);

        assert_eq!(
            slot.state.dirty,
            crate::UiNodeDirtyState::Dirty,
            "the removed entry marks the map dirty via the prefix join"
        );
        assert_eq!(
            slot.edit_entry_address, None,
            "prefix-only dirty rows offer no row-level revert target"
        );
    }

    #[test]
    fn present_option_projects_the_interior_edit_entry() {
        let registry = SlotShapeRegistry::default();
        let shape = option_u32_shape();
        let data = SlotData::Option(lpc_model::SlotOptionDyn::some_with_version(
            Revision::new(1),
            SlotData::Value(WithRevision::new(Revision::new(1), LpValue::U32(255))),
        ));
        let controller = SlotController::from_slot_data(
            slot_address("brightness"),
            "Brightness".to_string(),
            &data,
            SlotShapeView::Dynamic(&shape),
            &registry,
        );
        let (buffer, overlay) = overlay_join(&[(
            "brightness.some",
            lpc_model::SlotEditOp::AssignValue(LpValue::U32(64)),
        )]);
        let join = SlotEditJoin::new(&buffer, overlay, Default::default());

        let slot = controller.ui_config_slot(&join);

        assert_eq!(
            slot.edit_entry_address,
            Some(slot_address("brightness.some")),
            "a present option's inline value edit reverts at the interior address"
        );
    }

    #[test]
    fn absent_option_projects_its_own_removal_entry() {
        let registry = SlotShapeRegistry::default();
        let shape = option_u32_shape();
        let data = SlotData::Option(lpc_model::SlotOptionDyn::none_with_version(Revision::new(
            1,
        )));
        let controller = SlotController::from_slot_data(
            slot_address("brightness"),
            "Brightness".to_string(),
            &data,
            SlotShapeView::Dynamic(&shape),
            &registry,
        );
        let (buffer, overlay) = overlay_join(&[("brightness", lpc_model::SlotEditOp::Remove)]);
        let join = SlotEditJoin::new(&buffer, overlay, Default::default());

        let slot = controller.ui_config_slot(&join);

        assert_eq!(
            slot.edit_entry_address,
            Some(slot_address("brightness")),
            "the stored base-present removal reverts at the option's own path"
        );
    }

    /// Enum shape shared by the variant-switch revert-target tests:
    /// `Unset` (unit) / `PathPoints` (f32) / `SvgPath` (string).
    fn mapping_enum_shape() -> SlotShape {
        SlotShape::Enum {
            meta: SlotMeta::empty(),
            encoding: SlotEnumEncoding::default(),
            variants: vec![
                SlotVariantShape::new(
                    "Unset",
                    SlotShape::Unit {
                        meta: SlotMeta::empty(),
                    },
                )
                .unwrap(),
                SlotVariantShape::new("PathPoints", SlotShape::value(LpType::F32)).unwrap(),
                SlotVariantShape::new("SvgPath", SlotShape::value(LpType::String)).unwrap(),
            ],
        }
    }

    fn mapping_enum_controller(active: &str, payload: SlotData) -> SlotController {
        let registry = SlotShapeRegistry::default();
        let shape = mapping_enum_shape();
        let data = SlotData::Enum(lpc_model::SlotEnum::with_version(
            Revision::new(1),
            SlotName::parse(active).unwrap(),
            payload,
        ));
        SlotController::from_slot_data(
            slot_address("mapping"),
            "Mapping".to_string(),
            &data,
            SlotShapeView::Dynamic(&shape),
            &registry,
        )
    }

    #[test]
    fn enum_row_projects_the_variant_switch_entry_as_its_revert_target() {
        // After a variant switch round-trips, the overlay entry lives at the
        // variant child path (`mapping.SvgPath`), never at the enum's own
        // path — the enum row (where the variant select sits) must still
        // offer it as its revert target.
        let controller = mapping_enum_controller(
            "SvgPath",
            SlotData::Value(WithRevision::new(
                Revision::new(1),
                LpValue::String("mask.svg".to_string()),
            )),
        );
        let (buffer, overlay) =
            overlay_join(&[("mapping.SvgPath", lpc_model::SlotEditOp::EnsurePresent)]);
        let join = SlotEditJoin::new(&buffer, overlay, Default::default());

        let slot = controller.ui_config_slot(&join);

        assert_eq!(
            slot.state.dirty,
            crate::UiNodeDirtyState::Dirty,
            "the switch entry surfaces on the enum row via the prefix join"
        );
        assert_eq!(
            slot.edit_entry_address,
            Some(slot_address("mapping.SvgPath")),
            "the enum row's revert targets the variant-switch entry"
        );
        let UiConfigSlotBody::Record(record) = &slot.body else {
            panic!("expected the enum body to project its payload row");
        };
        assert_eq!(
            record.fields[0].edit_entry_address,
            Some(slot_address("mapping.SvgPath")),
            "the payload row's exact-match revert targets the same entry"
        );
    }

    #[test]
    fn enum_row_finds_the_pending_switch_entry_before_the_view_catches_up() {
        // Ack-to-refresh window: the view's active variant still lags the
        // acked switch, so the entry is at a non-active declared variant's
        // path — the declared-variant scan must still find it.
        let controller = mapping_enum_controller(
            "PathPoints",
            SlotData::Value(WithRevision::new(Revision::new(1), LpValue::F32(0.0))),
        );
        let (buffer, overlay) =
            overlay_join(&[("mapping.SvgPath", lpc_model::SlotEditOp::EnsurePresent)]);
        let join = SlotEditJoin::new(&buffer, overlay, Default::default());

        let slot = controller.ui_config_slot(&join);

        assert_eq!(
            slot.edit_entry_address,
            Some(slot_address("mapping.SvgPath")),
            "the pending switch is revertible from the enum row during the window"
        );
    }

    #[test]
    fn enum_row_with_only_payload_edits_offers_no_revert_target() {
        // Edits strictly under the variant path are ordinary prefix-dirty:
        // the enum row must not offer a revert that would not revert them.
        let controller = mapping_enum_controller(
            "SvgPath",
            SlotData::Value(WithRevision::new(
                Revision::new(1),
                LpValue::String("mask.svg".to_string()),
            )),
        );
        let (buffer, overlay) = overlay_join(&[(
            "mapping.SvgPath.sample_diameter",
            lpc_model::SlotEditOp::AssignValue(LpValue::F32(3.0)),
        )]);
        let join = SlotEditJoin::new(&buffer, overlay, Default::default());

        let slot = controller.ui_config_slot(&join);

        assert_eq!(
            slot.state.dirty,
            crate::UiNodeDirtyState::Dirty,
            "payload edits still mark the enum row dirty via the prefix join"
        );
        assert_eq!(
            slot.edit_entry_address, None,
            "prefix-only dirty enum rows offer no row-level revert target"
        );
    }

    #[test]
    fn value_and_record_slots_project_no_composite() {
        let registry = SlotShapeRegistry::default();
        let shape = SlotShape::value(LpType::F32);
        let controller = SlotController::from_slot_data(
            slot_address("brightness"),
            "Brightness".to_string(),
            &SlotData::Value(WithRevision::new(Revision::new(1), LpValue::F32(0.5))),
            SlotShapeView::Dynamic(&shape),
            &registry,
        );

        let slot = controller.ui_config_slot(&SlotEditJoin::empty());

        assert_eq!(slot.composite, None);
    }
}
