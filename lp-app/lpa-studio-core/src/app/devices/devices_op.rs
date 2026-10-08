//! The devices page's ONE verb: carry an [`Action`] to the roster.
//!
//! Every device gesture in the UI is an `lpa-devices` [`Action`] and nothing
//! else. There is deliberately no per-verb op enum here: the old system's
//! `DeviceOp` grew a variant per flow and each variant grew its own state,
//! which is the disease the rebuilt model exists to cure. The op is a thin
//! envelope so the existing [`UiAction`](crate::UiAction) dispatch (node id +
//! downcast) can carry a model action, and the model's own vocabulary stays
//! the only device vocabulary.

use core::any::Any;

use lpa_devices::{Action, UpdateIntentFacts};

use crate::{ActionClass, ActionConfirmation, ActionMeta, ActionPriority, ControllerOp};

/// How the device this gesture targets is reached.
///
/// The ONLY thing it forks is meta TEXT. There is no second action, no second
/// flow and no `is_sim` in the fold: `Connect` and `Disconnect` are what
/// power a sim on and off (PD8, Q15), because a sim's link is a link and
/// opening it is opening it. What differs is what the words mean to a
/// person — nobody "connects" to a sim they just started.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DeviceFace {
    /// A board at the end of a wire.
    #[default]
    Wire,
    /// A sim: a runtime this tab started.
    Sim,
}

impl DeviceFace {
    /// The face for a device whose registry row records `transport`.
    pub fn from_transport(transport: &str) -> Self {
        match transport == super::device_records::SIM_TRANSPORT {
            true => Self::Sim,
            false => Self::Wire,
        }
    }
}

/// One device gesture, verbatim from the model's action vocabulary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DevicesOp {
    pub action: Action,
    /// What the device is, for the two verbs whose words depend on it.
    pub face: DeviceFace,
}

impl DevicesOp {
    /// The node id device actions target. Routed by `StudioController`
    /// directly — there is no controller struct behind it, only the roster.
    pub const NODE_ID: &'static str = "studio|devices";

    /// One gesture on a board at the end of a wire.
    pub fn new(action: Action) -> Self {
        Self {
            action,
            face: DeviceFace::Wire,
        }
    }

    /// One gesture on a sim.
    pub fn on_sim(action: Action) -> Self {
        Self {
            action,
            face: DeviceFace::Sim,
        }
    }

    /// The action this op carries.
    pub fn action(&self) -> &Action {
        &self.action
    }

    /// This op as a dispatchable [`UiAction`](crate::UiAction).
    pub fn action_for(action: Action) -> crate::UiAction {
        crate::UiAction::from_op(crate::ControllerId::new(Self::NODE_ID), Self::new(action))
    }

    /// The same, for a device whose transport is a sim: identical dispatch,
    /// different words on the two power verbs.
    pub fn sim_action_for(action: Action) -> crate::UiAction {
        crate::UiAction::from_op(
            crate::ControllerId::new(Self::NODE_ID),
            Self::on_sim(action),
        )
    }
}

impl ControllerOp for DevicesOp {
    fn default_action_meta(&self) -> ActionMeta {
        // The two verbs whose words are about the device, not the act.
        // Everything else reads the same on either face: a push is a push,
        // and Forget takes the same things away.
        match (&self.action, self.face) {
            (Action::Connect { .. }, DeviceFace::Sim) => {
                return ActionMeta::new(
                    "Power on",
                    "Start this sim and open it.",
                    ActionPriority::Primary,
                );
            }
            (Action::Disconnect { .. }, DeviceFace::Sim) => {
                return ActionMeta::new(
                    "Power off",
                    "Stop this sim. Studio keeps it, so you can power it on again.",
                    ActionPriority::Secondary,
                );
            }
            // Same verb, same priority, same destruction — but a sim has no
            // port grant to hand back and nothing on a board to leave
            // untouched, so the confirm says what is actually at stake
            // (D46: a sim is a thin record, and this is the record).
            (Action::Forget { .. }, DeviceFace::Sim) => {
                return ActionMeta::new(
                    "Forget",
                    "Remove this sim and the name you gave it. There is nothing else to remove.",
                    ActionPriority::Tertiary,
                )
                .lasting(ActionConfirmation::new(
                    "Forget this sim?",
                    "Its record and name go; nothing else exists.",
                    "Forget",
                ));
            }
            _ => {}
        }
        match &self.action {
            // The add slot reads "Connect a board" and then its two paths
            // (G3, 2026-09-24): the heading carries the goal, so each
            // button names only HOW — "via USB", "via Bluetooth", each
            // with its icon. "start a board here" (D44) stays the slot's
            // detour below them.
            Action::AddFromUsb => ActionMeta::new(
                "via USB",
                "Pick the USB port your LightPlayer board is plugged into.",
                ActionPriority::Primary,
            )
            .with_icon("usb")
            // `navigator.serial.requestPort()`: a real click or nothing.
            .needs_user_activation(),
            // The sibling path. Its summary says what Bluetooth cannot do
            // up front, because a user who adds a piece over Bluetooth and
            // then looks for "Update firmware" is owed the reason before,
            // not after.
            Action::AddFromBle => ActionMeta::new(
                "via Bluetooth",
                "Pick your LightPlayer piece from the browser's Bluetooth list. \
                 Play and edit work over Bluetooth; firmware updates need USB.",
                ActionPriority::Secondary,
            )
            .with_icon("bluetooth")
            // `navigator.bluetooth.requestDevice()`: a real click or nothing.
            .needs_user_activation(),
            Action::AdoptLink { .. } => ActionMeta::new(
                "Set up this device",
                "Remember this board so it can be set up.",
                ActionPriority::Primary,
            ),
            Action::DismissLink { .. } => ActionMeta::new(
                "Dismiss",
                "Stop looking at this port and hand the grant back.",
                ActionPriority::Tertiary,
            )
            .lasting(ActionConfirmation::new(
                "Dismiss this port?",
                "Studio hands the browser's permission for this port back. \
                     You can pick it again from the add-device card.",
                "Dismiss",
            )),
            Action::Connect { .. } => ActionMeta::new(
                "Connect",
                "Open the port and ask the board what it is.",
                ActionPriority::Primary,
            ),
            Action::Reconnect { .. } => ActionMeta::new(
                "Reconnect…",
                "Pick this board's port again. Some boards can't be \
                 re-recognized after a replug, so the browser asks once more.",
                ActionPriority::Primary,
            )
            // The browser's chooser again: a real click or nothing.
            .needs_user_activation(),
            Action::Disconnect { .. } => ActionMeta::new(
                "Disconnect",
                "Close the port. The board keeps running; Studio stops watching it.",
                ActionPriority::Secondary,
            ),
            Action::Forget { .. } => ActionMeta::new(
                "Forget",
                "Remove this device, what Studio remembers about it, and the \
                 browser's permission for its port.",
                ActionPriority::Tertiary,
            )
            .lasting(ActionConfirmation::new(
                "Forget this device?",
                "Studio removes the device, its remembered name, and the \
                     browser's permission for its port. Nothing on the board changes.",
                "Forget",
            )),
            Action::CancelActivity { .. } => ActionMeta::new(
                "Cancel",
                "Stop what Studio is doing to this device.",
                ActionPriority::Secondary,
            ),
            // The layout question's yes (the C6 repartition): the backup is
            // already stored, and the board is rewritten now. The user's
            // decision, never an assistant's (D7: Lasting, so the agent
            // hands it over). The layout sheet that asks the question draws
            // it as one press — the sheet is the asking (G1 walk,
            // 2026-10-03) — and only the sheet: the level stays Lasting.
            Action::ConfirmFlashLayout { .. } => ActionMeta::new(
                "Continue",
                "Write the new firmware and move this board's files to it.",
                ActionPriority::Primary,
            )
            .lasting(ActionConfirmation::new(
                "Rewrite this board now?",
                "The board gets the new firmware and its files move to the new layout. \
                 The backup stays in this browser.",
                "continue",
            )),
            Action::Identify { .. } => ActionMeta::new(
                "Identify again",
                "Ask the board what it is, right now.",
                ActionPriority::Secondary,
            ),
            // Whatever the chip ran before is gone: the user's call, even
            // when an assistant proposes it (D7: Lasting, so the button arms
            // and the agent hands it over).
            Action::Flash { .. } => ActionMeta::new(
                "Flash firmware",
                "Write LightPlayer firmware for the picked board onto this chip.",
                ActionPriority::Primary,
            )
            .lasting(ActionConfirmation::new(
                "Replace what this board runs?",
                "The firmware on this chip is replaced, and anything stored with \
                 it may go too. Your library copies are untouched.",
                "flash",
            )),
            // No confirmation: the empty face's picker IS the deliberate
            // gesture, and a board with nothing on it has nothing to lose.
            // (Pushing OVER a project is M4's banking question, not this
            // face's.)
            Action::Push { .. } => ActionMeta::new(
                "Put it on the board",
                "Send the picked project to this board and start it running.",
                ActionPriority::Primary,
            ),
            // An over-the-air update keeps a copy of what it replaces and
            // rolls back by itself, and the board keeps its files: nothing is
            // lost, so it is Routine and the agent may press it (N9). An
            // older version is the one exception — it may not read the
            // board's project — so choosing one is Lasting. The card's
            // offers (`device_update_offers`) set each verb's own label.
            Action::Update {
                intent:
                    UpdateIntentFacts::Install {
                        version,
                        allow_downgrade: true,
                    },
                ..
            } => ActionMeta::new(
                "Install",
                "Put an older version on this board over its own link.",
                ActionPriority::Primary,
            )
            .lasting(ActionConfirmation::new(
                "Install an older version?",
                format!(
                    "{version} is older than what this board runs, and an older version \
                     may not read the board's project."
                ),
                "install",
            )),
            Action::Update {
                intent: UpdateIntentFacts::Reinstall,
                ..
            } => ActionMeta::new(
                "Reinstall",
                "Write this board's own firmware again, from this Studio, its cache or the \
                 release store.",
                ActionPriority::Primary,
            ),
            Action::Update { .. } => ActionMeta::new(
                "Update",
                "Update this board's firmware over its own link. Studio keeps a copy of \
                 what it replaces, and a failed update rolls back by itself.",
                ActionPriority::Primary,
            ),
            Action::ResetBoard { .. } => ActionMeta::new(
                "Reset",
                "Restart the board and see what starts up: a hardware reset over a cable, \
                 a restart request over Bluetooth or Wi‑Fi.",
                ActionPriority::Secondary,
            ),
            // No confirmation: it takes nothing away. The worst case is a
            // crash the ledger records again — which is what the second
            // sentence promises rather than hides.
            Action::ClearFaults { .. } => ActionMeta::new(
                "Clear faults",
                "Forget the crash ledger and retry the quarantined nodes. \
                 If the fault recurs the card degrades again.",
                ActionPriority::Secondary,
            ),
            Action::Erase { .. } => ActionMeta::new(
                "Factory reset",
                "Erase the firmware and everything stored on this board.",
                ActionPriority::Tertiary,
            )
            .lasting(ActionConfirmation::new(
                "Factory reset this board?",
                "Everything on its flash is erased — firmware, projects, settings. \
                     Its identity lives in silicon and survives; Studio keeps the entry.",
                "reset",
            )),
            // Destructive on the BOARD and nowhere else, which is exactly
            // what the confirm has to say: the library copy is a different
            // object and this does not touch it.
            // "Remove" alone read as nothing in particular beside a running
            // picture (G1 2026-09-06): the verb names its subject, and the
            // armed reading is "Confirm remove".
            Action::RemoveProject { .. } => ActionMeta::new(
                "Remove project",
                "Stop what this board is running and delete it from the board.",
                ActionPriority::Tertiary,
            )
            .lasting(ActionConfirmation::new(
                "Remove the project from this board?",
                "The board stops running it and the project is deleted from the \
                     board's storage. The firmware stays, and your copy in the \
                     library is untouched.",
                "remove",
            )),
            Action::SetName { .. } => ActionMeta::new(
                "Rename",
                "Change what Studio calls this device.",
                ActionPriority::Secondary,
            ),
            Action::SetAutoconnect { .. } => ActionMeta::new(
                "Connect automatically",
                "Open this device's port whenever it appears.",
                ActionPriority::Tertiary,
            ),
        }
    }

    /// Every device gesture is [`ActionClass::Recovery`].
    ///
    /// Not laziness: a device action owns a port for its duration and its
    /// whole point is to be reachable while something else is stuck — that is
    /// the class the retired `DeviceOp` variants all carried, and it is what
    /// makes Forget work mid-activity from the queue's point of view too. The
    /// model bounds the work itself (deadlines are its timers), so no
    /// quiet-gap budget belongs here.
    fn action_class(&self) -> ActionClass {
        ActionClass::Recovery
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
    use super::*;
    use lpa_devices::DeviceId;

    #[test]
    fn every_action_renders_a_label_and_owns_the_connection() {
        let device = DeviceId(1);
        let link = lpa_devices::LinkId(1);
        for action in [
            Action::AddFromUsb,
            Action::AdoptLink { link },
            Action::DismissLink { link },
            Action::Connect { device },
            Action::Disconnect { device },
            Action::Forget { device },
            Action::CancelActivity { device },
            Action::Identify { device },
            Action::Flash {
                device,
                board_id: "seeed-xiao-esp32c6".to_string(),
                build_id: "esp32c6-4mb".to_string(),
                park_first: false,
                name: None,
                restore_backup: false,
            },
            Action::SetName {
                device,
                name: "Kitchen".to_string(),
            },
            Action::SetAutoconnect {
                device,
                enabled: true,
            },
            Action::ResetBoard { device },
            Action::ClearFaults { device },
        ] {
            let op = DevicesOp::new(action.clone());
            assert!(
                !op.default_action_meta().label.is_empty(),
                "{action:?} renders nothing"
            );
            assert_eq!(op.action_class(), ActionClass::Recovery, "{action:?}");
        }
    }

    /// The verbs only the user's own click may press (PD5, D7): the
    /// browser's pickers need user activation, and whatever is gone for
    /// good — a flash, a forget, a wipe — is Lasting. Everything else an
    /// assistant may press for them.
    #[test]
    fn the_pickers_and_the_flash_need_the_users_own_click() {
        let device = DeviceId(1);
        let flash = Action::Flash {
            device,
            board_id: "seeed/xiao-esp32-c6".to_string(),
            build_id: "esp32c6-4mb".to_string(),
            park_first: false,
            name: None,
            restore_backup: false,
        };
        // (action, needs a real click, lasting)
        for (action, activation, lasting) in [
            (Action::AddFromUsb, true, false),
            (Action::AddFromBle, true, false),
            (Action::Reconnect { device }, true, false),
            (flash, false, true),
            (Action::Forget { device }, false, true),
            (Action::Erase { device }, false, true),
            (Action::RemoveProject { device }, false, true),
            (Action::Connect { device }, false, false),
            (Action::Push { device }, false, false),
            (Action::Disconnect { device }, false, false),
        ] {
            let meta = DevicesOp::new(action.clone()).default_action_meta();
            assert_eq!(meta.needs_user_activation, activation, "{action:?}");
            assert_eq!(meta.consequence.arms(), lasting, "{action:?}");
            assert_eq!(meta.needs_user(), activation || lasting, "{action:?}");
            if activation {
                assert!(
                    meta.consequence.is_routine(),
                    "a picker is a platform fact, not a consequence: {action:?}"
                );
            }
        }
    }

    /// The one fork, and its bounds: a sim is powered on and off, and
    /// every other verb reads exactly the same on either face. There is no
    /// second action behind the words — the model still sees `Connect` and
    /// `Disconnect`, which is what keeps a sim from being a fifth flow.
    #[test]
    fn a_sim_is_powered_on_and_off_and_nothing_else_changes() {
        let device = DeviceId(1);

        let on = DevicesOp::on_sim(Action::Connect { device }).default_action_meta();
        assert_eq!(on.label, "Power on");
        assert_eq!(on.priority, ActionPriority::Primary);

        let off = DevicesOp::on_sim(Action::Disconnect { device }).default_action_meta();
        assert_eq!(off.label, "Power off");
        assert!(
            off.summary.contains("power it on again"),
            "powering off must not read as losing the device: {}",
            off.summary
        );

        for action in [
            Action::Forget { device },
            Action::Identify { device },
            Action::ResetBoard { device },
            Action::Erase { device },
            Action::RemoveProject { device },
        ] {
            assert_eq!(
                DevicesOp::on_sim(action.clone())
                    .default_action_meta()
                    .label,
                DevicesOp::new(action.clone()).default_action_meta().label,
                "{action:?} is the same verb on either face"
            );
        }
    }

    /// G3: under the slot's "Connect a board" heading, each transport's
    /// verb names only its path; USB stays the Primary.
    #[test]
    fn the_add_slots_verbs_name_their_path() {
        let meta = DevicesOp::new(Action::AddFromUsb).default_action_meta();
        let ble = DevicesOp::new(Action::AddFromBle).default_action_meta();

        assert_eq!(meta.label, "via USB");
        assert_eq!(ble.label, "via Bluetooth");
        assert_eq!(meta.priority, ActionPriority::Primary);
        assert!(
            meta.summary.contains("USB port"),
            "the transport specifics stay in the summary: {}",
            meta.summary
        );
    }

    /// D46: forgetting a sim takes a record and a name, and the confirm
    /// says exactly that — no port grant handed back, nothing on a board
    /// left untouched, because there is no board.
    #[test]
    fn forgetting_a_sim_promises_only_what_a_sim_has() {
        let meta = DevicesOp::on_sim(Action::Forget {
            device: DeviceId(1),
        })
        .default_action_meta();
        let confirmation = meta
            .consequence
            .copy()
            .cloned()
            .expect("forget is lasting and says what goes");

        assert!(meta.consequence.wears_error_tint());
        assert_eq!(confirmation.title, "Forget this sim?");
        assert_eq!(
            confirmation.message,
            "Its record and name go; nothing else exists."
        );
        let wire = DevicesOp::new(Action::Forget {
            device: DeviceId(1),
        })
        .default_action_meta()
        .consequence
        .copy()
        .cloned()
        .expect("forget is lasting and says what goes");
        assert!(
            wire.message.contains("permission for its port"),
            "a board's confirm still names the grant: {}",
            wire.message
        );
    }

    /// The face is read off the registry column, so the words and the row
    /// cannot drift apart.
    #[test]
    fn the_face_comes_from_the_transport_column() {
        assert_eq!(DeviceFace::from_transport("sim"), DeviceFace::Sim);
        assert_eq!(DeviceFace::from_transport("USB"), DeviceFace::Wire);
        assert_eq!(
            DeviceFace::from_transport(""),
            DeviceFace::Wire,
            "a row that predates transport recording is not a sim"
        );
    }

    /// Clear faults asks nothing and threatens nothing: it takes no
    /// project, no firmware and no record away, and the worst case — the
    /// fault comes straight back — is what its own description promises.
    /// A confirm here would train people to click through the ones that
    /// matter.
    #[test]
    fn clearing_faults_is_reversible_and_asks_nothing() {
        let meta = DevicesOp::new(Action::ClearFaults {
            device: DeviceId(1),
        })
        .default_action_meta();
        assert_eq!(meta.label, "Clear faults");
        assert!(meta.consequence.is_routine());
        assert_eq!(meta.priority, ActionPriority::Secondary);
        assert!(
            meta.summary.contains("degrades again"),
            "the honest outcome is part of the offer: {}",
            meta.summary
        );
    }

    /// The irreversible ones arm first. Forget is reachable everywhere by
    /// model design, so the armed second click is the only thing standing
    /// between a stuck card and a deleted record.
    #[test]
    fn the_lasting_gestures_carry_their_copy() {
        for action in [
            Action::Forget {
                device: DeviceId(1),
            },
            Action::DismissLink {
                link: lpa_devices::LinkId(1),
            },
        ] {
            let meta = DevicesOp::new(action.clone()).default_action_meta();
            assert!(meta.consequence.arms(), "{action:?}");
            assert!(meta.consequence.copy().is_some(), "{action:?}");
        }
    }
}
