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

use lpa_devices::event::{Command, Input};
use lpa_devices::journal::Scope;
use lpa_devices::link::LinkId;
use lpa_devices::record::DeviceRecord;
use lpa_devices::roster::{Roster, RosterConfig};
use lpa_devices::time::Millis;
use lpa_devices::view::{RosterView, roster_view};

use crate::app::places::RegisteredDevice;

use super::device_effects::{DeviceEffects, PendingWrites};

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
}

impl Default for DeviceRosterView {
    fn default() -> Self {
        Self {
            roster: RosterView {
                devices: Vec::new(),
                pending: Vec::new(),
            },
            transport_available: false,
        }
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
    /// Ids handed to registry rows that predate the model (`device_id`
    /// absent). Counted down from a high base so it can never collide with
    /// the roster's own minting, which starts at 1.
    next_legacy_id: u64,
}

/// Where legacy registry rows' device ids start.
///
/// The roster mints from 1 upward and `load_records` raises its counter to
/// the highest id it loads — so a legacy row taking an id from up here would
/// push every future mint above it. Counting DOWN keeps both ranges apart
/// without either side knowing about the other.
const LEGACY_ID_BASE: u64 = u64::MAX / 2;

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
            next_legacy_id: LEGACY_ID_BASE,
        }
    }

    pub fn effects_mut(&mut self) -> &mut DeviceEffects {
        &mut self.effects
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
    /// represented, by the model's handle or by uid, are skipped.
    pub fn load_records(&mut self, rows: &[RegisteredDevice]) {
        let mut records: Vec<DeviceRecord> = Vec::new();
        for row in rows {
            if self.is_already_known(row) {
                continue;
            }
            let fallback = self.next_legacy_id;
            self.next_legacy_id = self.next_legacy_id.saturating_add(1);
            let record = super::device_records::record_from_registry_row(row, fallback);
            self.keys.insert(record.device.0, row.uid.clone());
            records.push(record);
        }
        if records.is_empty() {
            return;
        }
        self.roster.load_records(records);
    }

    /// Whether the roster already has an entry for this row.
    fn is_already_known(&self, row: &RegisteredDevice) -> bool {
        self.roster.devices().iter().any(|device| {
            row.device_id == Some(device.id.0)
                || device
                    .identity
                    .uid
                    .as_ref()
                    .is_some_and(|uid| uid.0 == row.uid)
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
        let commands = self.roster.handle(now, input);
        self.note_dropped_links(&commands);
        self.effects.apply(commands);
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
        }
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

    /// Legacy rows (no `device_id`) get ids from a range the roster's own
    /// minting never reaches, so a hello that creates a device cannot collide
    /// with a rehydrated one.
    #[test]
    fn legacy_rows_take_ids_the_roster_will_never_mint() {
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

        let ids: Vec<u64> = roster
            .roster()
            .devices()
            .iter()
            .map(|device| device.id.0)
            .collect();

        assert_eq!(ids, vec![LEGACY_ID_BASE, LEGACY_ID_BASE + 1]);
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
}
