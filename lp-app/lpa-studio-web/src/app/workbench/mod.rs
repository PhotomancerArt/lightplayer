//! The studio workbench — the editor chrome (PanelDock model).
//!
//! An IntelliJ-shaped frame on the project editor routes: a full-height
//! `[strip][dock][center][dock][strip]` row under the site header. The
//! **shell owns the docks**; the center's **view tabs** (Nodes · Mapping)
//! own only the center; the **user owns visibility** through the edge
//! strips. Nothing rearranges itself:
//!
//! - Four panels with FIXED homes — left: Nodes (the project pane) ·
//!   Fixtures; right: Device · Outputs. The assignment lives in
//!   [`PanelId::side`], a data table by design, so experiments are a
//!   constant edit — but there is deliberately no user arrangement in v1
//!   ("things have one home", spike round-2 ruling).
//! - Edge strips toggle panels **radio-per-side**: clicking a strip
//!   button switches the side's open panel; re-clicking collapses the
//!   side. One panel per side keeps the dense panels honest (the
//!   radiance-scale Outputs rail never shares a column).
//! - Panel visibility is remembered **per view** ([`PanelMemory`]),
//!   seeded by each view's defaults — the Nodes view opens the Nodes
//!   tree + Device, the Mapping view opens Fixtures + Outputs — so
//!   switching views feels like the view helping, never like the room
//!   rearranging. The memory is ephemeral by design (spike A2): losing
//!   it on reload costs one click.
//!
//! Design record: `spikes/studio-chrome/index.html` (rounds 1–3, ratified
//! 2026-08-12). The Mapping view's center is a placeholder until the
//! unified-editor plan mounts the arrange canvas here; the Fixtures and
//! Outputs panels arrive with that plan's substrate (this module's P2).

#[cfg(feature = "stories")]
pub(crate) mod workbench_stories;

use dioxus::prelude::*;
use lpa_studio_core::{ProjectEditorView, UiAction, UiDeviceCard, UiPaneView};

use crate::app::ProjectNodeWorkspace;
use crate::core::PaneView;

/// Which center the workbench renders — the route's view suffix
/// ([`crate::router::ProjectView`]), narrowed to the views the workbench
/// hosts (play and patch short-circuit before the workbench mounts).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WorkbenchView {
    #[default]
    Nodes,
    Mapping,
}

/// The four dockable panels. `side` is the fixed-home table (ratified:
/// content/structure left, hardware right).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelId {
    /// The project pane — the node tree, add-node, save/change UI.
    /// Named "Nodes" in the UI: the whole workbench is the project.
    Nodes,
    /// fixture → object → instance with channel chips (P2).
    Fixtures,
    /// The lens session's device card (D43), docked.
    Device,
    /// box → port → wire-window cells (P2).
    Outputs,
}

/// Which dock a panel lives in. Panels never move sides (v1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockSide {
    Left,
    Right,
}

impl PanelId {
    /// The fixed-home table: which dock this panel opens into.
    pub fn side(self) -> DockSide {
        match self {
            PanelId::Nodes | PanelId::Fixtures => DockSide::Left,
            PanelId::Device | PanelId::Outputs => DockSide::Right,
        }
    }

    /// The strip/panel-header label.
    pub fn title(self) -> &'static str {
        match self {
            PanelId::Nodes => "Nodes",
            PanelId::Fixtures => "Fixtures",
            PanelId::Device => "Device",
            PanelId::Outputs => "Outputs",
        }
    }

    /// Strip order per side (top to bottom).
    fn strip(side: DockSide) -> [PanelId; 2] {
        match side {
            DockSide::Left => [PanelId::Nodes, PanelId::Fixtures],
            DockSide::Right => [PanelId::Device, PanelId::Outputs],
        }
    }
}

/// One view's dock state: at most one open panel per side (radio).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DockState {
    pub left: Option<PanelId>,
    pub right: Option<PanelId>,
}

impl DockState {
    fn toggle(&mut self, panel: PanelId) {
        let slot = match panel.side() {
            DockSide::Left => &mut self.left,
            DockSide::Right => &mut self.right,
        };
        *slot = if *slot == Some(panel) {
            None
        } else {
            Some(panel)
        };
    }
}

/// Per-view panel visibility (R3-2 ruling: remembered by main view, so
/// each view keeps its own arrangement), seeded with the ratified
/// defaults. Ephemeral session state by design.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelMemory {
    nodes: DockState,
    mapping: DockState,
}

impl Default for PanelMemory {
    fn default() -> Self {
        PanelMemory {
            nodes: DockState {
                left: Some(PanelId::Nodes),
                right: Some(PanelId::Device),
            },
            mapping: DockState {
                left: Some(PanelId::Fixtures),
                right: Some(PanelId::Outputs),
            },
        }
    }
}

impl PanelMemory {
    fn view(&self, view: WorkbenchView) -> DockState {
        match view {
            WorkbenchView::Nodes => self.nodes,
            WorkbenchView::Mapping => self.mapping,
        }
    }

    fn view_mut(&mut self, view: WorkbenchView) -> &mut DockState {
        match view {
            WorkbenchView::Nodes => &mut self.nodes,
            WorkbenchView::Mapping => &mut self.mapping,
        }
    }
}

/// The workbench frame: strips, docks, view tabs, center. Mounted by the
/// studio shell whenever a project editor renders in a workbench view;
/// staying ONE component across Nodes↔Mapping keeps [`PanelMemory`]
/// alive through view switches.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn WorkbenchFrame(
    view: WorkbenchView,
    /// The pane column's panes (the project pane and its siblings) — the
    /// Nodes panel's body, re-housed whole.
    panes: Vec<UiPaneView>,
    /// The open project's editor view — the Nodes center.
    project_editor: ProjectEditorView,
    /// The lens session's device card — the Device panel's body. Pinned
    /// present by the core whenever panes render; an unplugged device
    /// fades rather than vanishes.
    lens_card: Option<UiDeviceCard>,
    running: bool,
    #[props(default)] now_secs: Option<f64>,
    /// The Nodes tab's href (the suffix-less project address).
    workspace_href: String,
    /// The Mapping tab's href; `None` hides the tab (a device lens has
    /// no mapping address yet).
    #[props(default)]
    mapping_href: Option<String>,
    /// Stories only: preset panel memory (collapsed docks and the like).
    #[props(default)]
    initial_memory: Option<PanelMemory>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let mut memory = use_signal(move || initial_memory.unwrap_or_default());
    let docks = memory.read().view(view);

    rsx! {
        div { class: "tw:flex tw:min-h-0 tw:flex-1 tw:overflow-hidden tw:rounded-lg tw:border tw:border-border-strong tw:bg-background",
            EdgeStrip {
                side: DockSide::Left,
                open: docks.left,
                on_toggle: move |panel| memory.write().view_mut(view).toggle(panel),
            }
            if let Some(panel) = docks.left {
                PanelDock {
                    panel,
                    panes: panes.clone(),
                    lens_card: lens_card.clone(),
                    running,
                    now_secs,
                    on_hide: move |()| memory.write().view_mut(view).toggle(panel),
                    on_action,
                }
            }
            div { class: "tw:flex tw:min-w-0 tw:flex-1 tw:flex-col tw:border-x tw:border-border-subtle",
                ViewTabs { view, workspace_href, mapping_href }
                match view {
                    WorkbenchView::Nodes => rsx! {
                        div { class: "tw:min-h-0 tw:flex-1 tw:overflow-y-auto tw:p-3.5",
                            ProjectNodeWorkspace { view: project_editor, on_action }
                        }
                    },
                    WorkbenchView::Mapping => rsx! {
                        // The honest placeholder: the arrange canvas is the
                        // unified-editor plan's first mount here.
                        div { class: "tw:flex tw:min-h-0 tw:flex-1 tw:items-center tw:justify-center",
                            div { class: "tw:rounded-lg tw:border tw:border-dashed tw:border-border-strong tw:px-8 tw:py-6 tw:text-center",
                                p { class: "tw:m-0 tw:text-sm tw:font-semibold tw:text-muted-foreground", "Mapping" }
                                p { class: "tw:m-0 tw:mt-1 tw:text-xs tw:text-dim-foreground",
                                    "The arrange canvas lands here next — the Fixtures and Outputs panels are already live."
                                }
                            }
                        }
                    },
                }
            }
            if let Some(panel) = docks.right {
                PanelDock {
                    panel,
                    panes: panes.clone(),
                    lens_card: lens_card.clone(),
                    running,
                    now_secs,
                    on_hide: move |()| memory.write().view_mut(view).toggle(panel),
                    on_action,
                }
            }
            EdgeStrip {
                side: DockSide::Right,
                open: docks.right,
                on_toggle: move |panel| memory.write().view_mut(view).toggle(panel),
            }
        }
    }
}

/// The center's view tabs: plain links (the route listener turns a click
/// into a same-session view swap, exactly like the play/patch toggles).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ViewTabs(view: WorkbenchView, workspace_href: String, mapping_href: Option<String>) -> Element {
    rsx! {
        div { class: "tw:flex tw:min-h-[34px] tw:flex-none tw:items-end tw:gap-1 tw:border-b tw:border-border-subtle tw:bg-surface-muted tw:px-2",
            ViewTab {
                label: "Nodes",
                href: workspace_href,
                active: view == WorkbenchView::Nodes,
            }
            if let Some(href) = mapping_href {
                ViewTab {
                    label: "Mapping",
                    href,
                    active: view == WorkbenchView::Mapping,
                }
            }
        }
    }
}

/// One view tab: the nav-tab grammar (accent underline = you are here),
/// scaled for the workbench's tighter row.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ViewTab(label: &'static str, href: String, active: bool) -> Element {
    let class = if active {
        "tw:relative tw:px-3 tw:py-1.5 tw:text-xs tw:font-bold tw:text-heading tw:no-underline tw:after:absolute tw:after:inset-x-3 tw:after:bottom-0 tw:after:h-0.5 tw:after:rounded-full tw:after:bg-accent tw:after:content-['']"
    } else {
        "tw:px-3 tw:py-1.5 tw:text-xs tw:font-bold tw:text-subtle-foreground tw:no-underline tw:transition-colors tw:hover:text-strong-foreground"
    };
    rsx! {
        a { class: "{class}", href: "{href}", "{label}" }
    }
}

/// One edge strip: the side's panel toggles, always visible, vertical
/// labels. The strip is the collapsed state's handle — with no panel
/// open it is all that remains of the side.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn EdgeStrip(side: DockSide, open: Option<PanelId>, on_toggle: EventHandler<PanelId>) -> Element {
    let border = match side {
        DockSide::Left => "tw:border-r",
        DockSide::Right => "tw:border-l",
    };
    // The left strip's glyphs read bottom-up (rotated 180°) so their
    // baselines face the dock they open, IntelliJ-style.
    let rotate = match side {
        DockSide::Left => "writing-mode: vertical-rl; transform: rotate(180deg);",
        DockSide::Right => "writing-mode: vertical-rl;",
    };
    rsx! {
        div { class: "tw:flex tw:w-[27px] tw:flex-none tw:flex-col tw:items-center tw:gap-1.5 tw:py-2 {border} tw:border-border-subtle tw:bg-surface-muted",
            for panel in PanelId::strip(side) {
                button {
                    class: if open == Some(panel) {
                        "tw:cursor-pointer tw:rounded tw:border tw:border-border-strong tw:bg-surface-raised tw:px-0.5 tw:py-2 tw:text-[9.5px] tw:font-semibold tw:uppercase tw:tracking-[0.12em] tw:text-accent"
                    } else {
                        "tw:cursor-pointer tw:rounded tw:border tw:border-transparent tw:bg-transparent tw:px-0.5 tw:py-2 tw:text-[9.5px] tw:font-semibold tw:uppercase tw:tracking-[0.12em] tw:text-dim-foreground tw:hover:bg-background-wash tw:hover:text-strong-foreground"
                    },
                    style: "{rotate}",
                    title: "{panel.title()} panel",
                    onclick: move |_| on_toggle.call(panel),
                    "{panel.title()}"
                }
            }
        }
    }
}

/// One open panel in a dock: small-caps header + scrolling body. Bodies
/// re-house existing components whole — the project pane column, the
/// device card — never redesigns of them.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PanelDock(
    panel: PanelId,
    panes: Vec<UiPaneView>,
    lens_card: Option<UiDeviceCard>,
    running: bool,
    now_secs: Option<f64>,
    on_hide: EventHandler<()>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let width = match panel.side() {
        DockSide::Left => "tw:w-[270px]",
        DockSide::Right => "tw:w-[320px]",
    };
    rsx! {
        div { class: "tw:flex {width} tw:flex-none tw:flex-col tw:bg-surface-subtle",
            div { class: "tw:flex tw:min-h-[28px] tw:flex-none tw:items-center tw:gap-1.5 tw:border-b tw:border-border-subtle tw:bg-surface-muted tw:px-2.5",
                span { class: "tw:text-[10px] tw:font-semibold tw:uppercase tw:tracking-[0.13em] tw:text-dim-foreground",
                    "{panel.title()}"
                }
                span { class: "tw:flex-1" }
                button {
                    class: "tw:cursor-pointer tw:rounded tw:border-none tw:bg-transparent tw:px-1.5 tw:text-xs tw:text-dim-foreground tw:hover:bg-background-wash tw:hover:text-strong-foreground",
                    title: "Hide the {panel.title()} panel",
                    onclick: move |_| on_hide.call(()),
                    "—"
                }
            }
            div { class: "tw:min-h-0 tw:flex-1 tw:overflow-y-auto tw:p-2.5",
                match panel {
                    PanelId::Nodes => rsx! {
                        div { class: "tw:grid tw:content-start tw:gap-3.5",
                            for (index, pane) in panes.into_iter().enumerate() {
                                PaneView {
                                    key: "{pane.node_id}",
                                    view: pane,
                                    primary: index == 0,
                                    running,
                                    on_action,
                                }
                            }
                        }
                    },
                    PanelId::Device => rsx! {
                        if let Some(card) = lens_card {
                            crate::app::home::device_card::DeviceCard {
                                sim: card.sim,
                                pane: true,
                                card,
                                now_secs,
                                on_action,
                            }
                        }
                    },
                    // P2 makes these real (the #409 surface DTOs); until
                    // then the toggles are honest about what is coming.
                    PanelId::Fixtures | PanelId::Outputs => rsx! {
                        div { class: "tw:mt-3 tw:rounded-lg tw:border tw:border-dashed tw:border-border-strong tw:px-4 tw:py-5 tw:text-center tw:text-xs tw:text-dim-foreground",
                            "The {panel.title()} panel arrives with the next commits."
                        }
                    },
                }
            }
        }
    }
}
