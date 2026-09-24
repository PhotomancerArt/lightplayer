//! The canonicalization spec: which package paths count toward the hash.
//!
//! Everything under the reserved `/.lp/` namespace is excluded; everything
//! else — including `/project.json` — is included. `/.lp/` is reserved for
//! machine metadata (the provenance sidecar lands at `/.lp/meta.json`), so
//! metadata churn can never destabilize a package's content hash. This
//! exclusion is part of the hash *specification*, not caller configuration:
//! there are deliberately no knobs.
//!
//! No other exclusion exists. Transient overlay values never reach the
//! filesystem (Save filters them out in the studio editing model), so there
//! is nothing further to exclude. If that assumption ever breaks, stop and
//! surface it — do not silently widen this rule.

use lpfs::LpPath;

/// The reserved metadata namespace, excluded from the canonical hash.
pub const RESERVED_META_DIR: &str = "/.lp";

/// Whether a package path participates in the canonical content hash.
pub fn is_hashed_path(path: &LpPath) -> bool {
    let s = path.as_str();
    s != RESERVED_META_DIR && !s.starts_with("/.lp/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excludes_reserved_namespace() {
        assert!(!is_hashed_path(LpPath::new("/.lp")));
        assert!(!is_hashed_path(LpPath::new("/.lp/meta.json")));
        assert!(!is_hashed_path(LpPath::new("/.lp/nested/deep.json")));
    }

    /// The access sidecar, by name. Cloud push, publish and fork-of-share
    /// move snapshot trees built from exactly the hashed paths, so this one
    /// exclusion is what keeps a project's login keys off the cloud — and
    /// what keeps a secret change from altering the content hash that
    /// bind-by-hash (#781) matches a device on. A future "hash /.lp too"
    /// change must fail here first, and loudly.
    #[test]
    fn the_access_sidecar_is_never_hashed_so_never_pushed_published_or_forked() {
        assert!(!is_hashed_path(LpPath::new("/.lp/access.json")));
    }

    #[test]
    fn includes_everything_else() {
        assert!(is_hashed_path(LpPath::new("/project.json")));
        assert!(is_hashed_path(LpPath::new("/shader.glsl")));
        assert!(is_hashed_path(LpPath::new(
            "/modules/plasma-modx/module.json"
        )));
        // similar-looking but not the reserved dir
        assert!(is_hashed_path(LpPath::new("/.lpx")));
        assert!(is_hashed_path(LpPath::new("/foo/.lp/bar")));
    }
}
