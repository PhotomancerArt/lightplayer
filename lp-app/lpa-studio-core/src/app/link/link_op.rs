use core::any::Any;

use lpa_link::{LinkEndpointId, LinkProviderKind};

use crate::{ActionConfirmation, ActionMeta, ActionPriority, ControllerOp};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkOp {
    RefreshProviders,
    ConnectServer,
    ResetDevice,
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

impl ControllerOp for LinkOp {
    fn default_action_meta(&self) -> ActionMeta {
        match self {
            Self::ProvisionFirmware => ActionMeta::new(
                "Flash firmware",
                "Flash the packaged LightPlayer firmware onto this ESP32.",
                ActionPriority::Primary,
            )
            .with_confirmation(ActionConfirmation::new(
                "Flash firmware",
                "This will write LightPlayer firmware to the selected ESP32. Continue?",
                "Flash firmware",
            )),
            Self::ResetToBlank => ActionMeta::new(
                "Wipe device",
                "Erase firmware and device data from this ESP32.",
                ActionPriority::Tertiary,
            )
            .with_confirmation(ActionConfirmation::new(
                "Wipe device",
                "This erases firmware and device data from the selected ESP32.",
                "Wipe device",
            )),
            Self::ResetDevice => ActionMeta::new(
                "Reset device",
                "Reboot the connected device without erasing firmware or data.",
                ActionPriority::Tertiary,
            ),
            Self::ConnectServer => ActionMeta::new(
                "Connect server",
                "Attach Studio to the server protocol over the open link session.",
                ActionPriority::Primary,
            ),
            Self::DisconnectLink => ActionMeta::new(
                "Disconnect",
                "Close the current device session and return to connection choices.",
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

    fn clone_box(&self) -> Box<dyn ControllerOp> {
        Box::new(self.clone())
    }

    fn eq_op(&self, other: &dyn ControllerOp) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}
