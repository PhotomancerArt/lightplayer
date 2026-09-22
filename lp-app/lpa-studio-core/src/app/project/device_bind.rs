//! Binding a running board's project to the library package it already IS.
//!
//! Opening a board is not a push (D19 is about opening a *project*): the
//! board is already running something, and the editor connects to it as it
//! stands. What was missing is the second half — saying *which* library
//! project that running content is. Until something sets
//! `ProjectController::active_library_uid`, the editor is looking at an
//! anonymous storage directory: the address bar cannot heal `/device/<uid>`
//! into `/p/<slug>-prj…?on=…` (D51), the header reads the storage id
//! ("studio") instead of the project's name, and a save has no library copy
//! to pull into.
//!
//! The bind answers that question the only way that cannot lie: by
//! CONTENT. A candidate library package is bound only when its head content
//! hash equals the canonical hash the board computes over its own project
//! directory (the same `lpc_history::hash_package` on both sides). A
//! candidate whose hash differs is NOT bound and NOT written — the board
//! has content this library does not have at head, and silently adopting
//! either side's bytes would lose somebody's work (D4; the divergence UX is
//! its own effort).
//!
//! [`BindOutcome::NoCandidate`] is the seam the adoption step hooks into:
//! the board runs a project no library package answers for, which is a
//! thing to *pull*, not a thing to bind.

use lpc_history::ContentHash;

/// What the lens runtime is running, read once at the top of the bind.
///
/// The two facts travel together because they are only meaningful
/// together: the hash says WHICH project, and the version says which
/// revision that hash was taken at — the baseline a library copy bound to
/// it must carry as its `last_synced`, or the first save would either
/// re-pull the whole project or (worse) skip a write that landed while the
/// bind was running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RunningPackage {
    /// Canonical `lpc_history::hash_package` of the runtime's project
    /// directory — the same function the library runs over its own copy,
    /// which is what makes the two comparable at all.
    pub hash: ContentHash,
    /// The runtime fs revision the hash was read at.
    pub version: lpc_model::FsVersion,
}

/// What one attempt to bind the running project to a library package
/// found.
///
/// Every arm is a normal outcome, never an error: binding is bookkeeping
/// that makes the open richer, and a bind that cannot happen must leave the
/// open exactly as good as it was before this existed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BindOutcome {
    /// The library package is now the active project — no push, no engine
    /// reload, nothing sent to the board.
    Bound {
        /// The `prj…` uid the lens runtime now reports (D51's route
        /// identity).
        uid: String,
    },
    /// The candidate is the right project by NAME but not by content: the
    /// library head and the board disagree (D4). Nothing was bound and
    /// nothing was written; the caller says so in the console.
    Differs {
        uid: String,
        /// The candidate package's head hash, in the library.
        library: ContentHash,
        /// The canonical hash the board reports for what it runs.
        board: ContentHash,
    },
    /// No library package answers for what the board is running — neither
    /// the registry's association nor a scan of library heads matched
    /// (D6). The board's project is not in this library, which is what
    /// adoption is for.
    NoCandidate,
    /// There is nothing to bind: no library is attached (the storeless
    /// demo path), or no running project is connected.
    NotApplicable,
}
