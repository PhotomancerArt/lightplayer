use crate::LinkCapabilities;
use crate::providers::LinkProviderKind;

/// Static metadata describing a provider kind available to an application.
///
/// Descriptors are intentionally lower-level than Studio provider cards. They
/// describe what `lpa-link` can construct and what operations the provider can
/// perform; product layers can add UX intent, ordering, copy, and recovery
/// affordances on top. The registry only constructs providers compiled for the
/// current feature and target matrix, so every descriptor it returns is usable
/// in the current build/runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkProviderDescriptor {
    /// Built-in provider kind and stable app-boundary key.
    pub kind: LinkProviderKind,
    /// Short technical label supplied by `lpa-link`.
    pub label: &'static str,
    /// Low-level operations supported by the provider class.
    pub capabilities: LinkCapabilities,
}

impl LinkProviderDescriptor {
    pub fn new(
        kind: LinkProviderKind,
        label: &'static str,
        capabilities: LinkCapabilities,
    ) -> Self {
        Self {
            kind,
            label,
            capabilities,
        }
    }
}
