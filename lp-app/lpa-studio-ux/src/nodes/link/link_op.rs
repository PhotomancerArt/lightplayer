use core::any::Any;

use lpa_link::{LinkEndpointId, LinkProviderKind};

use crate::{ActionConfirmation, ActionMeta, ActionPriority, UxOp};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkOp {
    RefreshProviders,
    ConnectServer,
    ProvisionFirmware,
    ResetToBlank,
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
            Self::ProvisionFirmware => ActionMeta::new(
                "Provision firmware",
                "Flash the packaged LightPlayer firmware onto this ESP32.",
                ActionPriority::Primary,
            )
            .with_confirmation(ActionConfirmation::new(
                "Provision firmware",
                "This will write LightPlayer firmware to the selected ESP32. Continue?",
                "Provision firmware",
            )),
            Self::ResetToBlank => ActionMeta::new(
                "Reset to blank",
                "Erase this ESP32 so it is no longer provisioned.",
                ActionPriority::Tertiary,
            )
            .with_confirmation(ActionConfirmation::new(
                "Reset device to blank",
                "This erases firmware and device data from the selected ESP32.",
                "Erase device",
            )),
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
