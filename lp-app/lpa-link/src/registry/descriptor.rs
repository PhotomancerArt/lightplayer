use crate::LinkCapabilities;
use crate::providers::{LinkProviderAvailability, LinkProviderKind};

/// Static metadata describing a provider kind available to an application.
///
/// Descriptors are intentionally lower-level than Studio provider cards. They
/// describe what `lpa-link` can construct and what operations the provider can
/// perform; product layers can add UX intent, ordering, copy, and recovery
/// affordances on top.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkProviderDescriptor {
    /// Built-in provider kind and stable app-boundary key.
    pub kind: LinkProviderKind,
    /// Short technical label supplied by `lpa-link`.
    pub label: &'static str,
    /// Whether the provider is usable in the current build/runtime.
    pub availability: LinkProviderAvailability,
    /// Low-level operations supported by the provider class.
    pub capabilities: LinkCapabilities,
}

impl LinkProviderDescriptor {
    pub fn available(
        kind: LinkProviderKind,
        label: &'static str,
        capabilities: LinkCapabilities,
    ) -> Self {
        Self {
            kind,
            label,
            availability: LinkProviderAvailability::Available,
            capabilities,
        }
    }
}
