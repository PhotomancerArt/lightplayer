//! The versions "Other version…" can put on a board: the store's release
//! index for the board's target, the releases looked up by exact version
//! ([`super::store_lookups`]), this Studio's own build, and — when the index
//! is missing (offline, or a server that predates it) — the store's
//! `latest`, each read against the board.
//!
//! The choices live in [`super::UpdateOfferFacts`], not in the board's
//! [`super::UpdateStanding`]: the standing stays the board's own story
//! (what `decide()` says and what is running), and the store's facts only
//! widen what the card offers.
//!
//! - **A custom build picked from files** ("From a file…") is listed first,
//!   marked as from a file; it stands in for a listed version of the same
//!   name (the person picked those files on purpose).
//! - **Only the board's target.** The index is per target and must name the
//!   board's; this Studio's own build and `latest` are listed only when
//!   built for it.
//! - **One entry per version**, newest first by [`FirmwareAge`]'s order
//!   (the only version order); a dev build, which has no order against a
//!   release, leads, as this Studio's.
//! - **Drawn, not hidden:** the board's own version is marked `on_board`,
//!   and the build the board refused after a failed start `refused`; the
//!   offer draws both disabled.
//! - **Recent:** the first [`RECENT_CHOICES`], plus this Studio's own and
//!   the board's own; the rest are found by typing in the offer's box.

use lpa_devices::{FirmwareAge, WireVersion};
use lpa_update::HostBuildFacts;
use lpc_firmware_release::{ReleaseIndex, ReleaseIndexEntry};

use super::device_update_route::{
    FIRST_BLUETOOTH_UPDATE_RELEASE, FIRST_WIFI_UPDATE_RELEASE, UpdateLink,
};
use super::device_update_version::UpdateVersion;
use super::store_lookups::StoreLookups;
use super::update_build_facts::{StoreLatest, StoreReleases};

/// How many choices the picker shows before anything is typed in its box.
pub const RECENT_CHOICES: usize = 5;

/// One version "Other version…" can install, read against the board.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallChoice {
    /// The version, with its build id when known.
    pub version: UpdateVersion,
    /// The **board** against this choice: `Older` means the choice is newer
    /// than what the board runs (an update), `Newer` that it is older (a
    /// downgrade), `Different`/`Unknown` that they do not order.
    pub age: FirmwareAge,
    /// The board runs it now.
    pub on_board: bool,
    /// The board refused this build after it failed to start.
    pub refused: bool,
    /// This Studio's own build.
    pub own: bool,
    /// The custom build picked from files ("From a file…").
    pub from_file: bool,
    /// The choice's wire protocol against this Studio's (`BoardOlder`: the
    /// choice speaks an older one); `None` when not known.
    pub wire: Option<WireVersion>,
    /// The wireless link this version can't be updated over again (it is
    /// from before updates over that link): after it, the board needs
    /// another way — a USB cable, or Bluetooth after Wi‑Fi — once. `None`
    /// over USB, and for a version that keeps the link.
    pub stranded_over: Option<UpdateLink>,
    /// When the release was published (RFC 3339), when the index says.
    pub published_at: Option<String>,
    /// Shown with nothing typed in the box.
    pub recent: bool,
}

impl InstallChoice {
    /// Whether the board would take a step forward: the choice is known to
    /// be newer than the board's version.
    pub fn is_known_newer(&self) -> bool {
        self.age == FirmwareAge::Older
    }

    /// Whether it is an older version than the board's (a downgrade, which
    /// `decide()` refuses without `allow_downgrade`).
    pub fn is_older(&self) -> bool {
        self.age == FirmwareAge::Newer
    }

    /// Whether the choice speaks an older wire protocol than this Studio.
    pub fn speaks_older_wire(&self) -> bool {
        matches!(self.wire, Some(WireVersion::BoardOlder { .. }))
    }

    /// Whether the choice speaks a newer wire protocol than this Studio.
    pub fn speaks_newer_wire(&self) -> bool {
        matches!(self.wire, Some(WireVersion::BoardNewer { .. }))
    }

    /// Whether it can be picked (it is neither on the board nor refused).
    pub fn is_pickable(&self) -> bool {
        !self.on_board && !self.refused
    }
}

/// Everything the choices are read from.
#[derive(Clone, Copy, Debug)]
pub struct InstallChoiceInputs<'a> {
    /// What the board runs.
    pub board: &'a UpdateVersion,
    /// The board's target (from its manifest).
    pub target: &'a str,
    /// The build the board refused after a failed start, when it did.
    pub refused: Option<&'a UpdateVersion>,
    /// This Studio's own build (update-capable by construction).
    pub own: Option<&'a HostBuildFacts>,
    /// The store's latest release.
    pub store_latest: Option<&'a StoreLatest>,
    /// The store's release index.
    pub store_releases: Option<&'a StoreReleases>,
    /// The releases looked up by exact version.
    pub store_lookups: Option<&'a StoreLookups>,
    /// The custom build picked from files.
    pub file_build: Option<&'a HostBuildFacts>,
    /// The link the install would ride.
    pub link: UpdateLink,
    /// This Studio's wire protocol version (`lpc_wire::WIRE_PROTO_VERSION`).
    pub studio_wire_proto: u32,
}

/// The release index for `target`, when the store listed that target's.
pub fn index_for<'a>(
    releases: Option<&'a StoreReleases>,
    target: &str,
) -> Option<&'a ReleaseIndex> {
    releases
        .filter(|releases| releases.target() == target)
        .map(|releases| &releases.index)
}

/// The board's version choices. See the module docs.
pub fn install_choices(inputs: &InstallChoiceInputs<'_>) -> Vec<InstallChoice> {
    let index = index_for(inputs.store_releases, inputs.target);
    let mut found: Vec<Found> = Vec::new();
    if let Some(index) = index {
        for entry in &index.releases {
            found.push(Found::of_entry(entry));
        }
    }
    for entry in inputs
        .store_lookups
        .into_iter()
        .flat_map(|lookups| lookups.found(inputs.target))
    {
        if !found.iter().any(|f| f.version.version == entry.version) {
            found.push(Found::of_entry(entry));
        }
    }
    if index.is_none()
        && let Some(latest) = inputs
            .store_latest
            .filter(|latest| latest.target() == inputs.target)
        && !found.iter().any(|f| f.version.version == latest.version())
    {
        found.push(Found::of_build(&latest.facts, false));
    }
    if let Some(own) = inputs
        .own
        .filter(|own| own.identity.target == inputs.target)
    {
        let own = Found::of_build(own, true);
        match found
            .iter_mut()
            .find(|f| f.version.version == own.version.version)
        {
            // This Studio's build is a listed release: one entry, its own.
            Some(listed) => listed.own = true,
            None => found.push(own),
        }
    }
    if let Some(file) = inputs
        .file_build
        .filter(|file| file.identity.target == inputs.target)
    {
        found.retain(|f| f.version.version != file.identity.version);
        found.push(Found {
            from_file: true,
            ..Found::of_build(file, false)
        });
    }
    found.sort_by(|a, b| {
        b.from_file
            .cmp(&a.from_file)
            .then(order(&a.version, &b.version))
    });

    found
        .into_iter()
        .enumerate()
        .map(|(at, f)| {
            let age = inputs.board.age_against(&f.version);
            let on_board = same_build(inputs.board, &f.version);
            InstallChoice {
                stranded_over: stranded_over(inputs.link, &f.version, age),
                refused: inputs.refused.is_some_and(|r| same_build(r, &f.version)),
                wire: match f.own {
                    true => Some(WireVersion::Match),
                    false => f
                        .wire_proto
                        .map(|proto| WireVersion::compare(proto, inputs.studio_wire_proto)),
                },
                recent: at < RECENT_CHOICES || f.own || f.from_file || on_board,
                on_board,
                own: f.own,
                from_file: f.from_file,
                published_at: f.published_at,
                version: f.version,
                age,
            }
        })
        .collect()
}

/// A choice before it is read against the board.
struct Found {
    version: UpdateVersion,
    own: bool,
    from_file: bool,
    wire_proto: Option<u32>,
    published_at: Option<String>,
}

impl Found {
    fn of_entry(entry: &ReleaseIndexEntry) -> Self {
        Self {
            version: UpdateVersion::with_build_id(&entry.version, entry.build_id()),
            own: false,
            from_file: false,
            wire_proto: Some(entry.wire_proto),
            published_at: entry.published_at.clone(),
        }
    }

    fn of_build(build: &HostBuildFacts, own: bool) -> Self {
        Self {
            version: UpdateVersion::with_build_id(
                &build.identity.version,
                &build.identity.build_id,
            ),
            own,
            from_file: false,
            wire_proto: Some(build.identity.wire_proto),
            published_at: None,
        }
    }
}

/// Newest first; a version that does not order against another (a dev
/// build) before it.
fn order(a: &UpdateVersion, b: &UpdateVersion) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a.is_dev(), b.is_dev()) {
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        _ => {}
    }
    match a.age_against(b) {
        FirmwareAge::Newer => Ordering::Less,
        FirmwareAge::Older => Ordering::Greater,
        _ => Ordering::Equal,
    }
}

/// The same build: the same version, and the same build id when both are
/// known.
fn same_build(a: &UpdateVersion, b: &UpdateVersion) -> bool {
    a.version == b.version
        && match (&a.build_id, &b.build_id) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        }
}

/// Over a wireless link, `link` when `choice` predates updates over it
/// ([`FIRST_BLUETOOTH_UPDATE_RELEASE`], [`FIRST_WIFI_UPDATE_RELEASE`]);
/// while no release is named, any choice older than the board's (`age` is
/// the board against it).
fn stranded_over(link: UpdateLink, choice: &UpdateVersion, age: FirmwareAge) -> Option<UpdateLink> {
    let first = match link {
        UpdateLink::Usb => return None,
        UpdateLink::Bluetooth => FIRST_BLUETOOTH_UPDATE_RELEASE,
        UpdateLink::Wifi => FIRST_WIFI_UPDATE_RELEASE,
    };
    needs_usb_after_with(first, link, choice, age).then_some(link)
}

/// Whether [`stranded_over`] holds, with the first release as a parameter,
/// so both readings are tested.
fn needs_usb_after_with(
    first: Option<&str>,
    link: UpdateLink,
    choice: &UpdateVersion,
    age: FirmwareAge,
) -> bool {
    if !link.is_wireless() {
        return false;
    }
    match first {
        Some(first) => choice.age_against(&UpdateVersion::new(first)) == FirmwareAge::Older,
        None => age == FirmwareAge::Newer,
    }
}

#[cfg(test)]
mod tests {
    use lpc_firmware_release::{ReleaseIndexEntry, Requires, TargetName};

    use super::*;
    use crate::app::devices::device_update_fixtures::{build, studio_y};
    use crate::app::devices::store_lookups::StoreLookup;

    const PROTO: u32 = 40;

    #[test]
    fn newest_first_by_number_with_the_boards_own_marked() {
        let releases = releases(&["2026.10.06-10", "2026.10.06-9", "2026.10.05-2"]);
        let board = UpdateVersion::new("2026.10.06-9");
        let choices = install_choices(&inputs(&board, None, Some(&releases)));
        assert_eq!(
            versions(&choices),
            ["2026.10.06-10", "2026.10.06-9", "2026.10.05-2"]
        );
        let on_board: Vec<bool> = choices.iter().map(|c| c.on_board).collect();
        assert_eq!(on_board, [false, true, false]);
        assert!(choices[0].is_known_newer());
        assert!(choices[2].is_older());
        // This Studio's own (Y, 2026.10.05-2) is a listed release: one entry.
        assert!(choices[2].own);
    }

    #[test]
    fn another_targets_index_is_never_listed() {
        let mut releases = releases(&["2026.10.06-10"]);
        releases.index.target = "esp32s3-8mb".into();
        let board = UpdateVersion::new("2026.10.03-1");
        let choices = install_choices(&inputs(&board, None, Some(&releases)));
        assert_eq!(versions(&choices), ["2026.10.05-2"], "only this Studio's");
        let y = studio_y();
        let mut other_target = inputs(&board, None, None);
        other_target.target = "esp32s3-8mb";
        other_target.own = Some(&y);
        assert!(install_choices(&other_target).is_empty());
    }

    #[test]
    fn with_no_index_the_choices_are_this_studios_and_the_stores_latest() {
        let board = UpdateVersion::new("2026.10.03-1");
        let latest = StoreLatest {
            facts: build("2026.10.07-4", [0xCC; 32], [0xCE; 32]),
        };
        let mut no_index = inputs(&board, None, None);
        no_index.store_latest = Some(&latest);
        let choices = install_choices(&no_index);
        assert_eq!(versions(&choices), ["2026.10.07-4", "2026.10.05-2"]);
        // With an index, `latest` is not added on top of it.
        let releases = releases(&["2026.10.06-10"]);
        no_index.store_releases = Some(&releases);
        assert_eq!(
            versions(&install_choices(&no_index)),
            ["2026.10.06-10", "2026.10.05-2"]
        );
    }

    #[test]
    fn a_dev_build_of_this_studio_leads() {
        let dev = build("626a1b851", [0xDD; 32], [0xDE; 32]);
        let releases = releases(&["2026.10.06-10", "2026.10.06-9"]);
        let board = UpdateVersion::new("2026.10.06-9");
        let mut with_dev = inputs(&board, None, Some(&releases));
        with_dev.own = Some(&dev);
        let choices = install_choices(&with_dev);
        assert_eq!(
            versions(&choices),
            ["626a1b851", "2026.10.06-10", "2026.10.06-9"]
        );
        assert!(choices[0].own);
        assert_eq!(choices[0].age, FirmwareAge::Different, "no order");
        assert_eq!(choices[0].wire, Some(WireVersion::Match));
    }

    #[test]
    fn each_choice_reads_its_wire_against_this_studio() {
        let mut releases = releases(&["2026.10.06-10", "2026.10.06-9", "2026.10.06-8"]);
        releases.index.releases[0].wire_proto = PROTO + 1;
        releases.index.releases[2].wire_proto = PROTO - 1;
        let board = UpdateVersion::new("2026.10.01-1");
        let choices = install_choices(&inputs(&board, None, Some(&releases)));
        assert!(choices[0].speaks_newer_wire());
        assert_eq!(choices[1].wire, Some(WireVersion::Match));
        assert!(choices[2].speaks_older_wire());
    }

    #[test]
    fn over_bluetooth_a_version_before_bluetooth_updates_needs_usb_after() {
        let older = UpdateVersion::new("2026.10.06-9");
        let newer = UpdateVersion::new("2026.10.07-20");
        // With the first release named: by version, whatever the board.
        let first = Some("2026.10.07-16");
        assert!(needs_usb_after_with(
            first,
            UpdateLink::Bluetooth,
            &older,
            FirmwareAge::Older
        ));
        assert!(!needs_usb_after_with(
            first,
            UpdateLink::Bluetooth,
            &newer,
            FirmwareAge::Newer
        ));
        // With none named: every choice older than the board's.
        assert!(needs_usb_after_with(
            None,
            UpdateLink::Bluetooth,
            &newer,
            FirmwareAge::Newer
        ));
        assert!(!needs_usb_after_with(
            None,
            UpdateLink::Bluetooth,
            &older,
            FirmwareAge::Older
        ));
        // Over USB, never.
        for first in [first, None] {
            assert!(!needs_usb_after_with(
                first,
                UpdateLink::Usb,
                &older,
                FirmwareAge::Newer
            ));
        }
    }

    /// Over Wi‑Fi no release is named yet: every choice older than the
    /// board's warns, and a newer one does not.
    #[test]
    fn over_wifi_every_older_version_is_stranded_until_a_release_is_named() {
        let any = UpdateVersion::new("2026.10.06-9");
        assert_eq!(FIRST_WIFI_UPDATE_RELEASE, None);
        assert_eq!(
            stranded_over(UpdateLink::Wifi, &any, FirmwareAge::Newer),
            Some(UpdateLink::Wifi)
        );
        assert_eq!(
            stranded_over(UpdateLink::Wifi, &any, FirmwareAge::Older),
            None
        );
        assert_eq!(
            stranded_over(UpdateLink::Usb, &any, FirmwareAge::Newer),
            None
        );
    }

    #[test]
    fn recent_is_the_first_five_plus_this_studios_and_the_boards_own() {
        let names: Vec<String> = (1..=14).rev().map(|n| format!("2026.10.06-{n}")).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let releases = releases(&names);
        // The board runs the oldest; this Studio's dev build leads.
        let board = UpdateVersion::new("2026.10.06-1");
        let dev = build("626a1b851", [0xDD; 32], [0xDE; 32]);
        let mut many = inputs(&board, None, Some(&releases));
        many.own = Some(&dev);
        let choices = install_choices(&many);
        assert_eq!(choices.len(), 15);
        let recent: Vec<&str> = choices
            .iter()
            .filter(|c| c.recent)
            .map(|c| c.version.version.as_str())
            .collect();
        assert_eq!(recent.len(), 6, "{recent:?}");
        assert_eq!(recent[0], "626a1b851");
        assert_eq!(recent[4], "2026.10.06-11", "the first five, own included");
        assert_eq!(recent[5], "2026.10.06-1", "the board's own");
    }

    #[test]
    fn a_release_looked_up_by_version_joins_the_list_in_its_place() {
        let releases = releases(&["2026.10.06-10", "2026.10.05-2"]);
        let board = UpdateVersion::new("2026.10.05-2");
        let mut lookups = StoreLookups::default();
        let older = releases.index.releases[0].clone();
        let older = ReleaseIndexEntry {
            version: "2026.09.30-2".to_string(),
            ..older
        };
        lookups.set(
            "esp32c6-4mb",
            "2026.09.30-2",
            StoreLookup::Found(older.clone()),
        );
        lookups.set("esp32s3-8mb", "2026.09.29-1", StoreLookup::Found(older));
        lookups.set("esp32c6-4mb", "2026.09.28-1", StoreLookup::Missing);
        let mut looked = inputs(&board, None, Some(&releases));
        looked.store_lookups = Some(&lookups);
        let choices = install_choices(&looked);
        assert_eq!(
            versions(&choices),
            ["2026.10.06-10", "2026.10.05-2", "2026.09.30-2"],
            "found for this target only, and in version order"
        );
        assert_eq!(choices[2].age, FirmwareAge::Newer, "older than the board's");
    }

    #[test]
    fn a_build_from_files_leads_and_stands_in_for_its_version() {
        let releases = releases(&["2026.10.06-10", "2026.10.05-2"]);
        let board = UpdateVersion::new("2026.10.03-1");
        let file = crate::app::devices::device_update_fixtures::file_build("2026.10.06-10");
        let mut picked = inputs(&board, None, Some(&releases));
        picked.file_build = Some(&file);
        let choices = install_choices(&picked);
        assert_eq!(versions(&choices), ["2026.10.06-10", "2026.10.05-2"]);
        assert!(choices[0].from_file && choices[0].recent);
        assert!(!choices[1].from_file);
        let mut other_target = file.clone();
        other_target.identity.target = "esp32s3-8mb".into();
        picked.file_build = Some(&other_target);
        assert!(install_choices(&picked).iter().all(|c| !c.from_file));
    }

    #[test]
    fn the_refused_build_is_marked() {
        let releases = releases(&["2026.10.06-10", "2026.10.05-2"]);
        let board = UpdateVersion::new("2026.10.03-1");
        let refused =
            UpdateVersion::with_build_id("2026.10.05-2", studio_y().identity.build_id.clone());
        let choices = install_choices(&inputs(&board, Some(&refused), Some(&releases)));
        let refused: Vec<bool> = choices.iter().map(|c| c.refused).collect();
        assert_eq!(refused, [false, true]);
        assert!(!choices[1].is_pickable());
    }

    fn inputs<'a>(
        board: &'a UpdateVersion,
        refused: Option<&'a UpdateVersion>,
        releases: Option<&'a StoreReleases>,
    ) -> InstallChoiceInputs<'a> {
        InstallChoiceInputs {
            board,
            target: "esp32c6-4mb",
            refused,
            own: Some(&Y),
            store_latest: None,
            store_releases: releases,
            store_lookups: None,
            file_build: None,
            link: UpdateLink::Usb,
            studio_wire_proto: PROTO,
        }
    }

    /// This Studio's Y, for the inputs to borrow.
    static Y: std::sync::LazyLock<HostBuildFacts> = std::sync::LazyLock::new(studio_y);

    fn releases(versions: &[&str]) -> StoreReleases {
        let target = TargetName::parse("esp32c6-4mb").unwrap();
        let entries = versions
            .iter()
            .map(|version| ReleaseIndexEntry {
                version: version.to_string(),
                commit: if *version == "2026.10.05-2" {
                    format!("626a1b851aaa{}", "0".repeat(28))
                } else {
                    "736d72856d243fce519c9f461f369f59fcbf175a".to_string()
                },
                wire_proto: PROTO,
                requires: Requires {
                    layout: 1,
                    loader: 1,
                },
                published_at: None,
            })
            .collect();
        StoreReleases {
            index: ReleaseIndex::newest_first(&target, entries),
        }
    }

    fn versions(choices: &[InstallChoice]) -> Vec<&str> {
        choices.iter().map(|c| c.version.version.as_str()).collect()
    }
}
