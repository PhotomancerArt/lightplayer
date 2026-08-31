//! The roster: links, routing, records, and the top-level verbs.
//!
//! ```text
//! Roster ──── owns ────► links (dumb transports) + the router
//! │                      DeviceRecords (persisted identity + prefs)
//! │                      Journal (flight recorder, both streams)
//! └── owns ────► Device (one per known device)
//! ```
//!
//! **Pending links are roster state, not devices.** A link that arrives
//! unrouted becomes a [`PendingLink`] — the roster-level "new device found,
//! identifying…" affordance — with three exits: identity resolves to a known
//! record (route there, or MERGE if an anonymous entry already existed),
//! identity resolves to a stranger (create a device), or the user acts on a
//! still-anonymous link (a blank chip may never identify itself, so user
//! action must be a creation trigger).
//!
//! Routing is **revisable**: a link that reveals a different identity than
//! assumed is re-routed and the correction is journaled.
//!
//! Journaling rule: an input is recorded **once**, at the scope that owns
//! it. Routed inputs are recorded by the device (or pending link) that
//! handles them; unowned inputs are recorded at [`Scope::Roster`]. Derived
//! notes give the roster-wide timeline its structure.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::device::Device;
use crate::event::{Action, Command, Event, Input};
use crate::evidence::{Classification, Evidence};
use crate::identity::{DeviceId, IdentityChain, IdentityMatch};
use crate::journal::{EvictionReason, Journal, JournalNote, Scope};
use crate::link::{LinkCommand, LinkId, LinkInfo};
use crate::record::DeviceRecord;
use crate::time::{Millis, TimerAllocator, TimerId};

/// Every knob the model needs, supplied by the app. Deliberately no
/// constants baked into the fold: the wire proto comes from `lpc-wire`, and
/// the budgets are product decisions the app owns.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RosterConfig {
    /// The wire proto this build speaks. The app MUST set this from
    /// `lpc_wire::WIRE_PROTO_VERSION`; this crate hardcodes no proto number.
    pub expected_proto: u32,
    pub open_baud: u32,
    /// Budget from "port open" to a verdict. Mirrors `lpa-link`'s
    /// `DEFAULT_READY_DEADLINE`: boot can take seconds.
    pub identify_deadline_ms: u64,
    /// Re-ask cadence for `ClientRequest::Hello`, mirroring the shipped
    /// `HELLO_REQUEST_INTERVAL`.
    pub hello_request_interval_ms: u64,
    /// How long a cancelled activity gets to wind down before it is evicted.
    pub cancel_grace_ms: u64,
    /// Slack between an activity's own settle time and the supervision
    /// backstop, so a reducer that settles on time is never evicted first.
    pub supervision_slack_ms: u64,
    /// How many times a silent identify re-asks on its own before the card
    /// settles at "no response" (a fresh window each time). A replugged
    /// board answering on the second window shouldn't need a human retry.
    pub identify_auto_retries: u32,
    /// Supervision backstop for the whole Flash activity: the esptool write
    /// window (a minute or more at full images) plus the reconnect ladder.
    pub flash_deadline_ms: u64,
    /// Wind-down grace for a cancelled flash. Wide because esptool-js cannot
    /// abort a write cleanly — the reducer holds the cancel through the
    /// write window with an honest label; this is the bound on that hold.
    pub flash_cancel_grace_ms: u64,
    /// How long each rung of the post-flash reconnect ladder waits for the
    /// boot hello before escalating (reopen → Normal → BothThenDrop → fail).
    pub flash_rung_ms: u64,
    /// The retry/ask cadence inside a rung: reopen a closed port (session
    /// adoption absorbs a re-enumerated one) or re-ask a quiet open one.
    pub flash_reopen_retry_ms: u64,
    /// Supervision backstop for the whole Push activity: the `lpa-client`
    /// conversation (clear, chunked writes, load, hash) over a serial wire.
    pub push_deadline_ms: u64,
    /// Wind-down grace for a cancelled push. Wide for the same reason the
    /// flash's is: the conversation clears the device's project dir before
    /// it writes, so a cancel is held until the write window closes rather
    /// than leaving half a project on the board.
    pub push_cancel_grace_ms: u64,
    /// How long a finished push waits for the board to REPORT what it is
    /// running before settling anyway. Wider than a heartbeat period: the
    /// loaded-project fact rides heartbeats, and a push that succeeded must
    /// not be reported as anything else just because the board is unhurried.
    pub push_observe_ms: u64,
    /// Silence before freshness flips to quiet. Wider than two heartbeat
    /// periods on purpose: a lossy wire must not flap the timeline.
    pub quiet_after_ms: u64,
    pub journal_capacity: usize,
}

impl Default for RosterConfig {
    fn default() -> Self {
        Self {
            expected_proto: 1,
            open_baud: 921_600,
            identify_deadline_ms: 5_000,
            hello_request_interval_ms: 1_000,
            cancel_grace_ms: 2_000,
            supervision_slack_ms: 1_000,
            identify_auto_retries: 2,
            flash_deadline_ms: 240_000,
            flash_cancel_grace_ms: 180_000,
            flash_rung_ms: 8_000,
            flash_reopen_retry_ms: 1_000,
            push_deadline_ms: 180_000,
            push_cancel_grace_ms: 120_000,
            push_observe_ms: 8_000,
            quiet_after_ms: 12_000,
            journal_capacity: 512,
        }
    }
}

/// The shared, non-device state a fold needs: config to read, journal to
/// write, timer generations to mint.
pub(crate) struct ModelCtx<'a> {
    pub config: &'a RosterConfig,
    pub journal: &'a mut Journal,
    pub timers: &'a mut TimerAllocator,
}

/// A link the roster is still identifying.
///
/// Internally it carries a provisional [`Device`] so the crate has exactly
/// ONE fold and ONE supervision path. That is a deliberate reuse, not a
/// promotion: the entry is absent from [`Roster::devices`], never projects
/// as a device card, and only becomes a device through one of the three
/// exits above.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PendingLink {
    pub link: LinkId,
    pub info: LinkInfo,
    pub since: Millis,
    provisional: Device,
}

impl PendingLink {
    /// The fold's conclusions about this link so far.
    pub fn evidence(&self) -> &Evidence {
        &self.provisional.evidence
    }

    pub fn identity(&self) -> &IdentityChain {
        &self.provisional.identity
    }

    /// Whether identification is still running.
    pub fn is_identifying(&self) -> bool {
        self.provisional.is_busy()
    }

    /// The verdict, once identification has settled.
    pub fn verdict(&self) -> Option<&Classification> {
        if self.provisional.is_busy() || !self.provisional.evidence.is_settled() {
            return None;
        }
        Some(&self.provisional.evidence.classification)
    }

    /// The provisional device id, minted at discovery so adoption needs no
    /// re-keying.
    pub fn device_id(&self) -> DeviceId {
        self.provisional.id
    }
}

/// Links, routing, records, devices, journal.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Roster {
    state: RosterState,
    devices: Vec<Device>,
    pending: Vec<PendingLink>,
    links: BTreeMap<LinkId, LinkInfo>,
    routes: BTreeMap<LinkId, DeviceId>,
}

impl Roster {
    pub fn new(config: RosterConfig) -> Self {
        let journal = Journal::new(config.journal_capacity);
        Self {
            state: RosterState {
                config,
                journal,
                timers: TimerAllocator::default(),
                next_device_id: 0,
            },
            devices: Vec::new(),
            pending: Vec::new(),
            links: BTreeMap::new(),
            routes: BTreeMap::new(),
        }
    }

    /// Rehydrate persisted records at startup: each becomes a detached
    /// device, so a granted port can be re-matched to a device the user
    /// already named.
    pub fn load_records(&mut self, records: impl IntoIterator<Item = DeviceRecord>) {
        for record in records {
            self.state.next_device_id = self.state.next_device_id.max(record.device.0);
            self.devices.push(Device::from_record(record));
        }
    }

    pub fn config(&self) -> &RosterConfig {
        &self.state.config
    }

    pub fn journal(&self) -> &Journal {
        &self.state.journal
    }

    pub fn devices(&self) -> &[Device] {
        &self.devices
    }

    pub fn device(&self, id: DeviceId) -> Option<&Device> {
        self.devices.iter().find(|device| device.id == id)
    }

    pub fn pending(&self) -> &[PendingLink] {
        &self.pending
    }

    /// What the effects layer told us about a link that is still attached.
    pub fn link_info(&self, link: LinkId) -> Option<&LinkInfo> {
        self.links.get(&link)
    }

    /// The one entry point. Everything the app does to the device layer goes
    /// through here, and everything the model wants done comes back as
    /// commands.
    pub fn handle(&mut self, now: Millis, input: Input) -> Vec<Command> {
        let mut commands = match &input {
            Input::Action(action) => self.handle_action(now, action, &input),
            Input::Event(event) => self.handle_event(now, event, &input),
        };
        commands.extend(self.settle_pending(now));
        commands.extend(self.reconcile_identities(now));
        commands
    }

    fn handle_action(&mut self, now: Millis, action: &Action, input: &Input) -> Vec<Command> {
        match action {
            Action::AddFromUsb => {
                self.state.journal.record_input(now, Scope::Roster, input);
                vec![Command::RequestUsbGrant]
            }
            Action::AdoptLink { link } => {
                self.state
                    .journal
                    .record_input(now, Scope::PendingLink(*link), input);
                self.adopt_pending(now, *link)
            }
            Action::DismissLink { link } => {
                self.state
                    .journal
                    .record_input(now, Scope::PendingLink(*link), input);
                self.dismiss_pending(now, *link)
            }
            Action::Forget { device } => self.forget(now, *device, input),
            // Roster-level like AddFromUsb — the chooser is the only way
            // back to a board whose grant did not survive (a
            // serial-number-less bridge loses it on replug); the picked
            // port folds back into this device through the identity merge.
            Action::Reconnect { device } => {
                self.state
                    .journal
                    .record_input(now, Scope::Device(*device), input);
                vec![Command::RequestUsbGrant]
            }
            // Flashing a still-pending link adopts it first: writing our
            // firmware onto a board is the strongest possible "keep this
            // one", and the adopted entry is what renders progress, outcome
            // and every escape. The provisional id survives adoption, so the
            // same gesture then reaches the device it created.
            Action::Flash { device, .. } => match self.holder_of(*device) {
                Some(Holder::Pending(index)) => {
                    let mut commands = self.adopt_pending_entry(now, index);
                    commands.extend(self.dispatch_to_device(now, *device, input));
                    commands
                }
                Some(Holder::Device) => self.dispatch_to_device(now, *device, input),
                None => {
                    self.state.journal.record_input(now, Scope::Roster, input);
                    Vec::new()
                }
            },
            _ => {
                // Device-targeted gestures reach pending links too: their
                // provisional entry is a real fold with a real activity, so
                // "stop identifying this thing" must work there as well.
                match action
                    .device()
                    .map(|device| (device, self.holder_of(device)))
                {
                    Some((device, Some(Holder::Device))) => {
                        self.dispatch_to_device(now, device, input)
                    }
                    Some((_, Some(Holder::Pending(index)))) => {
                        self.dispatch_to_pending(now, index, input)
                    }
                    _ => {
                        // Journal it anyway: a gesture aimed at nothing is
                        // exactly the kind of thing a bug timeline needs.
                        self.state.journal.record_input(now, Scope::Roster, input);
                        Vec::new()
                    }
                }
            }
        }
    }

    fn handle_event(&mut self, now: Millis, event: &Event, input: &Input) -> Vec<Command> {
        match event {
            Event::LinkAttached { link, info } => self.attach_link(now, *link, info, input),
            Event::LinkDetached { link } => self.detach_link(now, *link, input),
            Event::Link { link, .. } => match self.owner_of(*link) {
                Some(Owner::Device(device)) => self.dispatch_to_device(now, device, input),
                Some(Owner::Pending(index)) => self.dispatch_to_pending(now, index, input),
                None => {
                    self.state.journal.record_input(now, Scope::Roster, input);
                    Vec::new()
                }
            },
            Event::TimerFired { timer } => self.dispatch_timer(now, *timer, input),
            Event::ActivityMarker { device, .. } | Event::IdentityObserved { device, .. } => {
                self.dispatch_to_device(now, *device, input)
            }
        }
    }

    fn attach_link(
        &mut self,
        now: Millis,
        link: LinkId,
        info: &LinkInfo,
        input: &Input,
    ) -> Vec<Command> {
        self.links.insert(link, info.clone());
        if self.owner_of(link).is_some() {
            self.state.journal.record_input(now, Scope::Roster, input);
            return Vec::new();
        }

        // A link for an endpoint we already track SUPERSEDES the previous
        // generation in place: USB re-enumeration (a C6 hard reset, a
        // physical replug) can kill the old transport without a farewell,
        // so the next link on the same endpoint is that device's next
        // generation — never a new discovery. Without this, every
        // re-enumeration mints another "new device found" card (G1
        // finding 2026-08-31: a replugged C6 wallpapered the gallery,
        // one card per boot slice).
        if let Some(index) = self.devices.iter().position(|device| {
            device.identity.endpoint.as_ref() == Some(&info.endpoint)
                && device.link().is_some_and(|held| held != link)
        }) {
            let device_id = self.devices[index].id;
            let old = self.devices[index].link().expect("checked above");
            self.links.remove(&old);
            self.routes.remove(&old);
            self.state
                .journal
                .record_input(now, Scope::Device(device_id), input);
            self.state.journal.note(
                now,
                Scope::Roster,
                JournalNote::LinkRouted {
                    link,
                    to: device_id,
                },
            );
            self.routes.insert(link, device_id);
            let Self { devices, state, .. } = self;
            let device = &mut devices[index];
            // `fold_only` deliberately skips eviction, so the dead
            // generation's activity is evicted here — its ground is gone.
            let mut commands = device.evict(now, EvictionReason::LinkLost, &mut state.ctx());
            commands.extend(device.fold_only(
                now,
                &Event::LinkDetached { link: old },
                &mut state.ctx(),
            ));
            commands.retain(|command| !addresses_link(command, old));
            commands.extend(device.fold_only(
                now,
                &Event::LinkAttached {
                    link,
                    info: info.clone(),
                },
                &mut state.ctx(),
            ));
            commands.extend(device.spawn_identify(now, &mut state.ctx()));
            return commands;
        }
        if let Some(index) = self
            .pending
            .iter()
            .position(|pending| pending.info.endpoint == info.endpoint)
        {
            let old = self.pending[index].link;
            self.links.remove(&old);
            self.state.journal.record_input(now, Scope::Roster, input);
            self.state.journal.note(
                now,
                Scope::Roster,
                JournalNote::PendingLinkDismissed { link: old },
            );
            self.state
                .journal
                .note(now, Scope::Roster, JournalNote::PendingLinkOpened { link });
            let Self { pending, state, .. } = self;
            let entry = &mut pending[index];
            // Same eviction note as the device branch above.
            let mut commands =
                entry
                    .provisional
                    .evict(now, EvictionReason::LinkLost, &mut state.ctx());
            commands.extend(entry.provisional.fold_only(
                now,
                &Event::LinkDetached { link: old },
                &mut state.ctx(),
            ));
            commands.retain(|command| !addresses_link(command, old));
            commands.extend(entry.provisional.fold_only(
                now,
                &Event::LinkAttached {
                    link,
                    info: info.clone(),
                },
                &mut state.ctx(),
            ));
            commands.extend(entry.provisional.spawn_identify(now, &mut state.ctx()));
            entry.link = link;
            entry.info = info.clone();
            return commands;
        }

        // A device already bound to this endpoint gets the link back. The
        // binding is a presumption, not proof: identification may re-route.
        let candidate = self.devices.iter().position(|device| {
            device.link().is_none() && device.identity.endpoint.as_ref() == Some(&info.endpoint)
        });

        if let Some(index) = candidate {
            let device_id = self.devices[index].id;
            self.state
                .journal
                .record_input(now, Scope::Device(device_id), input);
            self.state.journal.note(
                now,
                Scope::Roster,
                JournalNote::LinkRouted {
                    link,
                    to: device_id,
                },
            );
            self.routes.insert(link, device_id);
            let Self { devices, state, .. } = self;
            let device = &mut devices[index];
            let mut commands = device.fold_only(
                now,
                &Event::LinkAttached {
                    link,
                    info: info.clone(),
                },
                &mut state.ctx(),
            );
            commands.extend(device.spawn_identify(now, &mut state.ctx()));
            return commands;
        }

        // Nobody claims it: it becomes the roster's "new device found".
        self.state.journal.record_input(now, Scope::Roster, input);
        self.state
            .journal
            .note(now, Scope::Roster, JournalNote::PendingLinkOpened { link });
        let device_id = self.state.mint_device_id();
        let mut provisional = Device::new(device_id, IdentityChain::default());
        let commands = {
            let Self { state, .. } = self;
            let attached = Event::LinkAttached {
                link,
                info: info.clone(),
            };
            let mut commands = provisional.fold_only(now, &attached, &mut state.ctx());
            commands.extend(provisional.spawn_identify(now, &mut state.ctx()));
            commands
        };
        self.pending.push(PendingLink {
            link,
            info: info.clone(),
            since: now,
            provisional,
        });
        // `handle` settles pending links after every input, so an
        // already-identified link does not need a second pass here.
        commands
    }

    fn detach_link(&mut self, now: Millis, link: LinkId, input: &Input) -> Vec<Command> {
        self.links.remove(&link);
        match self.owner_of(link) {
            Some(Owner::Device(device)) => {
                self.routes.remove(&link);
                self.dispatch_to_device(now, device, input)
            }
            Some(Owner::Pending(index)) => {
                let mut commands = self.dispatch_to_pending(now, index, input);
                let pending = self.pending.remove(index);
                self.state.journal.note(
                    now,
                    Scope::Roster,
                    JournalNote::PendingLinkDismissed { link: pending.link },
                );
                commands.retain(|command| !addresses_link(command, link));
                commands
            }
            None => {
                self.state.journal.record_input(now, Scope::Roster, input);
                Vec::new()
            }
        }
    }

    fn dispatch_timer(&mut self, now: Millis, timer: TimerId, input: &Input) -> Vec<Command> {
        match timer.scope {
            Scope::Device(device) => match self.holder_of(device) {
                Some(Holder::Device) => self.dispatch_to_device(now, device, input),
                Some(Holder::Pending(index)) => self.dispatch_to_pending(now, index, input),
                None => Vec::new(),
            },
            Scope::PendingLink(link) => match self.pending_index(link) {
                Some(index) => self.dispatch_to_pending(now, index, input),
                None => Vec::new(),
            },
            // The roster keeps no timers of its own in M1.
            Scope::Roster => Vec::new(),
        }
    }

    fn dispatch_to_device(&mut self, now: Millis, id: DeviceId, input: &Input) -> Vec<Command> {
        let Self { devices, state, .. } = self;
        let Some(device) = devices.iter_mut().find(|device| device.id == id) else {
            return Vec::new();
        };
        device.handle(now, input, &mut state.ctx())
    }

    fn dispatch_to_pending(&mut self, now: Millis, index: usize, input: &Input) -> Vec<Command> {
        let Self { pending, state, .. } = self;
        let Some(entry) = pending.get_mut(index) else {
            return Vec::new();
        };
        entry.provisional.handle(now, input, &mut state.ctx())
    }

    /// Exit 1 and 2: a pending link whose identification produced an
    /// identity joins (or creates) a device.
    fn settle_pending(&mut self, now: Millis) -> Vec<Command> {
        let mut commands = Vec::new();
        let mut index = 0;
        while index < self.pending.len() {
            let settled = !self.pending[index].provisional.is_busy()
                && self.pending[index].provisional.evidence.is_settled();
            let identified = !self.pending[index].provisional.identity.is_anonymous();
            if !settled || !identified {
                index += 1;
                continue;
            }
            let entry = self.pending.remove(index);
            commands.extend(self.promote_pending(now, entry));
        }
        commands
    }

    fn promote_pending(&mut self, now: Millis, entry: PendingLink) -> Vec<Command> {
        let link = entry.link;
        let provisional = entry.provisional;
        let existing = self.devices.iter().position(|device| {
            match device.identity.match_against(&provisional.identity) {
                Some(IdentityMatch::Uid) | Some(IdentityMatch::Mac) => true,
                Some(IdentityMatch::Endpoint) | None => false,
            }
        });

        match existing {
            Some(index) => {
                let target = self.devices[index].id;
                self.state.journal.note(
                    now,
                    Scope::Roster,
                    JournalNote::DevicesMerged {
                        from: provisional.id,
                        into: target,
                    },
                );
                self.state.journal.note(
                    now,
                    Scope::Roster,
                    JournalNote::LinkRouted { link, to: target },
                );
                self.routes.insert(link, target);
                let mut commands = discard_record(&provisional);
                let device = &mut self.devices[index];
                absorb(device, provisional);
                commands.push(Command::PersistRecord(device.record_snapshot()));
                commands
            }
            None => {
                let device_id = provisional.id;
                self.state.journal.note(
                    now,
                    Scope::Roster,
                    JournalNote::DeviceCreated {
                        device: device_id,
                        from_record: false,
                    },
                );
                self.state.journal.note(
                    now,
                    Scope::Roster,
                    JournalNote::LinkRouted {
                        link,
                        to: device_id,
                    },
                );
                self.routes.insert(link, device_id);
                self.devices.push(provisional);
                let device = self.devices.last_mut().expect("just pushed");
                vec![Command::PersistRecord(device.record_snapshot())]
            }
        }
    }

    /// Exit 3: the user acts on a still-anonymous link. A blank chip may
    /// never identify itself, so this is a creation trigger in its own
    /// right.
    fn adopt_pending(&mut self, now: Millis, link: LinkId) -> Vec<Command> {
        let Some(index) = self.pending_index(link) else {
            return Vec::new();
        };
        let mut commands = self.adopt_pending_entry(now, index);
        let Self { devices, state, .. } = self;
        let device = devices.last_mut().expect("adoption just pushed");
        commands.extend(device.spawn_identify(now, &mut state.ctx()));
        commands
    }

    /// The adoption itself (exit 3's mechanics, shared with flash-adopts):
    /// promote the pending entry to a device, journal the creation, keep the
    /// route. What runs NEXT — identify for a plain adopt, the flash for a
    /// flash gesture — is the caller's.
    fn adopt_pending_entry(&mut self, now: Millis, index: usize) -> Vec<Command> {
        let entry = self.pending.remove(index);
        let link = entry.link;
        let mut provisional = entry.provisional;
        provisional.intent.setup_requested = true;
        provisional.intent.connection = crate::intent::ConnectionIntent::Connected;
        let device_id = provisional.id;
        self.state.journal.note(
            now,
            Scope::Roster,
            JournalNote::DeviceCreated {
                device: device_id,
                from_record: false,
            },
        );
        self.state.journal.note(
            now,
            Scope::Roster,
            JournalNote::LinkRouted {
                link,
                to: device_id,
            },
        );
        self.routes.insert(link, device_id);
        self.devices.push(provisional);
        let device = self.devices.last_mut().expect("just pushed");
        vec![Command::PersistRecord(device.record_snapshot())]
    }

    fn dismiss_pending(&mut self, now: Millis, link: LinkId) -> Vec<Command> {
        let Some(index) = self.pending_index(link) else {
            return Vec::new();
        };
        let mut entry = self.pending.remove(index);
        let mut commands = {
            let Self { state, .. } = self;
            entry
                .provisional
                .evict(now, EvictionReason::DeviceForgotten, &mut state.ctx())
        };
        self.state.journal.note(
            now,
            Scope::Roster,
            JournalNote::PendingLinkDismissed { link },
        );
        commands.push(Command::Link {
            link,
            command: LinkCommand::Close,
        });
        commands.push(Command::RevokeGrant(entry.info.clone()));
        self.links.remove(&link);
        commands
    }

    /// Delete the entry, its record, and its grant — from EVERY state,
    /// including mid-activity (evict first, then remove) and including an
    /// anonymous board, which the shipped system could never forget.
    fn forget(&mut self, now: Millis, id: DeviceId, input: &Input) -> Vec<Command> {
        let Some(index) = self.index_of(id) else {
            self.state.journal.record_input(now, Scope::Roster, input);
            return Vec::new();
        };
        self.state
            .journal
            .record_input(now, Scope::Device(id), input);

        let mut commands = {
            let Self { devices, state, .. } = self;
            devices[index].evict(now, EvictionReason::DeviceForgotten, &mut state.ctx())
        };
        let device = self.devices.remove(index);
        let link = device.link();
        self.routes.retain(|_, routed| *routed != id);
        self.state.journal.note(
            now,
            Scope::Roster,
            JournalNote::DeviceForgotten { device: id },
        );

        if let Some(link) = link {
            commands.retain(|command| !addresses_link(command, link));
            commands.push(Command::Link {
                link,
                command: LinkCommand::Close,
            });
            if let Some(info) = self.links.remove(&link) {
                commands.push(Command::RevokeGrant(info));
            }
        }
        commands.push(Command::DeleteRecord(id));
        commands
    }

    /// Two entries that turn out to be one device get merged, and the
    /// correction is journaled. This is what makes routing revisable: the
    /// anonymous entry the user adopted last week and the record-matched
    /// entry that just said hello are the same board.
    fn reconcile_identities(&mut self, now: Millis) -> Vec<Command> {
        let mut commands = Vec::new();
        loop {
            let Some((from_index, into_index)) = self.find_merge_pair() else {
                return commands;
            };
            let from = self.devices.remove(from_index);
            let into_index = if from_index < into_index {
                into_index - 1
            } else {
                into_index
            };
            let into_id = self.devices[into_index].id;
            self.state.journal.note(
                now,
                Scope::Roster,
                JournalNote::DevicesMerged {
                    from: from.id,
                    into: into_id,
                },
            );
            if let Some(link) = from.link() {
                let previous = self.routes.insert(link, into_id);
                if let Some(previous) = previous {
                    if previous != into_id {
                        self.state.journal.note(
                            now,
                            Scope::Roster,
                            JournalNote::LinkRerouted {
                                link,
                                from: previous,
                                to: into_id,
                            },
                        );
                    }
                }
            }
            commands.extend(discard_record(&from));
            let device = &mut self.devices[into_index];
            absorb(device, from);
            commands.push(Command::PersistRecord(device.record_snapshot()));
        }
    }

    /// The pair to merge, as `(from, into)` indices. `into` keeps its id and
    /// record: prefer the entry that has a record, else the older id.
    fn find_merge_pair(&self) -> Option<(usize, usize)> {
        for left in 0..self.devices.len() {
            for right in (left + 1)..self.devices.len() {
                let matched = self.devices[left]
                    .identity
                    .match_against(&self.devices[right].identity);
                let strong = matches!(matched, Some(IdentityMatch::Uid) | Some(IdentityMatch::Mac));
                if !strong {
                    continue;
                }
                let keep_left = match (
                    self.devices[left].record.is_some(),
                    self.devices[right].record.is_some(),
                ) {
                    (true, false) => true,
                    (false, true) => false,
                    _ => self.devices[left].id <= self.devices[right].id,
                };
                return if keep_left {
                    Some((right, left))
                } else {
                    Some((left, right))
                };
            }
        }
        None
    }

    fn index_of(&self, id: DeviceId) -> Option<usize> {
        self.devices.iter().position(|device| device.id == id)
    }

    fn pending_index(&self, link: LinkId) -> Option<usize> {
        self.pending.iter().position(|entry| entry.link == link)
    }

    /// Which collection holds the entry with this device id.
    fn holder_of(&self, device: DeviceId) -> Option<Holder> {
        if self.index_of(device).is_some() {
            return Some(Holder::Device);
        }
        self.pending
            .iter()
            .position(|entry| entry.provisional.id == device)
            .map(Holder::Pending)
    }

    fn owner_of(&self, link: LinkId) -> Option<Owner> {
        if let Some(device) = self.routes.get(&link) {
            return Some(Owner::Device(*device));
        }
        self.pending_index(link).map(Owner::Pending)
    }
}

/// Which collection an entry lives in.
enum Holder {
    Device,
    Pending(usize),
}

/// Who currently owns a link.
enum Owner {
    Device(DeviceId),
    Pending(usize),
}

/// Shared non-device state. Split out so a device fold can borrow the
/// journal and the timer allocator while the roster still owns its lists.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct RosterState {
    config: RosterConfig,
    journal: Journal,
    timers: TimerAllocator,
    next_device_id: u64,
}

impl RosterState {
    fn ctx(&mut self) -> ModelCtx<'_> {
        ModelCtx {
            config: &self.config,
            journal: &mut self.journal,
            timers: &mut self.timers,
        }
    }

    fn mint_device_id(&mut self) -> DeviceId {
        self.next_device_id += 1;
        DeviceId(self.next_device_id)
    }
}

/// Fold `from` into `into`: the surviving entry keeps its id and record and
/// takes the fresher entry's bindings, name, live evidence and activity.
fn absorb(into: &mut Device, from: Device) {
    into.identity.absorb(&from.identity);
    // The endpoint is a transport ADDRESS, not an identity: the entry that
    // carries the live link knows the current one (a re-granted port mints
    // a fresh endpoint — V3/CH340 replug, G1 2026-08-31), and a stale
    // binding would send the next arrival through another pending round.
    if from.link().is_some() && from.identity.endpoint.is_some() {
        into.identity.endpoint = from.identity.endpoint.clone();
    }
    if into.intent.name.is_none() {
        into.intent.name = from.intent.name.clone();
    }
    into.intent.autoconnect |= from.intent.autoconnect;
    into.intent.setup_requested |= from.intent.setup_requested;
    if from.link().is_some() || into.link().is_none() {
        into.evidence = from.evidence;
        into.activity = from.activity;
    }
}

/// Delete the merged-away entry's record, but only if it ever had one: a
/// pending link that never reached the store has nothing to delete, and a
/// spurious delete is a command the app has to defend against.
fn discard_record(device: &Device) -> Vec<Command> {
    match device.record.is_some() {
        true => vec![Command::DeleteRecord(device.id)],
        false => Vec::new(),
    }
}

fn addresses_link(command: &Command, link: LinkId) -> bool {
    matches!(command, Command::Link { link: addressed, .. } if *addressed == link)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{DeviceUid, EndpointKey, PeerIdentity};
    use crate::link::LinkEvent;
    use crate::wire::{HelloFacts, ServerFrame};

    #[test]
    fn an_unclaimed_link_becomes_a_pending_link_that_is_identifying() {
        let mut roster = Roster::new(RosterConfig::default());

        let commands = roster.handle(Millis(0), attach(LinkId(1), "usb-1"));

        assert_eq!(roster.pending().len(), 1);
        assert!(roster.devices().is_empty());
        assert!(roster.pending()[0].is_identifying());
        assert!(commands.iter().any(|command| matches!(
            command,
            Command::Link {
                command: LinkCommand::Open { .. },
                ..
            }
        )));
    }

    #[test]
    fn a_pending_link_that_hellos_becomes_a_device_with_a_record() {
        let mut roster = Roster::new(RosterConfig::default());
        roster.handle(Millis(0), attach(LinkId(1), "usb-1"));
        roster.handle(Millis(10), opened(LinkId(1), "usb-1"));

        let commands = roster.handle(
            Millis(20),
            hello(LinkId(1), &roster_proto(&roster), "dev_abc"),
        );

        assert!(roster.pending().is_empty());
        assert_eq!(roster.devices().len(), 1);
        assert_eq!(
            roster.devices()[0].identity.uid,
            Some(DeviceUid("dev_abc".to_string()))
        );
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, Command::PersistRecord(_)))
        );
    }

    #[test]
    fn a_known_endpoint_routes_straight_to_its_device() {
        let mut roster = Roster::new(RosterConfig::default());
        roster.load_records(vec![DeviceRecord::new(
            DeviceId(4),
            IdentityChain {
                endpoint: Some(EndpointKey("usb-1".to_string())),
                uid: Some(DeviceUid("dev_abc".to_string())),
                ..Default::default()
            },
        )]);

        roster.handle(Millis(0), attach(LinkId(1), "usb-1"));

        assert!(roster.pending().is_empty());
        assert_eq!(roster.devices().len(), 1);
        assert!(
            roster.devices()[0].is_busy(),
            "routing confirms by identifying"
        );
        assert!(
            roster
                .journal()
                .notes()
                .any(|(_, note)| matches!(note, JournalNote::LinkRouted { .. }))
        );
    }

    /// G1 bench, the V3/CH340 case (2026-08-31): a serial-number-less
    /// bridge loses its Web Serial grant on replug (Chrome cannot re-match
    /// the device), so recovery is re-granting through "Add a device" —
    /// which arrives as a link on a BRAND NEW endpoint. The hello identity
    /// must merge that into the known device: forget-then-re-add is never
    /// required.
    #[test]
    fn a_regranted_port_on_a_new_endpoint_merges_into_the_known_device() {
        let mut roster = Roster::new(RosterConfig::default());
        roster.load_records(vec![DeviceRecord::new(
            DeviceId(4),
            IdentityChain {
                endpoint: Some(EndpointKey("usb-1".to_string())),
                uid: Some(DeviceUid("dev_abc".to_string())),
                ..Default::default()
            },
        )]);

        // The re-grant mints a new session, so a new endpoint key.
        roster.handle(Millis(0), attach(LinkId(7), "usb-2"));
        assert_eq!(roster.pending().len(), 1, "unknown endpoint identifies");

        roster.handle(Millis(10), opened(LinkId(7), "usb-2"));
        roster.handle(
            Millis(20),
            hello(LinkId(7), &roster_proto(&roster), "dev_abc"),
        );

        assert!(roster.pending().is_empty(), "the hello identity settles it");
        assert_eq!(roster.devices().len(), 1, "merged, never duplicated");
        let device = &roster.devices()[0];
        assert_eq!(device.id, DeviceId(4), "the KNOWN entry survives");
        assert_eq!(device.link(), Some(LinkId(7)));
        assert_eq!(
            device.identity.endpoint,
            Some(EndpointKey("usb-2".to_string())),
            "the endpoint binding follows the new grant"
        );
        assert!(
            roster
                .journal()
                .notes()
                .any(|(_, note)| matches!(note, JournalNote::DevicesMerged { .. }))
        );
    }

    #[test]
    fn forget_works_mid_activity_and_gives_the_grant_back() {
        let mut roster = Roster::new(RosterConfig::default());
        roster.handle(Millis(0), attach(LinkId(1), "usb-1"));
        roster.handle(Millis(10), opened(LinkId(1), "usb-1"));
        roster.handle(
            Millis(20),
            hello(LinkId(1), &roster_proto(&roster), "dev_abc"),
        );
        let device = roster.devices()[0].id;
        roster.handle(Millis(30), Input::Action(Action::Identify { device }));
        assert!(roster.device(device).expect("device").is_busy());

        let commands = roster.handle(Millis(40), Input::Action(Action::Forget { device }));

        assert!(roster.devices().is_empty());
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, Command::DeleteRecord(_)))
        );
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, Command::RevokeGrant(_)))
        );
        assert!(
            roster
                .journal()
                .notes()
                .any(|(_, note)| matches!(note, JournalNote::DeviceForgotten { .. }))
        );
    }

    #[test]
    fn an_anonymous_entry_merges_into_the_record_matched_one() {
        let mut roster = Roster::new(RosterConfig::default());
        // A record for a board the user named last week, currently offline.
        roster.load_records(vec![DeviceRecord {
            name: Some("Kitchen".to_string()),
            ..DeviceRecord::new(
                DeviceId(1),
                IdentityChain {
                    uid: Some(DeviceUid("dev_abc".to_string())),
                    ..Default::default()
                },
            )
        }]);
        // The same board shows up on a port nobody has seen.
        roster.handle(Millis(0), attach(LinkId(9), "usb-9"));
        roster.handle(Millis(10), opened(LinkId(9), "usb-9"));
        roster.handle(
            Millis(20),
            hello(LinkId(9), &roster_proto(&roster), "dev_abc"),
        );

        assert_eq!(roster.devices().len(), 1, "one board, one entry");
        let device = &roster.devices()[0];
        assert_eq!(device.id, DeviceId(1), "the record-holding entry survives");
        assert_eq!(device.intent.name.as_deref(), Some("Kitchen"));
        assert!(device.evidence.classification.is_light_player());
        assert!(
            roster
                .journal()
                .notes()
                .any(|(_, note)| matches!(note, JournalNote::DevicesMerged { .. }))
        );
    }

    #[test]
    fn a_blank_link_stays_pending_until_the_user_adopts_it() {
        // Uses the replay runner because the verdict arrives on a timer the
        // model asked for, and the runner owns the virtual clock.
        let config = RosterConfig::default();
        let mut replay = crate::replay::Replay::new(config);
        replay.feed(Millis(0), attach(LinkId(1), "usb-1"));
        replay.feed(Millis(10), opened(LinkId(1), "usb-1"));
        replay.feed(
            Millis(20),
            Input::link(
                LinkId(1),
                LinkEvent::Line("invalid header: 0xffffffff".to_string()),
            ),
        );
        replay.advance_to(Millis(config.identify_deadline_ms + 100));

        assert_eq!(
            replay.roster().pending().len(),
            1,
            "anonymous: no identity to join"
        );
        assert_eq!(
            replay.roster().pending()[0].verdict(),
            Some(&Classification::Blank)
        );

        let commands = replay.feed(
            Millis(9_000),
            Input::Action(Action::AdoptLink { link: LinkId(1) }),
        );

        assert!(replay.roster().pending().is_empty());
        assert_eq!(replay.roster().devices().len(), 1);
        assert!(replay.roster().devices()[0].intent.setup_requested);
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, Command::PersistRecord(_)))
        );
    }

    #[test]
    fn dismissing_a_pending_link_closes_it_and_revokes_the_grant() {
        let mut roster = Roster::new(RosterConfig::default());
        roster.handle(Millis(0), attach(LinkId(1), "usb-1"));

        let commands = roster.handle(
            Millis(10),
            Input::Action(Action::DismissLink { link: LinkId(1) }),
        );

        assert!(roster.pending().is_empty());
        assert!(commands.iter().any(|command| matches!(
            command,
            Command::Link {
                command: LinkCommand::Close,
                ..
            }
        )));
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, Command::RevokeGrant(_)))
        );
    }

    /// G1 finding 2026-08-31: USB re-enumeration (C6 hard reset, physical
    /// replug) kills a link without a farewell, and every re-arrival used
    /// to mint ANOTHER pending card — a replugged board wallpapered the
    /// gallery. A link on a known endpoint supersedes in place.
    #[test]
    fn a_relinked_endpoint_supersedes_its_pending_link_instead_of_minting_another() {
        let mut roster = Roster::new(RosterConfig::default());
        roster.handle(Millis(0), attach(LinkId(1), "usb-1"));
        assert_eq!(roster.pending().len(), 1);

        // Three silent generations later (no LinkDetached ever arrived)...
        roster.handle(Millis(100), attach(LinkId(2), "usb-1"));
        let commands = roster.handle(Millis(200), attach(LinkId(3), "usb-1"));

        // ...still exactly one pending card, wearing the newest link, and
        // still identifying (the new generation re-opens and re-asks).
        assert_eq!(roster.pending().len(), 1);
        assert_eq!(roster.pending()[0].link, LinkId(3));
        assert!(roster.pending()[0].is_identifying());
        assert!(commands.iter().any(|command| matches!(
            command,
            Command::Link {
                link: LinkId(3),
                command: LinkCommand::Open { .. },
            }
        )));
        // Nothing still addresses the dead generations.
        assert!(!commands.iter().any(
            |command| addresses_link(command, LinkId(1)) || addresses_link(command, LinkId(2))
        ));
    }

    #[test]
    fn a_relinked_endpoint_supersedes_a_devices_dead_link_and_reidentifies() {
        let mut roster = Roster::new(RosterConfig::default());
        roster.handle(Millis(0), attach(LinkId(1), "usb-1"));
        roster.handle(Millis(10), opened(LinkId(1), "usb-1"));
        roster.handle(
            Millis(20),
            hello(LinkId(1), &roster_proto(&roster), "dev_abc"),
        );
        assert_eq!(roster.devices().len(), 1);

        // The device still holds LinkId(1) — it died silently. The next
        // generation on the same endpoint replaces it in place.
        let commands = roster.handle(Millis(100), attach(LinkId(2), "usb-1"));

        assert_eq!(roster.devices().len(), 1);
        assert!(roster.pending().is_empty());
        assert_eq!(roster.devices()[0].link(), Some(LinkId(2)));
        assert!(commands.iter().any(|command| matches!(
            command,
            Command::Link {
                link: LinkId(2),
                command: LinkCommand::Open { .. },
            }
        )));
        assert!(
            !commands
                .iter()
                .any(|command| addresses_link(command, LinkId(1)))
        );
    }

    fn roster_proto(roster: &Roster) -> HelloFacts {
        HelloFacts {
            proto: roster.config().expected_proto,
            ..Default::default()
        }
    }

    fn attach(link: LinkId, endpoint: &str) -> Input {
        Input::Event(Event::LinkAttached {
            link,
            info: info(endpoint),
        })
    }

    fn opened(link: LinkId, endpoint: &str) -> Input {
        Input::link(
            link,
            LinkEvent::Opened {
                info: info(endpoint),
            },
        )
    }

    fn hello(link: LinkId, facts: &HelloFacts, uid: &str) -> Input {
        let mut facts = facts.clone();
        facts.identity = PeerIdentity {
            uid: Some(DeviceUid(uid.to_string())),
            ..Default::default()
        };
        Input::link(link, LinkEvent::Frame(ServerFrame::hello(1, facts)))
    }

    fn info(endpoint: &str) -> LinkInfo {
        LinkInfo {
            label: endpoint.to_string(),
            endpoint: EndpointKey(endpoint.to_string()),
            usb: None,
            serial_number: None,
        }
    }
}
