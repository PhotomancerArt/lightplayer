//! The editor's ONE toolbar row, data-driven (D1): activities describe
//! themselves as [`ToolbarGroup`]s of [`ToolbarItem`]s and the strip
//! renders whatever it is given, dispatching clicks by id — the fixture ↔
//! mapping morph is a data swap, and the patching view (R5) adds its verbs
//! as another list, never another strip.

use dioxus::prelude::*;
use dioxus_icons::lucide::{
    CircleDashed, Download, Grid3x3, Hash, MousePointer, Pentagon, Route, Scan, Spline, Upload,
};

use crate::base::icon::{StudioIcon, StudioIconName};

/// The icon vocabulary the strip knows how to draw. A closed set keeps
/// items plain data (`PartialEq`, no elements in the model).
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ToolbarIcon {
    Select,
    Grid,
    Ring,
    Path,
    Polygon,
    Numbers,
    Arrows,
    FitPreview,
    Upload,
    Download,
    Save,
    Revert,
}

impl ToolbarIcon {
    fn render(self) -> Element {
        match self {
            Self::Select => rsx! {
                MousePointer { size: 13 }
            },
            Self::Grid => rsx! {
                Grid3x3 { size: 13 }
            },
            Self::Ring => rsx! {
                CircleDashed { size: 13 }
            },
            Self::Path => rsx! {
                Spline { size: 13 }
            },
            Self::Polygon => rsx! {
                Pentagon { size: 13 }
            },
            Self::Numbers => rsx! {
                Hash { size: 13 }
            },
            Self::Arrows => rsx! {
                Route { size: 13 }
            },
            Self::FitPreview => rsx! {
                Scan { size: 13 }
            },
            Self::Upload => rsx! {
                Upload { size: 13 }
            },
            Self::Download => rsx! {
                Download { size: 13 }
            },
            Self::Save => rsx! {
                StudioIcon { name: StudioIconName::Save, size: 13 }
            },
            Self::Revert => rsx! {
                StudioIcon { name: StudioIconName::Revert, size: 13 }
            },
        }
    }
}

/// One strip entry.
#[derive(Clone, PartialEq)]
pub(crate) enum ToolbarItem {
    /// A dispatched action (tool, verb, toggle).
    Button {
        id: &'static str,
        icon: Option<ToolbarIcon>,
        label: Option<String>,
        title: String,
        active: bool,
        enabled: bool,
    },
    /// Breadcrumb-style link: accent text, no button chrome.
    Link {
        id: &'static str,
        label: String,
        title: String,
    },
    /// Inert text: activity labels, counts, save state.
    Status { text: String, kind: StatusKind },
}

/// How inert text reads: an activity LABEL (uppercase tracking), a MONO
/// readout (counts, save state), or an ATTENTION notice (apply failures).
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum StatusKind {
    Label,
    Mono,
    Attention,
}

/// A run of items; groups are separated by hairline dividers, and a
/// `trailing` group rides the right edge.
#[derive(Clone, PartialEq)]
pub(crate) struct ToolbarGroup {
    pub id: &'static str,
    pub trailing: bool,
    pub items: Vec<ToolbarItem>,
}

const BUTTON: &str = "tw:flex tw:cursor-pointer tw:items-center tw:gap-1 tw:rounded tw:border tw:border-border-strong tw:bg-card-subtle tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-[10.5px] tw:text-subtle-foreground tw:hover:text-strong-foreground tw:disabled:cursor-default tw:disabled:opacity-40";
const BUTTON_ON: &str = "tw:flex tw:cursor-pointer tw:items-center tw:gap-1 tw:rounded tw:border tw:border-selection-border tw:bg-selection-bg tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-[10.5px] tw:text-selection-border";

/// The one strip: renders the groups it is given, nothing more.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ToolbarStrip(
    groups: Vec<ToolbarGroup>,
    on_item: EventHandler<&'static str>,
) -> Element {
    rsx! {
        div { class: "tw:flex tw:min-h-[30px] tw:flex-none tw:items-center tw:gap-1.5 tw:border-b tw:border-border-subtle tw:bg-card-muted tw:px-2.5",
            for (group_index, group) in groups.iter().enumerate() {
                div {
                    key: "{group.id}",
                    class: if group.trailing { "tw:ml-auto tw:flex tw:items-center tw:gap-1.5" } else { "tw:flex tw:items-center tw:gap-1.5" },
                    if group_index > 0 && !group.trailing {
                        span { class: "tw:mr-1 tw:h-4 tw:w-px tw:flex-none tw:bg-border-strong" }
                    }
                    for (item_index, item) in group.items.iter().enumerate() {
                        {render_item(group.id, item_index, item, on_item)}
                    }
                }
            }
        }
    }
}

fn render_item(
    group_id: &'static str,
    index: usize,
    item: &ToolbarItem,
    on_item: EventHandler<&'static str>,
) -> Element {
    match item {
        ToolbarItem::Button {
            id,
            icon,
            label,
            title,
            active,
            enabled,
        } => {
            let id = *id;
            rsx! {
                button {
                    key: "{group_id}-{index}",
                    class: if *active { BUTTON_ON } else { BUTTON },
                    title: "{title}",
                    disabled: !enabled,
                    onclick: move |_| on_item.call(id),
                    if let Some(icon) = icon {
                        {icon.render()}
                    }
                    if let Some(label) = label {
                        "{label}"
                    }
                }
            }
        }
        ToolbarItem::Link { id, label, title } => {
            let id = *id;
            rsx! {
                button {
                    key: "{group_id}-{index}",
                    class: "tw:cursor-pointer tw:border-none tw:bg-transparent tw:p-0 tw:text-xs tw:text-selection-border",
                    title: "{title}",
                    onclick: move |_| on_item.call(id),
                    "{label}"
                }
            }
        }
        ToolbarItem::Status { text, kind } => rsx! {
            span {
                key: "{group_id}-{index}",
                class: match kind {
                    StatusKind::Label => "tw:text-[10px] tw:font-semibold tw:uppercase tw:tracking-[0.13em] tw:text-muted-foreground",
                    StatusKind::Mono => "tw:font-mono tw:text-[10px] tw:text-dim-foreground",
                    StatusKind::Attention => "tw:font-mono tw:text-[10px] tw:text-status-attention-foreground",
                },
                "{text}"
            }
        },
    }
}
