use core::any::Any;

use lpa_link::{LinkEndpointId, LinkProviderKind};

use crate::{ActionConfirmation, ActionMeta, ActionPriority, ControllerOp};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviceOp {
    OpenProvider {
        provider_id: LinkProviderKind,
    },
    OpenProviderForRecovery {
        provider_id: LinkProviderKind,
    },
    ConnectEndpoint {
        provider_id: LinkProviderKind,
        endpoint_id: LinkEndpointId,
    },
    ConnectLightPlayer,
    DisconnectLightPlayer,
    ResetDevice,
    ProvisionFirmware,
    ResetToBlank,
    DisconnectDevice,
    RefreshConnections,
}

impl ControllerOp for DeviceOp {
    fn default_action_meta(&self) -> ActionMeta {
        match self {
            Self::OpenProvider { .. } => ActionMeta::new(
                "Choose connection",
                "Select this way to connect a LightPlayer device.",
                ActionPriority::Primary,
            ),
            Self::OpenProviderForRecovery { .. } => ActionMeta::new(
                "Open for flashing",
                "Open the ESP32 connection without attaching LightPlayer.",
                ActionPriority::Secondary,
            ),
            Self::ConnectEndpoint { .. } => ActionMeta::new(
                "Connect device",
                "Open this device endpoint.",
                ActionPriority::Primary,
            ),
            Self::ConnectLightPlayer => ActionMeta::new(
                "Connect LightPlayer",
                "Attach Studio to LightPlayer on the connected device.",
                ActionPriority::Primary,
            ),
            Self::DisconnectLightPlayer => ActionMeta::new(
                "Disconnect",
                "Detach Studio from LightPlayer while keeping the device connected.",
                ActionPriority::Tertiary,
            ),
            Self::ResetDevice => ActionMeta::new(
                "Reset device",
                "Reboot the connected device without erasing firmware or data.",
                ActionPriority::Tertiary,
            ),
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
            Self::DisconnectDevice => ActionMeta::new(
                "Disconnect",
                "Close the current device session and return to connection choices.",
                ActionPriority::Tertiary,
            ),
            Self::RefreshConnections => ActionMeta::new(
                "Refresh connections",
                "Rebuild the connection catalog from available providers.",
                ActionPriority::Secondary,
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
