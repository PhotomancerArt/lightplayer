//! Resolving `--release <version|latest|previous>` to a concrete version.
//!
//! `latest` and an explicit version resolve for free when the package is
//! fetched (its own manifest core names the version it is — see
//! `package_fetch`). `previous` is the one case that needs a second source:
//! the lookup answers one release at a time, so finding "the newest one
//! before latest" means asking a list of releases. This reads GitHub's
//! ([`ReleaseCatalog`], `release_catalog_source`). lightplayer.app's release
//! index (`/api/v1/firmware/<target>/releases`) lists a split target's
//! installable releases and could stand in for it there; a target with no
//! update files (the S3, the classic) has no index.

use anyhow::{Context, Result, bail};
use lpc_firmware_release::ReleaseVersion;

/// What `--release` asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseRequest {
    Latest,
    Previous,
    Version(String),
}

impl ReleaseRequest {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "latest" => Ok(Self::Latest),
            "previous" => Ok(Self::Previous),
            _ if ReleaseVersion::parse(s).is_some() => Ok(Self::Version(s.to_string())),
            _ => bail!("`{s}` is not `latest`, `previous`, or a release version (YYYY.MM.DD-N)"),
        }
    }
}

/// One release as a catalog reports it: its version and which release
/// assets it has uploaded (so filtering by `<target>.package.json` is a
/// plain membership check).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRelease {
    pub version: String,
    pub asset_names: Vec<String>,
}

/// Where the GitHub releases list comes from, for `--release previous`.
pub trait ReleaseCatalog {
    /// Printed so `firmware install` can say which source it used.
    fn source_name(&self) -> &'static str;
    /// Every non-draft, non-prerelease release, in no particular order
    /// (newest-first is not assumed — [`previous_version`] sorts itself).
    fn list_releases(&self) -> Result<Vec<CatalogRelease>>;
}

/// The newest release strictly older than `latest_version` that carries
/// `<target>.package.json`.
///
/// Compares numerically (year, month, day, N), never lexicographically:
/// `2026.10.05-10` is newer than `2026.10.05-9`, which a string compare gets
/// backwards. The order is `lpc_firmware_release::ReleaseVersion`'s, the one
/// the release index sorts by; a dev build or a malformed tag does not parse
/// as one and is simply skipped, never chosen by a string sort.
pub fn previous_version(
    catalog: &dyn ReleaseCatalog,
    target: &str,
    latest_version: &str,
) -> Result<String> {
    let wanted = format!("{target}.package.json");
    let latest = ReleaseVersion::parse(latest_version)
        .context("the latest release's version is malformed")?;
    let releases = catalog.list_releases()?;
    let mut best: Option<ReleaseVersion> = None;
    for release in releases {
        if !release.asset_names.iter().any(|name| name == &wanted) {
            continue;
        }
        let Some(version) = ReleaseVersion::parse(&release.version) else {
            continue;
        };
        if version >= latest {
            continue;
        }
        if best.as_ref().is_none_or(|best| version > *best) {
            best = Some(version);
        }
    }
    best.map(|version| version.to_string()).ok_or_else(|| {
        anyhow::anyhow!(
            "no published release older than {latest_version} carries {wanted} (via {})",
            catalog.source_name()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubCatalog {
        source: &'static str,
        releases: Vec<CatalogRelease>,
    }

    impl ReleaseCatalog for StubCatalog {
        fn source_name(&self) -> &'static str {
            self.source
        }
        fn list_releases(&self) -> Result<Vec<CatalogRelease>> {
            Ok(self.releases.clone())
        }
    }

    fn release(version: &str, assets: &[&str]) -> CatalogRelease {
        CatalogRelease {
            version: version.to_string(),
            asset_names: assets.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn parses_the_three_forms() {
        assert_eq!(
            ReleaseRequest::parse("latest").unwrap(),
            ReleaseRequest::Latest
        );
        assert_eq!(
            ReleaseRequest::parse("previous").unwrap(),
            ReleaseRequest::Previous
        );
        assert_eq!(
            ReleaseRequest::parse("2026.10.05-3").unwrap(),
            ReleaseRequest::Version("2026.10.05-3".to_string())
        );
    }

    #[test]
    fn rejects_anything_else() {
        for bad in ["abc1234", "v2026.10.05-3", "Latest", "2026.10.05", ""] {
            assert!(ReleaseRequest::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn previous_is_the_newest_older_release_with_the_targets_asset() {
        let catalog = StubCatalog {
            source: "stub",
            releases: vec![
                release("2026.10.05-3", &["esp32c6-4mb.package.json"]),
                release("2026.10.05-2", &["esp32c6-4mb.package.json"]),
                release("2026.10.05-1", &[]), // no firmware for this target
            ],
        };
        assert_eq!(
            previous_version(&catalog, "esp32c6-4mb", "2026.10.05-3").unwrap(),
            "2026.10.05-2"
        );
    }

    #[test]
    fn previous_compares_numerically_not_lexicographically() {
        let catalog = StubCatalog {
            source: "stub",
            releases: vec![
                release("2026.10.05-9", &["esp32c6-4mb.package.json"]),
                release("2026.10.05-10", &["esp32c6-4mb.package.json"]),
            ],
        };
        // The newest release older than -11 is -10, not -9: a string sort
        // would get this backwards (`"...-10" < "...-9"` lexically).
        assert_eq!(
            previous_version(&catalog, "esp32c6-4mb", "2026.10.05-11").unwrap(),
            "2026.10.05-10"
        );
    }

    #[test]
    fn previous_skips_releases_missing_the_targets_asset() {
        let catalog = StubCatalog {
            source: "stub",
            releases: vec![
                release("2026.10.05-2", &["esp32s3-8mb.package.json"]),
                release("2026.10.05-1", &["esp32c6-4mb.package.json"]),
            ],
        };
        assert_eq!(
            previous_version(&catalog, "esp32c6-4mb", "2026.10.05-3").unwrap(),
            "2026.10.05-1"
        );
    }

    #[test]
    fn previous_fails_when_nothing_qualifies() {
        let catalog = StubCatalog {
            source: "stub",
            releases: vec![],
        };
        let error = previous_version(&catalog, "esp32c6-4mb", "2026.10.05-3").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("no published release older than"),
            "{error}"
        );
    }

    #[test]
    fn previous_ignores_dev_versions_and_releases_not_older_than_latest() {
        let catalog = StubCatalog {
            source: "stub",
            releases: vec![
                release("abc1234-dirty-101500PT", &["esp32c6-4mb.package.json"]),
                release("2026.10.05-3", &["esp32c6-4mb.package.json"]), // == latest
                release("2026.10.05-4", &["esp32c6-4mb.package.json"]), // > latest
                release("2026.10.05-1", &["esp32c6-4mb.package.json"]),
            ],
        };
        assert_eq!(
            previous_version(&catalog, "esp32c6-4mb", "2026.10.05-3").unwrap(),
            "2026.10.05-1"
        );
    }
}
