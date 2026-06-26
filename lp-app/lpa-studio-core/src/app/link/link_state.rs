use crate::{ConnectedDeviceSummary, EndpointChoice, ProgressState, ProviderChoice, UiIssue};
use lpa_link::LinkProviderKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkState {
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
    Managing {
        device: ConnectedDeviceSummary,
        progress: ProgressState,
    },
    Connected {
        device: ConnectedDeviceSummary,
    },
    Failed {
        issue: UiIssue,
    },
}
