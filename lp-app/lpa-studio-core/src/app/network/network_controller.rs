//! The network controller: reads each board's Wi‑Fi status once per
//! connection, runs the Wi‑Fi offers' changes, and holds what the popover
//! reads.
//!
//! It owns no IO, the way the access controller does not: it reads the
//! device model's evidence, spawns each conversation on the link's SHARED
//! wire (`DeviceEffects::conversation_io`), and every conversation ends by
//! posting a [`NetworkCommand`] back onto the actor's queue (invariant I7).
//! When the editor lens holds a link's wire, the step is parked for the
//! actor, which runs it through the lens's own client
//! ([`NetworkController::take_lens_step`]).
//!
//! **When it asks** (plan Q7): once per connection window on a link that
//! holds edit — USB, or Bluetooth unlocked at author — never on a play or
//! locked link; again when the popover opens ([`NetworkCommand::Refresh`]);
//! and every change's answer replaces the status. It scans when the
//! connect page asks ([`NetworkCommand::Scan`]), and never on a board whose
//! station is `unsupported`. No polling: nothing changes on a board that
//! cannot connect (M6 adds a poll while a test runs).
//!
//! **The in-row test** (the spike's 2B): a network added through the offer
//! becomes the device's `testing` network once the board has saved it, and
//! its row shows the test ([`super::UiWifiTest`]) until
//! [`NetworkCommand::DismissTest`].
//!
//! **It never stores the password** (plan Q8): a change's password lives
//! in its [`NetworkStep`] until the request leaves, and no state here holds
//! one.

use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

use lpa_devices::identity::DeviceId;
use lpa_devices::link::LinkId;
use lpa_devices::{Device, Roster};
use lpc_access::Tier;
use lpc_wire::server::{HeardNetwork, NetworkScan, NetworkStatus, StationState};

use super::device_network_ops::{NetworkRefusal, NetworkStep, run_network_step};
use super::network_command::{NetworkCommand, NetworkStepKind};
use super::network_op::{NetworkChange, NetworkOp};
use super::ui_device_wifi::UiDeviceWifi;
use crate::app::access::LoginWindow;
use crate::app::access::access_controller::{holds_its_files, is_bluetooth, login_window};
use crate::app::devices::device_effects::{DeviceEffects, DeviceTaskFuture};
use crate::app::studio::studio_command::StudioCommand;
use crate::app::studio::studio_view_channel::CommandSender;

/// How far a device's link reaches into its network settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WifiReach {
    /// Bluetooth unlocked for play: the row says it needs author; nothing
    /// is asked.
    PlayOnly,
    /// USB, or Bluetooth unlocked at author: read, change, forget.
    Edit,
}

/// Whether a link holds edit, from how it is reached and what it was
/// granted: a trusted (USB) link always does; a Bluetooth link only once
/// unlocked at author. `None`: the link shows no Wi‑Fi row at all (a
/// Bluetooth link nothing has unlocked).
pub fn wifi_reach_for(over_bluetooth: bool, granted: Option<Tier>) -> Option<WifiReach> {
    match (over_bluetooth, granted) {
        (false, _) => Some(WifiReach::Edit),
        (true, Some(Tier::Edit)) => Some(WifiReach::Edit),
        (true, Some(Tier::Play)) => Some(WifiReach::PlayOnly),
        (true, None) => None,
    }
}

/// See the module doc.
#[derive(Default)]
pub struct NetworkController {
    devices: BTreeMap<DeviceId, DeviceNetwork>,
    /// Steps parked for the lens's client, at most one per device.
    lens_steps: VecDeque<(DeviceId, NetworkStep)>,
    tx: Option<CommandSender>,
    spawner: Option<Rc<dyn Fn(DeviceTaskFuture)>>,
}

/// What Studio knows about one device's network settings.
#[derive(Clone, Debug, Default)]
struct DeviceNetwork {
    status: Option<NetworkStatus>,
    /// The connection window the status was last asked for on.
    asked_on: Option<LoginWindow>,
    reading: bool,
    writing: bool,
    error: Option<String>,
    /// The board refused for want of author: the controls are withdrawn
    /// until an answer comes back.
    needs_author: bool,
    /// What the radio heard at the last scan.
    heard: Option<Vec<HeardNetwork>>,
    scanning: bool,
    /// The network an add in flight is saving.
    adding: Option<String>,
    /// The network whose test shows in its row.
    testing: Option<String>,
}

impl NetworkController {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn set_command_sender(&mut self, tx: CommandSender) {
        self.tx = Some(tx);
    }

    pub(crate) fn set_spawner(&mut self, spawner: Rc<dyn Fn(DeviceTaskFuture)>) {
        self.spawner = Some(spawner);
    }

    // --- the drive --------------------------------------------------------

    /// Read the status of every device whose link holds edit and has not
    /// been asked on this connection. `granted` is the tier a Bluetooth
    /// link holds (the access controller's).
    pub fn drive(
        &mut self,
        roster: &Roster,
        effects: &DeviceEffects,
        granted: impl Fn(DeviceId) -> Option<Tier>,
    ) {
        for device in roster.devices() {
            let state = self.devices.get(&device.id).cloned().unwrap_or_default();
            let Some(window) = read_due(reach(device, granted(device.id)), &state) else {
                continue;
            };
            if self.dispatch(device.id, window.link, NetworkStep::Read, effects) {
                let state = self.devices.entry(device.id).or_default();
                state.asked_on = Some(window);
                state.reading = true;
            }
        }
        let live: Vec<DeviceId> = roster.devices().iter().map(|device| device.id).collect();
        self.devices.retain(|device, _| live.contains(device));
        self.lens_steps.retain(|(device, _)| live.contains(device));
    }

    /// Start `step` on `link`, or park it for the lens. False when it could
    /// not start now.
    fn dispatch(
        &mut self,
        device: DeviceId,
        link: LinkId,
        step: NetworkStep,
        effects: &DeviceEffects,
    ) -> bool {
        if effects.lens_holds_wire(link) {
            if self.lens_steps.iter().any(|(parked, _)| *parked == device) {
                return false;
            }
            self.lens_steps.push_back((device, step));
            return true;
        }
        if effects.wire_borrowed(link) {
            // An activity has the wire; a read tries again on the next drive.
            return false;
        }
        let (Some(io), Some(spawner), Some(tx)) = (
            effects.conversation_io(link),
            self.spawner.clone(),
            self.tx.clone(),
        ) else {
            return false;
        };
        spawner(Box::pin(async move {
            let mut client = io.into_client();
            let result = run_network_step(&mut client, device, step).await;
            tx.send(StudioCommand::Network(result));
        }));
        true
    }

    /// The next step parked for the lens's client, if any.
    pub(crate) fn take_lens_step(&mut self) -> Option<(DeviceId, NetworkStep)> {
        self.lens_steps.pop_front()
    }

    // --- commands and changes ---------------------------------------------

    /// Apply one command: a finished conversation, or a refresh.
    pub fn apply(
        &mut self,
        command: NetworkCommand,
        roster: &Roster,
        effects: &DeviceEffects,
        granted: impl Fn(DeviceId) -> Option<Tier>,
    ) {
        match command {
            NetworkCommand::Refresh { device } => {
                let Some(found) = roster.device(device) else {
                    return;
                };
                let Some((window, WifiReach::Edit)) = reach(found, granted(device)) else {
                    return;
                };
                let state = self.devices.entry(device).or_default();
                if state.reading || state.writing {
                    return;
                }
                if self.dispatch(device, window.link, NetworkStep::Read, effects) {
                    let state = self.devices.entry(device).or_default();
                    state.asked_on = Some(window);
                    state.reading = true;
                }
            }
            NetworkCommand::Scan { device } => {
                let Some(found) = roster.device(device) else {
                    return;
                };
                let Some((window, WifiReach::Edit)) = reach(found, granted(device)) else {
                    return;
                };
                let state = self.devices.entry(device).or_default();
                let can_scan = state
                    .status
                    .as_ref()
                    .is_some_and(|status| status.station != StationState::Unsupported);
                if !can_scan || state.scanning || state.reading || state.writing {
                    return;
                }
                if self.dispatch(device, window.link, NetworkStep::Scan, effects) {
                    self.devices.entry(device).or_default().scanning = true;
                }
            }
            NetworkCommand::DismissTest { device } => {
                if let Some(state) = self.devices.get_mut(&device) {
                    state.testing = None;
                }
            }
            NetworkCommand::Scanned { device, result } => {
                let state = self.devices.entry(device).or_default();
                state.scanning = false;
                match result {
                    Ok(NetworkScan::Heard(heard)) => state.heard = Some(heard),
                    Ok(NetworkScan::Unsupported) => state.heard = None,
                    Err(NetworkRefusal::NotPermitted(_)) => state.needs_author = true,
                    Err(NetworkRefusal::Said(error)) => state.error = Some(error),
                }
            }
            NetworkCommand::Answered {
                device,
                kind,
                result,
            } => {
                let state = self.devices.entry(device).or_default();
                let added = match kind {
                    NetworkStepKind::Read => {
                        state.reading = false;
                        None
                    }
                    NetworkStepKind::Write => {
                        state.writing = false;
                        state.adding.take()
                    }
                };
                match result {
                    Ok(status) => {
                        if let Some(added) = added {
                            // Saved: its test runs in its row (2B).
                            state.testing = Some(added);
                        }
                        state.status = Some(status);
                        state.error = None;
                        state.needs_author = false;
                    }
                    Err(NetworkRefusal::NotPermitted(_)) => {
                        // The row says it needs author instead of an error.
                        state.needs_author = true;
                        state.error = None;
                    }
                    Err(NetworkRefusal::Said(error)) => {
                        // A refused change keeps the status the board last
                        // answered: the refusal is said beside it.
                        state.error = Some(error);
                    }
                }
            }
        }
    }

    /// Run one Wi‑Fi offer's change on `op.device`'s link, or say why it
    /// cannot run (the sentence also lands in the popover).
    pub fn start_change(
        &mut self,
        op: NetworkOp,
        roster: &Roster,
        effects: &DeviceEffects,
        granted: impl Fn(DeviceId) -> Option<Tier>,
    ) -> Result<(), String> {
        let device = op.device;
        let result = self.try_start(op, roster, effects, granted);
        let state = self.devices.entry(device).or_default();
        match &result {
            Ok(adding) => {
                state.writing = true;
                state.error = None;
                state.adding = adding.clone();
            }
            Err(error) => state.error = Some(error.clone()),
        }
        result.map(|_| ())
    }

    /// Start the change; `Ok` names the network an add is saving.
    fn try_start(
        &mut self,
        op: NetworkOp,
        roster: &Roster,
        effects: &DeviceEffects,
        granted: impl Fn(DeviceId) -> Option<Tier>,
    ) -> Result<Option<String>, String> {
        let device = op.device;
        let found = roster
            .device(device)
            .ok_or_else(|| "this device is gone".to_string())?;
        let (window, reach) =
            reach(found, granted(device)).ok_or_else(|| "connect this device first".to_string())?;
        if reach != WifiReach::Edit {
            return Err(crate::app::access::not_permitted_sentence(Tier::Edit).to_string());
        }
        if self.devices.get(&device).is_some_and(|state| state.writing) {
            return Err("a Wi‑Fi change is already on its way to this board".to_string());
        }
        if !effects.lens_holds_wire(window.link) && effects.wire_borrowed(window.link) {
            return Err(
                "this device is busy (another job has it) — try again in a moment".to_string(),
            );
        }
        let (step, adding) = match op.change {
            NetworkChange::Add {
                ssid,
                password,
                hidden,
            } => (
                NetworkStep::Add {
                    ssid: ssid.clone(),
                    password,
                    hidden,
                },
                Some(ssid),
            ),
            NetworkChange::Forget { ssid } => (NetworkStep::Forget { ssid }, None),
            NetworkChange::Switches { wifi, cloud_relay } => {
                (NetworkStep::Switches { wifi, cloud_relay }, None)
            }
        };
        if self.dispatch(device, window.link, step, effects) {
            Ok(adding)
        } else {
            Err("this device cannot be changed from here right now".to_string())
        }
    }

    // --- views ------------------------------------------------------------

    /// The Wi‑Fi facts for one card, when its link shows a Wi‑Fi row.
    pub fn device_view(&self, device: &Device, granted: Option<Tier>) -> Option<UiDeviceWifi> {
        let (_, reach) = reach(device, granted)?;
        let state = self.devices.get(&device.id);
        Some(UiDeviceWifi {
            device: device.id,
            can_edit: reach == WifiReach::Edit && !state.is_some_and(|state| state.needs_author),
            status: state.and_then(|state| state.status.clone()),
            reading: state.is_some_and(|state| state.reading),
            writing: state.is_some_and(|state| state.writing),
            error: state.and_then(|state| state.error.clone()),
            heard: state.and_then(|state| state.heard.clone()),
            scanning: state.is_some_and(|state| state.scanning),
            testing: state.and_then(|state| state.testing.clone()),
        })
    }
}

/// The window to read the status on now, when one is due: the link holds
/// edit, nothing is in flight, and this connection has not been asked. A
/// link at play (or locked) is never asked (plan Q7).
fn read_due(reach: Option<(LoginWindow, WifiReach)>, state: &DeviceNetwork) -> Option<LoginWindow> {
    let (window, WifiReach::Edit) = reach? else {
        return None;
    };
    (state.asked_on != Some(window) && !state.reading && !state.writing).then_some(window)
}

/// The connection window and the reach of `device`'s link, when it shows a
/// Wi‑Fi row: an open link to a LightPlayer board that has said hello, not
/// a browser sim (no radio, no device store), not a board holding its files
/// for the C6 layout change (its filesystem is RAM until Finish update, so
/// a write there would vanish), and unlocked when it is Bluetooth.
fn reach(device: &Device, granted: Option<Tier>) -> Option<(LoginWindow, WifiReach)> {
    let endpoint = device.identity.endpoint.as_ref()?;
    if endpoint
        .0
        .starts_with(crate::app::devices::sim_record::SIM_ENDPOINT_PREFIX)
    {
        return None;
    }
    if !device.evidence.classification.is_light_player() || holds_its_files(device) {
        return None;
    }
    let window = login_window(device)?;
    let reach = wifi_reach_for(is_bluetooth(device), granted)?;
    Some((window, reach))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plan Q6/Q7: USB always reaches; Bluetooth reaches at author, shows
    /// the row without asking at play, and shows nothing while locked.
    #[test]
    fn only_a_link_at_author_is_asked() {
        assert_eq!(wifi_reach_for(false, None), Some(WifiReach::Edit));
        assert_eq!(
            wifi_reach_for(true, Some(Tier::Edit)),
            Some(WifiReach::Edit)
        );
        assert_eq!(
            wifi_reach_for(true, Some(Tier::Play)),
            Some(WifiReach::PlayOnly)
        );
        assert_eq!(wifi_reach_for(true, None), None);
    }

    /// One read per connection, on an author link only, never while
    /// something is in flight.
    #[test]
    fn a_read_is_due_once_per_window_and_only_at_author() {
        let window = |link: u64| LoginWindow {
            link: LinkId(link),
            hello_at: lpa_devices::time::Millis(5),
        };
        let fresh = DeviceNetwork::default();
        assert_eq!(
            read_due(Some((window(1), WifiReach::Edit)), &fresh),
            Some(window(1))
        );
        assert_eq!(
            read_due(Some((window(1), WifiReach::PlayOnly)), &fresh),
            None,
            "a play link is never asked"
        );
        assert_eq!(read_due(None, &fresh), None, "a locked link is never asked");
        let asked = DeviceNetwork {
            asked_on: Some(window(1)),
            ..DeviceNetwork::default()
        };
        assert_eq!(read_due(Some((window(1), WifiReach::Edit)), &asked), None);
        assert_eq!(
            read_due(Some((window(2), WifiReach::Edit)), &asked),
            Some(window(2)),
            "a new connection is asked again"
        );
        let writing = DeviceNetwork {
            writing: true,
            ..DeviceNetwork::default()
        };
        assert_eq!(read_due(Some((window(1), WifiReach::Edit)), &writing), None);
    }

    /// A refused change keeps the last status and says why; the next
    /// answer clears it.
    #[test]
    fn a_refusal_keeps_the_status_and_says_why() {
        let mut network = NetworkController::new();
        let roster = Roster::new(Default::default());
        let effects = DeviceEffects::new();
        let status = NetworkStatus {
            wifi: true,
            cloud_relay: true,
            networks: Vec::new(),
            station: lpc_wire::server::StationState::Unsupported,
        };
        network.apply(
            NetworkCommand::Answered {
                device: DeviceId(1),
                kind: NetworkStepKind::Read,
                result: Ok(status.clone()),
            },
            &roster,
            &effects,
            |_| None,
        );
        network.apply(
            NetworkCommand::Answered {
                device: DeviceId(1),
                kind: NetworkStepKind::Write,
                result: Err(NetworkRefusal::Said(
                    "cannot save the network: …".to_string(),
                )),
            },
            &roster,
            &effects,
            |_| None,
        );
        let state = &network.devices[&DeviceId(1)];
        assert_eq!(state.status, Some(status.clone()));
        assert!(state.error.as_deref().unwrap().starts_with("cannot save"));
        network.apply(
            NetworkCommand::Answered {
                device: DeviceId(1),
                kind: NetworkStepKind::Read,
                result: Ok(status.clone()),
            },
            &roster,
            &effects,
            |_| None,
        );
        assert_eq!(network.devices[&DeviceId(1)].error, None);

        // A refusal for want of author withdraws the controls (the row says
        // so) instead of showing an error; the next answer brings them back.
        network.apply(
            NetworkCommand::Answered {
                device: DeviceId(1),
                kind: NetworkStepKind::Read,
                result: Err(NetworkRefusal::NotPermitted(Tier::Edit)),
            },
            &roster,
            &effects,
            |_| None,
        );
        let state = &network.devices[&DeviceId(1)];
        assert!(state.needs_author);
        assert_eq!(state.error, None);
        network.apply(
            NetworkCommand::Answered {
                device: DeviceId(1),
                kind: NetworkStepKind::Read,
                result: Ok(status),
            },
            &roster,
            &effects,
            |_| None,
        );
        assert!(!network.devices[&DeviceId(1)].needs_author);
    }

    /// An add that the board saved becomes the row's test; a refused one
    /// does not; Done dismisses it.
    #[test]
    fn a_saved_add_becomes_the_rows_test() {
        let mut network = NetworkController::new();
        let roster = Roster::new(Default::default());
        let effects = DeviceEffects::new();
        let status = NetworkStatus {
            wifi: true,
            cloud_relay: true,
            networks: vec![lpc_wire::server::SavedNetworkInfo {
                ssid: "lp-walk-net".to_string(),
                has_password: true,
                hidden: false,
                last: None,
            }],
            station: StationState::Unsupported,
        };
        let answered = |network: &mut NetworkController, result| {
            network.apply(
                NetworkCommand::Answered {
                    device: DeviceId(1),
                    kind: NetworkStepKind::Write,
                    result,
                },
                &roster,
                &effects,
                |_| None,
            )
        };
        network.devices.entry(DeviceId(1)).or_default().adding = Some("lp-walk-net".to_string());
        answered(&mut network, Err(NetworkRefusal::Said("no".to_string())));
        assert_eq!(network.devices[&DeviceId(1)].testing, None);
        network.devices.entry(DeviceId(1)).or_default().adding = Some("lp-walk-net".to_string());
        answered(&mut network, Ok(status));
        assert_eq!(
            network.devices[&DeviceId(1)].testing.as_deref(),
            Some("lp-walk-net")
        );
        network.apply(
            NetworkCommand::DismissTest {
                device: DeviceId(1),
            },
            &roster,
            &effects,
            |_| None,
        );
        assert_eq!(network.devices[&DeviceId(1)].testing, None);
    }
}
