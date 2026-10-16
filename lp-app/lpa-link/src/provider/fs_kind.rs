//! Which filesystem a firmware image mounts on `lpfs` — part of what a
//! layout inspection reports, so the vocabulary lives with it (every build),
//! and the detection with the migration (`layout_migration::fs_kind`).

/// A filesystem kind.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum FsKind {
    /// littlefs (`littlefs-rust`), the firmware's filesystem until the
    /// tree-store cutover.
    #[default]
    Littlefs,
    /// The tree store (`lp-tree-store`, format version 3).
    Tree,
}

impl FsKind {
    /// The kind in words, for a refusal or a summary.
    pub fn word(self) -> &'static str {
        match self {
            Self::Littlefs => "littlefs",
            Self::Tree => "tree store",
        }
    }
}
