use crate::{ActionKind, UxCommand};
use lpa_link::{LinkEndpointId, LinkProviderKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkAction {
    RefreshProviders,
    SelectProvider {
        provider_id: LinkProviderKind,
    },
    ConnectEndpoint {
        provider_id: LinkProviderKind,
        endpoint_id: LinkEndpointId,
    },
}

impl LinkAction {
    pub const REFRESH_PROVIDERS: ActionKind = ActionKind::new("link", "refresh-providers");
    pub const SELECT_PROVIDER: ActionKind = ActionKind::new("link", "select-provider");
    pub const CONNECT_ENDPOINT: ActionKind = ActionKind::new("link", "connect-endpoint");
}

impl UxCommand for LinkAction {
    fn action_kind(&self) -> ActionKind {
        match self {
            Self::RefreshProviders => Self::REFRESH_PROVIDERS,
            Self::SelectProvider { .. } => Self::SELECT_PROVIDER,
            Self::ConnectEndpoint { .. } => Self::CONNECT_ENDPOINT,
        }
    }
}
