use dioxus::prelude::*;
use dioxus_icons::lucide::{
    Asterisk, Boxes, Check, ChevronDown, ChevronRight, CircleAlert, CircleDot, CircleMinus, Clock,
    Copy, Cpu, Download, Droplet, Ellipsis, Eraser, Eye, FlaskConical, Folder, Funnel, Image, Info,
    Lightbulb, Link2, Link2Off, ListMusic, Locate, LocateFixed, MousePointerClick, Pencil, Play,
    Plus, Radio, Save, Settings, Sparkles, SquareArrowRight, Trash2, TriangleAlert, Undo2, Upload,
    Usb, X, Zap,
};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StudioIcon(name: StudioIconName, size: u32) -> Element {
    match name {
        StudioIconName::Play => rsx! { Play { size } },
        StudioIconName::Usb => rsx! { Usb { size } },
        StudioIconName::Test => rsx! { FlaskConical { size } },
        StudioIconName::StatusRunning => rsx! { Play { size } },
        StudioIconName::StatusIdle => rsx! { CircleMinus { size } },
        StudioIconName::StatusError => rsx! { CircleAlert { size } },
        StudioIconName::StepComplete => rsx! { Check { size } },
        StudioIconName::StepActive => rsx! { Asterisk { size } },
        StudioIconName::StepAttention => rsx! { TriangleAlert { size } },
        StudioIconName::AssignedValue => rsx! { CircleDot { size } },
        StudioIconName::BoundValue => rsx! { Link2 { size } },
        StudioIconName::ChildValue => rsx! { SquareArrowRight { size } },
        StudioIconName::NodeTreeItem => rsx! { Boxes { size } },
        StudioIconName::Edited => rsx! { Pencil { size } },
        StudioIconName::Info => rsx! { Info { size } },
        StudioIconName::InfoBare => rsx! {
            span {
                class: "tw:inline-flex tw:items-center tw:justify-center tw:font-mono tw:font-bold",
                style: "font-size: {size}px; line-height: {size}px;",
                "i"
            }
        },
        StudioIconName::UnboundValue => rsx! { Link2Off { size } },
        StudioIconName::Expanded => rsx! { ChevronDown { size } },
        StudioIconName::Collapsed => rsx! { ChevronRight { size } },
        StudioIconName::NodeSelect => rsx! { Locate { size } },
        StudioIconName::NodeSelected => rsx! { LocateFixed { size } },
        StudioIconName::NodeKind(kind) => match kind {
            NodeKindIcon::Clock => rsx! { Clock { size } },
            NodeKindIcon::Fixture => rsx! { Lightbulb { size } },
            NodeKindIcon::Shader => rsx! { Sparkles { size } },
            NodeKindIcon::Compute => rsx! { Cpu { size } },
            NodeKindIcon::Output => rsx! { Zap { size } },
            NodeKindIcon::Playlist => rsx! { ListMusic { size } },
            NodeKindIcon::Project => rsx! { Folder { size } },
            NodeKindIcon::Texture => rsx! { Image { size } },
            NodeKindIcon::Radio => rsx! { Radio { size } },
            NodeKindIcon::Button => rsx! { MousePointerClick { size } },
            NodeKindIcon::Fluid => rsx! { Droplet { size } },
            NodeKindIcon::Visual => rsx! { Eye { size } },
            NodeKindIcon::Generic => rsx! { Boxes { size } },
        },
        StudioIconName::Save => rsx! { Save { size } },
        StudioIconName::Revert => rsx! { Undo2 { size } },
        StudioIconName::Apply => rsx! { Zap { size } },
        StudioIconName::Settings => rsx! { Settings { size } },
        StudioIconName::Filter => rsx! { Funnel { size } },
        StudioIconName::Eraser => rsx! { Eraser { size } },
        StudioIconName::Add => rsx! { Plus { size } },
        StudioIconName::Remove => rsx! { Trash2 { size } },
        StudioIconName::Cancel => rsx! { X { size } },
        StudioIconName::More => rsx! { Ellipsis { size } },
        StudioIconName::Copy => rsx! { Copy { size } },
        StudioIconName::Download => rsx! { Download { size } },
        StudioIconName::Upload => rsx! { Upload { size } },
    }
}

pub fn action_icon_name(icon: Option<&str>) -> Option<StudioIconName> {
    match icon {
        Some("play") => Some(StudioIconName::Play),
        Some("usb") => Some(StudioIconName::Usb),
        Some("test-tube") => Some(StudioIconName::Test),
        Some("save") => Some(StudioIconName::Save),
        Some("revert") => Some(StudioIconName::Revert),
        Some("apply") => Some(StudioIconName::Apply),
        Some("add") => Some(StudioIconName::Add),
        Some("remove") => Some(StudioIconName::Remove),
        Some("edit") => Some(StudioIconName::Edited),
        Some("copy") => Some(StudioIconName::Copy),
        Some("download") => Some(StudioIconName::Download),
        Some("upload") => Some(StudioIconName::Upload),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StudioIconName {
    Play,
    Usb,
    Test,
    StatusRunning,
    StatusIdle,
    StatusError,
    StepComplete,
    StepActive,
    StepAttention,
    AssignedValue,
    BoundValue,
    ChildValue,
    NodeTreeItem,
    Edited,
    Info,
    InfoBare,
    UnboundValue,
    Expanded,
    Collapsed,
    NodeSelect,
    NodeSelected,
    /// Per-node-type glyph, doubling as the node's select control.
    NodeKind(NodeKindIcon),
    Save,
    Revert,
    /// Lightning bolt: apply the edited asset body to the running project.
    Apply,
    /// Gear: the console's device-settings popover trigger.
    Settings,
    /// Funnel: marks the console's display-level threshold as a filter.
    Filter,
    /// Eraser: the console's Clear control.
    Eraser,
    /// Plus: set/add affordances (option-presence set; composite add).
    Add,
    /// Trash: remove/clear affordances (option-presence clear; entry
    /// removal — the P5 gesture-button glyph direction).
    Remove,
    /// X: dismiss/cancel affordances (the map add-entry key input's cancel
    /// gesture) — distinct from [`Self::Remove`], which destroys a value.
    Cancel,
    /// Ellipsis: the gallery card menu trigger.
    More,
    /// Duplicate/fork-a-copy affordances.
    Copy,
    /// Export-to-file affordances.
    Download,
    /// Import-from-file affordances.
    Upload,
}

/// The per-node-type glyph family. Mapped from the node's human-readable
/// kind label via [`node_kind_icon`]; unknown kinds fall back to `Generic`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeKindIcon {
    Clock,
    Fixture,
    Shader,
    Compute,
    Output,
    Playlist,
    Project,
    Texture,
    Radio,
    Button,
    Fluid,
    Visual,
    Generic,
}

/// Resolve a node's kind label (e.g. "Clock", "Fixture", "Compute") to its
/// type glyph. Matches the labels produced by `node_kind_label` in
/// `lpa-studio-core`; anything unrecognized reads as `Generic` (the cube).
pub fn node_kind_icon(kind_label: &str) -> StudioIconName {
    let kind = match kind_label {
        "Clock" => NodeKindIcon::Clock,
        "Fixture" => NodeKindIcon::Fixture,
        "Shader" => NodeKindIcon::Shader,
        "Compute" => NodeKindIcon::Compute,
        "Output" => NodeKindIcon::Output,
        "Playlist" => NodeKindIcon::Playlist,
        "Project" => NodeKindIcon::Project,
        "Texture" => NodeKindIcon::Texture,
        "Control Radio" | "Radio" => NodeKindIcon::Radio,
        "Button" => NodeKindIcon::Button,
        "Fluid" => NodeKindIcon::Fluid,
        "Visual" => NodeKindIcon::Visual,
        _ => NodeKindIcon::Generic,
    };
    StudioIconName::NodeKind(kind)
}
