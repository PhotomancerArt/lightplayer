//! Where `previous_version`'s "every release" comes from: the `gh` CLI when
//! it is on `PATH` (what `scripts/release/release-firmware.sh` prefers,
//! over this same repo), else the public GitHub REST API.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::release_resolve::{CatalogRelease, ReleaseCatalog};

/// This repo on GitHub — `agent-context.toml`'s `[ship]` block and
/// `scripts/release/release-firmware.sh`'s own default
/// (`${GITHUB_REPOSITORY:-PhotomancerArt/lightplayer}`) agree on it.
const GITHUB_REPO: &str = "PhotomancerArt/lightplayer";

/// One page, same as `release-firmware.sh`'s `per_page=100`: this is a desk
/// command run by hand, not a historical audit, and in practice only the
/// most recent releases carry firmware at all.
const PER_PAGE: u32 = 100;

#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<RawAsset>,
}

#[derive(Deserialize)]
struct RawAsset {
    name: String,
    state: String,
}

/// Draft and prerelease releases are never published firmware (the same
/// filter `release-firmware.sh`'s `gh api --jq` applies); a tag that is not
/// `v<release version>` cannot become a [`CatalogRelease`] at all, since
/// [`previous_version`](super::release_resolve::previous_version) only
/// orders release versions.
fn to_catalog_releases(raw: Vec<RawRelease>) -> Vec<CatalogRelease> {
    raw.into_iter()
        .filter(|r| !r.draft && !r.prerelease)
        .filter_map(|r| {
            let version = r.tag_name.strip_prefix('v')?.to_string();
            let asset_names = r
                .assets
                .into_iter()
                .filter(|a| a.state == "uploaded")
                .map(|a| a.name)
                .collect();
            Some(CatalogRelease {
                version,
                asset_names,
            })
        })
        .collect()
}

/// `gh api repos/<owner>/<repo>/releases` — the same call
/// `release-firmware.sh` makes, minus its `--jq` filter (parsed here
/// instead, so a parse error names what changed rather than mis-scraping).
pub struct GhReleaseCatalog;

impl ReleaseCatalog for GhReleaseCatalog {
    fn source_name(&self) -> &'static str {
        "the `gh` CLI"
    }

    fn list_releases(&self) -> Result<Vec<CatalogRelease>> {
        let gh = gh_binary().context("`gh` is not on PATH")?;
        let output = Command::new(&gh)
            .args([
                "api",
                &format!("repos/{GITHUB_REPO}/releases?per_page={PER_PAGE}"),
            ])
            .output()
            .with_context(|| format!("running {}", gh.display()))?;
        if !output.status.success() {
            bail!(
                "`gh api repos/{GITHUB_REPO}/releases` failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let raw: Vec<RawRelease> = serde_json::from_slice(&output.stdout)
            .context("gh api's releases list was not the JSON this expects")?;
        Ok(to_catalog_releases(raw))
    }
}

/// `GET https://api.github.com/repos/<owner>/<repo>/releases` — the
/// fallback when `gh` is not on `PATH`. Unauthenticated: fine for this
/// public repo at the rate a desk command runs.
pub struct GithubApiReleaseCatalog {
    client: reqwest::blocking::Client,
}

impl GithubApiReleaseCatalog {
    pub fn new() -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("lp-cli-firmware-install")
            .build()
            .context("building the HTTP client")?;
        Ok(Self { client })
    }
}

impl ReleaseCatalog for GithubApiReleaseCatalog {
    fn source_name(&self) -> &'static str {
        "the GitHub API"
    }

    fn list_releases(&self) -> Result<Vec<CatalogRelease>> {
        let url =
            format!("https://api.github.com/repos/{GITHUB_REPO}/releases?per_page={PER_PAGE}");
        let response = self
            .client
            .get(&url)
            .send()
            .with_context(|| format!("GET {url}"))?
            .error_for_status()
            .with_context(|| format!("GET {url}"))?;
        let raw: Vec<RawRelease> = response
            .json()
            .context("the GitHub API's releases list was not the JSON this expects")?;
        Ok(to_catalog_releases(raw))
    }
}

/// `gh` on `PATH` (or named by `$GH_BIN`), else the public API — the choice
/// is printed via [`ReleaseCatalog::source_name`].
pub fn default_catalog() -> Result<Box<dyn ReleaseCatalog>> {
    if gh_binary().is_some() {
        Ok(Box::new(GhReleaseCatalog))
    } else {
        Ok(Box::new(GithubApiReleaseCatalog::new()?))
    }
}

/// `$GH_BIN` when set (an empty value turns `gh` off, same convention as
/// `board_bench::board_binary`'s `$BOARD_BIN`), else the first `gh` on
/// `PATH`.
fn gh_binary() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("GH_BIN") {
        let path = PathBuf::from(explicit);
        return is_executable(&path).then_some(path);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("gh"))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_releases_page_filtering_drafts_and_prereleases() {
        let json = r#"[
            {"tag_name": "v2026.10.05-3", "draft": false, "prerelease": false,
             "assets": [{"name": "esp32c6-4mb.package.json", "state": "uploaded"},
                        {"name": "stale.bin", "state": "starter"}]},
            {"tag_name": "v2026.10.05-2", "draft": true, "prerelease": false, "assets": []},
            {"tag_name": "v2026.10.05-1", "draft": false, "prerelease": true, "assets": []}
        ]"#;
        let raw: Vec<RawRelease> = serde_json::from_str(json).unwrap();
        let releases = to_catalog_releases(raw);
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].version, "2026.10.05-3");
        assert_eq!(releases[0].asset_names, vec!["esp32c6-4mb.package.json"]);
    }

    #[test]
    fn a_tag_without_the_v_prefix_is_skipped() {
        let json =
            r#"[{"tag_name": "nightly", "draft": false, "prerelease": false, "assets": []}]"#;
        let raw: Vec<RawRelease> = serde_json::from_str(json).unwrap();
        assert!(to_catalog_releases(raw).is_empty());
    }

    /// `GH_BIN` is this module's only test of the binary lookup; nothing
    /// else in the crate reads it, so there is no cross-test race the way
    /// `board_bench`'s tests guard `BOARD_BIN` against.
    #[test]
    fn gh_binary_follows_gh_bin() {
        // SAFETY: test-only env mutation; restored before returning.
        unsafe { std::env::set_var("GH_BIN", "/nonexistent/gh") };
        let empty = gh_binary();
        unsafe { std::env::remove_var("GH_BIN") };
        assert!(empty.is_none());
    }
}
