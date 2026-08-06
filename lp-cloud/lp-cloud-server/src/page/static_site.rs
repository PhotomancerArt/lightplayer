//! The built Studio artifact, as this process sees it.
//!
//! `index.html` is read **once**, at startup, and kept in memory: it is
//! served on nearly every request, and OG injection copies it per share URL.
//! Everything else is read from disk per request, which is a page-cache hit
//! and keeps a deploy that rewrites files under a running process honest.
//!
//! With no artifact configured the site is still a site — a built-in
//! placeholder page with a `</head>` for the injector. That is what lets
//! `just cloud-serve` bring the edge up in seconds without a ten-minute web
//! build, and it is why every page test in this crate can use a tempdir.

use std::path::{Path, PathBuf};

/// The placeholder served when `LP_CLOUD_STATIC_DIR` names nothing.
const PLACEHOLDER_INDEX: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>LightPlayer cloud</title>
</head>
<body>
<h1>LightPlayer cloud</h1>
<p>The service is running with no static app artifact.
Set <code>LP_CLOUD_STATIC_DIR</code> to a <code>just studio-web-deploy-dir</code>
output to serve the Studio.</p>
</body>
</html>
"#;

/// The static half of the page plane.
#[derive(Debug)]
pub struct StaticSite {
    root: Option<PathBuf>,
    index_html: Vec<u8>,
}

impl StaticSite {
    /// Read `root/index.html` into memory, or fall back to the placeholder.
    ///
    /// A configured directory with no `index.html` is a misconfiguration
    /// worth shouting about, but not worth refusing to boot over: the API
    /// and content planes are perfectly useful without the app, and a
    /// service that will not start is a worse outage than one serving a
    /// placeholder.
    pub fn open(root: Option<&Path>) -> Self {
        let Some(root) = root else {
            return Self {
                root: None,
                index_html: PLACEHOLDER_INDEX.as_bytes().to_vec(),
            };
        };

        let index_path = root.join("index.html");
        match std::fs::read(&index_path) {
            Ok(bytes) => {
                log::info!(
                    "static site: {} ({} bytes)",
                    index_path.display(),
                    bytes.len()
                );
                Self {
                    root: Some(root.to_path_buf()),
                    index_html: bytes,
                }
            }
            Err(error) => {
                log::warn!(
                    "static site: no index.html at {} ({error}) — serving the placeholder page",
                    index_path.display()
                );
                Self {
                    root: Some(root.to_path_buf()),
                    index_html: PLACEHOLDER_INDEX.as_bytes().to_vec(),
                }
            }
        }
    }

    /// The cached document, which every SPA route and every share URL is
    /// built from.
    pub fn index_html(&self) -> &[u8] {
        &self.index_html
    }

    /// Read a static file by request path, or `None` if there is no such
    /// file.
    ///
    /// The path is resolved segment by segment against the artifact root and
    /// **never** by string concatenation: `..` and absolute segments are
    /// rejected rather than normalized, so no request can name a file
    /// outside the artifact. `index.html` comes from the in-memory copy so
    /// there is only ever one answer to "what is the document".
    pub fn file(&self, request_path: &str) -> Option<Vec<u8>> {
        let relative = safe_relative_path(request_path)?;
        if relative == Path::new("index.html") {
            return Some(self.index_html.clone());
        }
        let root = self.root.as_ref()?;
        let path = root.join(relative);
        path.is_file().then(|| std::fs::read(path).ok())?
    }

    /// Whether an artifact directory was configured at all.
    pub fn has_artifact(&self) -> bool {
        self.root.is_some()
    }
}

/// A request path as a relative path under the artifact root, or `None` if
/// it tries to escape.
fn safe_relative_path(request_path: &str) -> Option<PathBuf> {
    // One leading slash, not "as many as you like": `//secret.txt` is not a
    // path this service means to answer, and quietly collapsing it would be
    // the same lenience that makes traversal bugs.
    let trimmed = request_path.strip_prefix('/').unwrap_or(request_path);
    if trimmed.is_empty() {
        return None;
    }
    let mut path = PathBuf::new();
    for segment in trimmed.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." || segment.contains('\\') {
            return None;
        }
        path.push(segment);
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_no_artifact_the_placeholder_is_still_a_document() {
        let site = StaticSite::open(None);
        assert!(!site.has_artifact());
        assert!(
            String::from_utf8_lossy(site.index_html()).contains("</head>"),
            "the injector needs a </head> to insert before"
        );
        assert_eq!(site.file("/assets/app.js"), None);
    }

    #[test]
    fn serves_files_and_the_cached_index() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), b"<head></head>").unwrap();
        std::fs::create_dir(dir.path().join("assets")).unwrap();
        std::fs::write(dir.path().join("assets/app-a1b2c3d4.js"), b"console.log(1)").unwrap();

        let site = StaticSite::open(Some(dir.path()));
        assert_eq!(site.index_html(), b"<head></head>");
        assert_eq!(
            site.file("/assets/app-a1b2c3d4.js").as_deref(),
            Some(&b"console.log(1)"[..])
        );
        assert_eq!(site.file("/assets/missing.js"), None);
        // the document always comes from the cached copy
        assert_eq!(
            site.file("/index.html").as_deref(),
            Some(&b"<head></head>"[..])
        );
    }

    /// Traversal is refused, not normalized: a request that names `..` is
    /// never a request this service intends to answer.
    #[test]
    fn no_request_can_escape_the_artifact() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), b"<head></head>").unwrap();
        std::fs::write(dir.path().join("secret.txt"), b"shh").unwrap();
        let site = StaticSite::open(Some(dir.path()));

        for path in [
            "/../secret.txt",
            "/assets/../../secret.txt",
            "/./secret.txt",
            "//secret.txt",
            "/",
        ] {
            assert_eq!(site.file(path), None, "for {path}");
        }
        assert_eq!(site.file("/secret.txt").as_deref(), Some(&b"shh"[..]));
    }
}
