//! How long a static file may be cached.
//!
//! Two rules and a default:
//!
//! - **HTML is never cached.** Every deploy changes which asset names the
//!   document points at, and a cached document points at names that are
//!   gone. This is also the document OG tags are injected into, and those
//!   differ per share URL.
//! - **A content-hashed asset is cached forever.** `main-a1b2c3d4.js` cannot
//!   change meaning: a new build is a new name.
//! - **Everything else gets five minutes** — long enough to matter on a page
//!   load, short enough that a stale `manifest.json` or `dev-settings.json`
//!   fixes itself while you are still looking at it.
//!
//! The middle rule is a heuristic over file names, so it is worth being
//! precise about which way it may err: a *false negative* (a hashed file
//! treated as unhashed) costs a revalidation, while a *false positive*
//! (a stable name pinned for a year) is a file that cannot be updated. The
//! test at the bottom holds the real artifact's stable names — firmware
//! manifests, the wasm sidecar — on the safe side of that line.

/// Never cache: the document changes on every deploy and per share URL.
pub const NO_CACHE: &str = "no-cache";

/// Cache forever: content-addressed by name.
pub const IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// The default for everything else.
pub const SHORT: &str = "public, max-age=300";

/// The `Cache-Control` for a static file, by name.
pub fn for_file(file_name: &str) -> &'static str {
    if file_name.ends_with(".html") {
        NO_CACHE
    } else if looks_content_hashed(file_name) {
        IMMUTABLE
    } else {
        SHORT
    }
}

/// Whether a file name carries a build hash.
///
/// The shape every bundler emits: a segment of at least eight alphanumeric
/// characters mixing letters and digits (`main-a1b2c3d4.js`,
/// `assets/dxh1f2e3d4c5b6.wasm`). Words do not mix in digits, and versions
/// are too short.
fn looks_content_hashed(file_name: &str) -> bool {
    file_name.split(['-', '.', '_']).any(|segment| {
        segment.len() >= 8
            && segment.chars().all(|c| c.is_ascii_alphanumeric())
            && segment.chars().any(|c| c.is_ascii_digit())
            && segment.chars().any(|c| c.is_ascii_alphabetic())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_is_always_revalidated() {
        assert_eq!(for_file("index.html"), NO_CACHE);
        assert_eq!(for_file("404.html"), NO_CACHE);
    }

    #[test]
    fn hashed_bundles_are_immutable() {
        for name in [
            "main-a1b2c3d4.js",
            "lpa_studio_web_bg-9f8e7d6c5b4a.wasm",
            "index-4e2b19ca.css",
            // The fw-browser engine sidecar (P2, hashed engine sidecar):
            // `scripts/sync-engine-sidecar.sh` names these, growing the hash
            // segment past 16 hex chars if it ever fails to mix a digit and
            // a letter — see `sixteen_hex_chars_are_not_always_mixed` below
            // for why that guard exists at all.
            "fw_browser-a1b2c3d4e5f60718.js",
            "fw_browser_bg-a1b2c3d4e5f60718.wasm",
        ] {
            assert_eq!(for_file(name), IMMUTABLE, "for {name}");
        }
    }

    /// The names in the real artifact that must stay updatable. Pinning any
    /// of these for a year would outlive several deploys.
    #[test]
    fn stable_names_are_not_pinned_for_a_year() {
        for name in [
            "manifest.json",
            "dev-settings.json",
            // The engine sidecar itself is hashed (see
            // `hashed_bundles_are_immutable`) — only the tiny manifest that
            // points at it stays on the short tier, the same as any other
            // small pointer file that a deploy can change underneath it.
            "engine-manifest.json",
            "favicon.ico",
            "logo.svg",
        ] {
            assert_eq!(for_file(name), SHORT, "for {name}");
        }
    }

    /// `scripts/sync-engine-sidecar.sh` grows its hash slice past 16 hex
    /// characters when the first 16 fail to mix a digit and a letter — this
    /// is why that guard exists: a plausible (if rare) hex slice can be
    /// all-digit or all-letter, and `looks_content_hashed` would silently
    /// misclassify it as an unhashed name (5-minute cache instead of
    /// immutable).
    #[test]
    fn sixteen_hex_chars_are_not_always_mixed() {
        assert!(!looks_content_hashed("fw_browser-1111111111111111.js"));
        assert!(!looks_content_hashed("fw_browser-abcdefabcdefabcd.js"));
        // A longer slice that mixes both classes is what the script falls
        // back to in that case, and must be recognized.
        assert!(looks_content_hashed("fw_browser-1111111111111111a.js"));
    }
}
