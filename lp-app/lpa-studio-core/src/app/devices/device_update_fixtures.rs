//! Fixtures for a board's firmware-update story: the real inputs the
//! standing is read from — a board manifest as the board reports it, this
//! Studio's build facts, the device view with its Update activity — for
//! this crate's tests and for the web's stories (feature `story-fixtures`).
//!
//! A story built from these calls the same [`update_standing`],
//! [`update_words`], [`update_offers`] and [`update_session_words`] the
//! controller does, so a card in a story says and offers exactly what core
//! decides, and cannot drift from it. Nothing here decides anything: it
//! only builds inputs.
//!
//! The sample board is the update-states spike's: on X `2026.10.03-1`,
//! with this Studio carrying Y `2026.10.05-2`. The store's release index
//! ([`release_index`]) lists fourteen releases over five days, X and Y
//! among them, the oldest five in an older wire language than this
//! Studio's.
//!
//! [`update_standing`]: super::device_update_standing::update_standing
//! [`update_offers`]: super::device_update_offers::update_offers

use lpa_devices::view::{ActivityView, DeviceView, Escape};
use lpa_devices::{
    ActivityKind, UpdateActivityView, UpdateBoardState, UpdateFacts, UpdateIntentFacts,
    UpdateOutcomeFacts, UpdateStageFacts,
};
use lpa_update::{HostBuildFacts, HostIdentity, HostPieceFacts};
use lpc_access::Tier;
use lpc_firmware_release::{
    OtaManifest, PackageRef, PieceFile, ReleaseIndex, ReleaseIndexEntry, Requires, TargetName,
    sha256_hex,
};
use lpc_update::{BoardManifest, BoardState, PieceKind, TransferView, sha256_to_hex};

use super::device_identity::device_chip;
use super::device_update_offers::UpdateOfferFacts;
use super::device_update_route::UpdateLink;
use super::device_update_standing::UpdateStandingInputs;
use super::device_update_words::{
    UiDeviceUpdate, UiSessionUpdate, update_session_words, update_words,
};
use super::firmware_file_build::{PickedFirmwareFile, read_firmware_files};
use super::store_lookups::{StoreLookup, StoreLookups};
use super::update_build_facts::StoreReleases;

/// One row of the update-states table, as a fixture builds it. The link a
/// row is told over is the view's own (a Bluetooth view, a USB one).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateFixtureRow {
    /// The board runs this Studio's build.
    UpToDate,
    /// The board runs X; this Studio has Y.
    Available,
    /// The board runs a dev build; this Studio has Y.
    AvailableDevBoard,
    /// An update is reading X back first (18%).
    BackingUp,
    /// An update is writing Y (40%).
    Updating,
    /// The last step of an update this Studio is running: the engine is
    /// being installed (70%).
    Finishing,
    /// An interrupted update to Y, found half-way on connect, is being
    /// finished with no click (70%).
    FinishingResumed,
    /// The board's missing firmware X is being put back (35%).
    Restoring,
    /// Another device holds the board's transfer (40%).
    AnotherDevice,
    /// The board's loader predates updates over a link.
    NeedsUsbOnce,
    /// The board's firmware keeps crashing.
    KeepsCrashing,
    /// The board needs a version this Studio could not get.
    CantGetVersion,
    /// An update to Y did not start, so the board went back to X.
    RolledBack,
    /// The board runs a newer version than this Studio's.
    Newer,
    /// An update is available, but this link is unlocked for play only.
    PlayOnly,
    /// Over Wi‑Fi, the board's release predates Wi‑Fi updates: the last
    /// update heard nothing on the update channel. Always told over Wi‑Fi.
    NotOverWifiYet,
}

impl UpdateFixtureRow {
    /// Every row, in the table's order.
    pub const ALL: [Self; 16] = [
        Self::UpToDate,
        Self::Available,
        Self::AvailableDevBoard,
        Self::BackingUp,
        Self::Updating,
        Self::Finishing,
        Self::FinishingResumed,
        Self::Restoring,
        Self::AnotherDevice,
        Self::NeedsUsbOnce,
        Self::KeepsCrashing,
        Self::CantGetVersion,
        Self::RolledBack,
        Self::Newer,
        Self::PlayOnly,
        Self::NotOverWifiYet,
    ];
}

/// A board in one update row: everything the standing is read from.
#[derive(Clone, Debug)]
pub struct UpdateFixture {
    /// The card, with the row's Update activity or last outcome on it.
    pub view: DeviceView,
    /// The board's update facts, as the device model mirrors them.
    pub facts: UpdateFacts,
    /// This Studio's own build.
    pub own: HostBuildFacts,
    /// The user's tier on the board over Bluetooth or Wi‑Fi.
    pub tier: Option<Tier>,
    /// The link the board is reached over (the view's; [`Self::over_wifi`]
    /// for a board on the LAN).
    pub link: UpdateLink,
    /// The store's release index ([`release_index`]); `None` is a Studio
    /// that could not read it ([`Self::offline`]).
    pub releases: Option<StoreReleases>,
    /// Releases looked up by exact version ([`Self::looked_up`]).
    pub lookups: StoreLookups,
    /// A custom build picked from files ([`Self::with_file_build`]).
    pub file: Option<HostBuildFacts>,
}

impl UpdateFixture {
    /// `view` (a ready LightPlayer, over USB or Bluetooth) in `row`.
    /// [`UpdateFixtureRow::NotOverWifiYet`] is told over Wi‑Fi whatever the
    /// view says.
    pub fn new(row: UpdateFixtureRow, view: DeviceView) -> Self {
        use UpdateFixtureRow as Row;
        let link = match (row, view.is_over_bluetooth()) {
            (Row::NotOverWifiYet, _) => UpdateLink::Wifi,
            (_, true) => UpdateLink::Bluetooth,
            (_, false) => UpdateLink::Usb,
        };
        let (manifest, view, tier) = match row {
            Row::UpToDate => (board_y(), view, None),
            Row::Available => (board_x(), view, None),
            Row::AvailableDevBoard => (manifest("5eb70a7c2", [0xAA; 32], [0xAE; 32]), view, None),
            Row::BackingUp => (
                board_x(),
                with_update_activity(view, Some(UpdateStageFacts::BackingUp), Some(18)),
                None,
            ),
            Row::Updating => (
                board_x(),
                with_update_activity(view, Some(UpdateStageFacts::Updating), Some(40)),
                None,
            ),
            Row::Finishing => (
                board_x(),
                with_update_activity(view, Some(UpdateStageFacts::Finishing), Some(70)),
                None,
            ),
            Row::FinishingResumed => (
                board_x(),
                with_update_intent(
                    view,
                    UpdateIntentFacts::Auto,
                    Some(UpdateStageFacts::Finishing),
                    Some(70),
                ),
                None,
            ),
            Row::Restoring => (
                needs_engine(),
                with_update_activity(view, Some(UpdateStageFacts::Restoring), Some(35)),
                None,
            ),
            Row::AnotherDevice => (busy(), view, None),
            Row::NeedsUsbOnce => (old_loader(), view, None),
            Row::KeepsCrashing => (crashing(), view, None),
            Row::CantGetVersion => (
                BoardManifest {
                    state: BoardState::NeedsEngine,
                    engine_len: None,
                    ..manifest("2026.09.28-4", [0xA9; 32], [0xA8; 32])
                },
                DeviceView {
                    last_update_outcome: Some(UpdateOutcomeFacts::MissingEngine { offline: true }),
                    ..view
                },
                None,
            ),
            Row::RolledBack => (refused(), view, None),
            Row::Newer => (newer(), view, None),
            Row::PlayOnly => (board_x(), view, Some(Tier::Play)),
            Row::NotOverWifiYet => (
                board_x(),
                DeviceView {
                    last_update_outcome: Some(UpdateOutcomeFacts::NotOverWifi),
                    ..view
                },
                None,
            ),
        };
        Self {
            view,
            facts: facts_of(&manifest),
            own: studio_y(),
            tier,
            link,
            releases: Some(StoreReleases {
                index: release_index(),
            }),
            lookups: StoreLookups::default(),
            file: None,
        }
    }

    /// The same board, with the custom build `version` picked from files.
    pub fn with_file_build(mut self, version: &str) -> Self {
        self.file = Some(file_build(version));
        self
    }

    /// The same board, after the box looked up `version` and the store
    /// answered `lookup`.
    pub fn looked_up(mut self, version: &str, lookup: StoreLookup) -> Self {
        self.lookups.set(
            &self.facts.target.clone().unwrap_or_default(),
            version,
            lookup,
        );
        self
    }

    /// The same board, reached on the LAN (Wi‑Fi) instead.
    pub fn over_wifi(mut self) -> Self {
        self.link = UpdateLink::Wifi;
        self
    }

    /// The same board and Studio a few days on, after every link's first
    /// update release (`2026.10.08-9`): the board on `2026.10.09-1`, this
    /// Studio's build `2026.10.09-2` (the same hashes).
    pub fn on_recent_firmware(mut self) -> Self {
        self.facts = facts_of(&manifest("2026.10.09-1", [0xAA; 32], [0xAE; 32]));
        self.own = build("2026.10.09-2", [0xBB; 32], [0xBE; 32]);
        self
    }

    /// The same board, with no release index: only this Studio's build to
    /// choose from, and the list says why.
    pub fn offline(mut self) -> Self {
        self.releases = None;
        self
    }

    /// The standing's inputs, as the controller assembles them.
    pub fn inputs(&self) -> UpdateStandingInputs<'_> {
        UpdateStandingInputs {
            view: &self.view,
            facts: Some(&self.facts),
            own: Some(&self.own),
            tier: self.tier,
            link: self.link,
            store_latest: None,
            store_releases: self.releases.as_ref(),
            store_lookups: Some(&self.lookups),
            file_build: self.file.as_ref(),
        }
    }

    /// The card's offer facts: the standing and its route, over a link
    /// that carries the update channel.
    pub fn offer_facts(&self) -> UpdateOfferFacts {
        UpdateOfferFacts::read(&self.inputs(), true)
    }

    /// The card's update words.
    pub fn words(&self) -> Option<UiDeviceUpdate> {
        update_words(&self.offer_facts().standing)
    }

    /// The editor popover's update words.
    pub fn session_words(&self) -> Option<UiSessionUpdate> {
        update_session_words(
            &self.offer_facts().standing,
            device_chip(&self.view).as_deref(),
            self.view.identity_label.as_deref(),
        )
    }
}

/// This Studio's build Y: `2026.10.05-2`, core `BB…`, engine `BE…`.
pub fn studio_y() -> HostBuildFacts {
    build("2026.10.05-2", [0xBB; 32], [0xBE; 32])
}

/// A build of `version` with these core and engine hashes.
pub fn build(version: &str, core: [u8; 32], engine: [u8; 32]) -> HostBuildFacts {
    HostBuildFacts::from_parts(
        HostIdentity {
            target: "esp32c6-4mb".into(),
            chip: "esp32c6".into(),
            version: version.into(),
            build_id: format!("{version}+{}", commit_for(version)),
            wire_proto: 36,
            layout: 1,
            min_loader: 1,
        },
        HostPieceFacts {
            sha256: core,
            len: 20_000,
        },
        HostPieceFacts {
            sha256: engine,
            len: 40_000,
        },
    )
}

/// The board on X: `2026.10.03-1`, running.
pub fn board_x() -> BoardManifest {
    manifest("2026.10.03-1", [0xAA; 32], [0xAE; 32])
}

/// A running board on `version` with these core and engine hashes.
pub fn manifest(version: &str, core: [u8; 32], engine: [u8; 32]) -> BoardManifest {
    BoardManifest {
        proto: 1,
        target: "esp32c6-4mb".into(),
        chip: "esp32c6".into(),
        version: version.into(),
        build_id: format!("{version}+{}", commit_for(version)),
        wire_proto: 36,
        core_sha256: sha256_to_hex(&core),
        core_len: 18_000,
        engine_sha256: sha256_to_hex(&engine),
        engine_len: Some(38_000),
        layout: 1,
        loader: 1,
        region_len: 3_375_104,
        state: BoardState::Running,
        refused_build: None,
        transfer: None,
    }
}

/// The board already on Y, by its hashes.
pub fn board_y() -> BoardManifest {
    manifest("2026.10.05-2", [0xBB; 32], [0xBE; 32])
}

/// A board on a newer version than this Studio's.
pub fn newer() -> BoardManifest {
    manifest("2026.10.07-4", [0xCC; 32], [0xCE; 32])
}

/// The board on X, refusing Y (its trial boot failed).
pub fn refused() -> BoardManifest {
    BoardManifest {
        refused_build: Some(studio_y().build_hash()),
        ..board_x()
    }
}

/// The board on X, whose engine keeps crashing.
pub fn crashing() -> BoardManifest {
    BoardManifest {
        state: BoardState::EngineCrashing,
        ..board_x()
    }
}

/// The board on X, missing its engine.
pub fn needs_engine() -> BoardManifest {
    BoardManifest {
        state: BoardState::NeedsEngine,
        engine_len: None,
        ..board_x()
    }
}

/// The board on trial of Y's core, fetching its engine.
pub fn on_trial_of_y() -> BoardManifest {
    BoardManifest {
        state: BoardState::OnTrial,
        engine_len: None,
        ..board_y()
    }
}

/// The board half-way through this Studio's transfer of Y.
pub fn continuing() -> BoardManifest {
    BoardManifest {
        state: BoardState::Updating,
        transfer: Some(TransferView {
            kind: PieceKind::Core,
            done: 14_000,
            total: 20_000,
            busy: false,
            build_hash: studio_y().build_hash(),
        }),
        ..board_x()
    }
}

/// The board taking Y from another device (40% through).
pub fn busy() -> BoardManifest {
    BoardManifest {
        state: BoardState::Updating,
        transfer: Some(TransferView {
            kind: PieceKind::Core,
            done: 8_000,
            total: 20_000,
            busy: true,
            build_hash: studio_y().build_hash(),
        }),
        ..board_x()
    }
}

/// The board on X with a loader that predates updates over a link.
pub fn old_loader() -> BoardManifest {
    BoardManifest {
        loader: 0,
        ..board_x()
    }
}

/// The device model's mirror of `m`, as the evidence carries it.
pub fn facts_of(m: &BoardManifest) -> UpdateFacts {
    UpdateFacts {
        state: match m.state {
            BoardState::Running => UpdateBoardState::Running,
            BoardState::NeedsEngine => UpdateBoardState::NeedsEngine,
            BoardState::EngineCrashing => UpdateBoardState::EngineCrashing,
            BoardState::Updating => UpdateBoardState::Updating,
            BoardState::OnTrial => UpdateBoardState::OnTrial,
            BoardState::Unknown => UpdateBoardState::Unknown,
        },
        version: Some(m.version.clone()),
        target: Some(m.target.clone()),
        build_id: Some(m.build_id.clone()),
        transfer: None,
        refused_build: m.refused_build,
        manifest_json: String::from_utf8(m.to_json()).expect("a manifest is UTF-8 JSON"),
    }
}

/// `view` running an install of Y at `stage`, `percent` through. Cancel is
/// one of its escapes while the stage allows it (backing up, or before the
/// first stage).
pub fn with_update_activity(
    view: DeviceView,
    stage: Option<UpdateStageFacts>,
    percent: Option<u8>,
) -> DeviceView {
    let intent = UpdateIntentFacts::Install {
        version: "2026.10.05-2".to_string(),
        allow_downgrade: false,
    };
    with_update_intent(view, intent, stage, percent)
}

/// [`with_update_activity`] for an update started with `intent`: the
/// no-click `Auto` is what a Studio runs when it finds a board half-way.
pub fn with_update_intent(
    mut view: DeviceView,
    intent: UpdateIntentFacts,
    stage: Option<UpdateStageFacts>,
    percent: Option<u8>,
) -> DeviceView {
    let cancellable = stage.is_none_or(UpdateStageFacts::allows_cancel);
    view.activity = Some(ActivityView {
        kind: ActivityKind::Update,
        label: "Updating…".to_string(),
        percent,
        cancellable,
        cancel_requested: false,
        layout: None,
        update: Some(UpdateActivityView {
            intent,
            stage,
            done: 0,
            total: 0,
            outcome: None,
            between_legs: false,
        }),
    });
    view.escapes.retain(|escape| *escape != Escape::Cancel);
    if cancellable {
        view.escapes.insert(0, Escape::Cancel);
    }
    view
}

/// The store's release index for the sample target: fourteen releases,
/// newest first, from `2026.10.07-4` (the newer board's) down to
/// `2026.10.02-1`, X and Y among them. The five oldest speak the wire
/// language before this Studio's.
pub fn release_index() -> ReleaseIndex {
    const RELEASES: [(&str, &str); 14] = [
        ("2026.10.07-4", "2026-10-07T16:28:38Z"),
        ("2026.10.07-3", "2026-10-07T13:54:40Z"),
        ("2026.10.07-2", "2026-10-07T12:32:57Z"),
        ("2026.10.07-1", "2026-10-07T09:12:05Z"),
        ("2026.10.05-2", "2026-10-05T21:40:11Z"),
        ("2026.10.05-1", "2026-10-05T17:03:52Z"),
        ("2026.10.04-3", "2026-10-04T22:15:30Z"),
        ("2026.10.04-2", "2026-10-04T18:47:09Z"),
        ("2026.10.04-1", "2026-10-04T11:20:44Z"),
        ("2026.10.03-4", "2026-10-03T23:05:18Z"),
        ("2026.10.03-3", "2026-10-03T19:31:02Z"),
        ("2026.10.03-2", "2026-10-03T15:58:27Z"),
        ("2026.10.03-1", "2026-10-03T10:44:13Z"),
        ("2026.10.02-1", "2026-10-02T20:09:36Z"),
    ];
    let target = TargetName::parse("esp32c6-4mb").expect("the sample target");
    let entries = RELEASES
        .iter()
        .enumerate()
        .map(|(at, (version, published))| ReleaseIndexEntry {
            version: version.to_string(),
            commit: format!("{}{}", commit_for(version), "0".repeat(28)),
            wire_proto: match at {
                0..9 => lpc_wire::WIRE_PROTO_VERSION,
                _ => lpc_wire::WIRE_PROTO_VERSION - 1,
            },
            requires: Requires {
                layout: 1,
                loader: 1,
            },
            published_at: Some(published.to_string()),
        })
        .collect();
    ReleaseIndex::newest_first(&target, entries)
}

/// Release `version` as the store answers a look-up of it: an older
/// release the index no longer lists, in the wire language before this
/// Studio's.
pub fn looked_up_release(version: &str) -> StoreLookup {
    StoreLookup::Found(ReleaseIndexEntry {
        version: version.to_string(),
        commit: format!("{}{}", commit_for(version), "0".repeat(28)),
        wire_proto: lpc_wire::WIRE_PROTO_VERSION - 1,
        requires: Requires {
            layout: 1,
            loader: 1,
        },
        published_at: None,
    })
}

/// `len` bytes of a piece, from `seed`: what a build picked from files
/// carries in the stories and tests.
pub fn piece_bytes(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|at| seed.wrapping_add((at % 251) as u8))
        .collect()
}

/// The `ota-manifest.json` of a build at `version` with these pieces, as
/// `lp-cli firmware package` writes it into a build's `ota/` folder (no
/// encodings).
pub fn ota_manifest_for(version: &str, core: &[u8], engine: &[u8]) -> OtaManifest {
    let piece = |file: &str, bytes: &[u8]| PieceFile {
        file: file.to_string(),
        length: bytes.len() as u64,
        sha256: sha256_hex(bytes),
    };
    let manifest = OtaManifest {
        format: 1,
        target: "esp32c6-4mb".to_string(),
        chip: "esp32c6".to_string(),
        version: version.to_string(),
        commit: format!("{}{}", commit_for(version), "0".repeat(28)),
        wire_proto: lpc_wire::WIRE_PROTO_VERSION,
        requires: Requires {
            layout: 1,
            loader: 1,
        },
        core: piece("core.bin", core),
        engine: piece("engine.bin", engine),
        encodings: Vec::new(),
        package: PackageRef {
            file: "package.json".to_string(),
            length: 2,
            sha256: sha256_hex(b"{}"),
            image: piece("fw-esp32c6-merged.bin", b"image"),
        },
    };
    manifest.validate().expect("a valid manifest");
    manifest
}

/// A custom build `version` read from its files, as "From a file…" holds
/// it.
pub fn file_build(version: &str) -> HostBuildFacts {
    let core = piece_bytes(0xC0, 4096 + 7);
    let engine = piece_bytes(0xE0, 2 * 4096 + 9);
    let manifest = ota_manifest_for(version, &core, &engine);
    let files = [
        ("ota-manifest.json", manifest.to_json_bytes()),
        ("core.bin", core),
        ("engine.bin", engine),
    ]
    .into_iter()
    .map(|(name, bytes)| PickedFirmwareFile {
        name: name.to_string(),
        bytes,
    })
    .collect::<Vec<_>>();
    read_firmware_files(&files, "esp32c6-4mb")
        .expect("the sample files read")
        .facts
}

/// A stable fake commit per version (a dev version is its own commit).
fn commit_for(version: &str) -> String {
    if version.bytes().all(|b| b.is_ascii_hexdigit()) {
        return format!("{version:0<12}");
    }
    match version {
        "2026.10.03-1" => "a41c9e2d11f0".to_string(),
        "2026.10.05-2" => "626a1b851aaa".to_string(),
        "2026.10.07-4" => "c08d1f3eeee0".to_string(),
        _ => "f00dfac00000".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use lpa_devices::view::{FIRMWARE_NEEDS_USB, FirmwareFace, LoadedProject};
    use lpa_devices::{DeviceId, DeviceStatus, FirmwareAge};

    use super::*;
    use crate::app::devices::device_update_standing::UpdateStanding;

    /// Every row a story draws lands on its own standing, with words, over
    /// USB and over Bluetooth.
    #[test]
    fn every_row_reads_as_its_own_standing() {
        use UpdateFixtureRow as Row;
        for row in UpdateFixtureRow::ALL {
            for over_bluetooth in [false, true] {
                let fixture = UpdateFixture::new(row, ready(over_bluetooth));
                let standing = fixture.offer_facts().standing;
                let landed = match row {
                    Row::UpToDate => matches!(standing, UpdateStanding::UpToDate { .. }),
                    Row::Available | Row::AvailableDevBoard => {
                        matches!(standing, UpdateStanding::Available { .. })
                    }
                    Row::BackingUp => matches!(standing, UpdateStanding::BackingUp { .. }),
                    Row::Updating => matches!(standing, UpdateStanding::Updating { .. }),
                    Row::Finishing => {
                        matches!(standing, UpdateStanding::Finishing { resumed: false, .. })
                    }
                    Row::FinishingResumed => {
                        matches!(standing, UpdateStanding::Finishing { resumed: true, .. })
                    }
                    Row::Restoring => matches!(standing, UpdateStanding::Restoring { .. }),
                    Row::AnotherDevice => {
                        matches!(standing, UpdateStanding::AnotherDevice { .. })
                    }
                    Row::NeedsUsbOnce => matches!(standing, UpdateStanding::NeedsUsbOnce { .. }),
                    Row::KeepsCrashing => {
                        matches!(standing, UpdateStanding::KeepsCrashing { .. })
                    }
                    Row::CantGetVersion => {
                        matches!(standing, UpdateStanding::CantGetVersion { .. })
                    }
                    Row::RolledBack => matches!(standing, UpdateStanding::RolledBack { .. }),
                    Row::Newer => matches!(standing, UpdateStanding::Newer { .. }),
                    Row::PlayOnly => matches!(standing, UpdateStanding::PlayOnly { .. }),
                    Row::NotOverWifiYet => {
                        matches!(standing, UpdateStanding::NotOverWifiYet { .. })
                    }
                };
                assert!(
                    landed,
                    "{row:?} (bluetooth: {over_bluetooth}) → {standing:?}"
                );
                assert!(fixture.words().is_some(), "{row:?}");
                assert!(fixture.session_words().is_some(), "{row:?}");
            }
        }
    }

    fn ready(over_bluetooth: bool) -> DeviceView {
        DeviceView {
            id: DeviceId(7),
            title: "Porch lights".to_string(),
            status: DeviceStatus::Ready,
            state_label: "Ready".to_string(),
            detail: None,
            freshness_label: None,
            identity_label: Some("60:55:f9:0a:0b:0c".to_string()),
            detected_chip: Some("esp32c6".to_string()),
            board_id: None,
            firmware_face: FirmwareFace::LightPlayer {
                firmware: None,
                wire: lpa_devices::WireVersion::Match,
                age: FirmwareAge::Unknown,
            },
            remembered_firmware: None,
            degraded: None,
            loaded_project: LoadedProject::Empty,
            engine_fps: None,
            link_counters: None,
            can_receive_project: true,
            can_remove_project: false,
            activity: None,
            last_outcome: None,
            last_update_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            firmware_blocked: over_bluetooth.then(|| FIRMWARE_NEEDS_USB.to_string()),
            update_blocked: None,
            escapes: vec![Escape::Disconnect, Escape::Forget],
        }
    }
}
