//! Releases looked up by their exact version: "Other version…"'s box
//! names a release the store's index does not list (the index holds the
//! newest hundred; an older one is still installable by its version), and
//! its look-up asks the store for that release's `ota-manifest.json` at
//! `/firmware/<target>/<version>/` — the same lookup an install of it makes.
//!
//! A found release becomes one more [`super::InstallChoice`], read against
//! the board like an index entry; until then the box's press says where the
//! look-up stands. Held per (target, version) for this Studio's life; an
//! offline answer may be asked again.

use std::collections::BTreeMap;

use lpc_firmware_release::ReleaseIndexEntry;

/// Where one look-up stands.
#[derive(Clone, Debug, PartialEq)]
pub enum StoreLookup {
    /// Asked; the store has not answered yet.
    Looking,
    /// The store holds it: its entry, as the index would list it (no
    /// publish time).
    Found(ReleaseIndexEntry),
    /// The store has no such release for the target (or refused what it
    /// served).
    Missing,
    /// The store could not be reached: asking again may work.
    Offline,
}

/// Every look-up this Studio made, by (target, version).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StoreLookups(BTreeMap<(String, String), StoreLookup>);

impl StoreLookups {
    /// Where the look-up of `version` for `target` stands, if asked.
    pub fn get(&self, target: &str, version: &str) -> Option<&StoreLookup> {
        self.0.get(&(target.to_string(), version.to_string()))
    }

    /// Record where the look-up of `version` for `target` stands.
    pub fn set(&mut self, target: &str, version: &str, lookup: StoreLookup) {
        self.0
            .insert((target.to_string(), version.to_string()), lookup);
    }

    /// Whether a look-up of `version` for `target` should be asked: never
    /// asked, or the last answer was offline.
    pub fn wants(&self, target: &str, version: &str) -> bool {
        matches!(self.get(target, version), None | Some(StoreLookup::Offline))
    }

    /// Every release found for `target`.
    pub fn found<'a>(&'a self, target: &'a str) -> impl Iterator<Item = &'a ReleaseIndexEntry> {
        self.0
            .iter()
            .filter(move |((t, _), _)| t == target)
            .filter_map(|(_, lookup)| match lookup {
                StoreLookup::Found(entry) => Some(entry),
                _ => None,
            })
    }

    /// Every look-up for `target` the store has not found (yet), by
    /// version.
    pub fn unfound<'a>(
        &'a self,
        target: &'a str,
    ) -> impl Iterator<Item = (&'a str, &'a StoreLookup)> {
        self.0
            .iter()
            .filter(move |((t, _), lookup)| t == target && !matches!(lookup, StoreLookup::Found(_)))
            .map(|((_, version), lookup)| (version.as_str(), lookup))
    }
}

#[cfg(test)]
mod tests {
    use lpc_firmware_release::Requires;

    use super::*;

    #[test]
    fn a_lookup_is_asked_once_unless_the_store_was_offline() {
        let mut lookups = StoreLookups::default();
        assert!(lookups.wants("esp32c6-4mb", "2026.09.30-2"));
        lookups.set("esp32c6-4mb", "2026.09.30-2", StoreLookup::Looking);
        assert!(!lookups.wants("esp32c6-4mb", "2026.09.30-2"));
        lookups.set("esp32c6-4mb", "2026.09.30-2", StoreLookup::Offline);
        assert!(lookups.wants("esp32c6-4mb", "2026.09.30-2"));
        lookups.set("esp32c6-4mb", "2026.09.30-2", StoreLookup::Missing);
        assert!(!lookups.wants("esp32c6-4mb", "2026.09.30-2"));
    }

    #[test]
    fn found_and_unfound_are_per_target() {
        let mut lookups = StoreLookups::default();
        lookups.set(
            "esp32c6-4mb",
            "2026.09.30-2",
            StoreLookup::Found(entry("2026.09.30-2")),
        );
        lookups.set("esp32c6-4mb", "2026.09.29-1", StoreLookup::Missing);
        lookups.set(
            "esp32s3-8mb",
            "2026.09.30-1",
            StoreLookup::Found(entry("2026.09.30-1")),
        );
        let found: Vec<&str> = lookups
            .found("esp32c6-4mb")
            .map(|e| e.version.as_str())
            .collect();
        assert_eq!(found, ["2026.09.30-2"]);
        let unfound: Vec<&str> = lookups.unfound("esp32c6-4mb").map(|(v, _)| v).collect();
        assert_eq!(unfound, ["2026.09.29-1"]);
    }

    fn entry(version: &str) -> ReleaseIndexEntry {
        ReleaseIndexEntry {
            version: version.to_string(),
            commit: "a".repeat(40),
            wire_proto: 40,
            requires: Requires {
                layout: 1,
                loader: 1,
            },
            published_at: None,
        }
    }
}
