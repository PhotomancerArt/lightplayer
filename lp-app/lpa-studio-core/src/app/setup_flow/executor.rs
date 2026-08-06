//! The thin executor: `SetupCommand` → machinery that already exists.
//!
//! Design: `docs/design/device-setup-flow.md` §8. This module decides
//! **nothing** — every decision is in the reducer, and every effect is an
//! op the studio already runs. In particular it does not touch the flash
//! implementation or the card-owned op flows: those encode the espflash /
//! S3-pty / #292 lessons, and this flow composes them.
//!
//! The mapping is a pure function so it can be asserted in tests: the
//! wiring that actually awaits these ops belongs to the controller (P06).

use lpa_link::LinkProviderKind;

use crate::app::device::{DeployOp, DeviceOp};
use crate::app::library::CatalogOp;
use crate::app::places::RegisteredDevice;
use crate::{DeviceTarget, HomeOp};

use super::command::SetupCommand;

/// What the executor needs that a command does not carry.
#[derive(Debug, Clone, PartialEq)]
pub struct SetupExecutorContext {
    /// The card key ops address this target with. `None` before a session
    /// exists — only [`SetupDispatch::RequestPort`] is dispatchable then.
    pub card_key: Option<String>,
    /// Wall-clock seconds for registry rows, injected like every other
    /// clock read in core.
    pub now: f64,
    /// The transport label recorded on a registry row at sighting.
    pub transport: String,
    /// The uid the bound session's identity resolves to RIGHT NOW — the
    /// impure fact the reducer cannot know. It is what a registry write
    /// is addressed with when the probe anchored no identity of its own:
    /// a blank board has none until the flash gives it one, and by
    /// provision time the post-flash hello has (design §3, A1).
    pub resolved_uid: Option<String>,
}

impl SetupExecutorContext {
    pub fn new(now: f64) -> Self {
        Self {
            card_key: None,
            now,
            transport: "USB".to_string(),
            resolved_uid: None,
        }
    }

    pub fn with_card_key(mut self, card_key: impl Into<String>) -> Self {
        self.card_key = Some(card_key.into());
        self
    }

    pub fn with_resolved_uid(mut self, uid: Option<String>) -> Self {
        self.resolved_uid = uid;
        self
    }

    fn target(&self) -> DeviceTarget {
        match &self.card_key {
            Some(key) => DeviceTarget::card(key.clone()),
            None => DeviceTarget::Ambient,
        }
    }
}

/// One command, resolved to the machinery that performs it.
///
/// No `PartialEq`: `CatalogOp` has none, and giving one to a whole op enum
/// so a test can use `assert_eq!` is the tail wagging the dog — the tests
/// destructure instead.
///
/// The two non-op variants name work the controller already does inline
/// and that has no op value to point at; they are dispatch instructions,
/// not new implementations.
#[derive(Debug, Clone)]
pub enum SetupDispatch {
    /// `DeviceOp::OpenProvider` on the serial provider: the browser's own
    /// port chooser, then a connect.
    Device(DeviceOp),
    /// `DeployOp::PushProject`: the M5 direct-push lane.
    Deploy(DeployOp),
    /// A catalog op run under the library lock (generate, registry write).
    Catalog(CatalogOp),
    /// A home op (kept for the card-facing gestures P06 wires up).
    Home(HomeOp),
    /// Read the live session's `DeviceSnapshot` — `detected_chip`,
    /// `probed_mac`, `recent_lines` — and classify it with
    /// [`classify_board`](super::classify_board). Escalate with
    /// `DeviceOp::ProbeBootloaderMode` ONLY when the passive read comes
    /// back `Unresponsive`: that probe reboots the device, which is why it
    /// is never automatic.
    ReadProbe,
    /// Attach the lens to this target's session
    /// (`StudioController::attach_lens`) — the F5 device home.
    AttachLens,
    /// Put the card in its Failed phase with the incomplete-flash copy
    /// (the card-owned op flow's own `CardOp::failed`, unchanged).
    MarkIncompleteFlash,
    /// The command names work that cannot be ADDRESSED on this target, so
    /// there is nothing to run. The only case today: a registry write for
    /// a board no identity anchors — neither the probe nor the live
    /// session could name it, and a row under an empty uid is worse than
    /// no row. The caller logs `reason`; the flow carries on.
    Skip { reason: &'static str },
}

/// Resolve one command. Pure: it builds op values, it does not run them.
pub fn dispatch_for(command: &SetupCommand, context: &SetupExecutorContext) -> SetupDispatch {
    match command {
        SetupCommand::RequestPort => SetupDispatch::Device(DeviceOp::OpenProvider {
            provider_id: LinkProviderKind::BrowserSerialEsp32,
        }),
        SetupCommand::ProbeBoard => SetupDispatch::ReadProbe,
        SetupCommand::ReleasePort => SetupDispatch::Device(DeviceOp::DisconnectDevice {
            target: context.target(),
        }),
        SetupCommand::Flash { board_id, .. } => {
            SetupDispatch::Device(DeviceOp::ProvisionFirmware {
                target: context.target(),
                // The name is a provision-time REGISTRY write under the
                // probed identity; there is no stamp step to ride along
                // with the flash any more.
                setup_name: None,
                board_id: Some(board_id.clone()),
            })
        }
        SetupCommand::GenerateProject { board_id } => {
            SetupDispatch::Catalog(CatalogOp::GenerateForBoard {
                board_id: board_id.clone(),
            })
        }
        SetupCommand::WriteRegistry {
            hardware_uid,
            hardware_origin,
            name,
            board_id,
        } => {
            // The SESSION's resolved uid leads: by provision time the
            // flashed board has said hello, and that identity is the one
            // the push gate will read. The probe's uid is the fallback for
            // the boards that anchored one before the flash.
            let Some(uid) = context
                .resolved_uid
                .clone()
                .or_else(|| hardware_uid.clone())
            else {
                return SetupDispatch::Skip {
                    reason: "no identity anchors this board, so there is no row to write it to",
                };
            };
            SetupDispatch::Catalog(CatalogOp::UpsertRegisteredDevice(RegisteredDevice {
                uid,
                name: name.clone(),
                transport: context.transport.clone(),
                last_seen_at: context.now,
                // Sight-only: the merge upsert keeps what was last pushed.
                association: None,
                board_id: Some(board_id.clone()),
                hardware_id: hardware_origin.clone(),
                previous_uids: Vec::new(),
            }))
        }
        SetupCommand::RecordSighting { hardware_uid } => {
            SetupDispatch::Catalog(CatalogOp::UpsertRegisteredDevice(RegisteredDevice {
                uid: hardware_uid.clone(),
                // Adopt writes nothing: the merge upsert preserves the
                // remembered name, board, and association.
                name: String::new(),
                transport: context.transport.clone(),
                last_seen_at: context.now,
                association: None,
                board_id: None,
                hardware_id: None,
                previous_uids: Vec::new(),
            }))
        }
        SetupCommand::PushProject { project_uid } => SetupDispatch::Deploy(DeployOp::PushProject {
            key: project_uid.clone(),
            target: context.target(),
        }),
        SetupCommand::OpenDeviceHome => SetupDispatch::AttachLens,
        SetupCommand::MarkIncompleteFlash => SetupDispatch::MarkIncompleteFlash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C6: &str = "seeed/xiao-esp32-c6";

    fn context() -> SetupExecutorContext {
        SetupExecutorContext::new(1_754_000_000.0).with_card_key("dev_000000029EVDlKLX")
    }

    fn device_op(command: &SetupCommand) -> DeviceOp {
        match dispatch_for(command, &context()) {
            SetupDispatch::Device(op) => op,
            other => panic!("{command:?} must dispatch a device op, got {other:?}"),
        }
    }

    fn catalog_op(command: &SetupCommand) -> CatalogOp {
        match dispatch_for(command, &context()) {
            SetupDispatch::Catalog(op) => op,
            other => panic!("{command:?} must dispatch a catalog op, got {other:?}"),
        }
    }

    fn card() -> DeviceTarget {
        DeviceTarget::card("dev_000000029EVDlKLX")
    }

    #[test]
    fn request_port_opens_the_serial_provider() {
        assert_eq!(
            device_op(&SetupCommand::RequestPort),
            DeviceOp::OpenProvider {
                provider_id: LinkProviderKind::BrowserSerialEsp32,
            }
        );
    }

    #[test]
    fn flash_runs_the_existing_provision_op_against_the_cards_target() {
        assert_eq!(
            device_op(&SetupCommand::Flash {
                board_id: C6.to_string(),
                attempt: 2,
                replug_guidance: true,
            }),
            DeviceOp::ProvisionFirmware {
                target: card(),
                setup_name: None,
                board_id: Some(C6.to_string()),
            },
            "the flash implementation is composed, never reimplemented"
        );
    }

    #[test]
    fn generate_runs_the_catalog_generator() {
        let op = catalog_op(&SetupCommand::GenerateProject {
            board_id: C6.to_string(),
        });
        let CatalogOp::GenerateForBoard { board_id } = op else {
            panic!("generation is the P03 catalog op")
        };
        assert_eq!(board_id, C6);
    }

    fn write_registry(hardware_uid: Option<&str>) -> SetupCommand {
        SetupCommand::WriteRegistry {
            hardware_uid: hardware_uid.map(str::to_string),
            hardware_origin: Some("efuse:aa:bb:cc:dd:ee:ff".to_string()),
            name: "Porch sign".to_string(),
            board_id: C6.to_string(),
        }
    }

    #[test]
    fn the_registry_write_carries_name_board_and_origin_under_the_probed_uid() {
        let op = catalog_op(&write_registry(Some("dev_000000029EVDlKLX")));
        let CatalogOp::UpsertRegisteredDevice(row) = op else {
            panic!("the registry write is a catalog op")
        };
        assert_eq!(row.uid, "dev_000000029EVDlKLX");
        assert_eq!(row.name, "Porch sign");
        assert_eq!(row.board_id.as_deref(), Some(C6));
        assert_eq!(row.hardware_id.as_deref(), Some("efuse:aa:bb:cc:dd:ee:ff"));
        assert_eq!(row.association, None, "a sighting never invents a push");
        assert_eq!(row.last_seen_at, 1_754_000_000.0);
    }

    #[test]
    fn the_registry_write_falls_back_to_the_sessions_resolved_uid() {
        // G2 blank-C6 walk: the probe of a blank board anchors nothing —
        // the identity arrives with the post-flash hello. The write is
        // addressed with THAT uid, or the typed name lands nowhere and
        // the push refuses a board with no name.
        let context = context().with_resolved_uid(Some("dev_fromthehello".to_string()));
        let SetupDispatch::Catalog(CatalogOp::UpsertRegisteredDevice(row)) =
            dispatch_for(&write_registry(None), &context)
        else {
            panic!("an un-probed uid still writes a row")
        };
        assert_eq!(row.uid, "dev_fromthehello");
        assert_eq!(row.name, "Porch sign");
    }

    #[test]
    fn the_sessions_uid_wins_over_the_probes() {
        // They are the same board, and the session's is the one the push
        // gate will read — a probe uid that somehow disagreed would write
        // the name onto a row nothing looks at.
        let context = context().with_resolved_uid(Some("dev_fromthehello".to_string()));
        let SetupDispatch::Catalog(CatalogOp::UpsertRegisteredDevice(row)) =
            dispatch_for(&write_registry(Some("dev_fromtheprobe")), &context)
        else {
            panic!("a catalog op")
        };
        assert_eq!(row.uid, "dev_fromthehello");
    }

    #[test]
    fn a_board_no_identity_anchors_writes_no_row_at_all() {
        // Neither the probe nor the session can name it: a row under an
        // empty uid is worse than no row, so the command is skipped and
        // the reason is said out loud.
        let SetupDispatch::Skip { reason } = dispatch_for(&write_registry(None), &context()) else {
            panic!("an un-anchored board has no row to write")
        };
        assert!(reason.contains("no identity anchors"), "{reason}");
    }

    #[test]
    fn adopt_records_a_sighting_that_overwrites_nothing() {
        let op = catalog_op(&SetupCommand::RecordSighting {
            hardware_uid: "dev_000000029EVDlKLX".to_string(),
        });
        let CatalogOp::UpsertRegisteredDevice(row) = op else {
            panic!("a sighting is a catalog op")
        };
        // Empty name / no board: `upsert_device_merged` preserves what the
        // row already had. Adopt must not rename a board it just met.
        assert_eq!(row.name, "");
        assert_eq!(row.board_id, None);
        assert_eq!(row.association, None);
    }

    #[test]
    fn push_uses_the_direct_push_lane() {
        let dispatch = dispatch_for(
            &SetupCommand::PushProject {
                project_uid: "prj_1".to_string(),
            },
            &context(),
        );
        let SetupDispatch::Deploy(op) = dispatch else {
            panic!("the push is a deploy op")
        };
        assert_eq!(
            op,
            DeployOp::PushProject {
                key: "prj_1".to_string(),
                target: card(),
            }
        );
    }

    #[test]
    fn the_remaining_commands_name_work_that_already_exists() {
        assert!(matches!(
            dispatch_for(&SetupCommand::ProbeBoard, &context()),
            SetupDispatch::ReadProbe
        ));
        assert!(matches!(
            dispatch_for(&SetupCommand::OpenDeviceHome, &context()),
            SetupDispatch::AttachLens
        ));
        assert!(matches!(
            dispatch_for(&SetupCommand::MarkIncompleteFlash, &context()),
            SetupDispatch::MarkIncompleteFlash
        ));
        assert_eq!(
            device_op(&SetupCommand::ReleasePort),
            DeviceOp::DisconnectDevice { target: card() }
        );
    }

    #[test]
    fn without_a_card_key_ops_fall_back_to_the_ambient_target() {
        let context = SetupExecutorContext::new(0.0);
        let SetupDispatch::Device(op) = dispatch_for(&SetupCommand::ReleasePort, &context) else {
            panic!("release is a device op")
        };
        assert_eq!(
            op,
            DeviceOp::DisconnectDevice {
                target: DeviceTarget::Ambient,
            }
        );
    }
}
