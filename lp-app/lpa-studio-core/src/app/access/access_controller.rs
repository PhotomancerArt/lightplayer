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
//! - **Wi-Fi unlock** (a board on the LAN, a KEYED link — Wi-Fi M6 P07): the
//!   link itself presents this browser's held keys
//!   ([`NetworkLinkKeys`], kept here); a board none of them opens comes up
//!   holding nothing, and the same session machine asks for its password —
//!   whose keys go to the link, which moves onto them (`keyed_login.rs`).
//!   Everything below that says "Bluetooth" about a link that must be
//!   unlocked holds for these too ([`is_untrusted`]).
//! - **Through lightplayer.app's relay** (a keyed link like the LAN's, the
//!   network transport's P05): the link presents the held keys only — no
//!   anonymous key, no typed password (`network_link_keys.rs`) — so it is
//!   either up on a tier one of them grants, or not up at all. Studio reads
//!   the tier and the list, and never logs in over it.
//! - **USB sync** (trusted link, once per connection): read the list, remove
//!   retired account keys, add the held keys it is missing (dropping the
//!   oldest other browser key when the device is full), and raise
//!   [`AccessAdded`] for the toast when anything was added (plan D6). A
//!   Bluetooth link unlocked at edit only reads the list. A sync that fails
//!   says why in the panel.
//! - **Changes** from the access panel (Play and Author,
//!   a key group's trash can, Bluetooth), and Undo.

use core::time::Duration;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::Rc;

use lpa_devices::identity::DeviceId;
use lpa_devices::link::LinkId;
use lpa_devices::time::Millis;
use lpa_devices::{Device, Roster};
use lpc_access::{MAX_SECRETS_PER_FILE, SALT_BYTES, Tier};

use super::access_added::AccessAdded;
use super::access_command::AccessCommand;
use super::access_session::{AccessPhase, AccessSession, AccessStep, LoginWindow, TypedPassword};
use super::account_keys::AccountKeys;
use super::browser_key::{BrowserKey, FALLBACK_BROWSER_NAME};
use super::device_access_ops::{AccessOp, run_access_ops, sync_access};
use super::device_access_record::{DeviceAccessChange, DeviceAccessRecords, SetHere};
use super::key_groups::key_groups;
use super::key_holder::{HeldKey, held_keys};
use super::keyed_login::try_keyed_login;
use super::login_attempt::{LoginAttemptOutcome, try_login};
use super::login_key_cache::LoginKeyCache;
use super::network_link_keys::{NetworkLinkKeys, link_key};
use super::remembered_passwords::RememberedPasswords;
use super::two_passwords::{device_password_salts, password_lines, plan_password};
use super::ui_access_view::{
    UiAccessPanel, UiDeviceAccess, UiLoginPrompt, UiPasswordLine, UiUnlockOffer, access_line,
    account_key_refused_sentence, dropped_sentence, prompt_sentence,
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

/// What a change in flight will leave behind once the device takes it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PendingChange {
    /// A password this browser is setting, to show again.
    pub(crate) set: Option<SetHere>,
    /// What to say once it lands.
    pub(crate) notice: Option<String>,
}

/// See the module doc.
pub struct AccessController {
    sessions: BTreeMap<DeviceId, AccessSession>,
    remembered: RememberedPasswords,
    records: DeviceAccessRecords,
    keys: Rc<RefCell<LoginKeyCache>>,
    /// The keys a secure network link presents (Wi-Fi): the held ones, and
    /// what passwords typed for a board derived. Shared with the LAN
    /// provider, which reads them on every connection.
    network_keys: NetworkLinkKeys,
    /// A password a keyed login put on a link, to remember once the link's
    /// next hello says it opened the board (the Bluetooth flow remembers on
    /// the answer; a keyed link has none).
    remember_on_grant: BTreeMap<DeviceId, String>,
    browser: Option<BrowserKey>,
    account: Option<AccountKeys>,
    writes: BTreeMap<DeviceId, WriteStatus>,
    /// The change in flight on each device, for when it lands.
    pending: BTreeMap<DeviceId, PendingChange>,
    /// What each device's last change did on its own, for the panel.
    notices: BTreeMap<DeviceId, String>,
    /// Devices whose last USB sync could not add the signed-in account's
    /// key, with the board's (or the room rule's) reason: without that key
    /// the board never reaches lightplayer.app, so the card says so.
    account_refused: BTreeMap<DeviceId, String>,
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
            network_keys: NetworkLinkKeys::new(),
            remember_on_grant: BTreeMap::new(),
            browser: None,
            account: None,
            writes: BTreeMap::new(),
            pending: BTreeMap::new(),
            notices: BTreeMap::new(),
            account_refused: BTreeMap::new(),
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

    /// The keys a secure network link presents (a handle on the one store).
    pub fn network_link_keys(&self) -> NetworkLinkKeys {
        self.network_keys.clone()
    }

    /// Fill the held keys in now, rather than at the first drive that sees
    /// a LAN board — so the first link a page opens already presents this
    /// browser's key instead of coming up anonymous and being rekeyed.
    pub fn refresh_held_network_keys(&mut self) {
        let held = self.held();
        self.refresh_network_keys(&held);
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

    /// Whether a Bluetooth (or Wi-Fi) link to `device` holds a tier (so the
    /// lens may attach: an untrusted link that has not unlocked is answered
    /// nothing but hello and login).
    pub fn link_is_granted(&self, device: &Device) -> bool {
        if !is_untrusted(device) {
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
        // Every pass, not only once a network board is in the roster: a link
        // through the relay presents held keys only, so it cannot say hello
        // (and join the roster) until the keys it needs are already handed
        // over — the account's, loaded after the page. Unchanged keys move
        // nothing (`NetworkLinkKeys::set_held`).
        self.refresh_network_keys(&held);
        for device in roster.devices() {
            self.watch_restart(device);
            let Some(window) = login_window(device) else {
                if is_untrusted(device) {
                    self.sessions.entry(device.id).or_default().observe(None);
                }
                continue;
            };
            if is_untrusted(device) {
                let address = lan_address(device).map(str::to_string);
                let held_only = is_relayed(device);
                self.drive_unlock(device.id, window, &held, address, held_only, effects, now);
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
        self.account_refused
            .retain(|device, _| live.contains(device));
    }

    /// `address`: the board's socket URL when its link is a KEYED one (a
    /// board on the LAN), whose login ends in keys for the link.
    /// `held_only`: a link through the relay, which holds what its held key
    /// granted and is never logged in over — only its tier is read.
    #[allow(
        clippy::too_many_arguments,
        reason = "one more fact about the link than the LAN needed; each is read once"
    )]
    fn drive_unlock(
        &mut self,
        device: DeviceId,
        window: LoginWindow,
        held: &[HeldKey],
        address: Option<String>,
        held_only: bool,
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
        if held_only && !matches!(step, AccessStep::Check(_)) {
            // No password, remembered or typed, goes through the relay (ND7).
            return;
        }
        let run = match (address, step.clone()) {
            (
                Some(address),
                AccessStep::Login {
                    window,
                    held,
                    passwords,
                    typed,
                    challenge,
                },
            ) => AccessStep::KeyedLogin {
                window,
                address,
                held,
                passwords,
                typed,
                challenge,
                link_keys: self.network_keys.clone(),
            },
            (_, step) => step,
        };
        if self.dispatch(device, window.link, run, effects)
            && let Some(session) = self.sessions.get_mut(&device)
        {
            session.started(&step);
        }
    }

    /// The held keys a network link presents on its own: those installed at
    /// one PBKDF2 iteration (this browser's, the account's), which cost
    /// nothing to derive. A human-password key (an account password, at its
    /// full cost) reaches a board through the keyed login instead, which
    /// derives it only for a board that offers its salt.
    fn refresh_network_keys(&mut self, held: &[HeldKey]) {
        let keys = held
            .iter()
            .filter(|key| key.key.iterations <= 1)
            .map(|key| {
                let offer = lpc_access::LoginOffer {
                    salt: key.key.salt,
                    iterations: key.key.iterations,
                };
                let (k, _) = self
                    .keys
                    .borrow_mut()
                    .key_for_material(&offer, &key.key.material);
                (key.holder, link_key(key.key.salt, &k))
            })
            .collect();
        self.network_keys.set_held(keys);
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
                self.network_keys.forget_typed();
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
            AccessCommand::Checked {
                device,
                window,
                result,
            } => {
                let anything_known = !self.remembered.is_empty() || !self.held().is_empty();
                if let Some(password) = self.remember_on_grant.remove(&device)
                    && matches!(result, Ok((_, Some(_))))
                {
                    self.remembered.remember(&password, now_secs);
                    self.persist_passwords();
                }
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
                if outcome == LoginAttemptOutcome::Rekeyed {
                    // A keyed link has no answer to remember on: hold the
                    // password until the link's next hello says it opened
                    // the board (`Checked`).
                    let asked = typed.as_ref().is_some_and(|typed| typed.remember);
                    let password = typed
                        .as_ref()
                        .map(|typed| typed.password.clone())
                        .or_else(|| passwords.first().cloned());
                    if let Some(password) = password
                        && (asked || self.remembered.in_order().any(|known| known == password))
                    {
                        self.remember_on_grant.insert(device, password);
                    }
                }
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
                    // When an add that did not happen is the account's key,
                    // the card says what that costs: the board cannot be
                    // reached through lightplayer.app.
                    let account_missing = self.account.as_ref().is_some_and(|account| {
                        !synced
                            .listing
                            .entries
                            .iter()
                            .any(|entry| entry.salt == account.key_salt)
                    });
                    if let Some(key) = roster
                        .device(device)
                        .and_then(|found| record_key(&found.identity))
                    {
                        self.records.record(&key, synced.listing, now_secs, None);
                        self.persist_devices();
                    }
                    // A refused add (a full device nothing can make room on)
                    // is said in the panel, under the list the board did
                    // answer; a sync that went through clears what an
                    // earlier one said.
                    match (&synced.refused, account_missing) {
                        (Some(why), true) => {
                            self.account_refused.insert(device, why.clone());
                        }
                        _ => {
                            self.account_refused.remove(&device);
                        }
                    }
                    match &synced.refused {
                        Some(why) => {
                            if !matches!(self.writes.get(&device), Some(WriteStatus::Writing)) {
                                self.writes.insert(device, WriteStatus::Failed(why.clone()));
                            }
                        }
                        None => {
                            if matches!(self.writes.get(&device), Some(WriteStatus::Failed(_))) {
                                self.writes.remove(&device);
                            }
                        }
                    }
                    if !synced.added.is_empty() {
                        self.undo
                            .insert(device, synced.added.iter().map(|a| a.salt).collect());
                        self.added_generation += 1;
                        self.added = Some(AccessAdded {
                            device,
                            names: synced.added.into_iter().map(|a| a.label).collect(),
                            dropped: synced.dropped,
                            generation: self.added_generation,
                        });
                    }
                }
                Err(error) => {
                    // The list itself was not read (an older firmware, a
                    // lost link). The panel says why and still shows what
                    // it last knew. (A full device is not this: its list
                    // arrives with the refusal, above.)
                    log::warn!("access: reading {device:?}'s list failed: {error}");
                    self.writes.insert(device, WriteStatus::Failed(error));
                }
            },
            AccessCommand::Changed {
                device,
                result,
                bluetooth,
            } => match result {
                Ok(changed) => {
                    self.writes.remove(&device);
                    let pending = self.pending.remove(&device).unwrap_or_default();
                    let notice = [pending.notice, dropped_sentence(&changed.dropped)]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" ");
                    if !notice.is_empty() {
                        self.notices.insert(device, notice);
                    }
                    let found = roster.device(device);
                    if let Some(key) = found.and_then(|found| record_key(&found.identity)) {
                        self.records
                            .record(&key, changed.listing, now_secs, pending.set);
                        self.persist_devices();
                    }
                    // Bluetooth applies at boot: over USB, Studio restarts
                    // the device itself (AC1).
                    if bluetooth.is_some() && found.is_some_and(|found| !is_untrusted(found)) {
                        return Some(self.restart(device, roster));
                    }
                }
                Err(error) => {
                    self.pending.remove(&device);
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
            keep: self.held_salts(),
        })
    }

    /// Salts never dropped to make room: every key this browser holds.
    fn held_salts(&self) -> Vec<[u8; SALT_BYTES]> {
        self.held().iter().map(HeldKey::salt).collect()
    }

    /// The step a panel change runs and what it leaves behind, or why it
    /// cannot run. `listing` is the device's last answer (a Play or Author
    /// change is planned against it).
    pub(crate) fn change_step(
        &self,
        change: DeviceAccessChange,
        listing: Option<&super::AccessListing>,
        over_bluetooth: bool,
        now_secs: f64,
        random: AccessRandom<'_>,
    ) -> Result<(AccessStep, PendingChange), String> {
        let mut pending = PendingChange::default();
        let (ops, bluetooth) = match change {
            DeviceAccessChange::Remove { salts } => {
                (salts.into_iter().map(AccessOp::Remove).collect(), None)
            }
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
            DeviceAccessChange::SetPassword { tier, password } => {
                let listing = listing
                    .ok_or_else(|| "the device has not listed its access yet".to_string())?;
                let plan = plan_password(
                    listing,
                    tier,
                    password.as_deref(),
                    random(),
                    self.account.as_ref(),
                )?;
                pending.set = plan.set.map(|(salt, password)| SetHere { salt, password });
                pending.notice = plan.notice.map(str::to_string);
                (plan.ops, None)
            }
        };
        Ok((
            AccessStep::Change {
                ops,
                added_at: epoch_secs(now_secs),
                bluetooth,
                keep: self.held_salts(),
            },
            pending,
        ))
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
        let found = roster.device(device);
        let over_bluetooth = found.is_some_and(is_bluetooth);
        let listing = found
            .and_then(|found| record_key(&found.identity))
            .and_then(|key| self.records.get(&key))
            .map(|record| record.listing.clone());
        self.notices.remove(&device);
        match self.change_step(change, listing.as_ref(), over_bluetooth, now_secs, random) {
            Ok((step, pending)) => {
                self.start_step(device, step, roster, effects);
                if self.writes.get(&device) == Some(&WriteStatus::Writing) {
                    self.pending.insert(device, pending);
                }
            }
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
        if is_untrusted(found) && self.granted_tier(device) != Some(Tier::Edit) {
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
        let untrusted = is_untrusted(device);
        let session = self.sessions.get(&device.id);
        let open = record_key(&device.identity)
            .and_then(|key| self.records.get(&key))
            .map(|record| record.listing.open);
        let line = untrusted
            .then(|| session.map_or(AccessPhase::Unknown, |s| s.phase.clone()))
            .and_then(|phase| {
                if !device.evidence.presence.is_open() {
                    return None;
                }
                // Granted with no name, on a board open at edit: say why.
                if matches!(phase, AccessPhase::Granted { label: None, .. })
                    && open == Some(lpc_access::OpenTo::Edit)
                {
                    return Some("Open — no password".to_string());
                }
                access_line(
                    &phase,
                    crate::UiLinkKind::of_endpoint(device.identity.endpoint.as_ref()),
                )
            });
        let unlock = session.and_then(|session| match &session.phase {
            AccessPhase::Locked => Some(UiUnlockOffer::Locked),
            AccessPhase::Granted {
                tier: Tier::Play, ..
            } => Some(UiUnlockOffer::PlayOnly),
            _ => None,
        });
        let panel = self.panel(device);
        if !untrusted && panel.is_none() {
            return None;
        }
        Some(UiDeviceAccess {
            over_bluetooth,
            line,
            unlock: unlock.filter(|_| untrusted),
            panel,
            account_key_refused: self
                .account_refused
                .get(&device.id)
                .map(|why| account_key_refused_sentence(why)),
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
        // A held board's list waits with its files (see `holds_its_files`):
        // it has none to show, and none may be started in RAM.
        if holds_its_files(device) {
            return None;
        }
        let over_bluetooth = endpoint.is_bluetooth();
        let untrusted = is_untrusted(device);
        if untrusted && self.granted_tier(device.id) != Some(Tier::Edit) {
            return None;
        }
        let key = record_key(&device.identity)?;
        let record = self.records.get(&key);
        let (writing, error) = match self.writes.get(&device.id) {
            Some(WriteStatus::Writing) => (true, None),
            Some(WriteStatus::Failed(error)) => (false, Some(error.clone())),
            None => (false, None),
        };
        let account = self.account.as_ref();
        let (play, author) = record
            .map_or((UiPasswordLine::NotSet, UiPasswordLine::NotSet), |record| {
                password_lines(record, account)
            });
        let keys = record
            .map(|record| {
                let listing = &record.listing;
                let passwords: Vec<_> = [Tier::Play, Tier::Edit]
                    .into_iter()
                    .flat_map(|tier| device_password_salts(listing, tier, account))
                    .collect();
                key_groups(
                    listing,
                    &passwords,
                    self.browser.as_ref().map(|key| key.salt),
                    account,
                )
            })
            .unwrap_or_default();
        Some(UiAccessPanel {
            device: device.id,
            open: record.map_or(lpc_access::OpenTo::Nobody, |record| record.listing.open),
            play,
            author,
            keys,
            used: record.map_or(0, |record| record.listing.entries.len()),
            capacity: MAX_SECRETS_PER_FILE,
            ble_enabled: record.map(|record| record.listing.ble_enabled),
            restart_pending: record.is_some_and(|record| record.restart_pending),
            can_restart: !untrusted,
            over_bluetooth,
            writing,
            error,
            notice: self.notices.get(&device.id).cloned(),
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

pub(crate) fn is_bluetooth(device: &Device) -> bool {
    device
        .identity
        .endpoint
        .as_ref()
        .is_some_and(|endpoint| endpoint.is_bluetooth())
}

/// The socket URL of a board reached on the LAN right now, or `None`.
pub(crate) fn lan_address(device: &Device) -> Option<&str> {
    let endpoint = device.identity.endpoint.as_ref()?;
    lpa_link::providers::network_link::url_from_lan_endpoint(&endpoint.0)
}

/// Whether `device` is reached through lightplayer.app's relay right now.
pub(crate) fn is_relayed(device: &Device) -> bool {
    device
        .identity
        .endpoint
        .as_ref()
        .is_some_and(|endpoint| endpoint.is_relay())
}

/// A link that must be unlocked before it may do anything: Bluetooth, a
/// board on the LAN, and one through the relay (keyed links). Physical
/// connection is access; a radio or the internet is not.
pub(crate) fn is_untrusted(device: &Device) -> bool {
    is_bluetooth(device) || lan_address(device).is_some() || is_relayed(device)
}

/// A trusted link to a LightPlayer board that is not a browser sim: physical
/// connection is access, so its connect adds this browser's keys.
fn syncs_over_usb(device: &Device) -> bool {
    let Some(endpoint) = device.identity.endpoint.as_ref() else {
        return false;
    };
    !endpoint.is_bluetooth()
        && lan_address(device).is_none()
        && !endpoint.is_relay()
        && !endpoint
            .0
            .starts_with(crate::app::devices::sim_record::SIM_ENDPOINT_PREFIX)
        && device.evidence.classification.is_light_player()
        && !holds_its_files(device)
}

/// A board holding its files for the C6 layout change (its hello's `fs` is
/// `legacy_held`): it runs on a RAM filesystem, and its real device store —
/// keys, switches — waits in the old region with every other file until
/// Finish update moves it. Studio neither reads nor writes access there: an
/// add would land in a store that exists only until the next reboot, and a
/// list read from it is not the board's (G1 rehearsal, 2026-10-03: "Who has
/// access 1" on a board whose own list held 16). The firmware keeps
/// Bluetooth off on such a board for the same reason.
pub(crate) fn holds_its_files(device: &Device) -> bool {
    device
        .evidence
        .classification
        .hello()
        .is_some_and(|hello| hello.fs == lpa_devices::wire::BoardFs::LegacyHeld)
}

/// The device's current connection window, when its link is open and has
/// said hello.
pub(crate) fn login_window(device: &Device) -> Option<LoginWindow> {
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
        AccessStep::KeyedLogin {
            window,
            address,
            held,
            passwords,
            typed,
            challenge,
            link_keys,
        } => {
            let outcome = try_keyed_login(
                client,
                &held,
                &passwords,
                challenge,
                keys,
                &link_keys,
                &address,
                |delay| (timer.borrow_mut())(delay),
            )
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
            keep,
        } => AccessCommand::Changed {
            device,
            result: run_access_ops(client, &ops, added_at, &keep).await,
            bluetooth,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::access::DroppedKey;
    use crate::app::access::PromptReason;
    use crate::app::access::account_keys::tests::account;
    use crate::app::access::key_holder::InstallableKey;
    use crate::app::access::login_key_cache::DEFAULT_KDF_ITERATIONS;
    use crate::app::access::test_board::{FakeBoard, FakeBoardIo, block_on};
    use lpa_client::LpClient;
    use lpc_access::{OpenTo, SecretEntry, SecretKind};
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

    /// Bluefy, 2026-10-02: every silent reconnect after the first unlock
    /// came up locked, the board dropped it at its unlock deadline, and the
    /// phone showed a native "disconnected" alert per lap. Each new link is
    /// unlocked with the browser's key again — one answer, no sheet.
    #[test]
    fn every_silent_reconnect_is_unlocked_again_with_the_browser_key() {
        let access = controller();
        let browser = access.browser_key().unwrap().installable();
        let board = FakeBoard::with_entries(vec![browser.entry(1)]);
        let mut session = unlock(&access, &board, &[]);
        let held = access.held();
        for link in 2..=4 {
            board.drop_link();
            session.observe(None);
            session.observe(Some(window(link)));
            let mut client = board.client();
            let step = session.next_step(Millis(5), &held, &[]).unwrap();
            assert_eq!(step, AccessStep::Check(window(link)));
            session.started(&step);
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
            assert_eq!(
                (required, granted),
                (true, None),
                "a new link holds nothing"
            );
            session.checked(window(link), required, granted, true);
            let step = session
                .next_step(Millis(6), &held, &[])
                .expect("the new link is unlocked, not left to time out");
            run_login_on(&access, &mut client, &mut session, step, Millis(7));
            assert_eq!(board.granted(), Some(Tier::Edit), "link {link}");
            assert_eq!(session.prompt, None, "link {link}: no sheet");
        }
        assert_eq!(board.answers(), 4, "one answer per link");
        assert_eq!(board.failures(), 0);
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

    /// An open board (anyone nearby may play) is Play with no sheet: the
    /// check says so and nothing is sent to log in. The board's own refusal
    /// of an edit is what would ask for a password (Bluefy, 2026-10-02;
    /// `?ble=emu` cannot show this, it is a trusted link).
    #[test]
    fn an_open_board_is_play_with_no_sheet_and_no_answer() {
        let access = controller();
        let board = FakeBoard::open(&[("camp", Tier::Play, "smores")], OpenTo::Play);
        let session = check_only(&access, &board, 1);
        assert_eq!(
            session.phase,
            AccessPhase::Granted {
                tier: Tier::Play,
                label: None
            }
        );
        assert_eq!(session.prompt, None, "no sheet on an open board");
        assert_eq!(board.answers(), 0, "no login was attempted");
        assert_eq!(
            session.next_step(Millis(6), &access.held(), &["smores"]),
            None
        );
    }

    /// Bluefy, 2026-10-02: a board locked at connect raised the sheet; the
    /// owner then opened it (Play: anyone nearby) over USB, and the next
    /// Bluetooth link's hello grants Play. The sheet must close, with no
    /// login sent.
    #[test]
    fn a_sheet_raised_on_a_locked_board_closes_when_the_board_is_opened() {
        let mut access = controller();
        let board = FakeBoard::locked(&[("camp", Tier::Play, "smores")]);
        let mut session = unlock(&access, &board, &[]);
        assert_eq!(session.prompt, Some(PromptReason::NoPasswordKnown));
        assert_eq!(board.granted(), None, "the board refuses an unknown link");

        // The owner opens the board to anyone nearby, over USB.
        let (step, _) = access
            .change_step(
                DeviceAccessChange::SetPassword {
                    tier: Tier::Play,
                    password: None,
                },
                Some(&list(&board)),
                false,
                5.0,
                &counter(),
            )
            .unwrap();
        run_change(&mut access, &board, DeviceId(1), step);
        assert_eq!(board.store().open, OpenTo::Play);

        // The Bluetooth link drops and comes back: the new hello grants Play.
        board.drop_link();
        session.observe(None);
        session.observe(Some(window(2)));
        let step = session.next_step(Millis(5), &access.held(), &[]).unwrap();
        assert_eq!(step, AccessStep::Check(window(2)));
        session.started(&step);
        let mut client = board.client();
        let AccessCommand::Checked {
            result: Ok((required, granted)),
            ..
        } = block_on(run_step(
            &mut client,
            DeviceId(1),
            step,
            &access.keys(),
            instant_timer(),
        ))
        else {
            panic!("the check failed")
        };
        session.checked(window(2), required, granted, true);
        assert_eq!(session.prompt, None, "the board opened; the sheet closes");
        assert!(matches!(
            session.phase,
            AccessPhase::Granted {
                tier: Tier::Play,
                ..
            }
        ));
        assert_eq!(board.answers(), 0, "no login was sent");
    }

    /// An edit refused at Play: the board's own `NotPermitted` is what
    /// raises the edit sheet (no sheet before it), and the author password
    /// typed there lifts the link to edit, so the same write lands.
    #[test]
    fn an_edit_refused_at_play_raises_the_edit_sheet_until_the_author_password_is_typed() {
        let mut access = controller();
        let board = FakeBoard::open(
            &[
                ("friends", Tier::Play, "play-pw"),
                ("Author password", Tier::Edit, "edit-pw"),
            ],
            OpenTo::Play,
        );
        let device = DeviceId(1);
        let session = check_only(&access, &board, 1);
        assert_eq!(session.prompt, None, "play needs no sheet");
        access.sessions.insert(device, session);
        let title = |_: DeviceId| "Choker".to_string();
        assert!(access.prompt(title).is_none());

        // The board refuses the edit, by its tier.
        let mut client = board.client();
        let path = lpc_model::LpPath::new("/projects/x.json");
        let refused = block_on(client.fs_write(path, b"{}".to_vec()));
        assert!(
            matches!(
                refused,
                Err(lpa_client::ClientError::NotPermitted { needs: Tier::Edit })
            ),
            "{refused:?}"
        );
        access.note_needs_edit(device);
        let sheet = access.prompt(title).expect("the refusal raises the sheet");
        assert_eq!(sheet.device, device);
        assert_eq!(
            sheet.reason,
            prompt_sentence(&PromptReason::NeedsEdit, "Choker")
        );

        // The author password lifts the link to edit; the sheet closes.
        let mut session = access.sessions.remove(&device).unwrap();
        session.type_password(TypedPassword {
            password: "edit-pw".to_string(),
            remember: false,
        });
        let step = session.next_step(Millis(20), &access.held(), &[]).unwrap();
        run_login_on(&access, &mut client, &mut session, step, Millis(21));
        assert_eq!(session.prompt, None);
        assert!(matches!(
            session.phase,
            AccessPhase::Granted {
                tier: Tier::Edit,
                ..
            }
        ));
        assert!(block_on(client.fs_write(path, b"{}".to_vec())).is_ok());
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

        // The board drops the link and it reconnects: checked again, and a
        // challenge is held for the sheet, but no password is re-sent.
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
        let hold = session
            .next_step(Millis(12_001), &access.held(), &remembered)
            .unwrap();
        run_login(&access, &board, &mut session, hold, Millis(12_002));
        assert_eq!(session.phase, AccessPhase::Locked);
        assert!(session.prompt.is_some(), "the sheet stays up");
        assert_eq!(
            session.next_step(Millis(12_003), &access.held(), &remembered),
            None,
            "nothing more on this window until a password is typed"
        );
        assert_eq!(board.answers(), 1);
        assert_eq!(board.failures(), 1);
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

    /// The relay's auto-queue ticket (`full-access-file-blocks-relay`): a
    /// board full of other browsers' keys still takes the account's key
    /// (the oldest browsers make room), so it can reach lightplayer.app; a
    /// board full of passwords cannot, and the card says so — in words that
    /// name what it costs — instead of the add failing where only the
    /// Access panel shows it. A later sync with room clears it.
    #[test]
    fn a_usb_connect_that_cannot_add_the_account_key_says_so_on_the_card() {
        let account_key = account(None).held_keys()[0].key.clone();

        // Sixteen other browsers: room is made, the account's key goes on.
        let browsers: Vec<SecretEntry> = (0..MAX_SECRETS_PER_FILE as u8)
            .map(|n| {
                BrowserKey::mint(
                    &counter_from(n.wrapping_mul(2).wrapping_add(40)),
                    "Brave on Mac",
                )
                .installable()
                .entry(u64::from(n) + 1)
            })
            .collect();
        let board = FakeBoard::with_entries(browsers);
        let mut access = controller();
        access.account = Some(account(None));
        sync(&mut access, &board, DeviceId(1), 1, 100.0);
        assert!(
            board
                .store()
                .secrets
                .iter()
                .any(|entry| entry.salt == account_key.salt),
            "the account's key made it on"
        );
        assert!(access.account_refused.is_empty());

        // Sixteen passwords: nothing may be dropped, and the card says why.
        let passwords: Vec<SecretEntry> = (0..MAX_SECRETS_PER_FILE as u8)
            .map(|n| password_entry("friends", n))
            .collect();
        let full = FakeBoard::with_entries(passwords);
        let mut access = controller();
        access.account = Some(account(None));
        sync(&mut access, &full, DeviceId(2), 1, 100.0);
        let said = access
            .account_refused
            .get(&DeviceId(2))
            .map(|why| account_key_refused_sentence(why))
            .expect("the card says the account's key did not go on");
        assert_eq!(
            said,
            "Your account's key couldn't be added, so this board can't be reached through \
             lightplayer.app. This device is full, and nothing on it can make room on its own \
             — remove something from its list."
        );

        // Signed out, the same board says nothing about an account.
        let mut signed_out = controller();
        sync(&mut signed_out, &full, DeviceId(2), 1, 100.0);
        assert!(signed_out.account_refused.is_empty());

        // Room made by hand: the next sync puts the key on and clears it.
        let two: Vec<AccessOp> = full.store().secrets[..2]
            .iter()
            .map(|entry| AccessOp::Remove(entry.salt))
            .collect();
        block_on(run_access_ops(&mut full.usb(), &two, 0, &[])).expect("two removed");
        sync(&mut access, &full, DeviceId(2), 2, 200.0);
        assert!(
            access.account_refused.is_empty(),
            "{:?}",
            access.account_refused
        );
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
                .change_step(
                    DeviceAccessChange::SetBluetooth(false),
                    None,
                    true,
                    1.0,
                    &random
                )
                .unwrap_err()
                .contains("USB")
        );
        assert!(
            access
                .change_step(
                    DeviceAccessChange::SetBluetooth(false),
                    None,
                    false,
                    1.0,
                    &random
                )
                .is_ok()
        );
    }

    /// The Play line's Password: the old play password goes, the typed one
    /// comes at a fresh salt, and the board stops letting anyone play.
    #[test]
    fn a_play_password_replaces_the_old_one_and_closes_the_board() {
        let access = controller();
        let board = FakeBoard::open(&[("friends", Tier::Play, "old")], OpenTo::Play);
        let listing = list(&board);
        let random = counter_from(50);
        let (step, pending) = access
            .change_step(
                DeviceAccessChange::SetPassword {
                    tier: Tier::Play,
                    password: Some("camp-glow-17".to_string()),
                },
                Some(&listing),
                false,
                5.0,
                &random,
            )
            .unwrap();
        assert_eq!(
            pending.set,
            Some(SetHere {
                salt: [51; 16],
                password: "camp-glow-17".to_string()
            })
        );
        let mut access = access;
        run_change(&mut access, &board, DeviceId(1), step);
        let store = board.store();
        assert_eq!(store.open, OpenTo::Nobody);
        assert_eq!(board_labels(&board), ["Play password"]);
        assert_eq!(store.secrets[0].salt, [51; 16]);
        assert_eq!(store.secrets[0].kind, SecretKind::Password);
        assert_eq!(store.secrets[0].iterations, DEFAULT_KDF_ITERATIONS);
        // The new password unlocks for play over Bluetooth.
        let session = unlock(&access, &board, &["camp-glow-17"]);
        assert!(matches!(
            session.phase,
            AccessPhase::Granted {
                tier: Tier::Play,
                ..
            }
        ));
    }

    /// Author to Anyone: the board opens at edit and both passwords go, and
    /// the panel says Play followed.
    #[test]
    fn author_anyone_opens_the_board_and_says_play_followed() {
        let access = controller();
        let board = FakeBoard::locked(&[
            ("Play password", Tier::Play, "a"),
            ("Author password", Tier::Edit, "b"),
        ]);
        let listing = list(&board);
        let (step, pending) = access
            .change_step(
                DeviceAccessChange::SetPassword {
                    tier: Tier::Edit,
                    password: None,
                },
                Some(&listing),
                false,
                5.0,
                &counter(),
            )
            .unwrap();
        assert_eq!(
            pending.notice.as_deref(),
            Some(super::super::two_passwords::PLAY_FOLLOWS_NOTICE)
        );
        let mut access = access;
        run_change(&mut access, &board, DeviceId(1), step);
        assert_eq!(board.store().open, OpenTo::Edit);
        assert!(board.store().secrets.is_empty());
        assert_eq!(board.granted(), Some(Tier::Edit), "anyone nearby authors");
    }

    /// The desk board's bug: every dev-server origin is its own browser, so
    /// sixteen fill the board. The seventeenth plug-in drops the oldest and
    /// says so, where it used to fail with only a log line.
    #[test]
    fn the_seventeenth_origin_drops_the_oldest_browser_and_says_so() {
        let board = FakeBoard::fresh();
        for origin in 0..17u8 {
            let mut access = AccessController::new();
            access.ensure_browser_key(&counter_from(origin * 2), "Brave on Mac");
            sync(
                &mut access,
                &board,
                DeviceId(1),
                u64::from(origin),
                1_000.0 + f64::from(origin),
            );
            let added = access.access_added().expect("each origin adds its own key");
            assert_eq!(added.names, ["Brave on Mac"]);
            if origin < 16 {
                assert!(added.dropped.is_empty(), "origin {origin}");
            } else {
                assert_eq!(
                    added.dropped,
                    [DroppedKey {
                        label: "Brave on Mac".to_string(),
                        added_at: Some(1_000),
                    }]
                );
            }
        }
        let store = board.store();
        assert_eq!(store.secrets.len(), MAX_SECRETS_PER_FILE);
        assert!(
            store
                .secrets
                .iter()
                .all(|entry| entry.added_at != Some(1_000))
        );
        assert!(
            store
                .secrets
                .iter()
                .any(|entry| entry.added_at == Some(1_016))
        );
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
        let board = FakeBoard::open(&[], OpenTo::Play);
        let mut client = board.client();
        let error = block_on(run_access_ops(
            &mut client,
            &[AccessOp::Remove([0; 16])],
            1,
            &[],
        ))
        .expect_err("play cannot change the list");
        assert!(error.contains("author device password"), "{error}");
    }

    /// Wi-Fi M6 P07: a locked board on the LAN. Its keyed link came up on
    /// the anonymous key holding nothing; the password typed in the sheet
    /// becomes keys for the link — no answer is sent (one never grants on a
    /// keyed link) — and the hello of the session the link rekeys onto is
    /// the login's outcome: granted, and the sheet closes.
    #[test]
    fn a_password_typed_for_a_lan_board_becomes_link_keys_and_its_next_hello_grants() {
        let access = controller();
        let board = FakeBoard::locked(&[("mine", Tier::Edit, "pw")]);
        let (mut session, outcome) = keyed_login_typed(&access, &board, "pw");

        assert_eq!(outcome, LoginAttemptOutcome::Rekeyed);
        assert_eq!(board.answers(), 0, "a keyed link is never answered");
        assert!(access.network_link_keys().has_typed(LAN_BOARD));
        assert_eq!(session.phase, AccessPhase::LoggingIn, "the sheet unlocks…");
        assert_eq!(session.typed, None);

        // The link rekeys: a new session, a new window, its hello grants.
        session.observe(Some(window(2)));
        let step = session.next_step(Millis(8), &access.held(), &[]).unwrap();
        assert_eq!(step, AccessStep::Check(window(2)));
        session.started(&step);
        session.checked(window(2), true, Some(Tier::Edit), true);
        assert!(
            matches!(
                session.phase,
                AccessPhase::Granted {
                    tier: Tier::Edit,
                    ..
                }
            ),
            "{:?}",
            session.phase
        );
        assert_eq!(session.prompt, None);
    }

    /// The same, with the wrong password: the new session still holds
    /// nothing, and the sheet says the password was refused rather than
    /// asking as if nothing had been tried.
    #[test]
    fn a_wrong_password_for_a_lan_board_reopens_the_sheet_as_refused() {
        let access = controller();
        let board = FakeBoard::locked(&[("mine", Tier::Edit, "pw")]);
        let (mut session, outcome) = keyed_login_typed(&access, &board, "nope");
        assert_eq!(outcome, LoginAttemptOutcome::Rekeyed);

        session.observe(Some(window(2)));
        let step = session.next_step(Millis(8), &access.held(), &[]).unwrap();
        session.started(&step);
        session.checked(window(2), true, None, true);

        assert_eq!(session.phase, AccessPhase::Locked);
        assert_eq!(
            session.prompt,
            Some(PromptReason::Refused { retry_after_ms: 0 })
        );
    }

    /// What a LAN link presents on its own is this browser's key, exactly as
    /// the board derives it from the entry that key installs: the salt as
    /// the key id, `link_psk(K)` as the PSK.
    #[test]
    fn a_lan_link_presents_this_browsers_key_as_the_board_derives_it() {
        use lpa_link::providers::network_link::LinkKeys;
        let mut access = controller();
        let browser = access.browser_key().unwrap().installable();
        let entry = browser.entry(1);

        access.refresh_held_network_keys();
        let presented = access.network_link_keys().keys_for(LAN_BOARD);

        assert_eq!(presented.len(), 1, "{presented:?}");
        assert_eq!(presented[0].key_id, entry.salt);
        assert_eq!(presented[0].psk, lpc_access::link_psk(&entry.k));
    }

    /// A link through the relay presents held keys only, so it can say
    /// hello — and join the roster — only once they are handed over: every
    /// drive hands them to the network links, with no network board in the
    /// roster yet, the account's first for the relay.
    #[test]
    fn held_keys_reach_the_relay_before_any_network_board_is_known() {
        use lpa_link::providers::network_link::LinkKeys as _;
        const RELAY: &str = "wss://lightplayer.app/relay/board/a0f26287b48c";
        let mut access = controller();
        let empty = Roster::new(Default::default());
        let effects = DeviceEffects::new();
        access.drive(&empty, &effects, Millis(0), 1.0, &counter());
        let browser = access.browser_key().unwrap().installable().salt;
        let ids = |access: &AccessController| -> Vec<[u8; SALT_BYTES]> {
            access
                .network_link_keys()
                .keys_for(RELAY)
                .iter()
                .map(|key| key.key_id)
                .collect()
        };
        assert_eq!(ids(&access), vec![browser]);

        // The account's key loads after the page: the next drive hands it
        // over, ahead of the browser's.
        access.account = Some(account(None));
        access.drive(&empty, &effects, Millis(1), 2.0, &counter());
        let account_salt = account(None).held_keys()[0].key.salt;
        assert_eq!(ids(&access), vec![account_salt, browser]);
    }

    /// The board on the LAN the keyed-login tests address.
    const LAN_BOARD: &str = "ws://10.0.0.5/link";

    /// A locked LAN board's first window checked locked, a password typed in
    /// the sheet, and the keyed login the controller runs for it (the
    /// session plans a `Login`; `drive_unlock` makes it a `KeyedLogin` for a
    /// `lan:` device).
    fn keyed_login_typed(
        access: &AccessController,
        board: &FakeBoard,
        password: &str,
    ) -> (AccessSession, LoginAttemptOutcome) {
        let held = access.held();
        let mut session = AccessSession::default();
        session.observe(Some(window(1)));
        session.started(&AccessStep::Check(window(1)));
        session.checked(window(1), true, None, true);
        session.type_password(TypedPassword {
            password: password.to_string(),
            remember: false,
        });
        let step = session.next_step(Millis(6), &held, &[]).expect("a login");
        let AccessStep::Login {
            window: at,
            held: offered,
            passwords,
            typed,
            challenge,
        } = step.clone()
        else {
            panic!("{step:?}")
        };
        session.started(&step);
        let keyed = AccessStep::KeyedLogin {
            window: at,
            address: LAN_BOARD.to_string(),
            held: offered,
            passwords,
            typed,
            challenge,
            link_keys: access.network_link_keys(),
        };
        let mut client = board.client();
        let AccessCommand::LoggedIn { outcome, .. } = block_on(run_step(
            &mut client,
            DeviceId(1),
            keyed,
            &access.keys(),
            instant_timer(),
        )) else {
            panic!("a keyed login ends as a login")
        };
        session.logged_in(window(1), &outcome, true, Millis(7));
        (session, outcome)
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

    /// Only the check on a fresh untrusted link `link`: what the board's
    /// hello says, with nothing sent after it.
    fn check_only(access: &AccessController, board: &FakeBoard, link: u64) -> AccessSession {
        let mut session = AccessSession::default();
        session.observe(Some(window(link)));
        let step = session.next_step(Millis(5), &access.held(), &[]).unwrap();
        session.started(&step);
        let mut client = board.client();
        let AccessCommand::Checked {
            result: Ok((required, granted)),
            ..
        } = block_on(run_step(
            &mut client,
            DeviceId(1),
            step,
            &access.keys(),
            instant_timer(),
        ))
        else {
            panic!("the check failed")
        };
        session.checked(window(link), required, granted, true);
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

    /// The board's list, read over USB.
    fn list(board: &FakeBoard) -> super::super::AccessListing {
        let mut usb = board.usb();
        block_on(run_access_ops(&mut usb, &[], 0, &[]))
            .unwrap()
            .listing
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
