use core::any::Any;

use lpa_link::{LinkEndpointId, LinkProviderKind};

use crate::{ActionMeta, ActionPriority, UxOp};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkOp {
    RefreshProviders,
    ConnectServer,
    DisconnectLink,
    OpenProvider {
        provider_id: LinkProviderKind,
    },
    ConnectEndpoint {
        provider_id: LinkProviderKind,
        endpoint_id: LinkEndpointId,
    },
}

impl UxOp for LinkOp {
    fn default_action_meta(&self) -> ActionMeta {
        match self {
            Self::ConnectServer => ActionMeta::new(
                "Connect server",
                "Attach Studio to the server protocol over the open link session.",
                ActionPriority::Primary,
            ),
            Self::DisconnectLink => ActionMeta::new(
                "Disconnect link",
                "Close the current link session and return to provider selection.",
                ActionPriority::Tertiary,
            ),
            Self::RefreshProviders => ActionMeta::new(
                "Refresh providers",
                "Rebuild the provider catalog from lpa-link.",
                ActionPriority::Secondary,
            ),
            Self::OpenProvider { .. } => ActionMeta::new(
                "Open provider",
                "Open a link provider.",
                ActionPriority::Primary,
            ),
            Self::ConnectEndpoint { .. } => ActionMeta::new(
                "Open endpoint",
                "Open the selected link endpoint.",
                ActionPriority::Primary,
            ),
        }
    }

    fn clone_box(&self) -> Box<dyn UxOp> {
        Box::new(self.clone())
    }

    fn eq_op(&self, other: &dyn UxOp) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}
