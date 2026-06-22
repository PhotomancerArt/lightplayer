use crate::{ActionKind, UxCommand};
use lpa_link::{LinkEndpointId, LinkProviderKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkAction {
    RefreshProviders,
    OpenProvider {
        provider_id: LinkProviderKind,
    },
    ConnectEndpoint {
        provider_id: LinkProviderKind,
        endpoint_id: LinkEndpointId,
    },
}

impl LinkAction {
    pub const REFRESH_PROVIDERS: ActionKind = ActionKind::new("link", "refresh-providers");
    pub const OPEN_PROVIDER: ActionKind = ActionKind::new("link", "open-provider");
    pub const CONNECT_ENDPOINT: ActionKind = ActionKind::new("link", "connect-endpoint");
}

impl UxCommand for LinkAction {
    fn action_kind(&self) -> ActionKind {
        match self {
            Self::RefreshProviders => Self::REFRESH_PROVIDERS,
            Self::OpenProvider { .. } => Self::OPEN_PROVIDER,
            Self::ConnectEndpoint { .. } => Self::CONNECT_ENDPOINT,
        }
    }
}
