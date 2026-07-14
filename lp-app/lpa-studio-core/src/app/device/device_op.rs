use core::any::Any;
use core::time::Duration;

use lpa_link::{LinkEndpointId, LinkProviderKind};

use crate::{
    ActionClass, ActionConfirmation, ActionMeta, ActionPriority, ControllerOp, UiLogLevel,
};

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
    /// Set the connected server's process-global log level at runtime (wire
    /// `SetLogLevel`). Not persisted device-side: a reboot reverts to the
    /// logger-init default (Info).
    SetLogLevel {
        level: UiLogLevel,
    },
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
            .destructive()
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
            Self::SetLogLevel { level } => ActionMeta::new(
                format!("Set device log level: {}", level.label()),
                "Change the connected device's log verbosity until it reboots.",
                ActionPriority::Tertiary,
            ),
        }
    }

    fn action_class(&self) -> ActionClass {
        // Every device flow is a recovery-class op: it preempts an in-flight
        // passive refresh and any foreground action, and owns the connection
        // with no deadline. Mirrors the retired web policy, whose preemption
        // set was every `DeviceOp` variant (`ConnectLightPlayer` also had a
        // 12 s foreground timeout there, but recovery-class ownership of the
        // connection supersedes a deadline).
        match self {
            Self::OpenProvider { .. }
            | Self::OpenProviderForRecovery { .. }
            | Self::ConnectEndpoint { .. }
            | Self::ConnectLightPlayer
            | Self::DisconnectLightPlayer
            | Self::ResetDevice
            | Self::ProvisionFirmware
            | Self::ResetToBlank
            | Self::DisconnectDevice
            | Self::RefreshConnections => ActionClass::Recovery,
            // A quick request/ack on the existing connection — no reason to
            // preempt other foreground work or own the connection.
            Self::SetLogLevel { .. } => ActionClass::Foreground {
                deadline: Duration::from_secs(6),
            },
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

#[cfg(test)]
mod tests {
    use lpa_link::{LinkEndpointId, LinkProviderKind};

    use crate::{ActionClass, ControllerOp, DeviceOp, UiLogLevel};

    #[test]
    fn every_device_flow_op_is_recovery_class() {
        let ops = [
            DeviceOp::OpenProvider {
                provider_id: LinkProviderKind::BrowserWorker,
            },
            DeviceOp::OpenProviderForRecovery {
                provider_id: LinkProviderKind::BrowserWorker,
            },
            DeviceOp::ConnectEndpoint {
                provider_id: LinkProviderKind::BrowserWorker,
                endpoint_id: LinkEndpointId::new("endpoint"),
            },
            DeviceOp::ConnectLightPlayer,
            DeviceOp::DisconnectLightPlayer,
            DeviceOp::ResetDevice,
            DeviceOp::ProvisionFirmware,
            DeviceOp::ResetToBlank,
            DeviceOp::DisconnectDevice,
            DeviceOp::RefreshConnections,
        ];

        for op in ops {
            assert_eq!(op.action_class(), ActionClass::Recovery, "{op:?}");
        }
    }

    #[test]
    fn set_log_level_is_a_quick_foreground_op() {
        let op = DeviceOp::SetLogLevel {
            level: UiLogLevel::Debug,
        };
        assert!(matches!(op.action_class(), ActionClass::Foreground { .. }));
    }
}
