//! The access controller: unlocks Bluetooth links, keeps each device's
//! "Who has access" list, adds this browser's keys over USB, and holds what
//! this browser knows.
//!
//! It owns no IO. It reads the device model's evidence (which links are open
//! and have said hello), decides with [`AccessSession`] and the key holders,
//! and spawns each conversation on the link's SHARED wire
//! (`DeviceEffects::conversation_io`) — the frame feed's road, so the pump
//! keeps folding heartbeats meanwhile. Every spawned future ends by posting
//! an [`AccessCommand`] result back onto the actor's queue, so the state
//! changes in queue order like everything else (invariant I7).
//!
//! When the editor lens holds a link's wire (the pump is paused, so a shared
//! conversation would never be answered), the step is parked for the actor,
//! which runs it through the lens's own client
//! ([`AccessController::take_lens_step`]).
//!
//! Three things happen per device:
//!
//! - **Bluetooth unlock** (untrusted link): by salt with the held keys, else
//!   remembered passwords, else the Unlock sheet (`access_session.rs`).
//! - **USB sync** (trusted link, once per connection): read the list, add the
//!   held keys it is missing, remove retired account keys, and raise
//!   [`AccessAdded`] for the toast when anything was added (plan D6). A
//!   Bluetooth link unlocked at edit only reads the list.
//! - **Changes** from the access panel, and Undo.

use core::time::Duration;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::Rc;

use lpa_devices::identity::DeviceId;
use lpa_devices::link::LinkId;
use lpa_devices::time::Millis;
use lpa_devices::{Device, Roster};
use lpc_access::{SALT_BYTES, SecretKind, Tier};

use super::access_added::AccessAdded;
use super::access_command::AccessCommand;
use super::access_session::{AccessPhase, AccessSession, AccessStep, LoginWindow, TypedPassword};
use super::account_keys::AccountKeys;
use super::browser_key::{BrowserKey, FALLBACK_BROWSER_NAME};
use super::device_access_ops::{AccessOp, run_access_ops, sync_access};
use super::device_access_record::{DeviceAccessChange, DeviceAccessRecords, check_new_password};
use super::key_holder::{HeldKey, InstallableKey, held_keys};
use super::login_attempt::{LoginAttemptOutcome, try_login};
use super::login_key_cache::{DEFAULT_KDF_ITERATIONS, LoginKeyCache};
use super::remembered_passwords::RememberedPasswords;
use super::ui_access_view::{
    UiAccessEntry, UiAccessPanel, UiDeviceAccess, UiLoginPrompt, UiUnlockOffer, access_line,
    prompt_sentence,
};
use crate::app::devices::device_effects::{DeviceEffects, DeviceTaskFuture, DeviceTimerFuture};

use crate::app::studio::studio_command::StudioCommand;
use crate::app::studio::studio_view_channel::CommandSender;

/// A document for the web edge to persist, each under its own key.
#[derive(Clone, PartialEq, Eq)]
pub enum AccessPersist {
    /// `lp.ble.passwords.v1`.
    Passwords(String),
    /// `lp.access.device-lists.v1`.
    Devices(String),
    /// `lp.access.browser.v1`.
    Browser(String),
    /// `lp.access.account.v1`; `None` removes it (sign-out).
    Account(Option<String>),
}

type Timer = Rc<RefCell<dyn FnMut(Duration) -> DeviceTimerFuture>>;

/// The caller's randomness: 16 bytes a draw.
pub type AccessRandom<'a> = &'a dyn Fn() -> [u8; SALT_BYTES];

/// How a change to a device's list stands.
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
    browser: Option<BrowserKey>,
    account: Option<AccountKeys>,
    writes: BTreeMap<DeviceId, WriteStatus>,
    /// Devices a restart was asked of, with the hello window at the time: a
    /// newer hello is the restart having happened.
    restarts: BTreeMap<DeviceId, Option<Millis>>,
    /// The connection window each device's list was last read on.
    synced: BTreeMap<DeviceId, LoginWindow>,
    /// What the last USB sync added to each device, for Undo.
    undo: BTreeMap<DeviceId, Vec<[u8; SALT_BYTES]>>,
    /// Devices (by record key) whose automatic add was undone: not added to
    /// again this session (a re-plug would otherwise undo the Undo).
    declined: BTreeSet<String>,
    added: Option<AccessAdded>,
    added_generation: u64,
    /// Steps parked for the lens's client (the lens holds that wire), at
    /// most one per device.
    lens_steps: VecDeque<(DeviceId, AccessStep)>,
    on_persist: Option<Rc<dyn Fn(AccessPersist)>>,
    tx: Option<CommandSender>,
    spawner: Option<Rc<dyn Fn(DeviceTaskFuture)>>,
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
            browser: None,
            account: None,
            writes: BTreeMap::new(),
            restarts: BTreeMap::new(),
            synced: BTreeMap::new(),
            undo: BTreeMap::new(),
            declined: BTreeSet::new(),
            added: None,
            added_generation: 0,
            lens_steps: VecDeque::new(),
            on_persist: None,
            tx: None,
            spawner: None,
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

    /// This browser's key (its name is what devices list it as).
    pub fn browser_key(&self) -> Option<&BrowserKey> {
        self.browser.as_ref()
    }

    /// The signed-in account's keys, as last handed in (or cached).
    pub fn account_keys(&self) -> Option<&AccountKeys> {
        self.account.as_ref()
    }

    /// What the last USB connect added on its own, for the toast.
    pub fn access_added(&self) -> Option<&AccessAdded> {
        self.added.as_ref()
    }

    pub fn session(&self, device: DeviceId) -> Option<&AccessSession> {
        self.sessions.get(&device)
    }

    /// Whether a Bluetooth link to `device` holds a tier (so the lens may
    /// attach: an untrusted link that has not unlocked is answered nothing
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

    /// Every key this browser holds right now.
    pub fn held(&self) -> Vec<HeldKey> {
        held_keys(self.browser.as_ref(), self.account.as_ref())
    }

    // --- the drive --------------------------------------------------------

    /// Look at every device and start whatever conversation it needs:
    /// unlocking a Bluetooth link, reading a list, adding keys over USB.
    /// Synchronous: conversations are spawned.
    pub fn drive(
        &mut self,
        roster: &Roster,
        effects: &DeviceEffects,
        now: Millis,
        now_secs: f64,
        random: AccessRandom<'_>,
    ) {
        self.ensure_browser_key(random, FALLBACK_BROWSER_NAME);
        let held = self.held();
        for device in roster.devices() {
            self.watch_restart(device);
            let Some(window) = login_window(device) else {
                if is_bluetooth(device) {
                    self.sessions.entry(device.id).or_default().observe(None);
                }
                continue;
            };
            if is_bluetooth(device) {
                self.drive_unlock(device.id, window, &held, effects, now);
                if self.granted_tier(device.id) == Some(Tier::Edit)
                    && self.synced.get(&device.id) != Some(&window)
                {
                    // Read the list for the panel; never add over the air.
                    let step = self.sync_step(window, false, now_secs);
                    if self.dispatch(device.id, window.link, step, effects) {
                        self.synced.insert(device.id, window);
                    }
                }
            } else if syncs_over_usb(device) && self.synced.get(&device.id) != Some(&window) {
                let declined =
                    record_key(&device.identity).is_some_and(|key| self.declined.contains(&key));
                let step = self.sync_step(window, !declined, now_secs);
                if self.dispatch(device.id, window.link, step, effects) {
                    self.synced.insert(device.id, window);
                }
            }
        }
        let live: BTreeSet<DeviceId> = roster.devices().iter().map(|device| device.id).collect();
        self.sessions.retain(|device, _| live.contains(device));
        self.synced.retain(|device, _| live.contains(device));
    }

    fn drive_unlock(
        &mut self,
        device: DeviceId,
        window: LoginWindow,
        held: &[HeldKey],
        effects: &DeviceEffects,
        now: Millis,
    ) {
        let remembered: Vec<String> = self.remembered.in_order().map(str::to_string).collect();
        let remembered: Vec<&str> = remembered.iter().map(String::as_str).collect();
        let session = self.sessions.entry(device).or_default();
        session.observe(Some(window));
        let Some(step) = session.next_step(now, held, &remembered) else {
            return;
        };
        if self.dispatch(device, window.link, step.clone(), effects)
            && let Some(session) = self.sessions.get_mut(&device)
        {
            session.started(&step);
        }
    }

    /// The sync step for a device's current window: with every held key
    /// and the account's retired salts when it may `add` (USB, and its add
    /// not undone this session); else the list alone (Bluetooth).
    pub(crate) fn sync_step(&self, window: LoginWindow, add: bool, now_secs: f64) -> AccessStep {
        let (held, stale) = if add {
            (
                self.held(),
                self.account
                    .as_ref()
                    .map(|account| account.previous_key_salts.clone())
                    .unwrap_or_default(),
            )
        } else {
            (Vec::new(), Vec::new())
        };
        AccessStep::Sync {
            window,
            held,
            stale,
            added_at: epoch_secs(now_secs),
        }
    }

    /// Start `step` on `link`, or park it for the lens. False when it could
    /// not start now (the next drive tries again).
    fn dispatch(
        &mut self,
        device: DeviceId,
        link: LinkId,
        step: AccessStep,
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
            // An activity has the wire; the next drive tries again.
            return false;
        }
        let (Some(io), Some(timer), Some(spawner), Some(tx)) = (
            effects.conversation_io(link),
            effects.timer_factory(),
            self.spawner.clone(),
            self.tx.clone(),
        ) else {
            return false;
        };
        let keys = Rc::clone(&self.keys);
        spawner(Box::pin(async move {
            let mut client = io.into_client();
            let result = run_step(&mut client, device, step, &keys, timer).await;
            tx.send(StudioCommand::Access(result));
        }));
        true
    }

    /// The next step parked for the lens's client, if any.
    pub(crate) fn take_lens_step(&mut self) -> Option<(DeviceId, AccessStep)> {
        self.lens_steps.pop_front()
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
        random: AccessRandom<'_>,
    ) -> Option<AccessFollowUp> {
        match command {
            AccessCommand::MemoryLoaded {
                passwords_json,
                devices_json,
                browser_json,
                account_json,
            } => {
                if let Some(json) = passwords_json {
                    self.remembered = RememberedPasswords::from_json(&json);
                }
                if let Some(json) = devices_json {
                    self.records = DeviceAccessRecords::from_json(&json);
                }
                if let Some(key) = browser_json.as_deref().and_then(BrowserKey::from_json) {
                    self.browser = Some(key);
                }
                self.ensure_browser_key(random, FALLBACK_BROWSER_NAME);
                if let Some(keys) = account_json.as_deref().and_then(AccountKeys::from_json) {
                    self.account = Some(keys);
                }
            }
            AccessCommand::BrowserNameDefault(name) => {
                self.ensure_browser_key(random, &name);
                if self
                    .browser
                    .as_mut()
                    .is_some_and(|key| key.offer_default_name(&name))
                {
                    self.browser_changed();
                }
            }
            AccessCommand::BrowserNamePlaceholder(name) => {
                self.ensure_browser_key(random, &name);
                if self
                    .browser
                    .as_mut()
                    .is_some_and(|key| key.offer_placeholder_name(&name))
                {
                    self.browser_changed();
                }
            }
            AccessCommand::RenameBrowser(name) => {
                self.ensure_browser_key(random, &name);
                if self.browser.as_mut().is_some_and(|key| key.rename(&name)) {
                    self.browser_changed();
                }
            }
            AccessCommand::AccountKeys(keys) => {
                if self.account != keys {
                    self.account = keys;
                    self.persist(AccessPersist::Account(
                        self.account.as_ref().map(AccountKeys::to_json),
                    ));
                    // Connected devices get the account's keys now, not
                    // at their next plug-in.
                    self.synced.clear();
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
            AccessCommand::RememberPassword(password) => {
                if !password.is_empty() {
                    self.remembered.remember(&password, now_secs);
                    self.persist_passwords();
                }
            }
            AccessCommand::ForgetRememberedPasswords => {
                self.remembered.forget_all();
                self.keys.borrow_mut().clear();
                self.persist_passwords();
            }
            AccessCommand::Change { device, change } => {
                self.start_change(device, change, roster, effects, now_secs, random);
            }
            AccessCommand::UndoAutoAdd { device } => {
                if let Some(step) = self.undo_step(device, now_secs) {
                    if let Some(key) = roster
                        .device(device)
                        .and_then(|found| record_key(&found.identity))
                    {
                        self.declined.insert(key);
                    }
                    self.start_step(device, step, roster, effects);
                }
            }
            AccessCommand::Restart { device } => {
                return Some(self.restart(device, roster));
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
                let anything_known = !self.remembered.is_empty() || !self.held().is_empty();
                if let Some(session) = self.sessions.get_mut(&device) {
                    match result {
                        Ok((required, granted)) => {
                            session.checked(window, required, granted, anything_known)
                        }
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
                if let LoginAttemptOutcome::Granted {
                    password_index: Some(index),
                    ..
                } = &outcome
                    && let Some(password) = passwords.get(*index)
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
            AccessCommand::Synced {
                device,
                window: _,
                result,
            } => match result {
                Ok(synced) => {
                    if let Some(key) = roster
                        .device(device)
                        .and_then(|found| record_key(&found.identity))
                    {
                        self.records.record(&key, synced.listing, now_secs);
                        self.persist_devices();
                    }
                    if !synced.added.is_empty() {
                        self.undo
                            .insert(device, synced.added.iter().map(|a| a.salt).collect());
                        self.added_generation += 1;
                        self.added = Some(AccessAdded {
                            device,
                            names: synced.added.into_iter().map(|a| a.label).collect(),
                            generation: self.added_generation,
                        });
                    }
                }
                Err(error) => {
                    // Silent by design (an older firmware, a full device):
                    // the panel still shows what it last knew.
                    log::warn!("access: reading or adding to {device:?}'s list failed: {error}");
                }
            },
            AccessCommand::Changed {
                device,
                result,
                bluetooth,
            } => match result {
                Ok(listing) => {
                    self.writes.remove(&device);
                    let found = roster.device(device);
                    if let Some(key) = found.and_then(|found| record_key(&found.identity)) {
                        self.records.record(&key, listing, now_secs);
                        self.persist_devices();
                    }
                    // Bluetooth applies at boot: over USB, Studio restarts
                    // the device itself (AC1).
                    if bluetooth.is_some() && found.is_some_and(|found| !is_bluetooth(found)) {
                        return Some(self.restart(device, roster));
                    }
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

    fn restart(&mut self, device: DeviceId, roster: &Roster) -> AccessFollowUp {
        let hello_at = roster
            .device(device)
            .and_then(|device| device.evidence.hello_heard_at());
        self.restarts.insert(device, hello_at);
        AccessFollowUp::Restart(device)
    }

    /// The Undo step for the last USB add on `device`, taken once.
    pub(crate) fn undo_step(&mut self, device: DeviceId, now_secs: f64) -> Option<AccessStep> {
        let salts = self.undo.remove(&device)?;
        if self
            .added
            .as_ref()
            .is_some_and(|added| added.device == device)
        {
            self.added = None;
        }
        Some(AccessStep::Change {
            ops: salts.into_iter().map(AccessOp::Remove).collect(),
            added_at: epoch_secs(now_secs),
            bluetooth: None,
        })
    }

    /// The step a panel change runs, or why it cannot.
    pub(crate) fn change_step(
        &self,
        change: DeviceAccessChange,
        over_bluetooth: bool,
        now_secs: f64,
        random: AccessRandom<'_>,
    ) -> Result<AccessStep, String> {
        let (ops, bluetooth) = match change {
            DeviceAccessChange::Remove { salt } => (vec![AccessOp::Remove(salt)], None),
            DeviceAccessChange::SetOpen(open) => (
                vec![AccessOp::Switches {
                    ble_enabled: None,
                    open: Some(open),
                }],
                None,
            ),
            DeviceAccessChange::SetBluetooth(false) if over_bluetooth => {
                return Err(
                    "Bluetooth can only be turned off by USB — it is the link you are on."
                        .to_string(),
                );
            }
            DeviceAccessChange::SetBluetooth(on) => (
                vec![AccessOp::Switches {
                    ble_enabled: Some(on),
                    open: None,
                }],
                Some(on),
            ),
            DeviceAccessChange::AddPassword {
                label,
                tier,
                password,
            } => {
                check_new_password(&label, &password)?;
                (
                    vec![AccessOp::Add(InstallableKey {
                        label: label.trim().to_string(),
                        kind: SecretKind::Password,
                        tier,
                        salt: random(),
                        iterations: DEFAULT_KDF_ITERATIONS,
                        material: password.into_bytes(),
                    })],
                    None,
                )
            }
        };
        Ok(AccessStep::Change {
            ops,
            added_at: epoch_secs(now_secs),
            bluetooth,
        })
    }

    fn start_change(
        &mut self,
        device: DeviceId,
        change: DeviceAccessChange,
        roster: &Roster,
        effects: &DeviceEffects,
        now_secs: f64,
        random: AccessRandom<'_>,
    ) {
        let over_bluetooth = roster.device(device).is_some_and(is_bluetooth);
        match self.change_step(change, over_bluetooth, now_secs, random) {
            Ok(step) => self.start_step(device, step, roster, effects),
            Err(error) => {
                self.writes.insert(device, WriteStatus::Failed(error));
            }
        }
    }

    /// Start a change (or an Undo) on `device`'s link, or say why not.
    fn start_step(
        &mut self,
        device: DeviceId,
        step: AccessStep,
        roster: &Roster,
        effects: &DeviceEffects,
    ) {
        let fail = |this: &mut Self, message: &str| {
            this.writes
                .insert(device, WriteStatus::Failed(message.to_string()));
        };
        let Some(found) = roster.device(device) else {
            return fail(self, "this device is gone");
        };
        let Some(link) = found
            .evidence
            .presence
            .link()
            .filter(|_| found.evidence.presence.is_open())
        else {
            return fail(self, "connect this device first");
        };
        if is_bluetooth(found) && self.granted_tier(device) != Some(Tier::Edit) {
            return fail(self, super::not_permitted_sentence(Tier::Edit));
        }
        if !effects.lens_holds_wire(link) && effects.wire_borrowed(link) {
            return fail(
                self,
                "this device is busy (another job has it) — try again in a moment",
            );
        }
        if self.dispatch(device, link, step, effects) {
            self.writes.insert(device, WriteStatus::Writing);
        } else {
            fail(self, "this device cannot be changed from here right now");
        }
    }

    fn ensure_browser_key(&mut self, random: AccessRandom<'_>, name: &str) {
        if self.browser.is_some() {
            return;
        }
        let name = if name.trim().is_empty() {
            FALLBACK_BROWSER_NAME
        } else {
            name.trim()
        };
        self.browser = Some(BrowserKey::mint(random, name));
        self.persist_browser();
    }

    /// The browser key's name changed: persist it, and re-label it on the
    /// devices connected now (the rest re-label on their next connect).
    fn browser_changed(&mut self) {
        self.persist_browser();
        self.synced.clear();
    }

    fn watch_restart(&mut self, device: &Device) {
        let Some(before) = self.restarts.get(&device.id).copied() else {
            return;
        };
        let now = device.evidence.hello_heard_at();
        if now.is_some() && now != before {
            self.restarts.remove(&device.id);
            if let Some(key) = record_key(&device.identity)
                && self.records.note_restarted(&key)
            {
                self.persist_devices();
            }
        }
    }

    fn persist(&self, document: AccessPersist) {
        if let Some(hook) = &self.on_persist {
            hook(document);
        }
    }

    fn persist_passwords(&self) {
        self.persist(AccessPersist::Passwords(self.remembered.to_json()));
    }

    fn persist_devices(&self) {
        self.persist(AccessPersist::Devices(self.records.to_json()));
    }

    fn persist_browser(&self) {
        if let Some(key) = &self.browser {
            self.persist(AccessPersist::Browser(key.to_json()));
        }
    }

    // --- views ------------------------------------------------------------

    /// The access facts for one card.
    pub fn device_view(&self, device: &Device) -> Option<UiDeviceAccess> {
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
        let unlock = session.and_then(|session| match &session.phase {
            AccessPhase::Locked => Some(UiUnlockOffer::Locked),
            AccessPhase::Granted {
                tier: Tier::Play, ..
            } => Some(UiUnlockOffer::PlayOnly),
            _ => None,
        });
        let panel = self.panel(device);
        if !over_bluetooth && panel.is_none() {
            return None;
        }
        Some(UiDeviceAccess {
            over_bluetooth,
            line,
            unlock: unlock.filter(|_| over_bluetooth),
            panel,
        })
    }

    /// "Who has access", when this link may see it: USB, or a Bluetooth
    /// unlock at edit.
    fn panel(&self, device: &Device) -> Option<UiAccessPanel> {
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
        let key = record_key(&device.identity)?;
        let record = self.records.get(&key);
        let (writing, error) = match self.writes.get(&device.id) {
            Some(WriteStatus::Writing) => (true, None),
            Some(WriteStatus::Failed(error)) => (false, Some(error.clone())),
            None => (false, None),
        };
        let entries: Vec<UiAccessEntry> = record
            .map(|record| {
                record
                    .listing
                    .entries
                    .iter()
                    .map(|entry| UiAccessEntry {
                        label: entry.label.clone(),
                        kind: entry.kind,
                        tier: entry.tier,
                        salt_id: entry.salt,
                        is_this_browser: self
                            .browser
                            .as_ref()
                            .is_some_and(|key| key.salt == entry.salt),
                        is_account: self
                            .account
                            .as_ref()
                            .is_some_and(|account| account.owns_salt(&entry.salt)),
                        added_at: entry.added_at,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let open = record.is_some_and(|record| record.listing.open);
        Some(UiAccessPanel {
            device: device.id,
            count: entries.len() + usize::from(open),
            entries,
            ble_enabled: record.map(|record| record.listing.ble_enabled),
            open,
            restart_pending: record.is_some_and(|record| record.restart_pending),
            can_restart: !over_bluetooth,
            over_bluetooth,
            writing,
            error,
        })
    }

    /// The Unlock sheet, when a device needs one. `title` names a device.
    pub fn prompt(&self, title: impl Fn(DeviceId) -> String) -> Option<UiLoginPrompt> {
        self.sessions.iter().find_map(|(device, session)| {
            let reason = session.prompt.as_ref()?;
            let device_name = title(*device);
            let retry_after_ms = match reason {
                super::PromptReason::Refused { retry_after_ms } if *retry_after_ms > 0 => {
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

/// The key a device's list is cached under: its base MAC when it has one,
/// else its uid.
///
/// NOT the registry key, which is the uid and falls back to the MAC: a board
/// first seen before it was stamped is `mac:…` in the registry and becomes
/// `dev…` once a uid is stamped on it, and a record keyed on the old key
/// would vanish from the panel. The MAC is the one identity a board carries
/// from its first hello to its last.
fn record_key(identity: &lpa_devices::identity::IdentityChain) -> Option<String> {
    if let Some(mac) = &identity.mac {
        return Some(format!("mac:{}", mac.0));
    }
    identity.uid.as_ref().map(|uid| uid.0.clone())
}

fn is_bluetooth(device: &Device) -> bool {
    device
        .identity
        .endpoint
        .as_ref()
        .is_some_and(|endpoint| endpoint.is_bluetooth())
}

/// A trusted link to a LightPlayer board that is not a browser sim: physical
/// connection is access, so its connect adds this browser's keys.
fn syncs_over_usb(device: &Device) -> bool {
    let Some(endpoint) = device.identity.endpoint.as_ref() else {
        return false;
    };
    !endpoint.is_bluetooth()
        && !endpoint
            .0
            .starts_with(crate::app::devices::sim_record::SIM_ENDPOINT_PREFIX)
        && device.evidence.classification.is_light_player()
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

/// Whole epoch seconds, for an entry's `addedAt`.
fn epoch_secs(now_secs: f64) -> u64 {
    if now_secs.is_finite() && now_secs > 0.0 {
        now_secs as u64
    } else {
        0
    }
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
            held,
            passwords,
            typed,
            challenge,
        } => {
            let outcome = try_login(client, &held, &passwords, challenge, keys, |delay| {
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
        AccessStep::Sync {
            window,
            held,
            stale,
            added_at,
        } => AccessCommand::Synced {
            device,
            window,
            result: sync_access(client, &held, &stale, added_at).await,
        },
        AccessStep::Change {
            ops,
            added_at,
            bluetooth,
        } => AccessCommand::Changed {
            device,
            result: run_access_ops(client, &ops, added_at).await,
            bluetooth,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::access::PromptReason;
    use crate::app::access::account_keys::tests::account;
    use crate::app::access::test_board::{FakeBoard, FakeBoardIo, block_on};
    use lpa_client::LpClient;
    use lpc_access::SecretEntry;
    use std::cell::Cell;

    /// The whole Bluetooth unlock with this browser's key: check, ONE
    /// answer, granted by the browser's label — no sheet, no guess.
    #[test]
    fn a_browser_key_on_the_board_unlocks_silently() {
        let access = controller();
        let browser = access.browser_key().unwrap().installable();
        let board = FakeBoard::with_entries(vec![password_entry("friends", 1), browser.entry(1)]);
        let session = unlock(&access, &board, &[]);
        assert_eq!(
            session.phase,
            AccessPhase::Granted {
                tier: Tier::Edit,
                label: Some("Yona's MacBook".to_string())
            }
        );
        assert_eq!(session.prompt, None);
        assert_eq!(board.answers(), 1);
        assert_eq!(board.failures(), 0);
        assert_eq!(board.granted(), Some(Tier::Edit));
    }

    #[test]
    fn the_account_key_unlocks_a_board_this_browser_never_touched() {
        let mut access = controller();
        access.account = Some(account(None));
        let account_key = account(None).held_keys()[0].key.clone();
        let board = FakeBoard::with_entries(vec![account_key.entry(1)]);
        let session = unlock(&access, &board, &[]);
        assert_eq!(
            session.phase,
            AccessPhase::Granted {
                tier: Tier::Edit,
                label: Some("Yona's account".to_string())
            }
        );
        assert_eq!(board.answers(), 1);
    }

    /// An account password is derived (PBKDF2, at the cost the board
    /// offers) and matched by its salt.
    #[test]
    fn an_account_password_unlocks_through_the_kdf() {
        let mut access = controller();
        access.account = Some(account(Some("glitter-otter")));
        let mut play = account(Some("glitter-otter")).held_keys()[1].key.clone();
        play.iterations = 3;
        let board = FakeBoard::with_entries(vec![play.entry(1)]);
        let session = unlock(&access, &board, &[]);
        assert_eq!(
            session.phase,
            AccessPhase::Granted {
                tier: Tier::Play,
                label: Some("Yona's play password".to_string())
            }
        );
        assert_eq!(board.failures(), 0);
    }

    #[test]
    fn with_no_held_key_on_the_board_a_remembered_password_unlocks() {
        let mut access = controller();
        access.remembered.remember("smores", 1.0);
        let board = FakeBoard::locked(&[("camp", Tier::Play, "smores")]);
        let session = unlock(&access, &board, &["smores"]);
        assert_eq!(
            session.phase,
            AccessPhase::Granted {
                tier: Tier::Play,
                label: Some("camp".to_string())
            }
        );
        assert_eq!(board.answers(), 1);
    }

    /// A password shared by link is remembered (and persisted), so the next
    /// device offering it unlocks with no screen.
    #[test]
    fn a_shared_password_is_remembered_and_then_unlocks() {
        let mut access = controller();
        let persisted = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&persisted);
        access.set_on_persist(move |document| sink.borrow_mut().push(document));
        access.apply(
            AccessCommand::RememberPassword("maple-otter-42".to_string()),
            &Roster::new(Default::default()),
            &DeviceEffects::new(),
            Millis(0),
            5.0,
            &counter(),
        );
        assert_eq!(access.remembered().len(), 1);
        assert!(
            persisted
                .borrow()
                .iter()
                .any(|document| matches!(document, AccessPersist::Passwords(_)))
        );
        let board = FakeBoard::locked(&[("friends", Tier::Play, "maple-otter-42")]);
        let session = unlock(&access, &board, &["maple-otter-42"]);
        assert_eq!(
            session.phase,
            AccessPhase::Granted {
                tier: Tier::Play,
                label: Some("friends".to_string())
            }
        );
        assert_eq!(board.failures(), 0);
    }

    /// Nothing held is on the board and nothing is remembered: the sheet
    /// asks, and NO answer was sent. The typed password then answers the
    /// challenge the board left open (a begin never answered holds the
    /// board's one login slot).
    #[test]
    fn with_nothing_known_the_sheet_asks_and_no_answer_is_sent() {
        let access = controller();
        let board = FakeBoard::locked(&[("camp", Tier::Play, "smores")]);
        let mut session = unlock(&access, &board, &[]);
        assert_eq!(session.phase, AccessPhase::Locked);
        assert_eq!(session.prompt, Some(PromptReason::NoPasswordKnown));
        assert_eq!(board.answers(), 0, "no answer sent");
        assert_eq!(board.failures(), 0);

        session.type_password(TypedPassword {
            password: "smores".to_string(),
            remember: true,
        });
        let step = session.next_step(Millis(20), &access.held(), &[]).unwrap();
        let AccessStep::Login { challenge, .. } = &step else {
            panic!("{step:?}")
        };
        assert!(challenge.is_some(), "the open challenge is answered");
        run_login(&access, &board, &mut session, step, Millis(21));
        assert_eq!(
            session.phase,
            AccessPhase::Granted {
                tier: Tier::Play,
                label: Some("camp".to_string())
            }
        );
        assert_eq!(board.answers(), 1);
    }

    /// Automatic tries are capped per device: one wrong remembered password
    /// at most, and none again on a reconnect.
    #[test]
    fn never_more_than_one_automatic_wrong_answer_per_connect() {
        let mut access = controller();
        for (n, password) in ["a", "b", "c"].iter().enumerate() {
            access.remembered.remember(password, n as f64);
        }
        let board = FakeBoard::locked(&[("camp", Tier::Play, "smores")]);
        let remembered: Vec<String> = access.remembered.in_order().map(str::to_string).collect();
        let remembered: Vec<&str> = remembered.iter().map(String::as_str).collect();
        let mut session = unlock(&access, &board, &remembered);
        assert_eq!(board.failures(), 1);
        assert!(session.prompt.is_some());

        // The board drops the link and it reconnects: checked again, but
        // nothing is re-sent.
        session.observe(None);
        let second = LoginWindow {
            link: LinkId(2),
            hello_at: Millis(12_000),
        };
        session.observe(Some(second));
        let step = session
            .next_step(Millis(12_000), &access.held(), &remembered)
            .unwrap();
        assert_eq!(step, AccessStep::Check(second));
        session.started(&step);
        session.checked(second, true, None, true);
        assert_eq!(
            session.next_step(Millis(12_001), &access.held(), &remembered),
            None
        );
        assert_eq!(board.answers(), 1);
    }

    /// A USB connect adds exactly what is missing (and says what), then a
    /// reconnect with nothing to add is silent; Undo removes exactly those.
    #[test]
    fn a_usb_connect_adds_what_is_missing_then_is_silent_and_undo_removes_it() {
        let mut access = controller();
        access.account = Some(account(None));
        let account_key = account(None).held_keys()[0].key.clone();
        let board =
            FakeBoard::with_entries(vec![password_entry("friends", 1), account_key.entry(1)]);
        let device = DeviceId(3);

        sync(&mut access, &board, device, 1, 100.0);
        let added = access.access_added().expect("a toast").clone();
        assert_eq!(added.device, device);
        assert_eq!(added.names, ["Yona's MacBook"]);
        let labels = board_labels(&board);
        assert_eq!(labels, ["friends", "Yona's account", "Yona's MacBook"]);
        assert_eq!(board.store().secrets[2].added_at, Some(100));

        // Reconnect: nothing to add, no new toast.
        sync(&mut access, &board, device, 2, 200.0);
        assert_eq!(access.access_added().map(|a| a.generation), Some(1));
        assert_eq!(board.store().secrets.len(), 3);

        // Undo is only for the last add that added something; the silent
        // reconnect left the first add's Undo standing.
        let undo = access.undo_step(device, 300.0).expect("undo");
        run_change(&mut access, &board, device, undo);
        assert_eq!(board_labels(&board), ["friends", "Yona's account"]);
        assert!(access.access_added().is_none());
        assert!(access.undo_step(device, 301.0).is_none(), "taken once");
    }

    #[test]
    fn a_previous_account_key_is_removed_over_usb() {
        let mut access = controller();
        let keys = account(None);
        access.account = Some(keys.clone());
        let retired = InstallableKey {
            salt: keys.previous_key_salts[0],
            ..keys.held_keys()[0].key.clone()
        };
        let board = FakeBoard::with_entries(vec![retired.entry(1)]);
        sync(&mut access, &board, DeviceId(1), 1, 10.0);
        let store = board.store();
        assert!(
            store
                .secrets
                .iter()
                .all(|entry| entry.salt != keys.previous_key_salts[0])
        );
        assert!(
            store
                .secrets
                .iter()
                .any(|entry| entry.salt == keys.key_salt)
        );
        assert_eq!(
            access.access_added().unwrap().names,
            ["Yona's MacBook", "Yona's account"]
        );
    }

    #[test]
    fn a_rename_relabels_on_the_next_usb_connect_without_a_toast() {
        let mut access = controller();
        let board = FakeBoard::fresh();
        sync(&mut access, &board, DeviceId(1), 1, 10.0);
        assert_eq!(board_labels(&board), ["Yona's MacBook"]);
        let random = counter();
        let roster = Roster::new(Default::default());
        let effects = DeviceEffects::new();
        access.apply(
            AccessCommand::RenameBrowser("Studio laptop".to_string()),
            &roster,
            &effects,
            Millis(0),
            0.0,
            &random,
        );
        access.added = None;
        sync(&mut access, &board, DeviceId(1), 2, 20.0);
        assert_eq!(board_labels(&board), ["Studio laptop"]);
        assert!(access.access_added().is_none(), "a re-label is no add");
    }

    /// Two browsers, one board: the board merges, so neither erases the
    /// other's key.
    #[test]
    fn two_browsers_both_end_up_listed() {
        let board = FakeBoard::fresh();
        let mut first = controller();
        let mut second = AccessController::new();
        second.ensure_browser_key(&counter_from(100), "Yona's iPhone");
        sync(&mut first, &board, DeviceId(1), 1, 1.0);
        sync(&mut second, &board, DeviceId(1), 2, 2.0);
        assert_eq!(board_labels(&board), ["Yona's MacBook", "Yona's iPhone"]);
    }

    /// Over Bluetooth the list is read, never added to, and Bluetooth is
    /// not turned off from there.
    #[test]
    fn a_bluetooth_link_lists_but_never_adds_or_turns_itself_off() {
        let access = controller();
        let step = access.sync_step(window(1), false, 1.0);
        assert_eq!(
            step,
            AccessStep::Sync {
                window: window(1),
                held: Vec::new(),
                stale: Vec::new(),
                added_at: 1
            }
        );
        let random = counter();
        assert!(
            access
                .change_step(DeviceAccessChange::SetBluetooth(false), true, 1.0, &random)
                .unwrap_err()
                .contains("USB")
        );
        assert!(
            access
                .change_step(DeviceAccessChange::SetBluetooth(false), false, 1.0, &random)
                .is_ok()
        );
    }

    #[test]
    fn a_panel_password_is_added_as_a_password_entry_with_a_fresh_salt() {
        let access = controller();
        let random = counter_from(50);
        let step = access
            .change_step(
                DeviceAccessChange::AddPassword {
                    label: " friends ".to_string(),
                    tier: Tier::Play,
                    password: "pw".to_string(),
                },
                false,
                5.0,
                &random,
            )
            .unwrap();
        let AccessStep::Change { ops, .. } = &step else {
            panic!()
        };
        let [AccessOp::Add(key)] = ops.as_slice() else {
            panic!("{ops:?}")
        };
        assert_eq!(key.label, "friends");
        assert_eq!(key.kind, SecretKind::Password);
        assert_eq!(key.salt, [51; 16]);
        assert_eq!(key.iterations, DEFAULT_KDF_ITERATIONS);
    }

    /// The walk's finding: a restart can stamp a uid on a board first seen
    /// by MAC, and the record must still be found after it.
    #[test]
    fn a_record_is_keyed_on_the_mac_a_stamped_uid_does_not_move() {
        use lpa_devices::identity::{DeviceUid, IdentityChain, MacAddress};
        let before = IdentityChain {
            mac: Some(MacAddress("a0:f2:62:87:b4:8c".to_string())),
            ..Default::default()
        };
        let after = IdentityChain {
            uid: Some(DeviceUid("dev000000daqf6dvvqz".to_string())),
            ..before.clone()
        };
        assert_eq!(record_key(&before), record_key(&after));
        assert_eq!(
            record_key(&IdentityChain {
                uid: Some(DeviceUid("dev1".to_string())),
                ..Default::default()
            })
            .as_deref(),
            Some("dev1")
        );
    }

    /// A play unlock cannot change the list: the refusal is the sheet's
    /// sentence, never "failed".
    #[test]
    fn a_play_unlock_is_refused_a_change_by_name() {
        let board = FakeBoard::open(&[]);
        let mut client = board.client();
        let error = block_on(run_access_ops(&mut client, &[AccessOp::Remove([0; 16])], 1))
            .expect_err("play cannot change the list");
        assert!(error.contains("edit device password"), "{error}");
    }

    // --- helpers ----------------------------------------------------------

    fn window(link: u64) -> LoginWindow {
        LoginWindow {
            link: LinkId(link),
            hello_at: Millis(5),
        }
    }

    fn instant_timer() -> Timer {
        Rc::new(RefCell::new(|_delay: Duration| {
            Box::pin(core::future::ready(())) as DeviceTimerFuture
        }))
    }

    fn counter() -> impl Fn() -> [u8; SALT_BYTES] {
        counter_from(0)
    }

    fn counter_from(start: u8) -> impl Fn() -> [u8; SALT_BYTES] {
        let next = Cell::new(start);
        move || {
            next.set(next.get() + 1);
            [next.get(); SALT_BYTES]
        }
    }

    /// A controller whose browser key is named "Yona's MacBook".
    fn controller() -> AccessController {
        let mut access = AccessController::new();
        access.ensure_browser_key(&counter(), "Yona's MacBook");
        access
    }

    fn password_entry(label: &str, salt: u8) -> SecretEntry {
        SecretEntry::from_password(label, Tier::Play, b"x", [salt + 200; 16], 2)
    }

    fn board_labels(board: &FakeBoard) -> Vec<String> {
        board
            .store()
            .secrets
            .iter()
            .map(|entry| entry.label.clone())
            .collect()
    }

    /// Run a change on a USB link and apply its result.
    fn run_change(
        access: &mut AccessController,
        board: &FakeBoard,
        device: DeviceId,
        step: AccessStep,
    ) {
        let mut usb = board.usb();
        let result = block_on(run_step(
            &mut usb,
            device,
            step,
            &access.keys(),
            instant_timer(),
        ));
        assert!(
            matches!(&result, AccessCommand::Changed { result: Ok(_), .. }),
            "{result:?}"
        );
        let roster = Roster::new(Default::default());
        let effects = DeviceEffects::new();
        access.apply(result, &roster, &effects, Millis(0), 0.0, &counter());
    }

    /// Check, then the automatic unlock, on a fresh untrusted link.
    fn unlock(access: &AccessController, board: &FakeBoard, remembered: &[&str]) -> AccessSession {
        let mut session = AccessSession::default();
        session.observe(Some(window(1)));
        let held = access.held();
        let step = session.next_step(Millis(5), &held, remembered).unwrap();
        session.started(&step);
        let mut client = board.client();
        let checked = block_on(run_step(
            &mut client,
            DeviceId(1),
            step,
            &access.keys(),
            instant_timer(),
        ));
        let AccessCommand::Checked {
            result: Ok((required, granted)),
            ..
        } = checked
        else {
            panic!("{checked:?}")
        };
        session.checked(window(1), required, granted, true);
        let step = session.next_step(Millis(6), &held, remembered).unwrap();
        run_login_on(access, &mut client, &mut session, step, Millis(7));
        session
    }

    fn run_login(
        access: &AccessController,
        board: &FakeBoard,
        session: &mut AccessSession,
        step: AccessStep,
        now: Millis,
    ) {
        let mut client = board.client();
        run_login_on(access, &mut client, session, step, now);
    }

    fn run_login_on(
        access: &AccessController,
        client: &mut LpClient<FakeBoardIo>,
        session: &mut AccessSession,
        step: AccessStep,
        now: Millis,
    ) {
        session.started(&step);
        let done = block_on(run_step(
            client,
            DeviceId(1),
            step,
            &access.keys(),
            instant_timer(),
        ));
        let AccessCommand::LoggedIn {
            window,
            outcome,
            typed,
            ..
        } = done
        else {
            panic!("{done:?}")
        };
        session.logged_in(window, &outcome, typed.is_some(), now);
    }

    /// One USB connect's sync, applied back.
    fn sync(
        access: &mut AccessController,
        board: &FakeBoard,
        device: DeviceId,
        link: u64,
        now_secs: f64,
    ) {
        let step = access.sync_step(window(link), true, now_secs);
        let mut usb = board.usb();
        let result = block_on(run_step(
            &mut usb,
            device,
            step,
            &access.keys(),
            instant_timer(),
        ));
        let roster = Roster::new(Default::default());
        let effects = DeviceEffects::new();
        access.apply(result, &roster, &effects, Millis(0), now_secs, &counter());
    }
}
