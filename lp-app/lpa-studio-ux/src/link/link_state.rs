use crate::{ConnectedDeviceSummary, EndpointChoice, ProgressState, ProviderChoice, UxIssue};
use lpa_link::LinkProviderKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkState {
    SelectingProvider {
        providers: Vec<ProviderChoice>,
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
        issue: UxIssue,
    },
}
