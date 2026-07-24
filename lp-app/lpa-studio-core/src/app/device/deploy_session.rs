//! The deploy dialog's state machine (M5): one dialog, many entry states.
//!
//! Pure state + transitions — no IO. The studio controller derives the
//! entry state from the live environment, executes effects (flash,
//! stamp, push) through the existing seams, and applies these
//! transitions; the web edge renders [`DeployState`] as the modal.
//!
//! Rules encoded here (roadmap D8/D11/D14):
//! - Push is the ONLY mutating confirmation, and it is never a default:
//!   `Reviewing` always requires an explicit `ConfirmPush`.
//! - The novice wizard is a sequence of entry-state derivations, not a
//!   step counter: flash → (reconnect, re-pull) → stamp → choose → push.
//! - A failure returns to the step that failed (`resume`), never to the
//!   start of the wizard.
//! - `Done` keeps the dialog open with the target preselected — the
//!   provision-N-boards loop is swap cable → reconnect → re-derive.

use lpc_history::ContentHash;

use crate::app::places::{DeviceContent, DeviceIdentity, DeviceSyncState};

/// What a push would deploy: a library project's head at review time.
#[derive(Clone, Debug, PartialEq)]
pub struct DeployTarget {
    pub project_uid: String,
    pub slug: String,
    pub head: ContentHash,
    /// 1-based version number of `head` on its line, when known.
    pub version_number: Option<usize>,
}

/// The environment the entry state derives from — a snapshot of what the
/// studio controller knows when the dialog opens or a step completes.
#[derive(Clone, Debug, Default)]
pub struct DeployEnvironment {
    /// A hardware link is open (the sim never counts — D22).
    pub device_link_connected: bool,
    /// The server protocol answered (firmware is running).
    pub firmware_available: bool,
    /// Connect-as-pull result, when the protocol attached.
    pub device_sync: Option<DeviceSyncState>,
}

/// The dialog's current step.
#[derive(Clone, Debug, PartialEq)]
pub enum DeployState {
    /// No hardware link: the dialog's connect entry.
    NeedsDevice,
    /// Link up, no firmware answered: wizard step 1.
    Blank { flashed_once: bool },
    /// Link + firmware up but the connect-time pull hasn't landed yet —
    /// a transient "checking the device" step, never a claim about
    /// firmware (that bug shipped once: an unready wire rendered as
    /// "no firmware yet" on a perfectly healthy board).
    Inspecting,
    /// Firmware up, no stamped identity: wizard step 2. The name input
    /// starts from `suggested_name` (may be empty; Continue is disabled
    /// until non-empty — gently insist, D14).
    NeedsIdentity { suggested_name: String },
    /// Identity known, nothing chosen to push yet: wizard step 3.
    ChoosingPackage { device: DeviceIdentity },
    /// Reviewing a concrete push: what's on the device now vs the target.
    Reviewing {
        device: DeviceIdentity,
        target: DeployTarget,
        on_device: DeviceContent,
    },
    /// Firmware flash in flight (progress rides the activity stream).
    Flashing,
    /// Writing `/.lp/device.json` + registry entry.
    Stamping { name: String },
    /// Replace + verify in flight.
    Pushing {
        device: DeviceIdentity,
        target: DeployTarget,
    },
    /// Terminal happy state; the dialog stays open for the next board.
    Done {
        device: DeviceIdentity,
        pushed: DeployTarget,
    },
    /// A step failed; `Retry` returns to `resume`.
    Failed {
        message: String,
        resume: Box<DeployState>,
    },
}

/// One open dialog. Holds the chosen target across reconnects so the
/// N-boards loop re-enters review with the same package.
#[derive(Clone, Debug, PartialEq)]
pub struct DeploySession {
    pub state: DeployState,
    /// The chosen/preselected push target, surviving state re-derivation.
    pub target: Option<DeployTarget>,
}

/// An op arrived in a state it does not apply to (stale click, double
/// dispatch). Surfaced as a friendly refusal, never a crash.
#[derive(Debug, Clone, PartialEq)]
pub struct InvalidTransition {
    pub op: &'static str,
    pub state: &'static str,
}

impl core::fmt::Display for InvalidTransition {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} does not apply while {}", self.op, self.state)
    }
}

impl DeploySession {
    /// Open the dialog: derive the entry state from the environment and
    /// an optional preselected target.
    pub fn open(env: &DeployEnvironment, target: Option<DeployTarget>) -> Self {
        let mut session = Self {
            state: DeployState::NeedsDevice,
            target,
        };
        session.rederive(env);
        session
    }

    /// Re-derive the step from the environment. Called at open, after a
    /// flash reconnect, after stamping, after a push, and when the link
    /// or pull state changes underneath the dialog. In-flight and
    /// terminal-failure states are sticky — effects finish (or fail)
    /// before the environment speaks again.
    pub fn rederive(&mut self, env: &DeployEnvironment) {
        if matches!(
            self.state,
            DeployState::Flashing
                | DeployState::Stamping { .. }
                | DeployState::Pushing { .. }
                | DeployState::Failed { .. }
        ) {
            return;
        }
        let flashed_once = matches!(self.state, DeployState::Blank { flashed_once: true });
        self.state = derive_state(env, self.target.clone(), flashed_once);
    }

    /// The user chose a package (wizard step 3 or the picker in review).
    pub fn choose_target(
        &mut self,
        env: &DeployEnvironment,
        target: DeployTarget,
    ) -> Result<(), InvalidTransition> {
        match &self.state {
            DeployState::ChoosingPackage { .. } | DeployState::Reviewing { .. } => {
                self.target = Some(target);
                self.rederive(env);
                Ok(())
            }
            state => Err(invalid("ChoosePackage", state)),
        }
    }

    pub fn begin_flash(&mut self) -> Result<(), InvalidTransition> {
        match &self.state {
            // flashing is reachable from the wizard's blank step and, as a
            // separate firmware op, from any settled connected step
            DeployState::Blank { .. }
            | DeployState::NeedsIdentity { .. }
            | DeployState::ChoosingPackage { .. }
            | DeployState::Reviewing { .. }
            | DeployState::Done { .. } => {
                self.state = DeployState::Flashing;
                Ok(())
            }
            state => Err(invalid("FlashFirmware", state)),
        }
    }

    /// Flash finished (success or not); the caller reconnected and
    /// re-pulled, so the environment decides what comes next. A device
    /// still not answering after a flash stays `Blank` with
    /// `flashed_once` set (the UI can escalate its message).
    pub fn flash_finished(&mut self, env: &DeployEnvironment, flash_ok: bool) {
        if !matches!(self.state, DeployState::Flashing) {
            return;
        }
        if !env.firmware_available {
            self.state = DeployState::Blank { flashed_once: true };
            return;
        }
        if !flash_ok {
            // firmware answers but the flash reported trouble — surface it
            self.state = DeployState::Blank { flashed_once: true };
            return;
        }
        self.state = derive_state(env, self.target.clone(), true);
    }

    pub fn begin_stamp(&mut self, name: String) -> Result<(), InvalidTransition> {
        match &self.state {
            DeployState::NeedsIdentity { .. } if !name.trim().is_empty() => {
                self.state = DeployState::Stamping {
                    name: name.trim().to_string(),
                };
                Ok(())
            }
            state @ DeployState::NeedsIdentity { .. } => {
                Err(invalid("StampIdentity(empty)", state))
            }
            state => Err(invalid("StampIdentity", state)),
        }
    }

    /// Stamping finished; the caller re-pulled (identity now present, and
    /// adoption may have run), so the environment decides the next step.
    pub fn stamp_finished(&mut self, env: &DeployEnvironment) {
        if matches!(self.state, DeployState::Stamping { .. }) {
            self.state = derive_state(env, self.target.clone(), false);
        }
    }

    pub fn begin_push(&mut self) -> Result<(DeviceIdentity, DeployTarget), InvalidTransition> {
        match &self.state {
            DeployState::Reviewing { device, target, .. } => {
                let device = device.clone();
                let target = target.clone();
                self.state = DeployState::Pushing {
                    device: device.clone(),
                    target: target.clone(),
                };
                Ok((device, target))
            }
            state => Err(invalid("ConfirmPush", state)),
        }
    }

    pub fn push_finished(&mut self) {
        if let DeployState::Pushing { device, target } = &self.state {
            self.state = DeployState::Done {
                device: device.clone(),
                pushed: target.clone(),
            };
        }
    }

    /// Any in-flight effect failed: park on `Failed` pointing back at the
    /// step to retry (the state we were in when the effect started).
    pub fn fail(&mut self, message: impl Into<String>, resume: DeployState) {
        self.state = DeployState::Failed {
            message: message.into(),
            resume: Box::new(resume),
        };
    }

    /// Return to the failed step.
    pub fn retry(&mut self) -> Result<(), InvalidTransition> {
        match core::mem::replace(&mut self.state, DeployState::NeedsDevice) {
            DeployState::Failed { resume, .. } => {
                self.state = *resume;
                Ok(())
            }
            state => {
                let error = invalid("Retry", &state);
                self.state = state;
                Err(error)
            }
        }
    }

    /// Whether closing now would abandon an in-flight effect (the UI
    /// keeps the dialog up or minimizes it instead of closing).
    pub fn close_blocked(&self) -> bool {
        matches!(
            self.state,
            DeployState::Flashing | DeployState::Stamping { .. } | DeployState::Pushing { .. }
        )
    }
}

/// Entry/settled-state derivation — the wizard IS this function (D14).
fn derive_state(
    env: &DeployEnvironment,
    target: Option<DeployTarget>,
    flashed_once: bool,
) -> DeployState {
    if !env.device_link_connected {
        return DeployState::NeedsDevice;
    }
    if !env.firmware_available {
        return DeployState::Blank { flashed_once };
    }
    let Some(sync) = &env.device_sync else {
        // firmware answered but the pull hasn't landed — say "checking",
        // never "no firmware"; the controller re-derives when it lands
        return DeployState::Inspecting;
    };
    let Some(identity) = &sync.identity else {
        return DeployState::NeedsIdentity {
            suggested_name: String::new(),
        };
    };
    match target {
        Some(target) => DeployState::Reviewing {
            device: identity.clone(),
            target,
            on_device: sync.content.clone(),
        },
        None => DeployState::ChoosingPackage {
            device: identity.clone(),
        },
    }
}

fn invalid(op: &'static str, state: &DeployState) -> InvalidTransition {
    InvalidTransition {
        op,
        state: state_name(state),
    }
}

fn state_name(state: &DeployState) -> &'static str {
    match state {
        DeployState::NeedsDevice => "waiting for a device",
        DeployState::Blank { .. } => "waiting for firmware",
        DeployState::Inspecting => "checking the device",
        DeployState::NeedsIdentity { .. } => "naming the device",
        DeployState::ChoosingPackage { .. } => "choosing a project",
        DeployState::Reviewing { .. } => "reviewing a push",
        DeployState::Flashing => "flashing",
        DeployState::Stamping { .. } => "stamping identity",
        DeployState::Pushing { .. } => "pushing",
        DeployState::Done { .. } => "done",
        DeployState::Failed { .. } => "showing a failure",
    }
}

#[cfg(test)]
mod tests {
    use lpc_history::SyncRelation;

    use super::*;

    fn identity() -> DeviceIdentity {
        DeviceIdentity {
            uid: "dev_aaaaaaaaaaaaaaaa".to_string(),
            name: "Bench board".to_string(),
        }
    }

    fn target(n: usize) -> DeployTarget {
        DeployTarget {
            project_uid: "prj_aaaaaaaaaaaaaaaa".to_string(),
            slug: "2026-07-10-1000-porch".to_string(),
            head: ContentHash::of(format!("v{n}").as_bytes()),
            version_number: Some(n),
        }
    }

    fn env(link: bool, firmware: bool, sync: Option<DeviceSyncState>) -> DeployEnvironment {
        DeployEnvironment {
            device_link_connected: link,
            firmware_available: firmware,
            device_sync: sync,
        }
    }

    fn synced(identity: Option<DeviceIdentity>, content: DeviceContent) -> Option<DeviceSyncState> {
        Some(DeviceSyncState { identity, content })
    }

    #[test]
    fn wizard_walks_blank_to_done() {
        // no link → NeedsDevice
        let mut session = DeploySession::open(&env(false, false, None), None);
        assert_eq!(session.state, DeployState::NeedsDevice);

        // link, no firmware → Blank; flash
        session.rederive(&env(true, false, None));
        assert_eq!(
            session.state,
            DeployState::Blank {
                flashed_once: false
            }
        );
        session.begin_flash().unwrap();
        assert_eq!(session.state, DeployState::Flashing);

        // reconnected with firmware, no identity → NeedsIdentity
        let after_flash = env(true, true, synced(None, DeviceContent::Empty));
        session.flash_finished(&after_flash, true);
        assert!(matches!(session.state, DeployState::NeedsIdentity { .. }));

        // gently insist: empty names refuse
        assert!(session.begin_stamp("   ".to_string()).is_err());
        session
            .begin_stamp("Luna's porch sign".to_string())
            .unwrap();
        assert!(matches!(session.state, DeployState::Stamping { .. }));

        // stamped + re-pulled → ChoosingPackage
        let stamped = env(true, true, synced(Some(identity()), DeviceContent::Empty));
        session.stamp_finished(&stamped);
        assert!(matches!(session.state, DeployState::ChoosingPackage { .. }));

        // choose → Reviewing; ConfirmPush is the only way forward
        session.choose_target(&stamped, target(1)).unwrap();
        assert!(matches!(session.state, DeployState::Reviewing { .. }));
        let (device, chosen) = session.begin_push().unwrap();
        assert_eq!(device.name, "Bench board");
        assert_eq!(chosen, target(1));
        session.push_finished();
        assert!(matches!(session.state, DeployState::Done { .. }));
    }

    #[test]
    fn firmware_without_pull_result_says_checking_not_blank() {
        // regression: an unready wire must never render as "no firmware"
        let session = DeploySession::open(&env(true, true, None), None);
        assert_eq!(session.state, DeployState::Inspecting);
        // and it is NOT sticky: the pull landing re-derives forward
        let mut session = session;
        session.rederive(&env(
            true,
            true,
            synced(Some(identity()), DeviceContent::Empty),
        ));
        assert!(matches!(session.state, DeployState::ChoosingPackage { .. }));
    }

    #[test]
    fn behind_review_carries_the_relation_and_never_auto_pushes() {
        let sync = synced(
            Some(identity()),
            DeviceContent::Known {
                project_uid: "prj_aaaaaaaaaaaaaaaa".to_string(),
                slug: "2026-07-10-1000-porch".to_string(),
                observed: ContentHash::of(b"v1"),
                relation: SyncRelation::Behind,
            },
        );
        let environment = env(true, true, sync);
        let session = DeploySession::open(&environment, Some(target(2)));
        let DeployState::Reviewing { on_device, .. } = &session.state else {
            panic!("entry with a target reviews, got {:?}", session.state);
        };
        assert!(matches!(
            on_device,
            DeviceContent::Known {
                relation: SyncRelation::Behind,
                ..
            }
        ));
        // no transition fires without an explicit ConfirmPush — there is
        // simply no other path out of Reviewing besides ops
    }

    #[test]
    fn pre_targeted_open_reviews_and_a_different_choice_stays_reachable() {
        // The papercut fix (2026-07-23): a dialog opened with no explicit
        // target while the device runs a KNOWN project gets that project
        // resolved as the target by the controller — entry must land on
        // Reviewing, not the picker.
        let running = synced(
            Some(identity()),
            DeviceContent::Known {
                project_uid: "prj_aaaaaaaaaaaaaaaa".to_string(),
                slug: "2026-07-10-1000-porch".to_string(),
                observed: ContentHash::of(b"v1"),
                relation: SyncRelation::AtHead,
            },
        );
        let environment = env(true, true, running);
        let mut session = DeploySession::open(&environment, Some(target(1)));
        assert!(
            matches!(&session.state, DeployState::Reviewing { target, .. } if *target == self::target(1)),
            "a pre-targeted open reviews the running project, got {:?}",
            session.state
        );

        // Choosing a DIFFERENT project stays reachable from Reviewing —
        // the default never removes the choice.
        session.choose_target(&environment, target(2)).unwrap();
        assert!(
            matches!(&session.state, DeployState::Reviewing { target, .. } if *target == self::target(2)),
            "Reviewing re-derives onto the newly chosen target, got {:?}",
            session.state
        );
    }

    #[test]
    fn failure_resumes_the_failed_step_not_the_wizard_start() {
        let stamped = env(true, true, synced(Some(identity()), DeviceContent::Empty));
        let mut session = DeploySession::open(&stamped, Some(target(3)));
        let resume = session.state.clone();
        session.begin_push().unwrap();
        session.fail("serial hiccup", resume.clone());
        assert!(matches!(session.state, DeployState::Failed { .. }));
        session.retry().unwrap();
        assert_eq!(session.state, resume, "retry returns to Reviewing");
    }

    #[test]
    fn flash_that_leaves_no_firmware_escalates_blank() {
        let mut session = DeploySession::open(&env(true, false, None), None);
        session.begin_flash().unwrap();
        session.flash_finished(&env(true, false, None), false);
        assert_eq!(session.state, DeployState::Blank { flashed_once: true });
        // in-flight/failed states are sticky against rederive
        session.begin_flash().unwrap();
        session.rederive(&env(
            true,
            true,
            synced(Some(identity()), DeviceContent::Empty),
        ));
        assert_eq!(
            session.state,
            DeployState::Flashing,
            "rederive never interrupts"
        );
    }

    #[test]
    fn done_reenters_review_for_the_next_board() {
        let stamped = env(true, true, synced(Some(identity()), DeviceContent::Empty));
        let mut session = DeploySession::open(&stamped, Some(target(1)));
        session.begin_push().unwrap();
        session.push_finished();
        assert!(matches!(session.state, DeployState::Done { .. }));

        // swap cable: link drops, new board appears blank
        session.rederive(&env(true, false, None));
        assert_eq!(
            session.state,
            DeployState::Blank {
                flashed_once: false
            }
        );
        assert_eq!(session.target, Some(target(1)), "target survives the swap");

        // the new board comes up stamped → straight back to Reviewing
        let next = env(
            true,
            true,
            synced(
                Some(DeviceIdentity {
                    uid: "dev_bbbbbbbbbbbbbbbb".to_string(),
                    name: "Second board".to_string(),
                }),
                DeviceContent::Empty,
            ),
        );
        session.rederive(&next);
        assert!(matches!(session.state, DeployState::Reviewing { .. }));
    }

    #[test]
    fn ops_in_wrong_states_refuse_without_corrupting() {
        let mut session = DeploySession::open(&env(false, false, None), None);
        assert!(session.begin_push().is_err());
        assert!(session.begin_stamp("x".to_string()).is_err());
        assert!(session.retry().is_err());
        assert_eq!(session.state, DeployState::NeedsDevice);
        assert!(!session.close_blocked());
        session.state = DeployState::Flashing;
        assert!(session.close_blocked());
    }
}
