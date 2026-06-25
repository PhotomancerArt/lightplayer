//! Binding summaries for node data.

/// A human-readable binding endpoint shown in node binding popovers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiBindingEndpoint {
    /// Compact label for the endpoint, such as `bus#time.seconds`.
    pub label: String,
    /// Optional detail, usually the owning node or slot path.
    pub detail: Option<String>,
}

impl UiBindingEndpoint {
    /// Create a binding endpoint with no detail.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: None,
        }
    }

    /// Add secondary detail to the endpoint.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
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
}

impl Default for UiProducedBinding {
    fn default() -> Self {
        Self::none()
    }
}
