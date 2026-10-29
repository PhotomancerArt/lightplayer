//! How a path renders its segments: one for the whole path, or one per
//! subpath, when its subpaths are patched apart (names.html "Segment": "a
//! path may also split into one segment per subpath").

use serde::{Deserialize, Serialize};

/// One segment, or one per subpath. Written `"one"` (left out: the
/// default) or `"subpaths"` in map2d format 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathSplit {
    /// The whole path is one segment, at the path's own id.
    #[default]
    One,
    /// One segment per subpath that holds lamps, keyed `s1`, `s2`… by the
    /// subpath's place in the drawing.
    Subpaths,
}
