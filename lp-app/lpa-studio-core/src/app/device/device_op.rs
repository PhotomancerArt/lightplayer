use super::bootloader_entry_flow::BootloaderEntryFlow;
use super::device_target::DeviceTarget;
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
    /// One-click reconnect (M1): connect through an already-granted browser
    /// serial port with no chooser; the chooser only appears when no grant
    /// exists yet. `uid` names the remembered device the user clicked, so
    /// the connect window renders on THAT card (no transient twin) —
    /// identity read at connect stays the truth once it lands.
    ReconnectDevice {
        uid: Option<String>,
    },
    /// D32 auto-connect (M6): the load-time / hotplug attach sweep.
    /// Connect a granted port when one exists — attach + pull + show,
    /// nothing else (invariant 4: no push, no load, no editor touch).
    /// Never prompts a chooser, never toasts; failures land on card
    /// evidence (the ladder / In-use-elsewhere). Idempotent: a live
    /// device session or a busy connect flow makes it a no-op.
    AutoConnect,
    /// Attach Studio's protocol to the runtime behind `target`.
    ConnectLightPlayer {
        target: DeviceTarget,
    },
    /// Detach Studio's protocol from `target`, keeping the runtime.
    DisconnectLightPlayer {
        target: DeviceTarget,
    },
    ResetDevice {
        target: DeviceTarget,
    },
    /// Flash the packaged firmware. `setup_name` rides along from the
    /// blank board's SETUP FORM (state-flow model §1-A): after the flash
    /// lands and the wire is up, the controller stamps this name so the
    /// happy path never detours through Needs-a-name. `None` = a plain
    /// flash / firmware update (already-stamped or recovery contexts).
    ///
    /// `board_id` is the setup form's board choice (`vendor/product`,
    /// board-selection D4): after the flash lands and the wire is up, the
    /// controller writes the board's runtime manifest to the device's
    /// `/hardware.json` so the NEXT boot runs the right pin map. `None` =
    /// generic board — nothing written, the compiled-in default stands.
    /// Recovery flashes always pass `None` for both fields.
    ProvisionFirmware {
        target: DeviceTarget,
        setup_name: Option<String>,
        board_id: Option<String>,
    },
    /// Wipe the device's project storage back to blank (the
    /// Holds-unreadable-data card's way out — state-flow model rev
    /// 2026-07-26: the way out is BLANK, never push-over). Firmware
    /// stays; the board lands on Connected-empty.
    WipeProject {
        target: DeviceTarget,
    },
    ResetToBlank {
        target: DeviceTarget,
    },
    /// Write the boot-control record so the device's NEXT restart comes up
    /// without loading a project (`lp-bootctl`).
    ///
    /// The escape for a project that stops its own device from running —
    /// too bright, too power-hungry, or hanging the watchdog. Nothing is
    /// erased and the instruction is one-shot: the device consumes it as it
    /// boots, so the restart after that is normal again. That is why this is
    /// NOT destructive and does not sever a lens, unlike `ResetToBlank`.
    BootSafeOnce {
        target: DeviceTarget,
    },
    /// Read the device's filesystem partition over the bootloader and hand
    /// the user a ZIP of it.
    ///
    /// The rescue that has to happen BEFORE anything destructive: it works
    /// on a board that cannot boot, because the bytes come off through the
    /// ROM/stub bootloader rather than through the running server. Nothing
    /// is written, so this is not destructive — but it does own the wire and
    /// reboot the device, like every other management operation.
    BackUpFilesystem {
        target: DeviceTarget,
    },
    /// Ask the device whether a bootloader is listening, and fold the answer
    /// into the card's open bootloader-entry sheet.
    ///
    /// This is what makes the ritual's confirmation real. The passive
    /// classifier CANNOT answer it: a board already in download mode printed
    /// its ROM banner before Studio ever attached, so silence is the normal
    /// case rather than evidence. Only the SYNC handshake can say.
    ///
    /// User-triggered, never automatic — the handshake reboots the device.
    /// The user pressing "I've done that" IS the edge signal that a replug
    /// happened, which is exactly when probing is worth its cost.
    ProbeBootloaderMode {
        /// The card whose bootloader-entry sheet this answers — both the
        /// device to probe AND the sheet to fold the answer into, which
        /// is why it stays a card key rather than becoming a bare target.
        card_key: String,
        flow: BootloaderEntryFlow,
    },
    /// Disconnect ONE device session: close its link (the board keeps
    /// running; reconnecting adds it back) and remove it from the pool.
    DisconnectDevice {
        target: DeviceTarget,
    },
    /// Destroy THE simulator session (runtime-pool P3, Q5): quiesce the
    /// editor when the lens is on the sim, close the provider session
    /// (`worker.terminate()` on the web), remove it from the pool. The
    /// device session (and everything else) stays. Surfaced on the sim
    /// card's danger zone once the pool-fed roster lands (P4).
    StopSimulator,
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
            Self::ReconnectDevice { .. } => ActionMeta::new(
                "Reconnect",
                "Reconnect to a previously connected device.",
                ActionPriority::Primary,
            ),
            Self::AutoConnect => ActionMeta::new(
                "Auto-connect",
                "Connect a granted device automatically (attach only).",
                ActionPriority::Secondary,
            ),
            Self::ConnectLightPlayer { .. } => ActionMeta::new(
                "Connect LightPlayer",
                "Attach Studio to LightPlayer on the connected device.",
                ActionPriority::Primary,
            ),
            Self::DisconnectLightPlayer { .. } => ActionMeta::new(
                "Disconnect",
                "Detach Studio from LightPlayer while keeping the device connected.",
                ActionPriority::Tertiary,
            ),
            Self::ResetDevice { .. } => ActionMeta::new(
                "Reset device",
                "Reboot the connected device without erasing firmware or data.",
                ActionPriority::Tertiary,
            ),
            Self::ProvisionFirmware { .. } => ActionMeta::new(
                "Flash firmware",
                "Flash the packaged LightPlayer firmware onto this ESP32.",
                ActionPriority::Primary,
            )
            .with_confirmation(ActionConfirmation::new(
                "Flash firmware",
                "This will write LightPlayer firmware to the selected ESP32. Continue?",
                "Flash firmware",
            )),
            Self::BootSafeOnce { .. } => ActionMeta::new(
                "Start in safe mode",
                "Have this device start once in safe mode — dim, or with \
                 nothing loaded on older firmware — so a project that stops \
                 it from running can be fixed.",
                ActionPriority::Secondary,
            ),
            Self::BackUpFilesystem { .. } => ActionMeta::new(
                "Download a backup",
                "Copy everything on this device to a ZIP on your computer — \
                 works even if the board will not start.",
                ActionPriority::Secondary,
            ),
            Self::ProbeBootloaderMode { .. } => ActionMeta::new(
                "Check the device",
                "Ask the device whether it is listening in recovery mode.",
                ActionPriority::Primary,
            ),
            Self::WipeProject { .. } => ActionMeta::new(
                "Wipe project",
                "Delete the device's project storage; firmware stays.",
                ActionPriority::Tertiary,
            )
            .destructive()
            .with_confirmation(ActionConfirmation::new(
                "Wipe the project",
                // This used to say the content "can't be backed up". Since
                // M6 that is false: a raw filesystem backup does not need
                // Studio to understand the content, only to read the bytes.
                // Pointing at the way out is the honest gate.
                "Studio can't read this content, and wiping deletes it for \
                 good. Download a backup first if you might want it — that \
                 works even on content Studio can't open.",
                "Wipe",
            )),
            Self::ResetToBlank { .. } => ActionMeta::new(
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
            Self::DisconnectDevice { .. } => ActionMeta::new(
                "Disconnect",
                "Close this board's session. The board keeps running; connecting it again adds it back.",
                ActionPriority::Tertiary,
            ),
            Self::StopSimulator => ActionMeta::new(
                "Stop simulator",
                "Shut the simulator down; unsaved editor changes are discarded.",
                ActionPriority::Tertiary,
            )
            .destructive(),
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
            | Self::ReconnectDevice { .. }
            | Self::AutoConnect
            | Self::ConnectLightPlayer { .. }
            | Self::DisconnectLightPlayer { .. }
            | Self::ResetDevice { .. }
            | Self::ProvisionFirmware { .. }
            | Self::WipeProject { .. }
            | Self::ResetToBlank { .. }
            | Self::BootSafeOnce { .. }
            | Self::BackUpFilesystem { .. }
            | Self::ProbeBootloaderMode { .. }
            | Self::DisconnectDevice { .. }
            | Self::StopSimulator
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

    use crate::{ActionClass, ControllerOp, DeviceOp, DeviceTarget, UiLogLevel};

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
            DeviceOp::ReconnectDevice { uid: None },
            DeviceOp::ConnectLightPlayer {
                target: DeviceTarget::Ambient,
            },
            DeviceOp::DisconnectLightPlayer {
                target: DeviceTarget::card("runtime-1"),
            },
            DeviceOp::ResetDevice {
                target: DeviceTarget::card("runtime-1"),
            },
            DeviceOp::ProvisionFirmware {
                target: DeviceTarget::card("runtime-1"),
                setup_name: None,
                board_id: None,
            },
            DeviceOp::ResetToBlank {
                target: DeviceTarget::card("runtime-1"),
            },
            DeviceOp::BackUpFilesystem {
                target: DeviceTarget::card("runtime-1"),
            },
            DeviceOp::DisconnectDevice {
                target: DeviceTarget::card("runtime-1"),
            },
            DeviceOp::StopSimulator,
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
