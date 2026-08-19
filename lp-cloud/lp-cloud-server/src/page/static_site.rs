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

/// A static file as negotiation resolved it (see
/// [`StaticSite::file_negotiated`]).
#[derive(Debug, PartialEq, Eq)]
pub struct StaticFile {
    pub bytes: Vec<u8>,
    /// `Some("br")` when the precompressed twin is what `bytes` holds.
    pub encoding: Option<&'static str>,
    /// The identity size of the ORIGINAL file, whatever `bytes` holds.
    pub uncompressed_len: u64,
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

    /// Read a static file with content-encoding negotiation.
    ///
    /// The deploy lays a precompressed twin (`<file>.br`, written by
    /// `scripts/precompress-static.sh` at image build) beside every large
    /// text-like asset. A brotli-accepting request gets the twin's bytes
    /// with `encoding: Some("br")`; everyone else gets the original. Either
    /// way `uncompressed_len` is the ORIGINAL's size — the shell loader and
    /// the engine cache read it (via `x-uncompressed-length`) to show real
    /// download progress, because a fetch reader yields decompressed bytes
    /// that `Content-Length` stops describing once an encoding applies.
    ///
    /// Two deliberate asymmetries:
    ///
    /// - A twin is a *variant*, never a file: a request literally naming
    ///   `foo.wasm.br` answers 404, and a twin with no original beside it
    ///   (a deploy half-done or hand-damaged) is ignored rather than served
    ///   under a name nothing requested.
    /// - The document stays identity-encoded and is not negotiated here:
    ///   it is small, per-request mutated (OG injection), and `no-cache`.
    pub fn file_negotiated(&self, request_path: &str, accepts_brotli: bool) -> Option<StaticFile> {
        if request_path.ends_with(".br") {
            return None;
        }
        let relative = safe_relative_path(request_path)?;
        if relative == Path::new("index.html") {
            return Some(StaticFile {
                uncompressed_len: self.index_html.len() as u64,
                bytes: self.index_html.clone(),
                encoding: None,
            });
        }
        let root = self.root.as_ref()?;
        let path = root.join(relative);
        if !path.is_file() {
            return None;
        }
        let uncompressed_len = std::fs::metadata(&path).ok()?.len();
        if accepts_brotli {
            let twin = {
                let mut twin = path.clone().into_os_string();
                twin.push(".br");
                PathBuf::from(twin)
            };
            if twin.is_file()
                && let Ok(bytes) = std::fs::read(&twin)
            {
                return Some(StaticFile {
                    bytes,
                    encoding: Some("br"),
                    uncompressed_len,
                });
            }
        }
        std::fs::read(path).ok().map(|bytes| StaticFile {
            bytes,
            encoding: None,
            uncompressed_len,
        })
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

    /// The negotiation rules in one place: twin when accepted, original
    /// otherwise, and `uncompressed_len` always describes the original.
    #[test]
    fn a_brotli_twin_serves_only_to_a_request_that_accepts_it() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), b"<head></head>").unwrap();
        std::fs::write(dir.path().join("app-a1b2c3d4.wasm"), b"original bytes").unwrap();
        std::fs::write(dir.path().join("app-a1b2c3d4.wasm.br"), b"br!").unwrap();
        let site = StaticSite::open(Some(dir.path()));

        let negotiated = site.file_negotiated("/app-a1b2c3d4.wasm", true).unwrap();
        assert_eq!(negotiated.bytes, b"br!");
        assert_eq!(negotiated.encoding, Some("br"));
        assert_eq!(negotiated.uncompressed_len, 14);

        let identity = site.file_negotiated("/app-a1b2c3d4.wasm", false).unwrap();
        assert_eq!(identity.bytes, b"original bytes");
        assert_eq!(identity.encoding, None);
        assert_eq!(identity.uncompressed_len, 14);
    }

    /// A twin is a variant, not a file: naming it answers nothing, and an
    /// orphaned twin (no original) is never served at all.
    #[test]
    fn a_twin_is_invisible_as_a_file_of_its_own() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), b"<head></head>").unwrap();
        std::fs::write(dir.path().join("app.wasm"), b"original").unwrap();
        std::fs::write(dir.path().join("app.wasm.br"), b"br!").unwrap();
        std::fs::write(dir.path().join("orphan.js.br"), b"br?").unwrap();
        let site = StaticSite::open(Some(dir.path()));

        assert_eq!(site.file_negotiated("/app.wasm.br", true), None);
        assert_eq!(site.file_negotiated("/app.wasm.br", false), None);
        assert_eq!(site.file_negotiated("/orphan.js", true), None);
    }

    /// A file with no twin negotiates to itself, whatever the client
    /// accepts — the dev artifact has no twins and must keep working.
    #[test]
    fn a_file_without_a_twin_serves_identity_to_everyone() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), b"<head></head>").unwrap();
        std::fs::write(dir.path().join("plain.js"), b"console.log(1)").unwrap();
        let site = StaticSite::open(Some(dir.path()));

        let negotiated = site.file_negotiated("/plain.js", true).unwrap();
        assert_eq!(negotiated.bytes, b"console.log(1)");
        assert_eq!(negotiated.encoding, None);
        assert_eq!(negotiated.uncompressed_len, 14);
        // the in-memory document also answers, identity, with its own size
        let document = site.file_negotiated("/index.html", true).unwrap();
        assert_eq!(document.encoding, None);
        assert_eq!(document.uncompressed_len, document.bytes.len() as u64);
    }
}
