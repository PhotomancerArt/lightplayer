//! The tier table: what each request needs from the link it arrives on.
//!
//! [`classify`] is an exhaustive `match` over [`ClientRequest`] — and over
//! the inner [`WireProjectCommand`] and [`FsRequest`] — with **no wildcard
//! arm**, so a new wire variant does not compile until somebody decides
//! what tier it needs. That is the whole point: the board enforces tiers,
//! and a request nobody classified must never be answered by default.
//!
//! | Needs | Requests |
//! |---|---|
//! | Public | `Hello`, `LoginBegin`, `LoginAnswer` |
//! | Play | `ProjectRead`; `ProjectCommand` `PanelWrite`/`PanelClear`/`ReadOverlay`/`ReadInventory`; `ListAvailableProjects`, `ListLoadedProjects`; read-only fs (`Read`, `ListDir`, `ChangesSince`, `HashPackage`) inside the projects directory |
//! | Edit | everything else: `LoadProject`, `UnloadProject`, `StopAllProjects`, every other `ProjectCommand`, every fs write/delete and every fs read outside the projects directory, `SetLogLevel`, `Reboot`, `ClearFaults`, and the access requests (`AccessList`, `AccessAdd`, `AccessRemove`, `AccessSetSwitches`) |
//!
//! Separately, and on EVERY link at EVERY tier, the fs handlers never
//! return an access file's bytes (`handlers::handle_fs_request`,
//! `file_sync`); that gate is not a tier and lives with the fs code.

use lpc_access::{Tier, is_within_dir};
use lpc_wire::{ClientRequest, WireProjectCommand, server::FsRequest};

/// What a request needs from its link before it is answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Required {
    /// Answered on any link, tier or not: how a link gets a tier at all.
    Public,
    /// Needs at least play.
    Play,
    /// Needs edit.
    Edit,
}

impl Required {
    /// The tier this requirement names; `None` for [`Required::Public`].
    #[must_use]
    pub fn needs(self) -> Option<Tier> {
        match self {
            Self::Public => None,
            Self::Play => Some(Tier::Play),
            Self::Edit => Some(Tier::Edit),
        }
    }

    /// Whether a link holding `held` (`None` = no tier) may be answered.
    #[must_use]
    pub fn permits(self, held: Option<Tier>) -> bool {
        match self.needs() {
            None => true,
            Some(needs) => held.is_some_and(|tier| tier.satisfies(needs)),
        }
    }
}

/// The tier `request` needs. `projects_dir` is the server's projects base
/// directory: play may read project files, not the rest of the device.
#[must_use]
pub fn classify(request: &ClientRequest, projects_dir: &str) -> Required {
    match request {
        ClientRequest::Hello | ClientRequest::LoginBegin | ClientRequest::LoginAnswer { .. } => {
            Required::Public
        }
        ClientRequest::ProjectRead { .. }
        | ClientRequest::ListAvailableProjects
        | ClientRequest::ListLoadedProjects => Required::Play,
        ClientRequest::ProjectCommand { command, .. } => classify_project_command(command),
        ClientRequest::Filesystem(fs_request) => classify_fs(fs_request, projects_dir),
        ClientRequest::LoadProject { .. }
        | ClientRequest::UnloadProject { .. }
        | ClientRequest::StopAllProjects
        | ClientRequest::SetLogLevel { .. }
        | ClientRequest::Reboot
        | ClientRequest::ClearFaults
        | ClientRequest::AccessList
        | ClientRequest::AccessAdd { .. }
        | ClientRequest::AccessRemove { .. }
        | ClientRequest::AccessSetSwitches { .. } => Required::Edit,
    }
}

/// Play turns the panel's knobs and makes every project read (PQ5 — the
/// overlay and the inventory are reads too); everything else a project
/// command does is authoring or runtime control, and needs edit.
///
/// The two reads are safe at play because neither can carry an access
/// file's bytes: both are built from what the project runtime read through
/// `AccessGuardedFs`, which refuses those files, and the device store sits
/// outside every project's chroot. The overlay's pending edits are
/// client-authored values, not file bytes.
fn classify_project_command(command: &WireProjectCommand) -> Required {
    match command {
        WireProjectCommand::PanelWrite { .. }
        | WireProjectCommand::PanelClear { .. }
        | WireProjectCommand::ReadOverlay { .. }
        | WireProjectCommand::ReadInventory { .. } => Required::Play,
        WireProjectCommand::MutateOverlay { .. }
        | WireProjectCommand::CommitOverlay { .. }
        | WireProjectCommand::CreateNode { .. }
        | WireProjectCommand::RemoveNode { .. }
        | WireProjectCommand::NodeCommand { .. }
        | WireProjectCommand::PanelAutoSave { .. } => Required::Edit,
    }
}

/// Play may read inside the projects directory; every write, every delete,
/// and every read of the rest of the device needs edit.
fn classify_fs(request: &FsRequest, projects_dir: &str) -> Required {
    let read_path = match request {
        FsRequest::Read { path } | FsRequest::ListDir { path, .. } => path.as_str(),
        FsRequest::ChangesSince { prefix, .. } | FsRequest::HashPackage { prefix } => {
            prefix.as_str()
        }
        FsRequest::Write { .. }
        | FsRequest::WriteChunk { .. }
        | FsRequest::DeleteFile { .. }
        | FsRequest::DeleteDir { .. } => return Required::Edit,
    };
    if is_within_dir(read_path, projects_dir) {
        Required::Play
    } else {
        Required::Edit
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use lpc_model::AsLpPathBuf;

    #[test]
    fn public_requests_need_nothing() {
        for request in [
            ClientRequest::Hello,
            ClientRequest::LoginBegin,
            ClientRequest::LoginAnswer {
                macs: alloc::vec![],
            },
        ] {
            assert_eq!(classify(&request, "/projects"), Required::Public);
            assert!(Required::Public.permits(None));
        }
    }

    #[test]
    fn play_reads_project_files_only() {
        let read = |path: &str| {
            classify(
                &ClientRequest::Filesystem(FsRequest::Read {
                    path: path.as_path_buf(),
                }),
                "/projects/",
            )
        };
        assert_eq!(read("/projects/choker/project.json"), Required::Play);
        assert_eq!(read("/projects"), Required::Play);
        assert_eq!(read("/hardware.json"), Required::Edit);
        assert_eq!(read("/.lp/device.json"), Required::Edit);
        assert_eq!(read("/projects/../hardware.json"), Required::Edit);
    }

    #[test]
    fn every_fs_mutation_needs_edit_even_inside_projects() {
        let path = "/projects/choker/a.json".as_path_buf();
        for request in [
            FsRequest::Write {
                path: path.clone(),
                data: alloc::vec![],
            },
            FsRequest::WriteChunk {
                path: path.clone(),
                offset: 0,
                data: alloc::vec![],
            },
            FsRequest::DeleteFile { path: path.clone() },
            FsRequest::DeleteDir { path },
        ] {
            assert_eq!(
                classify(&ClientRequest::Filesystem(request), "/projects"),
                Required::Edit
            );
        }
    }

    #[test]
    fn permits_follows_edit_implies_play() {
        assert!(Required::Play.permits(Some(Tier::Play)));
        assert!(Required::Play.permits(Some(Tier::Edit)));
        assert!(!Required::Play.permits(None));
        assert!(Required::Edit.permits(Some(Tier::Edit)));
        assert!(!Required::Edit.permits(Some(Tier::Play)));
    }
}
