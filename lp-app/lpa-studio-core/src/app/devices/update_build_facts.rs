//! What this Studio knows about the builds it could put on a board: its own
//! build, the firmware store's `latest`, and the store's release index, by
//! their facts alone (DS5: a card decides without loading a build's bytes).
//!
//! The controller owns one [`UpdateBuildFacts`]. It starts empty, and with
//! it empty nothing changes on any card: every board's update standing is
//! [`UpdateStanding::Nothing`](super::UpdateStanding::Nothing), so the card
//! keeps today's words and today's USB flash. The update host fills it —
//! [`UpdateBuildFacts::set_own`] from this Studio's bundled
//! `ota-manifest.json` at start, [`UpdateBuildFacts::set_store_latest`]
//! when the firmware store answers its `latest` lookup for a board's
//! target, [`UpdateBuildFacts::set_store_releases`] when it answers the
//! release index for that target, [`UpdateBuildFacts::set_store_lookups`]
//! as the releases looked up by exact version answer — and from then on
//! every card reads its standing and its version choices against them.
//!
//! The store's answers are held for **one target at a time** (the first
//! target the controller sees, as for `latest`): every board in the field
//! today is one target. Holding one per target is future work.

use lpa_update::HostBuildFacts;
use lpc_firmware_release::ReleaseIndex;

use super::store_lookups::StoreLookups;

/// The firmware store's latest release for a target, by its facts: the
/// second version "Other version…" can offer (DS7), when it is not this
/// Studio's own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreLatest {
    pub facts: HostBuildFacts,
}

impl StoreLatest {
    /// The release's version (`2026.10.07-4`).
    pub fn version(&self) -> &str {
        &self.facts.identity.version
    }

    /// The opaque target it was built for: it is offered only to a board
    /// of the same target.
    pub fn target(&self) -> &str {
        &self.facts.identity.target
    }
}

/// The firmware store's release index for a target (verified by
/// `lpa-firmware-store`): every version "Other version…" can list for a
/// board of that target.
#[derive(Clone, Debug, PartialEq)]
pub struct StoreReleases {
    pub index: ReleaseIndex,
}

impl StoreReleases {
    /// The target every listed release was built for.
    pub fn target(&self) -> &str {
        &self.index.target
    }
}

/// This Studio's own build, the store's latest and its release index, when
/// known. See the module docs: empty until the update host fills it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UpdateBuildFacts {
    own: Option<HostBuildFacts>,
    store_latest: Option<StoreLatest>,
    store_releases: Option<StoreReleases>,
    store_lookups: StoreLookups,
    file_build: Option<HostBuildFacts>,
}

impl UpdateBuildFacts {
    /// This Studio's own build (read from its `ota-manifest.json`).
    pub fn own(&self) -> Option<&HostBuildFacts> {
        self.own.as_ref()
    }

    /// The store's latest release, once the store has said.
    pub fn store_latest(&self) -> Option<&StoreLatest> {
        self.store_latest.as_ref()
    }

    /// The store's release index, once the store has listed one.
    pub fn store_releases(&self) -> Option<&StoreReleases> {
        self.store_releases.as_ref()
    }

    /// The releases looked up by exact version, and where each stands.
    pub fn store_lookups(&self) -> &StoreLookups {
        &self.store_lookups
    }

    /// The custom build picked from files ("From a file…"), by its facts.
    pub fn file_build(&self) -> Option<&HostBuildFacts> {
        self.file_build.as_ref()
    }

    /// Install (or clear) the custom build picked from files.
    pub fn set_file_build(&mut self, build: Option<HostBuildFacts>) {
        self.file_build = build;
    }

    /// Install the look-ups as the update host holds them.
    pub fn set_store_lookups(&mut self, lookups: StoreLookups) {
        self.store_lookups = lookups;
    }

    /// Install (or clear) this Studio's own build facts.
    pub fn set_own(&mut self, own: Option<HostBuildFacts>) {
        self.own = own;
    }

    /// Install (or clear) the store's latest release.
    pub fn set_store_latest(&mut self, latest: Option<StoreLatest>) {
        self.store_latest = latest;
    }

    /// Install (or clear) the store's release index.
    pub fn set_store_releases(&mut self, releases: Option<StoreReleases>) {
        self.store_releases = releases;
    }
}

#[cfg(test)]
mod tests {
    use lpa_update::{HostIdentity, HostPieceFacts};

    use super::*;

    #[test]
    fn it_starts_empty_and_holds_what_it_is_given() {
        let mut facts = UpdateBuildFacts::default();
        assert!(facts.own().is_none());
        assert!(facts.store_latest().is_none());

        facts.set_own(Some(build("2026.10.05-2")));
        facts.set_store_latest(Some(StoreLatest {
            facts: build("2026.10.07-4"),
        }));
        assert_eq!(facts.own().unwrap().identity.version, "2026.10.05-2");
        let latest = facts.store_latest().unwrap();
        assert_eq!(latest.version(), "2026.10.07-4");
        assert_eq!(latest.target(), "esp32c6-4mb");

        assert!(facts.store_releases().is_none());
        let index = ReleaseIndex::newest_first(
            &lpc_firmware_release::TargetName::parse("esp32c6-4mb").unwrap(),
            Vec::new(),
        );
        facts.set_store_releases(Some(StoreReleases { index }));
        assert_eq!(facts.store_releases().unwrap().target(), "esp32c6-4mb");

        facts.set_own(None);
        assert!(facts.own().is_none());
    }

    fn build(version: &str) -> HostBuildFacts {
        HostBuildFacts::from_parts(
            HostIdentity {
                target: "esp32c6-4mb".into(),
                chip: "esp32c6".into(),
                version: version.into(),
                build_id: format!("{version}+626a1b851aaa"),
                wire_proto: 36,
                layout: 1,
                min_loader: 1,
            },
            HostPieceFacts {
                sha256: [1; 32],
                len: 10,
            },
            HostPieceFacts {
                sha256: [2; 32],
                len: 10,
            },
        )
    }
}
