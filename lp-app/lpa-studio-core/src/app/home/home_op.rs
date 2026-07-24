//! Gallery ops: open, create, and manage library packages from home.

use core::any::Any;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_ACTION_DEADLINE,
    PROJECT_LOAD_DEADLINE,
};

/// The node id home-gallery actions target. The gallery has no controller
/// struct of its own; `StudioController` routes these ops directly.
pub const HOME_NODE_ID: &str = "studio|home";

/// Zip archive bytes riding an import action. `Debug` prints the byte count,
/// not the archive.
#[derive(Clone, Eq, PartialEq)]
pub struct ZipBytes(pub Vec<u8>);

impl core::fmt::Debug for ZipBytes {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "ZipBytes({} bytes)", self.0.len())
    }
}

/// One home-gallery gesture. Package identity travels as the `prj_…` uid
/// string straight off the card view model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HomeOp {
    /// Open a library package — by slug (URLs) or `prj_…` uid (cards) —
    /// pushing its head to the simulator (D13/D19).
    OpenPackage {
        key: String,
    },
    /// Open an example: seed it into the library once, then open the copy.
    /// Also the ONE way to start a project (D17: "new project" is the
    /// examples place; the empty library points here).
    OpenExample {
        id: String,
    },
    RenamePackage {
        uid: String,
        name: String,
    },
    DuplicatePackage {
        uid: String,
    },
    DeletePackage {
        uid: String,
    },
    /// Install a package from zip bytes (button or drag-anywhere import).
    ImportZip {
        file_name: String,
        bytes: ZipBytes,
    },
    /// Rename a device (D34, inline on the card): registry always; a live
    /// session also writes the identity back to the device.
    RenameDevice {
        uid: String,
        name: String,
    },
    /// Forget a remembered device (D34 hygiene, offline-card popup).
    ForgetDevice {
        uid: String,
    },
    /// Name an anonymous connected device (the Needs-a-name card's inline
    /// form): mints a `dev_` uid and stamps the identity over the wire —
    /// card-anchored, never a dialog.
    NameDevice {
        name: String,
    },
}

impl ControllerOp for HomeOp {
    fn default_action_meta(&self) -> ActionMeta {
        match self {
            Self::OpenPackage { .. } => ActionMeta::new(
                "Open",
                "Open this project in the simulator.",
                ActionPriority::Primary,
            )
            .with_icon("play"),
            Self::OpenExample { .. } => ActionMeta::new(
                "Open example",
                "Run this example; it becomes yours on first save.",
                ActionPriority::Primary,
            )
            .with_icon("play"),
            Self::RenamePackage { .. } => {
                ActionMeta::new("Rename", "Rename this project.", ActionPriority::Secondary)
                    .with_icon("edit")
            }
            Self::DuplicatePackage { .. } => ActionMeta::new(
                "Duplicate",
                "Fork an independent copy of this project.",
                ActionPriority::Secondary,
            )
            .with_icon("copy"),
            Self::DeletePackage { .. } => ActionMeta::new(
                "Delete",
                "Delete this project and its history from your library.",
                ActionPriority::Tertiary,
            )
            .with_icon("remove")
            .destructive(),
            Self::ImportZip { .. } => ActionMeta::new(
                "Import zip",
                "Install a project from a zip archive.",
                ActionPriority::Secondary,
            )
            .with_icon("upload"),
            Self::RenameDevice { .. } => ActionMeta::new(
                "Rename device",
                "Rename this device; a connected device is updated too.",
                ActionPriority::Secondary,
            )
            .with_icon("edit"),
            Self::ForgetDevice { .. } => ActionMeta::new(
                "Forget device",
                "Remove this device from the list; connecting it again re-adds it.",
                ActionPriority::Tertiary,
            )
            .with_icon("remove")
            .destructive(),
            Self::NameDevice { .. } => ActionMeta::new(
                "Name device",
                "Name this device so Studio remembers it.",
                ActionPriority::Primary,
            )
            .with_icon("edit"),
        }
    }

    fn action_class(&self) -> ActionClass {
        match self {
            // Opens push files to the runtime and load the project — the
            // demo-load quiet-gap budget fits.
            Self::OpenPackage { .. } | Self::OpenExample { .. } => ActionClass::Foreground {
                deadline: PROJECT_LOAD_DEADLINE,
            },
            // Library/registry CRUD is local store work (a device rename's
            // live write-back is one small wire write); the standard budget
            // bounds it.
            Self::RenamePackage { .. }
            | Self::DuplicatePackage { .. }
            | Self::DeletePackage { .. }
            | Self::ImportZip { .. }
            | Self::RenameDevice { .. }
            | Self::ForgetDevice { .. }
            | Self::NameDevice { .. } => ActionClass::Foreground {
                deadline: PROJECT_ACTION_DEADLINE,
            },
        }
    }

    fn clone_box(&self) -> Box<dyn ControllerOp> {
        Box::new(self.clone())
    }

    fn eq_op(&self, other: &dyn ControllerOp) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_use_the_project_load_deadline() {
        for op in [
            HomeOp::OpenPackage {
                key: "prj_1".to_string(),
            },
            HomeOp::OpenExample {
                id: "examples/basic".to_string(),
            },
        ] {
            assert_eq!(
                op.action_class(),
                ActionClass::Foreground {
                    deadline: PROJECT_LOAD_DEADLINE,
                },
                "{op:?}"
            );
        }
    }

    #[test]
    fn library_crud_uses_the_project_action_deadline() {
        for op in [
            HomeOp::RenamePackage {
                uid: "prj_1".to_string(),
                name: "n".to_string(),
            },
            HomeOp::DuplicatePackage {
                uid: "prj_1".to_string(),
            },
            HomeOp::DeletePackage {
                uid: "prj_1".to_string(),
            },
            HomeOp::ImportZip {
                file_name: "a.zip".to_string(),
                bytes: ZipBytes(vec![1, 2]),
            },
            HomeOp::RenameDevice {
                uid: "dev_1".to_string(),
                name: "n".to_string(),
            },
            HomeOp::ForgetDevice {
                uid: "dev_1".to_string(),
            },
        ] {
            assert_eq!(
                op.action_class(),
                ActionClass::Foreground {
                    deadline: PROJECT_ACTION_DEADLINE,
                },
                "{op:?}"
            );
        }
    }

    #[test]
    fn zip_bytes_debug_hides_the_archive() {
        assert_eq!(format!("{:?}", ZipBytes(vec![0; 42])), "ZipBytes(42 bytes)");
    }
}
