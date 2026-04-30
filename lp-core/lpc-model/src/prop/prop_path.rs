//! Re-exports for parsing **property** paths: dot fields and array indices, as
//! used by the shared GLSL/struct path layer in `lps_shared`.

use alloc::vec::Vec;

pub use lps_shared::path::{LpsPathSeg as Segment, PathParseError, parse_path};

/// A parsed property path: `field`, `a.b[0]`, `config.spacing`, etc. (wire form
/// is a string; see [`parse_path`] and
/// `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/00-design.md` — `PropPath`
/// = `lps_shared::path`).
pub type PropPath = Vec<Segment>;

#[cfg(test)]
mod tests {
    use super::parse_path;

    #[test]
    fn prop_path_parse_speed() {
        let segs = parse_path("speed").unwrap();
        assert_eq!(segs.len(), 1);
    }

    #[test]
    fn prop_path_parse_nested() {
        let segs = parse_path("config.spacing").unwrap();
        assert_eq!(segs.len(), 2);
    }
}
