//! Tree path addressing (`TreePath`); structural sync deltas live in `lpc-wire`.

pub mod tree_path;

pub use tree_path::{NodePathSegment, PathError, TreePath};
