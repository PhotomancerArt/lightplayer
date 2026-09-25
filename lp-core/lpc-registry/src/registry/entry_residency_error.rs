//! Why a residency change was refused.

use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::ArtifactLocation;

/// A refused [`crate::ProjectRegistry::set_entry_resident`] /
/// [`crate::ProjectRegistry::make_only_resident`]. Nothing changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryResidencyError {
    /// No node use at that location in the effective tree (it may itself sit
    /// inside a dormant entry).
    UnknownPlaylist,
    /// The use exists but its effective def is not a loaded playlist.
    NotAPlaylist { def: ArtifactLocation },
    /// The playlist's effective def has no entry with this key.
    UnknownEntry {
        playlist: ArtifactLocation,
        entry: u32,
    },
    /// Unloading would drop artifacts that carry pending overlay edits a
    /// commit would write. A later commit could not write them (their defs
    /// would no longer be in the inventory), so the unload is refused until
    /// they are committed or discarded. Transient edits (Debug-role
    /// overrides, produced paths) never refuse: the unload drops them. `entry` is the entry the refused call targeted (for
    /// [`crate::ProjectRegistry::make_only_resident`], the one to load);
    /// `artifacts` are the def files that would have been unloaded.
    PendingEdits {
        playlist: ArtifactLocation,
        entry: u32,
        artifacts: Vec<ArtifactLocation>,
    },
}

impl core::fmt::Display for EntryResidencyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownPlaylist => write!(f, "no playlist at that location is loaded"),
            Self::NotAPlaylist { def } => {
                write!(f, "{} is not a loaded playlist", def.file_path())
            }
            Self::UnknownEntry { playlist, entry } => {
                write!(f, "playlist {} has no entry {entry}", playlist.file_path())
            }
            Self::PendingEdits {
                playlist,
                entry,
                artifacts,
            } => {
                let files: Vec<String> = artifacts
                    .iter()
                    .map(|artifact| String::from(artifact.file_path().as_str()))
                    .collect();
                write!(
                    f,
                    "cannot change residency for entry {entry} of playlist {}: \
                     pending edits on {} would be unloaded; commit or discard them first",
                    playlist.file_path(),
                    files.join(", ")
                )
            }
        }
    }
}

impl core::error::Error for EntryResidencyError {}
