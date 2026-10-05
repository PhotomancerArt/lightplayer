//! The studio's device sub-controller: one [`Roster`], one [`DeviceEffects`],
//! and the two things that join them to the app — the journal mirror and the
//! record writes.
//!
//! ```text
//!   StudioCommand::Device(Input) ─► DeviceRoster::handle ─► Roster::handle
//!                                        │                      │
//!                                        │              Vec<Command>
//!                                        │                      ▼
//!                                        │              DeviceEffects::apply
//!                                        ├─ journal lines ─► DeviceEventLog
//!                                        └─ PendingWrites ─► the registry
//! ```
//!
//! [`DeviceRoster::handle`] is synchronous from end to end. The record writes
//! it hands back are the one asynchronous step, and they are performed by the
//! controller AFTER the fold — never inside it (invariant I7).

use lpa_devices::event::{Command, Event, Input};
use lpa_devices::identity::{DeviceId, IdentityChain};
use lpa_devices::journal::Scope;
use lpa_devices::link::LinkId;
use lpa_devices::record::DeviceRecord;
use lpa_devices::roster::{Roster, RosterConfig};
use lpa_devices::time::Millis;
use lpa_devices::view::{DeviceView, Escape, RosterView, roster_view};

use crate::app::places::RegisteredDevice;

use super::device_effects::{DeviceEffects, PendingWrites};
use super::device_identity::device_identity_line;

/// One journal line on its way to the device event log.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalLine {
    /// The model's [`Scope`], rendered (`roster`, `device:3`, `pending-link:1`).
    pub scope: String,
    /// The entry itself, `Debug`-rendered. Deliberately not a parsed shape:
    /// this is a flight recorder, and its readers are forensics and tests.
    pub entry: String,
}

/// Everything the devices surface renders, plus why it might be empty.
///
/// [`Default`] is the honest pre-hydration shape: no devices, no ports, no
/// transport — what a host build and a first paint both have.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceRosterView {
    /// The model's projection — cards and pending links.
    pub roster: RosterView,
    /// Whether this build can reach real ports at all. `false` on a host
    /// build or a browser without Web Serial, and the page says so instead of
    /// showing an empty roster that looks like "no devices".
    pub transport_available: bool,
    /// Whether this browser can reach a USB port — Web Serial, or the
    /// `?emu=` shim that stands in for it. `false` on iPhone/iPad (Safari,
    /// Bluefy), Firefox and Safari, where the roster is still reachable
    /// (sims, Bluetooth) but the add slot must not offer the USB verb as its
    /// primary action: that verb could only fail there.
    ///
    /// Joined by the controller, which is what knows whether a serial
    /// transport was installed; the roster's own projection says `false`.
    pub usb_available: bool,
    /// Each registered device's editor address (round-2 M5): the model's
    /// handle → the registry uid `/device/<uid>` opens it by. A device
    /// without a row (still identifying) has no honest address and no Open.
    pub open_addresses: std::collections::BTreeMap<u64, String>,
    /// Each fed device's picture and its treatment, joined at the app view
    /// (frames are not evidence; the model's projection stays verbatim).
    /// Absent for a card with nothing honest to draw — it keeps its
    /// sentence.
    pub feeds: std::collections::BTreeMap<lpa_devices::DeviceId, super::DeviceCardFeedView>,
    /// Each sim-backed device's runtime band (PD11), joined here for the
    /// same reason the feeds are: the band is a fact about the RUNTIME
    /// behind a device, and the model deliberately does not know that a
    /// sim is a sim. Absent = a real board, which wears no band (D38).
    pub runtime_bands: std::collections::BTreeMap<lpa_devices::DeviceId, super::UiRuntimeBand>,
    /// Each device.s access facts (BLE M6): the login line over Bluetooth,
    /// and the device access panel where this link may write the store.
    pub access:
        std::collections::BTreeMap<lpa_devices::DeviceId, crate::app::access::UiDeviceAccess>,
    /// Each device's Wi‑Fi facts (the Connections group's Wi‑Fi row): the
    /// board's network status, read on a link that holds edit. Absent = no
    /// row (a sim, a held board, a Bluetooth link nothing unlocked).
    pub wifi: std::collections::BTreeMap<lpa_devices::DeviceId, crate::app::network::UiDeviceWifi>,
    /// Each device's layout facts (the C6 repartition): the question before
    /// its files move, the refusal, a board holding its files, a backup to
    /// put back. Absent = nothing to say.
    pub layout: std::collections::BTreeMap<lpa_devices::DeviceId, super::UiDeviceLayout>,
    /// The latest backup the user asked to download; the shell downloads
    /// when its `seq` advances.
    pub backup_download: Option<super::device_layout_effect::BackupDownload>,
}

impl Default for DeviceRosterView {
    fn default() -> Self {
        Self {
            roster: RosterView {
                devices: Vec::new(),
                pending: Vec::new(),
            },
            transport_available: false,
            usb_available: false,
            open_addresses: std::collections::BTreeMap::new(),
            feeds: std::collections::BTreeMap::new(),
            runtime_bands: std::collections::BTreeMap::new(),
            access: std::collections::BTreeMap::new(),
            wifi: std::collections::BTreeMap::new(),
            layout: std::collections::BTreeMap::new(),
            backup_download: None,
        }
    }
}

/// The page's split of [`DeviceRosterView::roster`] into cards worth
/// drawing and boards worth naming quietly underneath (D7: disconnect →
/// disappear).
#[derive(Clone, Debug, PartialEq)]
pub struct RosterSplit {
    /// Every device the page draws as a card, in roster order.
    pub connected: Vec<DeviceView>,
    /// Boards Studio remembers but cannot currently see — the "N remembered
    /// boards not connected" line's tiles.
    pub remembered: Vec<RememberedView>,
}

/// One tile in the remembered line: enough to name the board and offer its
/// two verbs, nothing else — an offline board draws no state zone, no
/// terminal, no activity, because it has none of those live.
#[derive(Clone, Debug, PartialEq)]
pub struct RememberedView {
    pub id: DeviceId,
    pub title: String,
    /// The board's catalog display name, or its raw id, or `None` when the
    /// board is not known at all yet — the same resolution the header's
    /// identity line uses ([`device_identity_line`]).
    pub board: Option<String>,
    /// "last heard 12 s ago", when this session ever saw the board live.
    /// `None` for a board rehydrated cold from the registry, which is
    /// honest: nothing here has heard it this session.
    pub last_seen_label: Option<String>,
    /// Reconnect + Forget, straight off the view's own projection — never
    /// re-derived, so the split can never offer an escape the model did not
    /// grant (invariant I3).
    pub escapes: Vec<Escape>,
    /// What this device IS, for the two verbs whose words depend on it: a
    /// powered-off SIM wears Power on in the Reconnect slot (Q5), because
    /// a runtime this tab makes has no port grant to ask back for.
    pub face: super::DeviceFace,
    /// The board's last picture, when this session pulled one before the
    /// port went away or a sidecar remembered one across a reload
    /// (`device_frame_snapshot`) — always `FeedLiveness::Offline` here,
    /// dimmed, "last frame · <age>". `None` keeps the tile's sentence.
    pub feed: Option<super::DeviceCardFeedView>,
}

/// Split a roster view into cards worth drawing and the quiet remembered
/// line underneath (D7). Connected order is preserved; remembered devices
/// keep the roster's own (last-seen-sorted) order too.
pub fn split_roster(roster: &DeviceRosterView) -> RosterSplit {
    let mut connected = Vec::new();
    let mut remembered = Vec::new();
    for device in &roster.roster.devices {
        if device.status == lpa_devices::device::DeviceStatus::Offline {
            remembered.push(RememberedView {
                id: device.id,
                title: device.title.clone(),
                board: device_identity_line(device).board,
                last_seen_label: device.freshness_label.clone(),
                escapes: device.escapes.clone(),
                face: match roster.runtime_bands.contains_key(&device.id) {
                    true => super::DeviceFace::Sim,
                    false => super::DeviceFace::Wire,
                },
                feed: roster.feeds.get(&device.id).cloned(),
            });
        } else {
            connected.push(device.clone());
        }
    }
    RosterSplit {
        connected,
        remembered,
    }
}

/// The [`Roster`] and its effects layer.
pub struct DeviceRoster {
    roster: Roster,
    effects: DeviceEffects,
    /// Journal entries already mirrored into the device event log, by the
    /// journal's own monotonic seq. The journal is a bounded ring, so a drain
    /// that falls behind skips what the ring dropped rather than replaying it.
    mirrored_through: u64,
    /// Which registry row each device's record lives in, by the model's
    /// handle. See [`Self::remember_key`].
    keys: std::collections::BTreeMap<u64, String>,
}

impl DeviceRoster {
    /// A roster with the app's config.
    ///
    /// ⚠️ Callers with a real transport must pass
    /// `lpa_link::device_link::wire::roster_config()`, not
    /// `RosterConfig::default()` — the default's `expected_proto` is a fixture
    /// value, and a build that speaks proto N must not call a proto-N device
    /// incompatible.
    pub fn new(config: RosterConfig) -> Self {
        Self {
            roster: Roster::new(config),
            effects: DeviceEffects::new(),
            keys: std::collections::BTreeMap::new(),
            mirrored_through: 0,
        }
    }

    pub fn effects_mut(&mut self) -> &mut DeviceEffects {
        &mut self.effects
    }

    pub fn effects(&self) -> &DeviceEffects {
        &self.effects
    }

    /// The registry uid a device's record lives under, when it has one — the
    /// address the editor lens opens it by (round-2 M5).
    pub fn key_for(&self, device: lpa_devices::DeviceId) -> Option<&str> {
        self.keys.get(&device.0).map(String::as_str)
    }

    /// The device whose record lives under `key` (the `/device/<uid>`
    /// address), when the roster holds it.
    pub fn device_for_key(&self, key: &str) -> Option<&lpa_devices::Device> {
        let id = self
            .keys
            .iter()
            .find(|(_, uid)| uid.as_str() == key)
            .map(|(id, _)| lpa_devices::DeviceId(*id))?;
        self.roster.device(id)
    }

    /// Whether the model still routes this link — false once the port died
    /// or was forgotten (the lens's unplug signal).
    pub fn link_is_routable(&self, link: lpa_devices::LinkId) -> bool {
        self.roster.link_info(link).is_some()
    }

    pub fn roster(&self) -> &Roster {
        &self.roster
    }

    /// Rehydrate the registry's rows as detached devices, so a board the user
    /// named last week has a card before its port is even open.
    ///
    /// The rows carry no endpoint (see `device_records`), so a granted port
    /// still arrives as a pending link and MERGES into its row once it says
    /// hello. That is the model's join, and it is revisable.
    ///
    /// **Idempotent.** The library re-hydrates on every settle (a save, another
    /// tab's transaction, a device row this roster just wrote), and loading a
    /// row the roster already holds would put a second card on screen for one
    /// board — the exact failure the rebuild exists to end. Rows already
    /// represented — by their registry key, or by the model's handle when
    /// the identities agree — are skipped.
    ///
    /// A row's `device_id` is a hint, not an identity: each page mints ids
    /// from 1, so two rows can wear the same one. The model re-keys such a
    /// row on load (`Roster::load_records`), and the key map follows the id
    /// it was ACTUALLY loaded under.
    ///
    /// A row that predates the model (`device_id` absent) takes the next id
    /// above every id this batch carries and every id the roster holds. Never
    /// a reserved high range: `Roster::load_records` raises its mint past the
    /// highest id it loads, so one id from up there dragged every later
    /// device's id up with it — and onto disk (the 2026-10-03 legacy-id
    /// ticket). Above the batch, no persisted row is re-keyed to make room;
    /// above the held ids, no live device is collided with.
    pub fn load_records(&mut self, rows: &[RegisteredDevice]) {
        let mut records: Vec<DeviceRecord> = Vec::new();
        let mut keys: Vec<String> = Vec::new();
        let mut next_legacy_id = self.highest_id_in_view(rows).saturating_add(1);
        for row in rows {
            if self.is_already_known(row) {
                continue;
            }
            let fallback = next_legacy_id;
            if row.device_id.is_none() {
                next_legacy_id = next_legacy_id.saturating_add(1);
            }
            records.push(super::device_records::record_from_registry_row(
                row, fallback,
            ));
            keys.push(row.uid.clone());
        }
        if records.is_empty() {
            return;
        }
        let loaded = self.roster.load_records(records);
        for (device, key) in loaded.into_iter().zip(keys) {
            self.keys.insert(device.0, key);
        }
    }

    /// The highest device id among `rows` and everything the roster holds
    /// (devices and pending links); 0 when there is none.
    fn highest_id_in_view(&self, rows: &[RegisteredDevice]) -> u64 {
        let persisted = rows.iter().filter_map(|row| row.device_id);
        let devices = self.roster.devices().iter().map(|device| device.id.0);
        let pending = self
            .roster
            .pending()
            .iter()
            .map(|entry| entry.device_id().0);
        persisted.chain(devices).chain(pending).max().unwrap_or(0)
    }

    /// Whether the roster already has an entry for this row.
    ///
    /// By KEY first: the device this roster loaded the row into or last
    /// persisted to it, or a device whose own identity keys to the row (a
    /// MAC-keyed row included — a uid-only comparison missed every board
    /// Studio flashes, which have no provisioned uid). By the model's handle
    /// only when the device wearing it does not contradict the row's
    /// identity: two boards whose rows share an id are two boards, and
    /// skipping the second hid its card and handed its frames and renames
    /// to the first (docs/defects/
    /// 2026-10-02-saved-records-sharing-a-device-id-misroute-the-board.md).
    fn is_already_known(&self, row: &RegisteredDevice) -> bool {
        let row_identity = super::device_records::record_from_registry_row(row, 0).identity;
        self.roster.devices().iter().any(|device| {
            if self.keys.get(&device.id.0) == Some(&row.uid)
                || super::device_records::registry_key(&device.identity).as_deref()
                    == Some(row.uid.as_str())
            {
                return true;
            }
            row.device_id == Some(device.id.0)
                && !identities_contradict(&device.identity, &row_identity)
        })
    }

    /// Remember which registry row a device's record was written to.
    ///
    /// The row key is the device's IDENTITY, not the model's handle — and by
    /// the time a `DeleteRecord` arrives the device is already gone from the
    /// fold, so there is nothing left to derive the key from. This is that
    /// memory, and nothing else reads it.
    pub fn remember_key(&mut self, device: lpa_devices::DeviceId, key: String) {
        self.keys.insert(device.0, key);
    }

    /// The registry row a device's record lives in, forgotten as it is taken.
    pub fn take_key(&mut self, device: lpa_devices::DeviceId) -> Option<String> {
        self.keys.remove(&device.0)
    }

    /// Fold one input and perform everything it asked for.
    ///
    /// Returns the journal lines this input produced, for the caller to mirror
    /// into the device event log (the caller owns the clock that stamps them).
    pub fn handle(&mut self, now: Millis, input: Input) -> Vec<JournalLine> {
        // Links that arrived from a spawned grant/sweep join the routing map
        // first, so the `LinkAttached` queued behind them is routable.
        self.effects.settle();
        let attached = match &input {
            Input::Event(Event::LinkAttached { link, .. }) => Some(*link),
            _ => None,
        };
        let commands = self.roster.handle(now, input);
        self.note_dropped_links(&commands);
        self.effects.apply(commands);
        // Only once a link's own attach has folded may the roster's silence
        // about it mean "let go" (see `DeviceEffects::retain_links`).
        if let Some(link) = attached {
            self.effects.attach_folded(link);
        }
        // The model is the authority on what is routed; anything it let go
        // stops being pumped.
        let roster = &self.roster;
        self.effects
            .retain_links(|link| roster.link_info(link).is_some());
        self.drain_journal()
    }

    /// Record writes the effects layer collected, for the controller to run
    /// against the library host.
    pub fn take_writes(&mut self) -> PendingWrites {
        self.effects.take_writes()
    }

    /// Sweep the grants this origin already holds (startup, and every
    /// `navigator.serial` connect).
    pub fn sweep_granted_ports(&mut self) {
        self.effects.sweep_granted_ports();
    }

    /// React to a `navigator.serial` disconnect.
    pub fn sweep_departed_ports(&mut self) {
        self.effects.sweep_departed_ports();
    }

    /// The projection the devices page renders.
    pub fn view(&self, now: Millis) -> DeviceRosterView {
        DeviceRosterView {
            roster: roster_view(&self.roster, now),
            transport_available: self.effects.is_wired(),
            // Joined by the controller (`device_roster_view`).
            usb_available: false,
            open_addresses: self.keys.clone(),
            // Filled by the controller, which owns the feeds and the
            // sidecars a band is read from.
            feeds: std::collections::BTreeMap::new(),
            runtime_bands: std::collections::BTreeMap::new(),
            access: std::collections::BTreeMap::new(),
            wifi: std::collections::BTreeMap::new(),
            // The verbs land in a scratch tree here; the studio view
            // publishes them for real (`publish_layout_offers`).
            layout: self.layout_views(now, &mut crate::UiOfferTree::new(), None),
            backup_download: self.effects.layout().download(),
        }
    }

    /// Publish every device card's layout verbs (`devices/<board>/…`: the
    /// question's Continue and Cancel, Download backup, Restore files,
    /// Finish update) into the view's offer tree — the same verbs, from the
    /// same decision, that [`Self::view`]'s layout facts name by path.
    /// `prefixes` is where the controller placed each device's verbs
    /// (`devices/<board>`), so these land beside the rest of its card's.
    pub fn publish_layout_offers(
        &self,
        now: Millis,
        offers: &mut crate::UiOfferTree,
        prefixes: &std::collections::BTreeMap<lpa_devices::DeviceId, crate::OfferPath>,
    ) {
        self.layout_views(now, offers, Some(prefixes));
    }

    /// The card's layout facts (C6 repartition) for every device with
    /// something to say: the question, the refusal, a board holding its
    /// files, a backup waiting to go back. Their verbs go into `offers`,
    /// under the device's prefix from `prefixes` when the controller placed
    /// one, else under its own [`crate::BoardRef`].
    fn layout_views(
        &self,
        now: Millis,
        offers: &mut crate::UiOfferTree,
        prefixes: Option<&std::collections::BTreeMap<lpa_devices::DeviceId, crate::OfferPath>>,
    ) -> std::collections::BTreeMap<lpa_devices::DeviceId, super::UiDeviceLayout> {
        let layout = self.effects.layout();
        self.roster
            .devices()
            .iter()
            .filter_map(|device| {
                let view = lpa_devices::view::device_view(device, now);
                let hello = device.evidence.classification.hello();
                let fs = hello.map(|hello| hello.fs).unwrap_or_default();
                let has_uid = hello.is_some_and(|hello| hello.identity.uid.is_some());
                let staged = layout.staged(device.id);
                let pending = device
                    .identity
                    .mac
                    .as_ref()
                    .and_then(|mac| layout.pending_for(&mac.0));
                let offers_at = prefixes
                    .and_then(|prefixes| prefixes.get(&device.id))
                    .cloned()
                    .unwrap_or_else(|| {
                        // No placement handed in: a board in a layout flow
                        // has said its MAC (the inspection probes it), so
                        // its ref is the controller's too. One that has not
                        // is `new-1` here, a prefix only this read uses.
                        crate::OfferPath::board(
                            &crate::BoardRef::known(&device.identity)
                                .unwrap_or(crate::BoardRef::New(1)),
                        )
                    });
                super::device_layout_view::device_layout_view(
                    &view,
                    offers_at,
                    fs,
                    has_uid,
                    staged.as_ref(),
                    pending.as_ref(),
                    offers,
                )
                .map(|ui| (device.id, ui))
            })
            .collect()
    }

    /// A `Close` for a link the model is releasing is the last thing that link
    /// will be asked to do; nothing else needs to know, but the log line is
    /// what makes an unexplained silent port explicable later.
    fn note_dropped_links(&self, commands: &[Command]) {
        for command in commands {
            if let Command::RevokeGrant(info) = command {
                log::debug!("handing the grant for {} back", info.label);
            }
        }
    }

    /// Journal entries appended since the last drain.
    fn drain_journal(&mut self) -> Vec<JournalLine> {
        let mut lines = Vec::new();
        let mut highest = self.mirrored_through;
        for entry in self.roster.journal().entries() {
            if entry.seq <= self.mirrored_through {
                continue;
            }
            highest = highest.max(entry.seq);
            lines.push(JournalLine {
                scope: scope_label(entry.scope),
                entry: format!("{:?}", entry.record),
            });
        }
        self.mirrored_through = highest;
        lines
    }
}

/// The model's scope as a stable, readable key.
fn scope_label(scope: Scope) -> String {
    match scope {
        Scope::Roster => "roster".to_string(),
        Scope::Device(device) => format!("device:{}", device.0),
        Scope::PendingLink(LinkId(link)) => format!("pending-link:{link}"),
    }
}

/// Whether two chains name different boards: a uid or a MAC both hold and
/// disagree on. Absence is not disagreement — a row written before the board
/// was provisioned has no uid, and that is the same board.
fn identities_contradict(left: &IdentityChain, right: &IdentityChain) -> bool {
    let uids_differ = matches!((&left.uid, &right.uid), (Some(a), Some(b)) if a != b);
    let macs_differ = matches!((&left.mac, &right.mac), (Some(a), Some(b)) if a != b);
    uids_differ || macs_differ
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_devices::event::Event;
    use lpa_devices::identity::EndpointKey;
    use lpa_devices::link::LinkInfo;

    fn info(endpoint: &str) -> LinkInfo {
        LinkInfo {
            label: endpoint.to_string(),
            endpoint: EndpointKey(endpoint.to_string()),
            usb: None,
            serial_number: None,
        }
    }

    #[test]
    fn a_fold_mirrors_its_journal_lines_once() {
        let mut roster = DeviceRoster::new(RosterConfig::default());

        let first = roster.handle(
            Millis(0),
            Input::Event(Event::LinkAttached {
                link: LinkId(1),
                info: info("usb-1"),
            }),
        );
        assert!(!first.is_empty(), "an attach is worth a timeline line");
        assert!(first.iter().any(|line| line.scope == "roster"), "{first:?}");

        let second = roster.handle(
            Millis(10),
            Input::Event(Event::LinkDetached { link: LinkId(1) }),
        );
        assert!(
            !second.iter().any(|line| first.contains(line)),
            "a drained line is never mirrored twice"
        );
    }

    /// The registry's own rows become cards before any port is open — which
    /// is the whole reason records exist.
    #[test]
    fn registry_rows_rehydrate_as_offline_devices() {
        let mut roster = DeviceRoster::new(RosterConfig::default());
        roster.load_records(&[RegisteredDevice {
            uid: "dev0000000000000001".to_string(),
            name: "Porch sign".to_string(),
            ..RegisteredDevice::default()
        }]);

        let view = roster.view(Millis(0));

        assert_eq!(view.roster.devices.len(), 1);
        assert_eq!(view.roster.devices[0].title, "Porch sign");
        assert_eq!(view.roster.devices[0].state_label, "Offline");
        assert!(
            !view.transport_available,
            "no seams installed: the page must say so rather than show an empty roster"
        );
    }

    /// Legacy rows (no `device_id`) take small ids, and so does every device
    /// minted after them: a reserved high range for legacy rows dragged the
    /// roster's mint up with it, and every later id landed — and was
    /// persisted — near `u64::MAX / 2`.
    #[test]
    fn a_roster_loaded_with_legacy_rows_mints_small_ids() {
        let mut roster = DeviceRoster::new(RosterConfig::default());
        roster.load_records(&[
            RegisteredDevice {
                uid: "dev0000000000000001".to_string(),
                ..RegisteredDevice::default()
            },
            RegisteredDevice {
                uid: "dev0000000000000002".to_string(),
                ..RegisteredDevice::default()
            },
        ]);
        roster.handle(
            Millis(0),
            Input::Event(Event::LinkAttached {
                link: LinkId(1),
                info: info("usb-1"),
            }),
        );

        let loaded: Vec<u64> = roster
            .roster()
            .devices()
            .iter()
            .map(|device| device.id.0)
            .collect();
        let minted: Vec<u64> = roster
            .roster()
            .pending()
            .iter()
            .map(|entry| entry.device_id().0)
            .collect();
        assert_eq!(loaded, vec![1, 2]);
        assert_eq!(minted, vec![3], "a new port's id follows the legacy rows");
        assert_eq!(roster.key_for(DeviceId(1)), Some("dev0000000000000001"));
        assert_eq!(roster.key_for(DeviceId(2)), Some("dev0000000000000002"));
    }

    /// A legacy row takes an id above every persisted one in its batch, so a
    /// persisted row later in the batch keeps the id it was saved with
    /// rather than being re-keyed out of its way.
    #[test]
    fn a_legacy_row_never_displaces_a_persisted_id() {
        let mut roster = DeviceRoster::new(RosterConfig::default());
        roster.load_records(&[
            RegisteredDevice {
                uid: "dev0000000000000001".to_string(),
                ..RegisteredDevice::default()
            },
            RegisteredDevice {
                uid: "dev0000000000000002".to_string(),
                device_id: Some(1),
                ..RegisteredDevice::default()
            },
        ]);

        assert_eq!(roster.key_for(DeviceId(1)), Some("dev0000000000000002"));
        assert_eq!(roster.key_for(DeviceId(2)), Some("dev0000000000000001"));
    }

    /// A record already saved with an id from the old legacy range still
    /// loads under that id and still resolves both ways — and loads once.
    #[test]
    fn a_record_saved_with_a_huge_id_still_loads_and_resolves() {
        const SAVED: u64 = u64::MAX / 2 + 1;
        let rows = [
            RegisteredDevice {
                uid: "dev0000000000000001".to_string(),
                name: "Porch sign".to_string(),
                device_id: Some(SAVED),
                ..RegisteredDevice::default()
            },
            RegisteredDevice {
                uid: "dev0000000000000002".to_string(),
                ..RegisteredDevice::default()
            },
        ];
        let mut roster = DeviceRoster::new(RosterConfig::default());
        roster.load_records(&rows);
        roster.load_records(&rows);

        assert_eq!(roster.roster().devices().len(), 2, "loaded once");
        assert_eq!(roster.key_for(DeviceId(SAVED)), Some("dev0000000000000001"));
        let device = roster
            .device_for_key("dev0000000000000001")
            .expect("the saved row resolves by its key");
        assert_eq!(device.id, DeviceId(SAVED));
        assert_eq!(
            device
                .record
                .as_ref()
                .and_then(|record| record.name.as_deref()),
            Some("Porch sign")
        );
        assert!(
            roster.key_for(DeviceId(SAVED + 1)).is_some(),
            "the legacy row sits beside it without colliding"
        );
    }

    /// Two boards whose rows wear the same model handle are two cards with
    /// two ids, each keyed to its own row — and re-hydrating (every library
    /// settle does) adds nothing.
    #[test]
    fn rows_sharing_a_handle_load_as_two_boards_and_stay_loaded_once() {
        let row = |mac: &str| RegisteredDevice {
            uid: format!("mac:{mac}"),
            hardware_id: Some(format!("efuse:{mac}")),
            device_id: Some(1),
            ..RegisteredDevice::default()
        };
        let rows = [row("02:00:00:00:00:01"), row("10:bd:a3:b0:8e:30")];
        let mut roster = DeviceRoster::new(RosterConfig::default());
        roster.load_records(&rows);
        roster.load_records(&rows);

        let ids: Vec<DeviceId> = roster
            .roster()
            .devices()
            .iter()
            .map(|device| device.id)
            .collect();
        assert_eq!(ids.len(), 2, "one card per board, loaded once: {ids:?}");
        assert_ne!(ids[0], ids[1]);
        assert_eq!(roster.key_for(ids[0]), Some("mac:02:00:00:00:00:01"));
        assert_eq!(roster.key_for(ids[1]), Some("mac:10:bd:a3:b0:8e:30"));
    }

    #[test]
    fn scopes_render_as_stable_keys() {
        assert_eq!(scope_label(Scope::Roster), "roster");
        assert_eq!(
            scope_label(Scope::Device(lpa_devices::DeviceId(3))),
            "device:3"
        );
        assert_eq!(scope_label(Scope::PendingLink(LinkId(1))), "pending-link:1");
    }

    /// Ticket 2026-09-27-busy-port-blocks-identify: two granted ports, one
    /// held by another process. The blocking half of this bug is already
    /// fixed on main (each link gets its own executor and its own Identify,
    /// so one that never opens cannot pause the other's) — this is its
    /// regression guard, straight from the model that proves it, so no
    /// future refactor can quietly re-couple the two links' identification.
    #[test]
    fn a_busy_ports_failed_open_does_not_delay_the_other_port() {
        use lpa_devices::replay::{Expect, Replay, Script, Step};
        use lpa_devices::roster::RosterConfig;

        let script = Script::new()
            .at(0, Step::attach(1, "usb-busy"))
            .at(0, Step::attach(2, "usb-lab"))
            // Link 1's port is held by another process: the open fails.
            .at(
                5,
                Step::Error {
                    link: 1,
                    message: "port busy".to_string(),
                },
            )
            // Link 2 opens and hellos cleanly, well inside identify's 5 s
            // deadline.
            .at(10, Step::opened(2))
            .at(20, Step::hello(2).uid("dev_lab"))
            .expect(Expect::new().devices(1).device_state("Ready").pending(1));

        Replay::new(RosterConfig::default())
            .run(&script.into_fixture("a busy port does not delay the other port"))
            .expect("scenario");
    }

    fn offline_view(id: u64, title: &str, board_id: Option<&str>) -> DeviceView {
        DeviceView {
            id: DeviceId(id),
            title: title.to_string(),
            status: lpa_devices::device::DeviceStatus::Offline,
            state_label: "Offline".to_string(),
            detail: None,
            freshness_label: Some("last heard 3 m ago".to_string()),
            identity_label: Some(format!("dev{id}")),
            detected_chip: None,
            board_id: board_id.map(str::to_string),
            firmware_face: lpa_devices::view::FirmwareFace::Unknown,
            remembered_firmware: None,
            degraded: None,
            loaded_project: lpa_devices::view::LoadedProject::Unknown,
            engine_fps: None,
            link_counters: None,
            can_receive_project: false,
            can_remove_project: false,
            activity: None,
            last_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            firmware_blocked: None,
            escapes: vec![Escape::Reconnect, Escape::Forget],
        }
    }

    fn ready_view(id: u64, title: &str) -> DeviceView {
        DeviceView {
            id: DeviceId(id),
            title: title.to_string(),
            status: lpa_devices::device::DeviceStatus::Ready,
            state_label: "Ready".to_string(),
            detail: None,
            freshness_label: None,
            identity_label: None,
            detected_chip: None,
            board_id: None,
            firmware_face: lpa_devices::view::FirmwareFace::Unknown,
            remembered_firmware: None,
            degraded: None,
            loaded_project: lpa_devices::view::LoadedProject::Empty,
            engine_fps: None,
            link_counters: None,
            can_receive_project: true,
            can_remove_project: false,
            activity: None,
            last_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            firmware_blocked: None,
            escapes: vec![Escape::Disconnect, Escape::Forget],
        }
    }

    /// D7: an offline device is a remembered tile, not a card — carrying its
    /// escapes verbatim (Reconnect + Forget, straight off the projection)
    /// and its board resolved the same way the header's identity line
    /// resolves one.
    #[test]
    fn split_roster_separates_offline_devices_into_remembered() {
        let view = DeviceRosterView {
            access: Default::default(),
            wifi: Default::default(),
            roster: RosterView {
                devices: vec![
                    ready_view(1, "Live board"),
                    offline_view(2, "Porch sign", Some("seeed/xiao-esp32-c6")),
                ],
                pending: Vec::new(),
            },
            transport_available: true,
            usb_available: true,
            open_addresses: Default::default(),
            // The remembered board's last picture (a sidecar across a
            // reload, or this session's last pull) rides the split.
            feeds: std::collections::BTreeMap::from([(
                DeviceId(2),
                super::super::DeviceCardFeedView {
                    frame: None,
                    frame_age_secs: Some(3_600.0),
                    engine_fps: None,
                    liveness: super::super::FeedLiveness::Offline,
                },
            )]),
            runtime_bands: std::collections::BTreeMap::new(),
            layout: std::collections::BTreeMap::new(),
            backup_download: None,
        };

        let split = split_roster(&view);

        assert_eq!(split.connected.len(), 1, "{split:?}");
        assert_eq!(split.connected[0].title, "Live board");

        assert_eq!(split.remembered.len(), 1, "{split:?}");
        let remembered = &split.remembered[0];
        assert_eq!(remembered.id, DeviceId(2));
        assert_eq!(remembered.title, "Porch sign");
        assert_eq!(
            remembered.board.as_deref(),
            Some("XIAO ESP32-C6"),
            "the same catalog resolution as the identity line"
        );
        assert_eq!(
            remembered.last_seen_label.as_deref(),
            Some("last heard 3 m ago")
        );
        assert_eq!(remembered.escapes, vec![Escape::Reconnect, Escape::Forget]);
        assert_eq!(
            remembered.feed.as_ref().map(|feed| feed.liveness),
            Some(super::super::FeedLiveness::Offline),
            "the last picture rides the split"
        );
    }

    /// Roster order (last-seen-sorted) survives the split for the cards that
    /// stay connected.
    #[test]
    fn split_roster_preserves_connected_order() {
        let view = DeviceRosterView {
            access: Default::default(),
            wifi: Default::default(),
            roster: RosterView {
                devices: vec![ready_view(1, "A"), ready_view(2, "B"), ready_view(3, "C")],
                pending: Vec::new(),
            },
            transport_available: true,
            usb_available: true,
            open_addresses: Default::default(),
            feeds: std::collections::BTreeMap::new(),
            runtime_bands: std::collections::BTreeMap::new(),
            layout: std::collections::BTreeMap::new(),
            backup_download: None,
        };

        let split = split_roster(&view);

        let titles: Vec<&str> = split
            .connected
            .iter()
            .map(|device| device.title.as_str())
            .collect();
        assert_eq!(titles, vec!["A", "B", "C"]);
        assert!(split.remembered.is_empty());
    }
}
