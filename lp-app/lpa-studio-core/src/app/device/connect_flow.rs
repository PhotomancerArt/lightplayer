//! The connect-flow view state: where the picker/open sequence stands.
//!
//! [`DeviceController`] drives this alongside its [`RuntimeAttachment`]:
//! the flow narrates the catalog → discovery → endpoint → connect sequence
//! for the views (gallery issue chip, deploy dialog endpoint choices),
//! while the attachment holds what actually got connected. `Connected` is
//! entered exactly when the attachment becomes non-`None`.
//!
//! There is deliberately no `Managing` variant: management runs inside
//! the hardware `DeviceSession` and never leaves the flow's `Connected`.
//!
//! [`DeviceController`]: super::DeviceController
//! [`RuntimeAttachment`]: super::RuntimeAttachment

use crate::{ConnectedDeviceSummary, EndpointChoice, ProgressState, ProviderChoice, UiIssue};
use lpa_link::LinkProviderKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConnectFlowState {
    SelectingProvider {
        providers: Vec<ProviderChoice>,
        issue: Option<UiIssue>,
    },
    DiscoveringEndpoints {
        provider_id: LinkProviderKind,
        progress: ProgressState,
    },
    SelectingEndpoint {
        provider_id: LinkProviderKind,
        endpoints: Vec<EndpointChoice>,
    },
    Connecting {
        endpoint: EndpointChoice,
        progress: ProgressState,
    },
    Connected {
        device: ConnectedDeviceSummary,
    },
    Failed {
        issue: UiIssue,
    },
}
