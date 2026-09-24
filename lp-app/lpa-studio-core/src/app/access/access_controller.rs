//! The access controller: runs login on connect and the device-store writes,
//! and holds what this browser remembers.
//!
//! It owns no IO. It reads the device model's evidence (which Bluetooth
//! links are open and have said hello), decides with [`AccessSession`], and
//! spawns each conversation on the link's SHARED wire
//! (`DeviceEffects::conversation_io`) — the frame feed's road, so the pump
//! keeps folding heartbeats meanwhile. Every spawned future ends by posting
//! an [`AccessCommand`] result back onto the actor's queue, so the state
//! changes in queue order like everything else (invariant I7).
//!
//! When the editor lens holds a link's wire (the pump is paused, so a shared
//! conversation would never be answered), the step is parked for the actor,
//! which runs it through the lens's own client
//! ([`AccessController::take_lens_step`]).

use core::time::Duration;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use lpa_devices::identity::DeviceId;
use lpa_devices::time::Millis;
use lpa_devices::{Device, Roster};
use lpc_access::{DeviceAccessFile, Tier};
use lpc_model::AsLpPath;

use super::access_command::AccessCommand;
use super::access_session::{
    AccessPhase, AccessSession, AccessStep, LoginWindow, PromptReason, TypedPassword,
};
use super::device_access_record::{DeviceAccessChange, DeviceAccessRecords, apply_access_change};
use super::login_attempt::{LoginAttemptOutcome, try_passwords};
use super::login_key_cache::{DEFAULT_KDF_ITERATIONS, LoginKeyCache};
use super::remembered_passwords::RememberedPasswords;
use super::ui_access_view::{
    UiAccessPanel, UiAccessSecret, UiDeviceAccess, UiLoginPrompt, access_line, prompt_sentence,
};
use crate::app::devices::device_effects::{DeviceEffects, DeviceTaskFuture, DeviceTimerFuture};
use crate::app::devices::device_records::registry_key;
use crate::app::studio::studio_command::StudioCommand;
use crate::app::studio::studio_view_channel::CommandSender;

/// A document for the web edge to persist, each under its own key.
#[derive(Clone, PartialEq, Eq)]
pub enum AccessPersist {
    /// `lp.ble.passwords.v1`.
    Passwords(String),
    /// `lp.ble.device-access.v1`.
    Devices(String),
}

type Timer = Rc<RefCell<dyn FnMut(Duration) -> DeviceTimerFuture>>;

/// How a device-store write stands.
#[derive(Clone, Debug, PartialEq, Eq)]
enum WriteStatus {
    Writing,
    Failed(String),
}

/// See the module doc.
pub struct AccessController {
    sessions: BTreeMap<DeviceId, AccessSession>,
    remembered: RememberedPasswords,
    records: DeviceAccessRecords,
    keys: Rc<RefCell<LoginKeyCache>>,
    writes: BTreeMap<DeviceId, WriteStatus>,
    /// Devices a restart was asked of, with the hello window at the time: a
    /// newer hello is the restart having happened.
    restarts: BTreeMap<DeviceId, Option<Millis>>,
    /// A step parked for the lens's client (the lens holds that wire).
    lens_step: Option<(DeviceId, AccessStep)>,
    on_persist: Option<Rc<dyn Fn(AccessPersist)>>,
    tx: Option<CommandSender>,
    spawner: Option<Rc<dyn Fn(DeviceTaskFuture)>>,
    /// Whether an account default password is set (the drive hears it).
    default_set: bool,
}

impl Default for AccessController {
    fn default() -> Self {
        Self::new()
    }
}

impl AccessController {
    pub fn new() -> Self {
        Self {
            sessions: BTreeMap::new(),
            remembered: RememberedPasswords::default(),
            records: DeviceAccessRecords::default(),
            keys: Rc::new(RefCell::new(LoginKeyCache::new())),
            writes: BTreeMap::new(),
            restarts: BTreeMap::new(),
            lens_step: None,
            on_persist: None,
            tx: None,
            spawner: None,
            default_set: false,
        }
    }

    // --- seams ------------------------------------------------------------

    pub fn set_on_persist(&mut self, hook: impl Fn(AccessPersist) + 'static) {
        self.on_persist = Some(Rc::new(hook));
    }

    pub(crate) fn set_command_sender(&mut self, tx: CommandSender) {
        self.tx = Some(tx);
    }

    pub(crate) fn set_spawner(&mut self, spawner: Rc<dyn Fn(DeviceTaskFuture)>) {
        self.spawner = Some(spawner);
    }

    /// The session's key cache, for a login run through the lens.
    pub(crate) fn keys(&self) -> Rc<RefCell<LoginKeyCache>> {
        Rc::clone(&self.keys)
    }

    // --- reads ------------------------------------------------------------

    /// Passwords remembered on this browser, most recent first.
    pub fn remembered(&self) -> &RememberedPasswords {
        &self.remembered
    }

    pub fn session(&self, device: DeviceId) -> Option<&AccessSession> {
        self.sessions.get(&device)
    }

    /// Whether a Bluetooth link to `device` holds a tier (so the lens may
    /// attach: an untrusted link that has not logged in is answered nothing
    /// but hello and login).
    pub fn link_is_granted(&self, device: &Device) -> bool {
        if !is_bluetooth(device) {
            return true;
        }
        self.sessions
            .get(&device.id)
            .is_some_and(|session| matches!(session.phase, AccessPhase::Granted { .. }))
    }

    /// The tier a Bluetooth device's link holds, when it holds one.
    pub fn granted_tier(&self, device: DeviceId) -> Option<Tier> {
        match self.sessions.get(&device).map(|session| &session.phase) {
            Some(AccessPhase::Granted { tier, .. }) => Some(*tier),
            _ => None,
        }
    }

    // --- the drive --------------------------------------------------------

    /// Look at every Bluetooth device and start whatever conversation its
    /// login needs. Synchronous: conversations are spawned.
    pub fn drive(
        &mut self,
        roster: &Roster,
        effects: &DeviceEffects,
        now: Millis,
        default_password: Option<&str>,
    ) {
        self.default_set = default_password.is_some();
        for device in roster.devices() {
            self.watch_restart(device);
            if !is_bluetooth(device) {
                continue;
            }
            let window = login_window(device);
            let session = self.sessions.entry(device.id).or_default();
            session.observe(window);
            let remembered: Vec<&str> = self.remembered.in_order().collect();
            let Some(step) = session.next_step(now, default_password, &remembered) else {
                continue;
            };
            let link = match &step {
                AccessStep::Check(window) | AccessStep::Login { window, .. } => window.link,
            };
            if effects.lens_holds_wire(link) {
                if self.lens_step.is_none() {
                    session.started(&step);
                    self.lens_step = Some((device.id, step));
                }
                continue;
            }
            if effects.wire_borrowed(link) {
                // An activity has the wire; the next drive tries again.
                continue;
            }
            let (Some(io), Some(timer), Some(spawner), Some(tx)) = (
                effects.conversation_io(link),
                effects.timer_factory(),
                self.spawner.clone(),
                self.tx.clone(),
            ) else {
                continue;
            };
            session.started(&step);
            let keys = Rc::clone(&self.keys);
            let id = device.id;
            spawner(Box::pin(async move {
                let mut client = io.into_client();
                let result = run_step(&mut client, id, step, &keys, timer).await;
                tx.send(StudioCommand::Access(result));
            }));
        }
        let live: BTreeSet<DeviceId> = roster.devices().iter().map(|device| device.id).collect();
        self.sessions.retain(|device, _| live.contains(device));
    }

    /// The step parked for the lens's client, if any.
    pub(crate) fn take_lens_step(&mut self) -> Option<(DeviceId, AccessStep)> {
        self.lens_step.take()
    }

    // --- applying commands -----------------------------------------------

    /// Apply one command. Returns a follow-up the studio controller must
    /// perform (a restart is a device-model action, which it owns).
    pub fn apply(
        &mut self,
        command: AccessCommand,
        roster: &Roster,
        effects: &DeviceEffects,
        now: Millis,
        now_secs: f64,
        salt: [u8; lpc_access::SALT_BYTES],
    ) -> Option<AccessFollowUp> {
        match command {
            AccessCommand::MemoryLoaded {
                passwords_json,
                devices_json,
            } => {
                if let Some(json) = passwords_json {
                    self.remembered = RememberedPasswords::from_json(&json);
                }
                if let Some(json) = devices_json {
                    self.records = DeviceAccessRecords::from_json(&json);
                }
            }
            AccessCommand::SubmitPassword {
                device,
                password,
                remember,
            } => {
                if !password.is_empty() {
                    self.sessions
                        .entry(device)
                        .or_default()
                        .type_password(TypedPassword { password, remember });
                }
            }
            AccessCommand::Dismiss { device } => {
                if let Some(session) = self.sessions.get_mut(&device) {
                    session.dismiss();
                }
            }
            AccessCommand::LogIn { device } => {
                let session = self.sessions.entry(device).or_default();
                match session.phase {
                    AccessPhase::Granted {
                        tier: Tier::Play, ..
                    } => session.needs_edit(),
                    _ => session.ask(),
                }
            }
            AccessCommand::ForgetRememberedPasswords => {
                self.remembered.forget_all();
                self.keys.borrow_mut().clear();
                self.persist_passwords();
            }
            AccessCommand::Change { device, change } => {
                self.start_write(device, change, roster, effects, salt);
            }
            AccessCommand::Restart { device } => {
                let hello_at = roster
                    .device(device)
                    .and_then(|device| device.evidence.hello_heard_at());
                self.restarts.insert(device, hello_at);
                return Some(AccessFollowUp::Restart(device));
            }
            AccessCommand::ProjectSecretAdd(_) | AccessCommand::ProjectSecretRevoke { .. } => {
                // The studio controller owns the open project; it handles
                // these before they reach here.
            }
            AccessCommand::Checked {
                device,
                window,
                result,
            } => {
                let anything_known = !self.remembered.is_empty();
                if let Some(session) = self.sessions.get_mut(&device) {
                    match result {
                        Ok((required, granted)) => session.checked(
                            window,
                            required,
                            granted,
                            anything_known || self.default_set,
                        ),
                        Err(_) => session.logged_in(
                            window,
                            &LoginAttemptOutcome::Failed("hello".to_string()),
                            true,
                            now,
                        ),
                    }
                }
            }
            AccessCommand::LoggedIn {
                device,
                window,
                outcome,
                passwords,
                typed,
            } => {
                if let LoginAttemptOutcome::Granted { password_index, .. } = &outcome
                    && let Some(password) = passwords.get(*password_index)
                {
                    // A typed password is remembered when asked; one that
                    // came FROM the remembered list moves to the front.
                    let from_memory = self.remembered.in_order().any(|known| known == password);
                    let asked = typed.as_ref().is_some_and(|typed| typed.remember);
                    if asked || from_memory {
                        self.remembered.remember(password, now_secs);
                        self.persist_passwords();
                    }
                }
                if let Some(session) = self.sessions.get_mut(&device) {
                    session.logged_in(window, &outcome, typed.is_some(), now);
                }
            }
            AccessCommand::Written {
                device,
                key,
                result,
            } => match result {
                Ok(store) => {
                    self.writes.remove(&device);
                    self.records.record(&key, store, now_secs);
                    self.persist_devices();
                }
                Err(error) => {
                    self.writes.insert(device, WriteStatus::Failed(error));
                }
            },
        }
        None
    }

    /// An action on `device`'s link was refused for want of edit.
    pub fn note_needs_edit(&mut self, device: DeviceId) {
        if let Some(session) = self.sessions.get_mut(&device) {
            session.needs_edit();
        }
    }

    fn start_write(
        &mut self,
        device: DeviceId,
        change: DeviceAccessChange,
        roster: &Roster,
        effects: &DeviceEffects,
        salt: [u8; lpc_access::SALT_BYTES],
    ) {
        let fail = |this: &mut Self, message: &str| {
            this.writes
                .insert(device, WriteStatus::Failed(message.to_string()));
        };
        let Some(found) = roster.device(device) else {
            return fail(self, "this piece is gone");
        };
        let Some(key) = registry_key(&found.identity) else {
            return fail(self, "this piece has not said who it is yet");
        };
        let Some(link) = found
            .evidence
            .presence
            .link()
            .filter(|_| found.evidence.presence.is_open())
        else {
            return fail(self, "connect this piece first");
        };
        if is_bluetooth(found) && self.granted_tier(device) != Some(Tier::Edit) {
            return fail(self, super::not_permitted_sentence(Tier::Edit));
        }
        if effects.wire_borrowed(link) {
            return fail(
                self,
                "this piece is busy (the editor or another job has it) — try again in a moment",
            );
        }
        let (Some(io), Some(spawner), Some(tx)) = (
            effects.conversation_io(link),
            self.spawner.clone(),
            self.tx.clone(),
        ) else {
            return fail(self, "this piece cannot be written to from here");
        };
        let current = self.records.get(&key).map(|record| record.store.clone());
        self.writes.insert(device, WriteStatus::Writing);
        spawner(Box::pin(async move {
            // Derived here, off the fold: the key costs a KDF.
            let result = match apply_access_change(
                current.as_ref(),
                &change,
                salt,
                DEFAULT_KDF_ITERATIONS,
            ) {
                Ok(store) => write_store(io, store).await,
                Err(error) => Err(error),
            };
            tx.send(StudioCommand::Access(AccessCommand::Written {
                device,
                key,
                result,
            }));
        }));
    }

    fn watch_restart(&mut self, device: &Device) {
        let Some(before) = self.restarts.get(&device.id).copied() else {
            return;
        };
        let now = device.evidence.hello_heard_at();
        if now.is_some() && now != before {
            self.restarts.remove(&device.id);
            if let Some(key) = registry_key(&device.identity)
                && self.records.note_restarted(&key)
            {
                self.persist_devices();
            }
        }
    }

    fn persist_passwords(&self) {
        if let Some(hook) = &self.on_persist {
            hook(AccessPersist::Passwords(self.remembered.to_json()));
        }
    }

    fn persist_devices(&self) {
        if let Some(hook) = &self.on_persist {
            hook(AccessPersist::Devices(self.records.to_json()));
        }
    }

    // --- views ------------------------------------------------------------

    /// The access facts for one card. `name` is the card's title.
    pub fn device_view(
        &self,
        device: &Device,
        default_password: Option<&str>,
    ) -> Option<UiDeviceAccess> {
        let over_bluetooth = is_bluetooth(device);
        let session = self.sessions.get(&device.id);
        let line = over_bluetooth
            .then(|| session.map_or(AccessPhase::Unknown, |s| s.phase.clone()))
            .and_then(|phase| {
                device
                    .evidence
                    .presence
                    .is_open()
                    .then(|| access_line(&phase))
                    .flatten()
            });
        let log_in = session.and_then(|session| match &session.phase {
            AccessPhase::Locked => Some("Log in".to_string()),
            AccessPhase::Granted {
                tier: Tier::Play, ..
            } => Some("Log in for edit".to_string()),
            _ => None,
        });
        let panel = self.panel(device, default_password);
        if !over_bluetooth && panel.is_none() {
            return None;
        }
        Some(UiDeviceAccess {
            over_bluetooth,
            line,
            log_in: log_in.filter(|_| over_bluetooth),
            panel,
        })
    }

    fn panel(&self, device: &Device, default_password: Option<&str>) -> Option<UiAccessPanel> {
        let endpoint = device.identity.endpoint.as_ref()?;
        // A browser sim has no radio and no device store worth writing.
        if endpoint
            .0
            .starts_with(crate::app::devices::sim_record::SIM_ENDPOINT_PREFIX)
        {
            return None;
        }
        let evidence = &device.evidence;
        if !evidence.presence.is_open() || !evidence.classification.is_light_player() {
            return None;
        }
        let over_bluetooth = endpoint.is_bluetooth();
        if over_bluetooth && self.granted_tier(device.id) != Some(Tier::Edit) {
            return None;
        }
        let key = registry_key(&device.identity)?;
        let record = self.records.get(&key);
        let (writing, error) = match self.writes.get(&device.id) {
            Some(WriteStatus::Writing) => (true, None),
            Some(WriteStatus::Failed(error)) => (false, Some(error.clone())),
            None => (false, None),
        };
        Some(UiAccessPanel {
            device: device.id,
            ble_enabled: record.map(|record| record.store.ble_enabled),
            open: record.is_some_and(|record| record.store.open),
            secrets: record
                .map(|record| {
                    record
                        .store
                        .secrets
                        .iter()
                        .map(|entry| UiAccessSecret {
                            label: entry.label.clone(),
                            tier: entry.tier,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            restart_pending: record.is_some_and(|record| record.restart_pending),
            can_restart: !over_bluetooth,
            writing,
            error,
            default_password: default_password.map(str::to_string),
        })
    }

    /// The password sheet, when a device needs one. `title` names a device.
    pub fn prompt(&self, title: impl Fn(DeviceId) -> String) -> Option<UiLoginPrompt> {
        self.sessions.iter().find_map(|(device, session)| {
            let reason = session.prompt.as_ref()?;
            let device_name = title(*device);
            let retry_after_ms = match reason {
                PromptReason::Refused { retry_after_ms } if *retry_after_ms > 0 => {
                    Some(*retry_after_ms)
                }
                _ => None,
            };
            Some(UiLoginPrompt {
                device: *device,
                reason: prompt_sentence(reason, &device_name),
                device_name,
                retry_after_ms,
                busy: session.busy && session.typed.is_some()
                    || matches!(session.phase, AccessPhase::LoggingIn),
            })
        })
    }
}

/// What the studio controller must do after a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessFollowUp {
    /// Restart this device (the card's Reset, a device-model action).
    Restart(DeviceId),
}

fn is_bluetooth(device: &Device) -> bool {
    device
        .identity
        .endpoint
        .as_ref()
        .is_some_and(|endpoint| endpoint.is_bluetooth())
}

/// The device's current connection window, when its link is open and has
/// said hello.
fn login_window(device: &Device) -> Option<LoginWindow> {
    let evidence = &device.evidence;
    if !evidence.presence.is_open() {
        return None;
    }
    Some(LoginWindow {
        link: evidence.presence.link()?,
        hello_at: evidence.hello_heard_at()?,
    })
}

/// Run one step on a client and say how it went.
pub(crate) async fn run_step<Io: lpa_client::ClientIo>(
    client: &mut lpa_client::LpClient<Io>,
    device: DeviceId,
    step: AccessStep,
    keys: &Rc<RefCell<LoginKeyCache>>,
    timer: Timer,
) -> AccessCommand {
    match step {
        AccessStep::Check(window) => AccessCommand::Checked {
            device,
            window,
            result: client
                .hello()
                .await
                .map(|hello| (hello.value.auth.required, hello.value.auth.granted))
                .map_err(|error| error.to_string()),
        },
        AccessStep::Login {
            window,
            passwords,
            typed,
        } => {
            let outcome = try_passwords(client, &passwords, keys, |delay| {
                (timer.borrow_mut())(delay)
            })
            .await;
            AccessCommand::LoggedIn {
                device,
                window,
                outcome,
                passwords,
                typed,
            }
        }
    }
}

/// Write the whole device store over a shared-link conversation.
async fn write_store(
    io: crate::app::devices::SharedLinkClientIo,
    store: DeviceAccessFile,
) -> Result<DeviceAccessFile, String> {
    let json = store.to_json().map_err(|error| error.to_string())?;
    let mut client = io.into_client();
    match client
        .fs_write(DeviceAccessFile::PATH.as_path(), json.into_bytes())
        .await
    {
        Ok(_) => Ok(store),
        Err(lpa_client::ClientError::NotPermitted { needs }) => {
            Err(super::not_permitted_sentence(needs).to_string())
        }
        Err(error) => Err(format!("the piece did not take it: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::access::test_board::{FakeBoard, block_on};

    fn window() -> LoginWindow {
        LoginWindow {
            link: lpa_devices::link::LinkId(1),
            hello_at: Millis(5),
        }
    }

    fn instant_timer() -> Timer {
        Rc::new(RefCell::new(|_delay: Duration| {
            Box::pin(core::future::ready(())) as DeviceTimerFuture
        }))
    }

    /// The whole login-on-connect flow against a board running the REAL
    /// verifier: check, two automatic tries (default wrong, remembered
    /// right), the grant, and the remembered password moved to the front.
    #[test]
    fn login_on_connect_against_a_locked_board() {
        let board = FakeBoard::locked(&[("camp", Tier::Play, "smores")]);
        let mut access = AccessController::new();
        access.remembered.remember("smores", 1.0);
        access.remembered.remember("other", 2.0);
        let keys = access.keys();
        let device = DeviceId(7);
        let mut session = AccessSession::default();
        session.observe(Some(window()));

        // Check.
        let step = session
            .next_step(Millis(5), Some("dflt"), &["other", "smores"])
            .unwrap();
        session.started(&step);
        let mut client = board.client();
        let checked = block_on(run_step(&mut client, device, step, &keys, instant_timer()));
        let AccessCommand::Checked { result, .. } = &checked else {
            panic!("{checked:?}")
        };
        assert_eq!(result, &Ok((true, None)));
        session.checked(window(), true, None, true);

        // Two automatic tries: the default, then the most recent remembered.
        let step = session
            .next_step(Millis(6), Some("dflt"), &["other", "smores"])
            .unwrap();
        let AccessStep::Login { passwords, .. } = &step else {
            panic!()
        };
        assert_eq!(passwords, &["dflt".to_string(), "other".to_string()]);
        session.started(&step);
        let done = block_on(run_step(&mut client, device, step, &keys, instant_timer()));
        let AccessCommand::LoggedIn { outcome, .. } = &done else {
            panic!()
        };
        assert!(
            matches!(outcome, LoginAttemptOutcome::Refused { .. }),
            "{outcome:?}"
        );
        session.logged_in(window(), outcome, false, Millis(7));
        // Capped at two: "smores" was never sent, and the sheet is up.
        assert_eq!(board.failures(), 2);
        assert!(session.prompt.is_some());
        assert_eq!(session.next_step(Millis(8), Some("dflt"), &[]), None);

        // The user types the right one.
        session.type_password(TypedPassword {
            password: "smores".to_string(),
            remember: true,
        });
        let step = session.next_step(Millis(9), Some("dflt"), &[]).unwrap();
        session.started(&step);
        let done = block_on(run_step(&mut client, device, step, &keys, instant_timer()));
        let AccessCommand::LoggedIn { outcome, .. } = &done else {
            panic!()
        };
        session.logged_in(window(), outcome, true, Millis(10));
        assert_eq!(
            session.phase,
            AccessPhase::Granted {
                tier: Tier::Play,
                label: Some("camp".to_string())
            }
        );
        assert_eq!(board.granted(), Some(Tier::Play));
    }

    /// A play login cannot write the device store: the refusal is the
    /// sheet's sentence, never "failed".
    #[test]
    fn a_play_login_is_refused_an_edit_by_name() {
        let board = FakeBoard::open(&[]);
        let io = board.io();
        let mut client = lpa_client::LpClient::new(io);
        let refused = block_on(client.fs_write(DeviceAccessFile::PATH.as_path(), b"{}".to_vec()))
            .expect_err("play cannot write");
        assert_eq!(
            refused,
            lpa_client::ClientError::NotPermitted { needs: Tier::Edit }
        );
        assert!(super::super::not_permitted_sentence(Tier::Edit).contains("edit password"));
    }
}
