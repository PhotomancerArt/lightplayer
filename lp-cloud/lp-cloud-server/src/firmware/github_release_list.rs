//! Reading GitHub's REST releases list: which releases exist, when each was
//! published, and which assets each has finished uploading.
//!
//! The rules are `lp-cli firmware install --release previous`'s
//! (`release_catalog_source.rs`): drafts and prereleases are never
//! published firmware, a tag must be `v<release version>`, and only an asset
//! in `state: "uploaded"` counts. Every other field is ignored.

use std::collections::BTreeSet;

use lpc_firmware_release::ReleaseVersion;
use serde::Deserialize;

/// One published release, as the list reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedRelease {
    /// The release (from its tag, `v<version>`).
    pub version: ReleaseVersion,
    /// When it was published, as GitHub spells it (RFC 3339, UTC).
    pub published_at: Option<String>,
    /// The names of its assets that have finished uploading.
    pub uploaded_assets: BTreeSet<String>,
}

#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

#[derive(Deserialize)]
struct RawAsset {
    name: String,
    state: String,
}

/// Read a releases list body. A body that is not the list's JSON shape is
/// an error (the caller's `BadUpstream`); a release that is a draft, a
/// prerelease, or tagged other than `v<release version>` is skipped.
pub fn parse_release_list(bytes: &[u8]) -> Result<Vec<ListedRelease>, String> {
    let raw: Vec<RawRelease> =
        serde_json::from_slice(bytes).map_err(|e| format!("releases list: {e}"))?;
    Ok(raw
        .into_iter()
        .filter(|r| !r.draft && !r.prerelease)
        .filter_map(|r| {
            let version = ReleaseVersion::parse(r.tag_name.strip_prefix('v')?)?;
            let uploaded_assets = r
                .assets
                .into_iter()
                .filter(|a| a.state == "uploaded")
                .map(|a| a.name)
                .collect();
            Some(ListedRelease {
                version,
                published_at: r.published_at,
                uploaded_assets,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_published_releases_and_their_uploaded_assets() {
        let json = br#"[
            {"tag_name": "v2026.10.06-19", "draft": false, "prerelease": false,
             "published_at": "2026-10-07T05:29:21Z", "body": "notes", "author": {"login": "x"},
             "assets": [{"name": "esp32c6-4mb.ota-manifest.json", "state": "uploaded", "download_count": 3},
                        {"name": "esp32c6-4mb.core.z", "state": "starter"}]},
            {"tag_name": "v2026.10.06-18", "draft": true, "prerelease": false, "assets": []},
            {"tag_name": "v2026.10.06-17", "draft": false, "prerelease": true, "assets": []},
            {"tag_name": "nightly", "draft": false, "prerelease": false, "assets": []},
            {"tag_name": "vabc1234", "draft": false, "prerelease": false, "assets": []},
            {"tag_name": "v2026.10.06-9", "draft": false, "prerelease": false, "published_at": null, "assets": []}
        ]"#;
        let releases = parse_release_list(json).unwrap();
        assert_eq!(releases.len(), 2);
        assert_eq!(releases[0].version.as_str(), "2026.10.06-19");
        assert_eq!(
            releases[0].published_at.as_deref(),
            Some("2026-10-07T05:29:21Z")
        );
        assert_eq!(
            releases[0].uploaded_assets,
            BTreeSet::from(["esp32c6-4mb.ota-manifest.json".to_string()])
        );
        assert_eq!(releases[1].version.as_str(), "2026.10.06-9");
        assert_eq!(releases[1].published_at, None);
    }

    #[test]
    fn a_body_that_is_not_the_list_is_an_error() {
        assert!(parse_release_list(b"{\"message\":\"API rate limit exceeded\"}").is_err());
        assert!(parse_release_list(b"not json").is_err());
        assert_eq!(parse_release_list(b"[]"), Ok(Vec::new()));
    }
}
