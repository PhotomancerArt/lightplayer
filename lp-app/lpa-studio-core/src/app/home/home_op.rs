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
    /// Was the ONE way to start a project (D17: "new project" is the
    /// examples place) until 2026-07-27, when [`Self::CreateProject`]
    /// deviated from D17 (`docs/adr/2026-07-27-node-authoring-operations.md`).
    OpenExample {
        id: String,
    },
    /// Create a pure-blank one-file project and OPEN it — create-and-open
    /// is the gesture: the user lands in the empty editor with the
    /// add-node picker. Deviates from D17 (the examples place is unbuilt
    /// and node authoring makes an empty project genuinely useful; see
    /// `docs/adr/2026-07-27-node-authoring-operations.md`). No name
    /// prompt: the default "Project" label is slugged/dated/deduped by
    /// the library, and rename lives on the card kebab.
    CreateProject,
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
    /// Install a package from a pasted `lp.package` share envelope
    /// (Cmd-V anywhere on the gallery, or the explicit paste affordance).
    ImportJson {
        text: String,
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
    /// card-anchored, never a dialog. `target` is that card (M4): with
    /// two blank boards attached, naming one must not stamp the other.
    NameDevice {
        target: crate::DeviceTarget,
        name: String,
    },
    /// Open the setup wizard on a target — the two half-height entry
    /// cards, "connect a device" and "simulate a device". The wizard is a
    /// card in the devices grid (flow design, F5b); this is what puts it
    /// there. Exactly one runs at a time.
    StartSetup {
        /// THE simulator rather than a board on the wire. The MACHINE
        /// never asks this — it asks the target's capabilities — but the
        /// gesture has to say which target it means.
        sim: bool,
    },
    /// One gesture on the open setup wizard. The reducer decides what it
    /// means; a gesture in a state that has no arm for it is inert by
    /// design (`docs/design/device-setup-flow.md` §2).
    Setup(crate::app::setup_flow::SetupGesture),
    /// Mutate a card's UI VIEW-STATE (select tab / open or close a sheet).
    /// A pure, synchronous view-state change — no wire, no library — kept
    /// core-owned so it survives the card ⇄ pane growth and is
    /// e2e-drivable (2026-07-25 re-home).
    CardUi(crate::app::home::card_ui_state::CardUiOp),
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
            Self::CreateProject => ActionMeta::new(
                "New",
                "Create a blank project and open it.",
                ActionPriority::Secondary,
            )
            .with_icon("add"),
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
            Self::ImportJson { .. } => ActionMeta::new(
                "Paste project",
                "Install a project from a pasted JSON envelope.",
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
            Self::StartSetup { sim: false } => ActionMeta::new(
                "Connect a device",
                "Set up a board on the end of a USB cable.",
                ActionPriority::Primary,
            )
            .with_icon("usb"),
            Self::StartSetup { sim: true } => ActionMeta::new(
                "Simulate a device",
                "Set up the simulator as a board — no hardware needed.",
                ActionPriority::Secondary,
            )
            .with_icon("play"),
            Self::Setup(_) => ActionMeta::new(
                "Set up",
                "Take the next step in setting up this device.",
                ActionPriority::Primary,
            ),
            Self::CardUi(_) => ActionMeta::new(
                "Card view",
                "Change what this card is showing.",
                ActionPriority::Tertiary,
            ),
        }
    }

    fn action_class(&self) -> ActionClass {
        match self {
            // Opens push files to the runtime and load the project — the
            // demo-load quiet-gap budget fits. Create-and-open ends in the
            // same open, so it shares the budget.
            Self::OpenPackage { .. } | Self::OpenExample { .. } | Self::CreateProject => {
                ActionClass::Foreground {
                    deadline: PROJECT_LOAD_DEADLINE,
                }
            }
            // Library/registry CRUD is local store work (a device rename's
            // live write-back is one small wire write); the standard budget
            // bounds it.
            Self::RenamePackage { .. }
            | Self::DuplicatePackage { .. }
            | Self::DeletePackage { .. }
            | Self::ImportZip { .. }
            | Self::ImportJson { .. }
            | Self::RenameDevice { .. }
            | Self::ForgetDevice { .. }
            | Self::NameDevice { .. } => ActionClass::Foreground {
                deadline: PROJECT_ACTION_DEADLINE,
            },
            // A pure view-state flip — synchronous, no wire; run it
            // inline like any local gesture (the standard budget never
            // engages because the handler never awaits).
            Self::CardUi(_) | Self::StartSetup { .. } => ActionClass::Foreground {
                deadline: PROJECT_ACTION_DEADLINE,
            },
            // A wizard gesture can turn into a flash, a probe, or a
            // bootloader connect — the device-flow class, for the same
            // reason every `DeviceOp` carries it: those own the connection
            // until they finish, and a deadline would fire mid-write.
            Self::Setup(_) => ActionClass::Recovery,
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
            HomeOp::CreateProject,
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
