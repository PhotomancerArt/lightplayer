use dioxus::prelude::*;
use dioxus_icons::lucide::{
    Activity, Asterisk, Bot, Boxes, ChartLine, Check, ChevronDown, ChevronRight, CircleAlert,
    CircleDot, CircleMinus, Clock, Copy, Cpu, Download, Droplet, Ellipsis, Eraser, Eye, Flag,
    FlaskConical, Folder, Funnel, Hash, Image, Info, Layers, Lightbulb, Link2, Link2Off, ListMusic,
    Locate, LocateFixed, Maximize2, Minimize2, MonitorPlay, MousePointerClick, Pencil, Play, Plus,
    Radio, Route, Save, Settings, Sparkles, SquareArrowRight, SquareTerminal, Trash2,
    TriangleAlert, Undo2, Upload, Usb, Waypoints, X, Zap,
};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StudioIcon(name: StudioIconName, size: u32) -> Element {
    match name {
        StudioIconName::Play => rsx! { Play { size } },
        StudioIconName::Usb => rsx! { Usb { size } },
        StudioIconName::Simulator => rsx! { MonitorPlay { size } },
        StudioIconName::Test => rsx! { FlaskConical { size } },
        StudioIconName::StatusRunning => rsx! { Play { size } },
        StudioIconName::StatusIdle => rsx! { CircleMinus { size } },
        StudioIconName::StatusError => rsx! { CircleAlert { size } },
        StudioIconName::StepComplete => rsx! { Check { size } },
        StudioIconName::StepActive => rsx! { Asterisk { size } },
        StudioIconName::StepAttention => rsx! { TriangleAlert { size } },
        StudioIconName::AssignedValue => rsx! { CircleDot { size } },
        StudioIconName::BoundValue => rsx! { Link2 { size } },
        StudioIconName::Bus => rsx! { Waypoints { size } },
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
        StudioIconName::AgentSettings => rsx! { Bot { size } },
        StudioIconName::Filter => rsx! { Funnel { size } },
        StudioIconName::Eraser => rsx! { Eraser { size } },
        StudioIconName::Add => rsx! { Plus { size } },
        StudioIconName::Remove => rsx! { Trash2 { size } },
        StudioIconName::Cancel => rsx! { X { size } },
        StudioIconName::More => rsx! { Ellipsis { size } },
        StudioIconName::Copy => rsx! { Copy { size } },
        StudioIconName::Download => rsx! { Download { size } },
        StudioIconName::Upload => rsx! { Upload { size } },
        StudioIconName::Grow => rsx! { Maximize2 { size } },
        StudioIconName::Shrink => rsx! { Minimize2 { size } },
        StudioIconName::MapNumbers => rsx! { Hash { size } },
        StudioIconName::MapArrows => rsx! { Route { size } },
        StudioIconName::MapUniverses => rsx! { Layers { size } },
        StudioIconName::MapLive => rsx! { Activity { size } },
        StudioIconName::Console => rsx! { SquareTerminal { size } },
        StudioIconName::Cue => rsx! { Flag { size } },
        StudioIconName::Agent => rsx! { Sparkles { size } },
        StudioIconName::Performance => rsx! { ChartLine { size } },
        StudioIconName::Danger => rsx! { TriangleAlert { size } },
    }
}

pub fn action_icon_name(icon: Option<&str>) -> Option<StudioIconName> {
    match icon {
        Some("play") => Some(StudioIconName::Play),
        Some("usb") => Some(StudioIconName::Usb),
        Some("test-tube") => Some(StudioIconName::Test),
        Some("save") => Some(StudioIconName::Save),
        Some("revert") => Some(StudioIconName::Revert),
        Some("apply") | Some("zap") => Some(StudioIconName::Apply),
        Some("add") => Some(StudioIconName::Add),
        Some("remove") => Some(StudioIconName::Remove),
        Some("edit") => Some(StudioIconName::Edited),
        Some("copy") => Some(StudioIconName::Copy),
        Some("download") => Some(StudioIconName::Download),
        Some("upload") => Some(StudioIconName::Upload),
        Some("grow") => Some(StudioIconName::Grow),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StudioIconName {
    Play,
    Usb,
    /// The sim runtime's card glyph — where a device card shows its
    /// transport, a sim card shows this instead (D36).
    Simulator,
    Test,
    StatusRunning,
    StatusIdle,
    StatusError,
    StepComplete,
    StepActive,
    StepAttention,
    AssignedValue,
    BoundValue,
    /// Waypoints: the bus — shared visual language wherever a bus channel
    /// appears (channel cards, binding chips, picker rows, popup wiring).
    Bus,
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
    /// Bot: the AI/agent-settings trigger. Deliberately distinct from
    /// [`Self::Settings`] (the gear stays reserved for a future real
    /// studio-settings surface) and from [`Self::Agent`] (the sparkles
    /// role marker on the node face's agent section).
    AgentSettings,
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
    /// Hash: wiring-order numbers on the mapping lamp view.
    MapNumbers,
    /// Route: wiring-direction arrows on the mapping lamp view.
    MapArrows,
    /// Layers: DMX-universe coloring on the mapping lamp view.
    MapUniverses,
    /// Activity: live output colors on the mapping lamp view.
    MapLive,
    /// Duplicate/fork-a-copy affordances.
    Copy,
    /// Export-to-file affordances.
    Download,
    /// Import-from-file affordances.
    Upload,
    /// Diagonal expand arrows: the device card's always-visible grow
    /// control (D40) — the ONE editor entry (and, at M7′ P3, the
    /// card→pane growth toggle).
    Grow,
    /// Diagonal collapse arrows: the grown pane's shrink control (D43) —
    /// back to the gallery card.
    Shrink,
    /// Terminal: the device card's Console tab (D42).
    Console,
    /// Flag: a playlist entry that waits for a trigger (cue) instead of
    /// auto-advancing on a duration.
    Cue,
    /// Sparkles: the shader-editing agent — the role marker on the node
    /// face's agent section (P2b item 2).
    Agent,
    /// Line chart: the device card's data-adaptive Performance tab.
    Performance,
    /// Warning triangle: the device card's Danger tab.
    Danger,
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

/// Resolve a node's kind label (e.g. "Clock", "Fixture", "Compute") or kind
/// slug (e.g. "clock", "compute_shader" — the icon tokens the add-node
/// picker entries carry, `node_kind_slug` in `lpa-studio-core`) to its type
/// glyph. Matches the labels produced by `node_kind_label` in
/// `lpa-studio-core`; anything unrecognized reads as `Generic` (the cube).
pub fn node_kind_icon(kind_label: &str) -> StudioIconName {
    let kind = match kind_label {
        "Clock" | "clock" => NodeKindIcon::Clock,
        "Fixture" | "fixture" => NodeKindIcon::Fixture,
        "Shader" | "shader" => NodeKindIcon::Shader,
        "Compute" | "Compute shader" | "compute_shader" => NodeKindIcon::Compute,
        "Output" | "output" => NodeKindIcon::Output,
        "Playlist" | "playlist" => NodeKindIcon::Playlist,
        "Project" | "project" => NodeKindIcon::Project,
        "Texture" | "texture" => NodeKindIcon::Texture,
        "Control Radio" | "Radio" | "radio" => NodeKindIcon::Radio,
        "Button" | "button" => NodeKindIcon::Button,
        "Fluid" | "fluid" => NodeKindIcon::Fluid,
        "Visual" => NodeKindIcon::Visual,
        _ => NodeKindIcon::Generic,
    };
    StudioIconName::NodeKind(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_picker_kind_slug_resolves_to_a_specific_glyph() {
        // The add-node picker's entry icons are kind slugs
        // (`node_kind_slug`); none of them may fall through to the generic
        // cube.
        for slug in [
            "shader",
            "texture",
            "playlist",
            "clock",
            "fixture",
            "output",
            "fluid",
            "compute_shader",
            "button",
            "radio",
        ] {
            assert_ne!(
                node_kind_icon(slug),
                StudioIconName::NodeKind(NodeKindIcon::Generic),
                "slug {slug} must map to its own kind glyph"
            );
        }
    }

    #[test]
    fn labels_and_slugs_agree_on_the_glyph() {
        assert_eq!(node_kind_icon("Shader"), node_kind_icon("shader"));
        assert_eq!(
            node_kind_icon("Compute shader"),
            node_kind_icon("compute_shader")
        );
        assert_eq!(node_kind_icon("Radio"), node_kind_icon("radio"));
    }
}
