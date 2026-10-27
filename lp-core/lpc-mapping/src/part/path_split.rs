//! How a path renders its segments: one for the whole path, or one per
//! subpath, when its subpaths are patched apart (names.html "Segment": "a
//! path may also split into one segment per subpath").

/// One segment, or one per subpath.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PathSplit {
    /// The whole path is one segment, at the path's own id.
    #[default]
    One,
    /// One segment per subpath that holds lamps, keyed `s1`, `s2`… by the
    /// subpath's place in the drawing.
    Subpaths,
}
