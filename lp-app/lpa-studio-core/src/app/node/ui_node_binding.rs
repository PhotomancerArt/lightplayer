//! Binding summaries for node data.

use crate::{UiSlotAffordance, UiSlotAspect, UiSlotAspectKind, UiSlotAspectRow};

/// A human-readable binding endpoint shown in node binding popovers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiBindingEndpoint {
    /// Compact label for the endpoint, such as `bus:time`.
    pub label: String,
    /// Optional detail, usually the owning node or slot path.
    pub detail: Option<String>,
    /// True when this wiring came from the slot's declarative `default_bind`
    /// rather than an authored binding (ADR 2026-07-09). Indicators wear a
    /// DEF badge and popovers explain the origin.
    pub default_origin: bool,
}

impl UiBindingEndpoint {
    /// Create a binding endpoint with no detail.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: None,
            default_origin: false,
        }
    }

    /// Add secondary detail to the endpoint.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Mark this endpoint as wired by declarative default policy.
    pub fn with_default_origin(mut self) -> Self {
        self.default_origin = true;
        self
    }
}

/// Binding state for a produced product or value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiProducedBindings {
    /// Optional bus target published by this output.
    pub bus_target: Option<UiBindingEndpoint>,
    /// Explicit target bindings authored from this output.
    pub target_bindings: Vec<UiBindingEndpoint>,
    /// Read-only consumers discovered from the project graph.
    pub consumers: Vec<UiBindingEndpoint>,
}

impl UiProducedBindings {
    /// Create an empty produced-binding summary.
    pub fn none() -> Self {
        Self {
            bus_target: None,
            target_bindings: Vec::new(),
            consumers: Vec::new(),
        }
    }

    /// Returns true when there is any route worth surfacing in the UI.
    pub fn has_any(&self) -> bool {
        self.bus_target.is_some() || !self.target_bindings.is_empty() || !self.consumers.is_empty()
    }
}

impl Default for UiProducedBindings {
    fn default() -> Self {
        Self::none()
    }
}

/// How a produced item participates in the graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiProducedBinding {
    /// Binding state associated with the produced item.
    pub bindings: UiProducedBindings,
    /// Optional slot revision for debugging freshness.
    pub revision: Option<String>,
}

impl UiProducedBinding {
    /// Create binding metadata with no routes.
    pub fn none() -> Self {
        Self {
            bindings: UiProducedBindings::none(),
            revision: None,
        }
    }

    /// Build the shared detail aspect for produced output routing.
    pub fn output_aspect(&self) -> UiSlotAspect {
        let mut aspect = UiSlotAspect::new(UiSlotAspectKind::Binding, "Output");

        if let Some(bus_target) = self.bindings.bus_target.as_ref() {
            aspect = aspect.with_row(endpoint_row("Published", bus_target));
        }
        for target in &self.bindings.target_bindings {
            aspect = aspect.with_row(endpoint_row("Bound to", target));
        }
        for consumer in &self.bindings.consumers {
            aspect = aspect.with_row(endpoint_row("Consumed by", consumer));
        }

        if let Some(revision) = self.revision.as_ref() {
            aspect = aspect.with_row(UiSlotAspectRow::new("Revision", revision.clone()));
        }

        if self.bindings.has_any() {
            aspect.with_affordance(UiSlotAffordance::Bound)
        } else if aspect.rows.is_empty() {
            aspect.with_row(UiSlotAspectRow::new("Unbound", ""))
        } else {
            aspect
        }
    }
}

impl Default for UiProducedBinding {
    fn default() -> Self {
        Self::none()
    }
}

fn endpoint_row(label: &'static str, endpoint: &UiBindingEndpoint) -> UiSlotAspectRow {
    let mut row = UiSlotAspectRow::new(label, endpoint.label.clone());
    if let Some(detail) = endpoint.detail.as_ref() {
        row = row.with_detail(detail.clone());
    }
    if endpoint.default_origin {
        row = row.with_detail("default binding");
    }
    row
}
