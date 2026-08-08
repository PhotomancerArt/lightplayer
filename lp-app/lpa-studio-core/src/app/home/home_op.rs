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

/// Which scaffold a "New project" gesture starts from (module authoring
/// unit, P4).
///
/// The templates are the New menu's whole vocabulary, so their labels and
/// one-line descriptions live here rather than in the web crate: the menu
/// renders the model, and a fourth template would be one arm here plus one
/// arm in the file generator.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProjectTemplate {
    /// The historical `New`: a pure-blank one-file package (the D17
    /// deviation, 2026-07-27). The library writes the minimal manifest and
    /// root module itself, so this template generates no files at all.
    #[default]
    Blank,
    /// A **pattern project** for a 1D effect: a 300-lamp strand plus the
    /// 32x16 panel, with `effect/` pre-designated as the export.
    Pattern1d,
    /// The same, for a 2D effect: the panel rig alone.
    Pattern2d,
}

impl ProjectTemplate {
    /// Menu row title.
    pub fn label(self) -> &'static str {
        match self {
            Self::Blank => "Blank",
            Self::Pattern1d => "1D pattern project",
            Self::Pattern2d => "2D pattern project",
        }
    }

    /// Menu row second line: what the template puts on the canvas, and —
    /// for the library kinds — what it publishes.
    pub fn description(self) -> &'static str {
        match self {
            Self::Blank => "empty canvas, no nodes",
            Self::Pattern1d => "strip + matrix rig · exports effect/",
            Self::Pattern2d => "matrix rig · exports effect/",
        }
    }

    /// Label the library slugs, dates, and dedupes the new package from.
    /// Kept distinct per template so a workbench does not land in the
    /// gallery indistinguishable from a blank one.
    pub fn default_project_name(self) -> &'static str {
        match self {
            Self::Blank => "Project",
            Self::Pattern1d => "1D pattern",
            Self::Pattern2d => "2D pattern",
        }
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
    /// Create a project from a template and OPEN it — create-and-open is
    /// the gesture: the user lands in the editor with something to do next.
    /// Deviates from D17 (the examples place is unbuilt and node authoring
    /// makes an empty project genuinely useful; see
    /// `docs/adr/2026-07-27-node-authoring-operations.md`). No name
    /// prompt: the template's default label is slugged/dated/deduped by
    /// the library, and rename lives on the card kebab.
    CreateProject {
        template: ProjectTemplate,
    },
    /// Create a project BUILT AROUND a library pattern's export, and open
    /// it (module authoring unit, P5): the pattern-project rig with the
    /// vendored export designated. The card-side twin of the add-node
    /// picker's import — import brings a pattern into what you are already
    /// working on, this hands you a workbench for it.
    ///
    /// Unlike [`Self::CreateProject`] it carries a `name`: the card's
    /// inline form has one, prefilled from the export, because "a project
    /// about someone else's fire module" deserves a name you chose.
    CreateFromPattern {
        /// Source package `prj_…` uid.
        uid: String,
        /// Which export folder to build around.
        export: String,
        /// The new project's label (slugged/dated/deduped by the library).
        name: String,
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
            // The blank arm keeps the bare "New" label the header chip has
            // always worn — it is the button that opens the template menu,
            // and the menu's own rows carry the per-template labels.
            Self::CreateProject {
                template: ProjectTemplate::Blank,
            } => ActionMeta::new(
                "New",
                "Create a blank project and open it.",
                ActionPriority::Secondary,
            )
            .with_icon("add"),
            Self::CreateProject { template } => ActionMeta::new(
                template.label(),
                match template {
                    ProjectTemplate::Pattern1d => {
                        "Create a 1D pattern project — a strand and a panel to judge \
                         the effect on, with `effect/` designated as the export — and \
                         open it."
                    }
                    _ => {
                        "Create a 2D pattern project — a panel to judge the effect on, \
                         with `effect/` designated as the export — and open it."
                    }
                },
                ActionPriority::Secondary,
            )
            .with_icon("add"),
            Self::CreateFromPattern { export, .. } => ActionMeta::new(
                "New project from this…",
                format!(
                    "Create a pattern project built around this project's {export} module — a \
                     rig to judge it on, with your own copy of the module designated as the \
                     export — and open it."
                ),
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
                "Disconnect this device, remove it from the list, and give up the \
                 browser's permission for its port; reconnecting it asks again.",
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
            Self::OpenPackage { .. }
            | Self::OpenExample { .. }
            | Self::CreateProject { .. }
            | Self::CreateFromPattern { .. } => ActionClass::Foreground {
                deadline: PROJECT_LOAD_DEADLINE,
            },
            // Library/registry CRUD is local store work (a device rename's
            // live write-back is one small wire write); the standard budget
            // bounds it.
            Self::RenamePackage { .. }
            | Self::DuplicatePackage { .. }
            | Self::DeletePackage { .. }
            | Self::ImportZip { .. }
            | Self::ImportJson { .. }
            | Self::RenameDevice { .. }
            | Self::NameDevice { .. } => ActionClass::Foreground {
                deadline: PROJECT_ACTION_DEADLINE,
            },
            // A forget takes the live session down and revokes the
            // transport's access before touching the registry, so it owns
            // the connection for the duration — the same reason every
            // `DeviceOp` is recovery-class. Under the local-CRUD budget it
            // shared with the renames, a slow disconnect would time out
            // mid-teardown.
            Self::ForgetDevice { .. } => ActionClass::Recovery,
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
            HomeOp::CreateProject {
                template: ProjectTemplate::Blank,
            },
            HomeOp::CreateProject {
                template: ProjectTemplate::Pattern1d,
            },
            HomeOp::CreateProject {
                template: ProjectTemplate::Pattern2d,
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

    /// The header chip keeps saying "New" — it opens the menu; only the
    /// template ROWS name templates.
    #[test]
    fn the_blank_template_keeps_the_bare_new_label() {
        assert_eq!(
            HomeOp::CreateProject {
                template: ProjectTemplate::Blank,
            }
            .default_action_meta()
            .label,
            "New"
        );
        for template in [ProjectTemplate::Pattern1d, ProjectTemplate::Pattern2d] {
            assert_eq!(
                HomeOp::CreateProject { template }
                    .default_action_meta()
                    .label,
                template.label(),
            );
        }
    }

    /// Every template presents a title, a one-line description, and a
    /// distinct library label — the three strings the New menu renders and
    /// the store slugs from.
    #[test]
    fn every_template_presents_itself() {
        let templates = [
            ProjectTemplate::Blank,
            ProjectTemplate::Pattern1d,
            ProjectTemplate::Pattern2d,
        ];
        for template in templates {
            assert!(!template.label().is_empty());
            assert!(!template.description().is_empty());
            assert!(!template.default_project_name().is_empty());
        }
        for (a, b) in [(0, 1), (0, 2), (1, 2)] {
            assert_ne!(
                templates[a].default_project_name(),
                templates[b].default_project_name(),
            );
        }
        // The historical blank label is what existing slugs were dated
        // from; changing it would rename what "New" makes.
        assert_eq!(ProjectTemplate::Blank.default_project_name(), "Project");
        assert_eq!(ProjectTemplate::default(), ProjectTemplate::Blank);
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

    /// Forget is a DEVICE flow, not library CRUD: it takes the live
    /// session down and revokes the transport's access to the board before
    /// the registry row goes, so it owns the connection and carries no
    /// deadline — the local-CRUD budget it used to share could expire
    /// mid-teardown.
    #[test]
    fn forgetting_a_device_owns_the_connection() {
        assert_eq!(
            HomeOp::ForgetDevice {
                uid: "dev_1".to_string(),
            }
            .action_class(),
            ActionClass::Recovery,
        );
    }

    #[test]
    fn zip_bytes_debug_hides_the_archive() {
        assert_eq!(format!("{:?}", ZipBytes(vec![0; 42])), "ZipBytes(42 bytes)");
    }
}
