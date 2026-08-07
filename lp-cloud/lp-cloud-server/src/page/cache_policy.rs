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
            "fw_browser_bg.wasm",
            "favicon.ico",
            "logo.svg",
        ] {
            assert_eq!(for_file(name), SHORT, "for {name}");
        }
    }
}
