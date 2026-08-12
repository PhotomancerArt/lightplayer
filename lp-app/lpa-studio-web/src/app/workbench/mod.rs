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
//! - Panels toggle **radio-per-side**. An EXPANDED dock says so with a
//!   TAB ROW at its top — both of the side's panels as tabs, the open one
//!   active; clicking the other switches, clicking the active one
//!   collapses the side (R4-1: the tab row replaces both the edge strip
//!   and the old "—" hide button while the dock is open). A COLLAPSED
//!   side falls back to the vertical edge strip, which is then the only
//!   thing left of that side and the handle that reopens it. One panel
//!   per side keeps the dense panels honest (the radiance-scale Outputs
//!   rail never shares a column).
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

pub mod panels;
#[cfg(feature = "stories")]
pub(crate) mod workbench_stories;

use dioxus::prelude::*;
use lpa_studio_core::{
    ProjectEditorView, UiAction, UiDeviceCard, UiPaneView, UiPatchSurface, UiPatchTarget,
    UiViewContent,
};

use crate::app::{ProjectNodeWorkspace, ProjectPane};
use crate::core::PaneView;
use panels::{FixturesPanel, OutputsPanel};

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
    /// Stories only: preset the mobile fold's summoned panel.
    #[props(default)]
    initial_summoned: Option<PanelId>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let mut memory = use_signal(move || initial_memory.unwrap_or_default());
    let docks = memory.read().view(view);
    // The Fixtures/Outputs panels' slice of the editor view (#409 DTOs)
    // and the surface's one shared selection.
    let surface = project_editor.patch_surface.clone();
    let patch_selection = project_editor.patch_selection.clone();
    // The mobile fold's summoned panel: below the fold breakpoint the
    // summon strip replaces the edge strips, and a summoned panel
    // temporarily REPLACES the main view (an overlay with a back
    // header). Desktop never reads this — the overlay is display-gated
    // to the fold, so widening the window simply reveals the desktop
    // docks again.
    let mut summoned = use_signal(move || initial_summoned);

    // The project pane's controller status, so the root card's re-housed
    // project popup keeps the same status word the pane header showed.
    let project_status = panes.iter().find_map(|pane| {
        matches!(pane.body, UiViewContent::ProjectEditor(_)).then(|| pane.status.clone())
    });

    rsx! {
        // No outer box (R5): the workbench is the page's working surface,
        // not a card on it. Hairline separators inside the row do the
        // dividing — a rounded border here was box-in-box at every width.
        // Below the fold (R4-2) the frame bleeds the shell's mobile inset
        // back out, so the summon strip is a full-width toolbar rather than
        // a floating bar; the site chrome above keeps the inset.
        div { class: "tw:flex tw:min-h-0 tw:flex-1 tw:overflow-hidden tw:bg-background tw:max-[960px]:-mx-[10px]",
            div { class: "tw:contents tw:max-[960px]:hidden",
                // Only a COLLAPSED side keeps its strip: an open dock wears
                // the tab row instead, so the side is named exactly once.
                if docks.left.is_none() {
                    EdgeStrip {
                        side: DockSide::Left,
                        open: docks.left,
                        on_toggle: move |panel| memory.write().view_mut(view).toggle(panel),
                    }
                }
                if let Some(panel) = docks.left {
                    PanelDock {
                        panel,
                        panes: panes.clone(),
                        lens_card: lens_card.clone(),
                        surface: surface.clone(),
                        patch_selection: patch_selection.clone(),
                        running,
                        now_secs,
                        on_tab: move |panel| memory.write().view_mut(view).toggle(panel),
                        on_action,
                    }
                }
            }
            div { class: "tw:flex tw:min-w-0 tw:flex-1 tw:flex-col tw:border-x tw:border-border-subtle tw:max-[960px]:border-x-0",
                SummonStrip {
                    view,
                    summoned: *summoned.read(),
                    workspace_href: workspace_href.clone(),
                    mapping_href: mapping_href.clone(),
                    on_summon: move |panel: PanelId| {
                        let current = *summoned.peek();
                        summoned.set(if current == Some(panel) { None } else { Some(panel) });
                    },
                }
                div { class: "tw:max-[960px]:hidden",
                    ViewTabs { view, workspace_href, mapping_href }
                }
                div { class: "tw:relative tw:flex tw:min-h-0 tw:flex-1 tw:flex-col",
                    match view {
                        WorkbenchView::Nodes => rsx! {
                            div { class: "tw:min-h-0 tw:flex-1 tw:overflow-y-auto tw:p-3.5 tw:max-[960px]:p-2",
                                ProjectNodeWorkspace {
                                    view: project_editor,
                                    project_status: project_status.clone(),
                                    on_action,
                                }
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
                    if let Some(panel) = *summoned.read() {
                        // The summoned panel, replacing main below the fold.
                        div { class: "tw:absolute tw:inset-0 tw:z-10 tw:hidden tw:flex-col tw:bg-background tw:max-[960px]:flex",
                            div { class: "tw:flex tw:min-h-[32px] tw:flex-none tw:items-center tw:gap-2 tw:border-b tw:border-border-subtle tw:bg-card-subtle tw:px-2.5",
                                button {
                                    class: "tw:cursor-pointer tw:border-none tw:bg-transparent tw:p-0 tw:text-xs tw:text-selection-border",
                                    onclick: move |_| summoned.set(None),
                                    "‹ back"
                                }
                                span { class: "tw:text-[10px] tw:font-semibold tw:uppercase tw:tracking-[0.13em] tw:text-muted-foreground",
                                    "{panel.title()}"
                                }
                            }
                            div { class: "tw:min-h-0 tw:flex-1 tw:overflow-y-auto tw:p-2.5",
                                PanelBody {
                                    panel,
                                    panes: panes.clone(),
                                    lens_card: lens_card.clone(),
                                    surface: surface.clone(),
                                    patch_selection: patch_selection.clone(),
                                    running,
                                    now_secs,
                                    on_action,
                                }
                            }
                        }
                    }
                }
            }
            div { class: "tw:contents tw:max-[960px]:hidden",
                if let Some(panel) = docks.right {
                    PanelDock {
                        panel,
                        panes: panes.clone(),
                        lens_card: lens_card.clone(),
                        surface: surface.clone(),
                        patch_selection: patch_selection.clone(),
                        running,
                        now_secs,
                        on_tab: move |panel| memory.write().view_mut(view).toggle(panel),
                        on_action,
                    }
                }
                if docks.right.is_none() {
                    EdgeStrip {
                        side: DockSide::Right,
                        open: docks.right,
                        on_toggle: move |panel| memory.write().view_mut(view).toggle(panel),
                    }
                }
            }
        }
    }
}

/// The fold's sticky strip: the desktop edge strips folded into one row —
/// view switch centered, the four panel summon buttons flanking it in
/// their home-side order. Hidden above the fold breakpoint.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SummonStrip(
    view: WorkbenchView,
    summoned: Option<PanelId>,
    workspace_href: String,
    mapping_href: Option<String>,
    on_summon: EventHandler<PanelId>,
) -> Element {
    let button = |panel: PanelId, glyph: &'static str| {
        let class = if summoned == Some(panel) {
            "tw:flex tw:h-[26px] tw:w-[30px] tw:flex-none tw:cursor-pointer tw:items-center tw:justify-center tw:rounded-md tw:border tw:border-selection-border tw:bg-selection-bg tw:text-xs tw:text-selection-border"
        } else {
            "tw:flex tw:h-[26px] tw:w-[30px] tw:flex-none tw:cursor-pointer tw:items-center tw:justify-center tw:rounded-md tw:border tw:border-border-strong tw:bg-card-subtle tw:text-xs tw:text-subtle-foreground tw:hover:text-strong-foreground"
        };
        rsx! {
            button {
                class: "{class}",
                title: "{panel.title()} panel",
                onclick: move |_| on_summon.call(panel),
                "{glyph}"
            }
        }
    };
    let seg = |label: &'static str, href: String, active: bool| {
        let class = if active {
            "tw:flex-1 tw:rounded tw:bg-card-raised tw:px-0 tw:py-1 tw:text-center tw:text-[11px] tw:font-semibold tw:text-strong-foreground tw:no-underline"
        } else {
            "tw:flex-1 tw:rounded tw:px-0 tw:py-1 tw:text-center tw:text-[11px] tw:font-semibold tw:text-subtle-foreground tw:no-underline"
        };
        rsx! {
            a { class: "{class}", href: "{href}", "{label}" }
        }
    };
    rsx! {
        div { class: "tw:hidden tw:min-h-[38px] tw:flex-none tw:items-center tw:gap-1.5 tw:border-b tw:border-border-strong tw:bg-card-muted tw:px-2 tw:max-[960px]:flex",
            {button(PanelId::Nodes, "▤")}
            {button(PanelId::Fixtures, "⬡")}
            div { class: "tw:mx-1 tw:flex tw:flex-1 tw:gap-0.5 tw:rounded-md tw:border tw:border-border-strong tw:p-0.5",
                {seg("Nodes", workspace_href, view == WorkbenchView::Nodes)}
                if let Some(href) = mapping_href {
                    {seg("Mapping", href, view == WorkbenchView::Mapping)}
                }
            }
            {button(PanelId::Outputs, "▦")}
            {button(PanelId::Device, "⌁")}
        }
    }
}

/// The center's view tabs: plain links (the route listener turns a click
/// into a same-session view swap, exactly like the play/patch toggles).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ViewTabs(view: WorkbenchView, workspace_href: String, mapping_href: Option<String>) -> Element {
    rsx! {
        div { class: "tw:flex tw:min-h-[38px] tw:flex-none tw:items-end tw:gap-1 tw:border-b tw:border-border-subtle tw:bg-card-muted tw:px-2",
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

/// One view tab: the nav-tab grammar (accent underline = you are here).
/// Deliberately HEAVIER than the dock tabs — bigger text, mixed case, a
/// 2px underline against the dock tab's hairline — so the tab hierarchy
/// reads view tabs > dock tabs at a glance (R4-1).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ViewTab(label: &'static str, href: String, active: bool) -> Element {
    let class = if active {
        "tw:relative tw:px-3.5 tw:py-2 tw:text-sm tw:font-bold tw:tracking-tight tw:text-heading tw:no-underline tw:after:absolute tw:after:inset-x-2 tw:after:bottom-0 tw:after:h-[2.5px] tw:after:rounded-full tw:after:bg-accent tw:after:content-['']"
    } else {
        "tw:px-3.5 tw:py-2 tw:text-sm tw:font-bold tw:tracking-tight tw:text-subtle-foreground tw:no-underline tw:transition-colors tw:hover:text-strong-foreground"
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
        div { class: "tw:flex tw:w-[27px] tw:flex-none tw:flex-col tw:items-center tw:gap-1.5 tw:py-2 {border} tw:border-border-subtle tw:bg-card-muted",
            for panel in PanelId::strip(side) {
                button {
                    class: if open == Some(panel) {
                        "tw:cursor-pointer tw:rounded tw:border tw:border-border-strong tw:bg-card-raised tw:px-0.5 tw:py-2 tw:text-[9.5px] tw:font-semibold tw:uppercase tw:tracking-[0.12em] tw:text-accent"
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

/// One open panel in a dock: the side's TAB ROW + scrolling body. Bodies
/// re-house existing components whole — the project pane column, the
/// device card — never redesigns of them.
///
/// The tab row is the whole side's control (R4-1): both of the side's
/// panels appear, the open one is active, the inactive one switches, and
/// the ACTIVE one collapses the side — which is why there is no "—"
/// button any more, and why the edge strip only returns once the side is
/// collapsed. Every click is the same radio toggle the strip dispatches.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PanelDock(
    panel: PanelId,
    panes: Vec<UiPaneView>,
    lens_card: Option<UiDeviceCard>,
    surface: Option<UiPatchSurface>,
    patch_selection: Option<UiPatchTarget>,
    running: bool,
    now_secs: Option<f64>,
    /// A tab press: the SAME radio toggle the strip sends — the active
    /// tab collapses the side, the other opens in its place.
    on_tab: EventHandler<PanelId>,
    on_action: EventHandler<UiAction>,
) -> Element {
    // The md middle: docks narrow rather than auto-collapsing — closing
    // a dock the user opened would be the room rearranging itself.
    let side = panel.side();
    let width = match side {
        DockSide::Left => "tw:w-[270px] tw:max-[1240px]:w-[225px]",
        DockSide::Right => "tw:w-[320px] tw:max-[1240px]:w-[265px]",
    };
    rsx! {
        div { class: "tw:flex {width} tw:flex-none tw:flex-col tw:bg-card-subtle",
            div { class: "tw:flex tw:min-h-[28px] tw:flex-none tw:items-stretch tw:border-b tw:border-border-subtle tw:bg-card-muted",
                for tab in PanelId::strip(side) {
                    DockTab {
                        key: "{tab.title()}",
                        panel: tab,
                        active: tab == panel,
                        on_press: move |panel| on_tab.call(panel),
                    }
                }
            }
            div { class: "tw:min-h-0 tw:flex-1 tw:overflow-y-auto tw:p-2.5",
                PanelBody {
                    panel,
                    panes,
                    lens_card,
                    surface,
                    patch_selection,
                    running,
                    now_secs,
                    on_action,
                }
            }
        }
    }
}

/// One dock tab: small-caps, deliberately LIGHTER than the center's view
/// tabs (the hierarchy reads view tabs > dock tabs) — the active one wears
/// the dock's own surface plus a hairline accent underline, and pressing it
/// collapses the side.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn DockTab(panel: PanelId, active: bool, on_press: EventHandler<PanelId>) -> Element {
    let class = if active {
        "tw:relative tw:cursor-pointer tw:border-none tw:bg-card-subtle tw:px-2.5 tw:py-1 tw:text-[10px] tw:font-semibold tw:uppercase tw:tracking-[0.13em] tw:text-strong-foreground tw:after:absolute tw:after:inset-x-0 tw:after:bottom-0 tw:after:h-px tw:after:bg-accent tw:after:content-['']"
    } else {
        "tw:cursor-pointer tw:border-none tw:bg-transparent tw:px-2.5 tw:py-1 tw:text-[10px] tw:font-semibold tw:uppercase tw:tracking-[0.13em] tw:text-dim-foreground tw:hover:bg-background-wash tw:hover:text-strong-foreground"
    };
    let title = if active {
        format!("Hide the {} panel", panel.title())
    } else {
        format!("Show the {} panel", panel.title())
    };
    rsx! {
        button {
            class,
            title: "{title}",
            aria_pressed: "{active}",
            onclick: move |_| on_press.call(panel),
            "{panel.title()}"
        }
    }
}

/// One panel's BODY, dock- and summon-agnostic: the docks and the mobile
/// summon overlay render the same content through this one component.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PanelBody(
    panel: PanelId,
    panes: Vec<UiPaneView>,
    lens_card: Option<UiDeviceCard>,
    surface: Option<UiPatchSurface>,
    patch_selection: Option<UiPatchTarget>,
    running: bool,
    now_secs: Option<f64>,
    on_action: EventHandler<UiAction>,
) -> Element {
    match panel {
        PanelId::Nodes => rsx! {
            div { class: "tw:grid tw:content-start tw:gap-3.5",
                for (index, pane) in panes.into_iter().enumerate() {
                    // The project pane renders FLAT here (ruling 2): the dock
                    // tab already names it, so a card inside the panel was
                    // box-in-box, and its header's [i] now lives on the root
                    // node card in the center. Every other pane keeps the
                    // shared `PaneView` path.
                    if let UiViewContent::ProjectEditor(editor) = pane.body.clone() {
                        ProjectPane {
                            key: "{pane.node_id}",
                            view: *editor,
                            status: pane.status.clone(),
                            running,
                            embedded: true,
                            on_action,
                        }
                    } else {
                        PaneView {
                            key: "{pane.node_id}",
                            view: pane,
                            primary: index == 0,
                            running,
                            on_action,
                        }
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
        PanelId::Fixtures => rsx! {
            FixturesPanel {
                surface,
                selection: patch_selection,
                on_action,
            }
        },
        PanelId::Outputs => rsx! {
            OutputsPanel {
                surface,
                selection: patch_selection,
                on_action,
            }
        },
    }
}
