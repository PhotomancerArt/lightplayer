//! The studio workbench — the editor chrome (PanelDock model, one-band
//! presentation).
//!
//! An IntelliJ-shaped frame on the project editor routes: ONE chrome
//! band under the site header, then a full-height `[dock][center][dock]`
//! row. The **shell owns the docks**; the band carries the **view tabs**
//! (Nodes · Graph) centered between the docks' tab segments; the **user
//! owns visibility** through those persistent tab rows. Nothing
//! rearranges itself:
//!
//! - Panels with FIXED homes — left: Tree (one panel, one ROLE — the
//!   view supplies the content: the project's node tree on Nodes, the
//!   Graph's tree on Graph, D10); right: Device · Outputs (Nodes) and
//!   Props · Device (Graph — Props is the Graph's inspector). The
//!   assignment lives in [`PanelId::side`] and [`roster`], data tables
//!   by design, so experiments are a constant edit — but there is
//!   deliberately no user arrangement in v1 ("things have one home",
//!   spike round-2 ruling).
//! - Panels toggle **radio-per-side** through the band's ATTACHED tabs
//!   (D7): the active tab shares its dock's fill and breaks the band's
//!   bottom border; pressing it collapses the side, whose tab row then
//!   REMAINS with no active tab — the persistent row is the reopen
//!   affordance (D11: edge strips and hide chevrons are gone). One panel
//!   per side keeps the dense panels honest (the radiance-scale Outputs
//!   rail never shares a column).
//! - Panel visibility is remembered **per view** ([`PanelMemory`]),
//!   seeded by each view's defaults — the Nodes view opens the Tree +
//!   Device, the Graph opens the Tree + Props — so switching
//!   views feels like the view helping, never like the room rearranging.
//!   The memory is ephemeral by design (spike A2): losing it on reload
//!   costs one click.
//!
//! Design record: `spikes/studio-chrome/index.html` (rounds 1–3 ratified
//! 2026-08-12; rounds 4–6 — the band, the attached tabs, the Tree merge
//! — ratified 2026-08-14). The Nodes center is the node workspace; the
//! Graph's center, Tree and Props are the mapping lab's canvas, tree and
//! inspector (`graph_view.rs`).

pub(crate) mod free_port_runs;
#[cfg(feature = "stories")]
pub(crate) mod graph_stories;
pub mod graph_view;
pub mod panels;
#[cfg(feature = "stories")]
pub(crate) mod workbench_stories;
#[cfg(feature = "stories")]
pub(crate) mod workbench_story_surfaces;

use dioxus::prelude::*;
use lpa_studio_core::{
    ProjectEditorView, UiAction, UiLensCard, UiPaneView, UiPatchSurface, UiSelection, UiViewContent,
};

use crate::app::{ProjectNodeWorkspace, ProjectPane};
use crate::core::PaneView;
use graph_view::{GraphCenter, GraphPage, GraphPropsPanel, GraphTreePanel, use_graph_page};
use panels::OutputsPanel;

/// Which center the workbench renders — the route's view suffix
/// ([`crate::router::ProjectView`]), narrowed to the views the workbench
/// hosts (only play still short-circuits before the workbench mounts).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WorkbenchView {
    #[default]
    Nodes,
    /// The mapping lab's canvas on the project (`graph_view.rs`): every
    /// node a card, fixtures drawn in place, coloured by the board.
    Graph,
}

/// One row of the view table: everything the chrome needs to draw and
/// address a workbench view. The tabs, hrefs, rosters, and memory all
/// key off [`VIEWS`], so adding a view is adding a row (plus its
/// [`roster`]/[`defaults`] arms).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewSpec {
    pub view: WorkbenchView,
    /// The view tab's label.
    pub label: &'static str,
    /// The route suffix this view lives at — the href builder's slot.
    pub route_view: crate::router::ProjectView,
}

/// The workbench's views, in tab order.
pub const VIEWS: &[ViewSpec] = &[
    ViewSpec {
        view: WorkbenchView::Nodes,
        label: "Nodes",
        route_view: crate::router::ProjectView::Workspace,
    },
    ViewSpec {
        view: WorkbenchView::Graph,
        // A working name (plan D4), revisited when it is the only view.
        label: "Graph",
        route_view: crate::router::ProjectView::Graph,
    },
];

/// The view a route suffix addresses: the [`VIEWS`] row that claims it,
/// or the default view (play short-circuits before the workbench
/// mounts, so an unclaimed suffix means the workspace).
pub fn view_for_route(route_view: crate::router::ProjectView) -> WorkbenchView {
    VIEWS
        .iter()
        .find(|spec| spec.route_view == route_view)
        .map(|spec| spec.view)
        .unwrap_or_default()
}

/// The view tabs' targets, one slot per [`VIEWS`] row: `None` hides the
/// tab. Every lens route (project, example, device) addresses every
/// view, so the app fills every slot; the `None` arm stays for surfaces
/// that render the frame without a lens address. Stories default to
/// inert fragments.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkbenchHrefs {
    entries: Vec<(WorkbenchView, Option<String>)>,
}

impl WorkbenchHrefs {
    pub fn from_entries(
        entries: impl IntoIterator<Item = (WorkbenchView, Option<String>)>,
    ) -> Self {
        WorkbenchHrefs {
            entries: entries.into_iter().collect(),
        }
    }

    /// The app's targets on a lens route: every view's same-session
    /// suffix on the current lens address, plain links like the play
    /// toggle. Every lens (project, example, device) addresses every
    /// view — `/device/<uid>/graph` arrived with the device route — so no
    /// tab hides. `None` off a lens.
    pub(crate) fn for_lens(route: &crate::router::StudioRoute) -> Option<Self> {
        route.is_lens().then(|| {
            Self::from_entries(
                VIEWS
                    .iter()
                    .map(|spec| (spec.view, Some(route.with_view(spec.route_view).path()))),
            )
        })
    }

    /// The route-less fallback (stories through the shell): only the
    /// default view is addressable, as an inert fragment.
    pub fn inert_default() -> Self {
        Self::from_entries(VIEWS.iter().map(|spec| {
            (
                spec.view,
                (spec.view == WorkbenchView::default()).then(|| "#".to_string()),
            )
        }))
    }

    /// Stories only: every view addressable as an inert fragment, so
    /// every tab draws without navigation.
    pub fn inert_all() -> Self {
        Self::from_entries(VIEWS.iter().map(|spec| (spec.view, Some("#".to_string()))))
    }

    fn href(&self, view: WorkbenchView) -> Option<String> {
        self.entries
            .iter()
            .find(|(entry, _)| *entry == view)
            .and_then(|(_, href)| href.clone())
    }
}

/// The dockable panels. `side` is the fixed-home table (ratified:
/// content/structure left, hardware right). A panel is a ROLE, not a
/// content mount: the VIEW supplies the [`PanelBody`] content (D10 —
/// the Tree shows the assembly on Nodes, the Graph's tree on Graph).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelId {
    /// The structure tree (D10): the project's node tree on the Nodes
    /// view, the Graph's tree on the Graph.
    Tree,
    /// The lens session's device card (D43), docked.
    Device,
    /// box → port → wire-window cells (P2).
    Outputs,
    /// The selection's properties (R4, Figma prior art): the Graph's
    /// inspector. Graph view only.
    Props,
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
            PanelId::Tree => DockSide::Left,
            PanelId::Device | PanelId::Outputs | PanelId::Props => DockSide::Right,
        }
    }

    /// The strip/panel-header label.
    pub fn title(self) -> &'static str {
        match self {
            PanelId::Tree => "Tree",
            PanelId::Device => "Device",
            PanelId::Outputs => "Outputs",
            PanelId::Props => "Props",
        }
    }

    /// The summon strip's glyph.
    pub fn glyph(self) -> &'static str {
        match self {
            PanelId::Tree => "⬡",
            PanelId::Device => "⌁",
            PanelId::Outputs => "▦",
            PanelId::Props => "≡",
        }
    }
}

/// Panel order per (view, side), top to bottom — the strip and tab-row
/// orders both read this one table. Total over [`VIEWS`]: Props exists
/// only where a canvas selection exists to describe (the Graph).
pub fn roster(view: WorkbenchView, side: DockSide) -> &'static [PanelId] {
    match (side, view) {
        (DockSide::Left, _) => &[PanelId::Tree],
        (DockSide::Right, WorkbenchView::Nodes) => &[PanelId::Device, PanelId::Outputs],
        // The Graph's outputs are cards on its canvas: Props (the lab's
        // inspector) and the board.
        (DockSide::Right, WorkbenchView::Graph) => &[PanelId::Props, PanelId::Device],
    }
}

/// Each view's ratified dock defaults — what [`PanelMemory`] seeds a
/// view with on first visit. Nodes opens the Tree + Device; the Graph
/// opens the Tree + Props.
pub fn defaults(view: WorkbenchView) -> DockState {
    match view {
        WorkbenchView::Nodes => DockState {
            left: Some(PanelId::Tree),
            right: Some(PanelId::Device),
        },
        // The Graph opens as the lab does: the tree, the canvas, the
        // inspector.
        WorkbenchView::Graph => DockState {
            left: Some(PanelId::Tree),
            right: Some(PanelId::Props),
        },
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
/// each view keeps its own arrangement): a small view-keyed map, each
/// view seeded lazily from [`defaults`] on first visit. Ephemeral
/// session state by design (spike A2): losing it on reload costs one
/// click.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PanelMemory {
    entries: Vec<(WorkbenchView, DockState)>,
}

impl PanelMemory {
    /// Stories only: preset one view's dock state.
    pub fn with(mut self, view: WorkbenchView, state: DockState) -> Self {
        *self.view_mut(view) = state;
        self
    }

    fn view(&self, view: WorkbenchView) -> DockState {
        self.entries
            .iter()
            .find(|(entry, _)| *entry == view)
            .map(|(_, state)| *state)
            .unwrap_or_else(|| defaults(view))
    }

    fn view_mut(&mut self, view: WorkbenchView) -> &mut DockState {
        if let Some(index) = self.entries.iter().position(|(entry, _)| *entry == view) {
            &mut self.entries[index].1
        } else {
            self.entries.push((view, defaults(view)));
            &mut self.entries.last_mut().expect("entry just pushed").1
        }
    }
}

/// The workbench frame: strips, docks, view tabs, center. Mounted by the
/// studio shell whenever a project editor renders in a workbench view;
/// staying ONE component across Nodes↔Graph keeps [`PanelMemory`]
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
    lens_card: Option<UiLensCard>,
    running: bool,
    #[props(default)] now_secs: Option<f64>,
    /// The view tabs' targets, one slot per [`VIEWS`] row.
    hrefs: WorkbenchHrefs,
    /// Stories only: preset panel memory (collapsed docks and the like).
    #[props(default)]
    initial_memory: Option<PanelMemory>,
    /// Stories only: preset the mobile fold's summoned panel.
    #[props(default)]
    initial_summoned: Option<PanelId>,
    /// The Graph's slice of the studio view (`None` until it is asked
    /// for).
    #[props(default)]
    graph_view: Option<lpa_studio_core::app::graph::UiGraphView>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let mut memory = use_signal(move || initial_memory.clone().unwrap_or_default());
    let graph_state = graph_view.as_ref().map(|g| g.state.clone());
    let graph = use_graph_page(graph_view);
    let docks = memory.read().view(view);
    // The Outputs panel's slice of the editor view (#409 DTOs) and the
    // surface's one shared selection.
    let surface = project_editor.patch_surface.clone();
    let patch_selection = project_editor.patch_selection.clone();
    // The mobile fold's summoned panel: below the fold breakpoint the
    // summon strip replaces the edge strips, and a summoned panel comes
    // up over the main view — a full replace at phone width, a
    // side-anchored sheet in the tablet band (see
    // [`summon_overlay_class`]). Desktop never reads this — the overlay
    // is display-gated to the fold, so widening the window simply
    // reveals the desktop docks again.
    let mut summoned = use_signal(move || initial_summoned);
    // A summoned panel is a full-screen pick surface: once a pick happens
    // — the selection moved, or the placed lamps changed — it dismisses
    // itself (G1 round 3, #6: "I expect it to go away once I do the only
    // thing I can, which is select something").
    {
        let selection = patch_selection.clone();
        let placed = surface
            .as_ref()
            .map(|surface| {
                surface
                    .outputs
                    .iter()
                    .flat_map(|output| output.bay.ports.iter())
                    .flat_map(|port| port.cells.iter())
                    .map(|cell| u64::from(cell.lamps))
                    .sum::<u64>()
            })
            .unwrap_or(0);
        let fingerprint = (selection, placed);
        let mut last = use_signal(|| None::<(UiSelection, u64)>);
        use_effect(use_reactive!(|fingerprint| {
            let changed = last
                .peek()
                .as_ref()
                .is_some_and(|previous| *previous != fingerprint);
            last.set(Some(fingerprint));
            if changed && summoned.peek().is_some() {
                summoned.set(None);
            }
        }));
    }

    rsx! {
        // No outer box (R5): the workbench is the page's working surface,
        // not a card on it. Hairline separators inside do the dividing —
        // a rounded border here was box-in-box at every width. Below the
        // fold (R4-2) the frame bleeds the shell's mobile inset back out,
        // so the summon strip is a full-width toolbar rather than a
        // floating bar; the site chrome above keeps the inset.
        div { class: "tw:flex tw:min-h-0 tw:flex-1 tw:flex-col tw:overflow-hidden tw:bg-background tw:max-[820px]:-mx-[10px]",
            SummonStrip {
                view,
                summoned: *summoned.read(),
                hrefs: hrefs.clone(),
                on_summon: move |panel: PanelId| {
                    let current = *summoned.peek();
                    summoned.set(if current == Some(panel) { None } else { Some(panel) });
                },
            }
            // ONE band across the workbench top (D7): the dock tab rows
            // and the view tabs share it — left segment over the left
            // dock, view tabs centered, right segment over the right
            // dock. A collapsed side keeps its (inactive) tab row: that
            // IS the reopen affordance (D11 — no edge strips).
            WorkbenchBand {
                view,
                hrefs: hrefs.clone(),
                docks,
                on_toggle: move |panel| memory.write().view_mut(view).toggle(panel),
            }
            div { class: "tw:flex tw:min-h-0 tw:flex-1",
                div { class: "tw:contents tw:max-[820px]:hidden",
                    if let Some(panel) = docks.left {
                        PanelDock {
                            panel,
                            view,
                            panes: panes.clone(),
                            lens_card: lens_card.clone(),
                            surface: surface.clone(),
                            patch_selection: patch_selection.clone(),
                            running,
                            now_secs,
                            graph: graph.clone(),
                            on_action,
                        }
                    }
                }
                div { class: "tw:relative tw:flex tw:min-h-0 tw:min-w-0 tw:flex-1 tw:flex-col tw:border-x tw:border-border-subtle tw:max-[820px]:border-x-0",
                    match view {
                        WorkbenchView::Nodes => rsx! {
                            div { class: "tw:min-h-0 tw:flex-1 tw:overflow-y-auto tw:p-3.5 tw:max-[820px]:p-2",
                                ProjectNodeWorkspace { view: project_editor, on_action }
                            }
                        },
                        WorkbenchView::Graph => rsx! {
                            GraphCenter {
                                page: graph.clone(),
                                state: graph_state.clone(),
                                on_action,
                            }
                        },
                    }
                    if let Some(panel) = *summoned.read() {
                        // The summoned panel below the fold, in two rungs
                        // (G1 2026-08-24: full-width panels at md read as
                        // phone behavior at tablet width). At PHONE width
                        // it replaces main outright; in the TABLET band it
                        // is a side-anchored SHEET over the canvas.
                        div { class: summon_overlay_class(panel),
                            div { class: "tw:flex tw:min-h-[32px] tw:flex-none tw:items-center tw:gap-2 tw:border-b tw:border-border-subtle tw:bg-card-subtle tw:px-2.5",
                                button {
                                    class: "tw:cursor-pointer tw:border-none tw:bg-transparent tw:p-0 tw:text-xs tw:text-selection-border",
                                    onclick: move |_| summoned.set(None),
                                    // The dismissal wears the rung's grammar:
                                    // a full-screen page goes BACK, a sheet
                                    // closes in place.
                                    span { class: "tw:min-[560px]:hidden", "‹ back" }
                                    span { class: "tw:hidden tw:min-[560px]:inline", "✕" }
                                }
                                span { class: "tw:text-[10px] tw:font-semibold tw:uppercase tw:tracking-[0.13em] tw:text-muted-foreground",
                                    "{panel.title()}"
                                }
                            }
                            div { class: "tw:min-h-0 tw:flex-1 tw:overflow-y-auto tw:p-2.5",
                                PanelBody {
                                    panel,
                                    view,
                                    panes: panes.clone(),
                                    lens_card: lens_card.clone(),
                                    surface: surface.clone(),
                                    patch_selection: patch_selection.clone(),
                                    running,
                                    now_secs,
                                    graph: graph.clone(),
                                    on_action,
                                }
                            }
                        }
                    }
                }
                div { class: "tw:contents tw:max-[820px]:hidden",
                    if let Some(panel) = docks.right {
                        PanelDock {
                            panel,
                            view,
                            panes: panes.clone(),
                            lens_card: lens_card.clone(),
                            surface: surface.clone(),
                            patch_selection: patch_selection.clone(),
                            running,
                            now_secs,
                            graph: graph.clone(),
                            on_action,
                        }
                    }
                }
            }
        }
    }
}

/// The fold's sticky strip: the desktop edge strips folded into one row —
/// view switch centered, the view's rostered panel summon buttons
/// flanking it in their home-side order. Hidden above the fold
/// breakpoint.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SummonStrip(
    view: WorkbenchView,
    summoned: Option<PanelId>,
    hrefs: WorkbenchHrefs,
    on_summon: EventHandler<PanelId>,
) -> Element {
    let button = |panel: PanelId| {
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
                "{panel.glyph()}"
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
        div { class: "tw:hidden tw:min-h-[38px] tw:flex-none tw:items-center tw:gap-1.5 tw:border-b tw:border-border-strong tw:bg-card-muted tw:px-2 tw:max-[820px]:flex",
            for panel in roster(view, DockSide::Left).iter().copied() {
                {button(panel)}
            }
            div { class: "tw:mx-1 tw:flex tw:flex-1 tw:gap-0.5 tw:rounded-md tw:border tw:border-border-strong tw:p-0.5",
                for spec in VIEWS.iter() {
                    if let Some(href) = hrefs.href(spec.view) {
                        {seg(spec.label, href, view == spec.view)}
                    }
                }
            }
            for panel in roster(view, DockSide::Right).iter().copied() {
                {button(panel)}
            }
        }
    }
}

/// The docks' widths, shared by [`PanelDock`] and the band's side
/// segments so the tab rows always sit exactly over their docks. The md
/// middle narrows rather than auto-collapsing — closing a dock the user
/// opened would be the room rearranging itself.
const LEFT_DOCK_WIDTH: &str = "tw:w-[270px] tw:max-[1240px]:w-[225px]";
const RIGHT_DOCK_WIDTH: &str = "tw:w-[320px] tw:max-[1240px]:w-[265px]";

/// The summoned overlay's classes, in the fold's two rungs (G1 2026-08-24:
/// a full-width panel at md read as phone behavior at tablet width). At
/// PHONE width (<560 — the site chrome's phone rung) the panel replaces
/// main outright; in the TABLET band (560–820) it is a side-anchored SHEET
/// at its dock's full desktop width (270/320, the un-narrowed
/// [`LEFT_DOCK_WIDTH`]/[`RIGHT_DOCK_WIDTH`]), sliding over the canvas from
/// its home side — the dock come to visit, wearing the dock's fill, with
/// the canvas still visible (and live) beside it. The tablet overrides
/// ride plain `min-[560px]:` variants because the base display gate
/// already ends the overlay at 820; above that the element is
/// display:none, so the overrides never reach the desktop docks.
fn summon_overlay_class(panel: PanelId) -> String {
    let sheet = match panel.side() {
        DockSide::Left => {
            "tw:min-[560px]:right-auto tw:min-[560px]:w-[270px] tw:min-[560px]:border-r"
        }
        DockSide::Right => {
            "tw:min-[560px]:left-auto tw:min-[560px]:w-[320px] tw:min-[560px]:border-l"
        }
    };
    format!(
        "tw:absolute tw:inset-0 tw:z-10 tw:hidden tw:flex-col tw:bg-background tw:max-[820px]:flex \
         tw:min-[560px]:border-border-strong tw:min-[560px]:bg-card-subtle tw:min-[560px]:shadow-lg {sheet}"
    )
}

/// The workbench band (D7): ONE chrome row across the workbench top —
/// each dock's tab row in a segment sized to its dock, the view tabs
/// centered between them. Panel tabs wear the ATTACHED treatment: the
/// active tab shares its dock's fill and breaks the band's bottom
/// border, so the tab reads as the panel's own. A collapsed side keeps
/// its tab row with no active tab — that persistent row IS the reopen
/// affordance (D11: no edge strips, no chevrons). Hidden below the fold,
/// where the summon strip is the bar.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn WorkbenchBand(
    view: WorkbenchView,
    hrefs: WorkbenchHrefs,
    docks: DockState,
    /// A tab press: the same radio `toggle` every chrome control
    /// dispatches — the active tab collapses its side, any other tab
    /// opens (or swaps to) its panel.
    on_toggle: EventHandler<PanelId>,
) -> Element {
    rsx! {
        div { class: "tw:flex tw:min-h-[38px] tw:flex-none tw:items-stretch tw:border-b tw:border-border-strong tw:bg-card-muted tw:max-[820px]:hidden",
            div { class: "tw:flex tw:flex-none tw:items-end tw:gap-1 tw:px-1.5 {LEFT_DOCK_WIDTH}",
                for tab in roster(view, DockSide::Left).iter().copied() {
                    BandPanelTab {
                        key: "{tab.title()}",
                        panel: tab,
                        active: docks.left == Some(tab),
                        on_press: move |panel| on_toggle.call(panel),
                    }
                }
            }
            div { class: "tw:flex tw:min-w-0 tw:flex-1 tw:items-end tw:justify-center tw:gap-1",
                for spec in VIEWS.iter() {
                    if let Some(href) = hrefs.href(spec.view) {
                        ViewTab {
                            key: "{spec.label}",
                            label: spec.label,
                            href,
                            active: view == spec.view,
                        }
                    }
                }
            }
            div { class: "tw:flex tw:flex-none tw:items-end tw:gap-1 tw:px-1.5 {RIGHT_DOCK_WIDTH}",
                for tab in roster(view, DockSide::Right).iter().copied() {
                    BandPanelTab {
                        key: "{tab.title()}",
                        panel: tab,
                        active: docks.right == Some(tab),
                        on_press: move |panel| on_toggle.call(panel),
                    }
                }
            }
        }
    }
}

/// One view tab: the nav-tab grammar (you-are-here underline). The active
/// underline is the full-rainbow static spectrum — the selection grammar's
/// horizontal you-are-here line (`.ux-here-line-x` is the canonical mark;
/// this site keeps its Tailwind `after:` utilities because the class
/// predates them, and matches the primitive's geometry exactly).
/// Deliberately the band's only PROMINENT text — bigger, mixed case, a
/// 2px underline against the panel tabs' quiet small-caps — so the
/// hierarchy reads view tabs > panel tabs at a glance (R4-1). Plain
/// links: the route listener turns a click into a same-session view
/// swap, exactly like the play toggle.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ViewTab(label: &'static str, href: String, active: bool) -> Element {
    let class = if active {
        "tw:relative tw:px-3.5 tw:py-2 tw:text-sm tw:font-bold tw:tracking-tight tw:text-heading tw:no-underline tw:after:absolute tw:after:inset-x-2 tw:after:bottom-0 tw:after:h-[2.5px] tw:after:rounded-full tw:after:bg-[linear-gradient(90deg,var(--studio-spectrum))] tw:after:content-[''] tw:max-[1240px]:px-2.5 tw:max-[1240px]:text-[13px]"
    } else {
        "tw:px-3.5 tw:py-2 tw:text-sm tw:font-bold tw:tracking-tight tw:text-subtle-foreground tw:no-underline tw:transition-colors tw:hover:text-strong-foreground tw:max-[1240px]:px-2.5 tw:max-[1240px]:text-[13px]"
    };
    rsx! {
        a { class: "{class}", href: "{href}", "{label}" }
    }
}

/// One panel tab on the band. Active = ATTACHED: rounded-top, bordered,
/// overlapping the band's bottom hairline by a pixel with the dock's own
/// fill, so the border visibly breaks under it and the tab merges into
/// the dock below (the round-4 "toggles don't look connected to the
/// panels" answer). Inactive = quiet small-caps text; pressing the
/// active tab collapses the side.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn BandPanelTab(panel: PanelId, active: bool, on_press: EventHandler<PanelId>) -> Element {
    // No-preflight trap: buttons name their bg/border; the unlayered
    // `font: inherit` trap puts the text utilities on the inner span.
    let class = if active {
        "tw:relative tw:top-px tw:cursor-pointer tw:rounded-t-md tw:border tw:border-b-0 tw:border-border-strong tw:bg-card-subtle tw:px-2.5 tw:pb-2 tw:pt-1.5 tw:max-[1240px]:px-2"
    } else {
        "tw:cursor-pointer tw:rounded-t-md tw:border tw:border-transparent tw:bg-transparent tw:px-2.5 tw:pb-2 tw:pt-1.5 tw:hover:bg-background-wash tw:max-[1240px]:px-2"
    };
    let text = if active {
        "tw:text-[10px] tw:font-semibold tw:uppercase tw:tracking-[0.13em] tw:text-strong-foreground"
    } else {
        "tw:text-[10px] tw:font-semibold tw:uppercase tw:tracking-[0.13em] tw:text-dim-foreground"
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
            span { class: "{text}", "{panel.title()}" }
        }
    }
}

/// One open panel in a dock: the scrolling body plus its optional
/// summary footer — the tabs live on the band above (D7). Bodies
/// re-house existing components whole — the project pane column, the
/// device card — never redesigns of them.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PanelDock(
    panel: PanelId,
    view: WorkbenchView,
    panes: Vec<UiPaneView>,
    lens_card: Option<UiLensCard>,
    surface: Option<UiPatchSurface>,
    patch_selection: UiSelection,
    running: bool,
    now_secs: Option<f64>,
    graph: GraphPage,
    on_action: EventHandler<UiAction>,
) -> Element {
    let width = match panel.side() {
        DockSide::Left => LEFT_DOCK_WIDTH,
        DockSide::Right => RIGHT_DOCK_WIDTH,
    };
    rsx! {
        div { class: "tw:flex {width} tw:flex-none tw:flex-col tw:bg-card-subtle",
            div { class: "tw:min-h-0 tw:flex-1 tw:overflow-y-auto tw:p-2.5",
                PanelBody {
                    panel,
                    view,
                    panes,
                    lens_card,
                    surface,
                    patch_selection,
                    running,
                    now_secs,
                    graph: graph.clone(),
                    on_action,
                }
            }
        }
    }
}

/// One panel's BODY, dock- and summon-agnostic: the docks and the mobile
/// summon overlay render the same content through this one component.
/// Dispatch is `(view, panel)` — a panel is a role, and the VIEW picks
/// what fills it (D10): the Tree shows the project's node tree on Nodes
/// and the Graph's tree on Graph.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PanelBody(
    panel: PanelId,
    view: WorkbenchView,
    panes: Vec<UiPaneView>,
    lens_card: Option<UiLensCard>,
    surface: Option<UiPatchSurface>,
    patch_selection: UiSelection,
    running: bool,
    now_secs: Option<f64>,
    graph: GraphPage,
    on_action: EventHandler<UiAction>,
) -> Element {
    match (panel, view) {
        (PanelId::Tree, WorkbenchView::Nodes) => rsx! {
            div { class: "tw:grid tw:content-start tw:gap-3.5",
                TreePanelActions { panes: panes.clone(), on_action }
                for (index, pane) in panes.into_iter().enumerate() {
                    // The project pane renders FLAT here (ruling 2): the dock
                    // tab already names it, so a card inside the panel was
                    // box-in-box. Every other pane keeps the shared
                    // `PaneView` path.
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
        (PanelId::Tree, WorkbenchView::Graph) => rsx! {
            GraphTreePanel { page: graph }
        },
        (PanelId::Props, WorkbenchView::Graph) => rsx! {
            GraphPropsPanel { page: graph }
        },
        (PanelId::Device, _) => rsx! {
            if let Some(card) = lens_card {
                crate::app::layout::studio_shell::LensCardPane { card, on_action }
            }
        },
        // Never rostered on Nodes: Props is the Graph's inspector.
        (PanelId::Props, WorkbenchView::Nodes) => rsx! {},
        (PanelId::Outputs, _) => rsx! {
            OutputsPanel {
                surface,
                selection: patch_selection,
                on_action,
            }
        },
    }
}

/// The Tree panel's debug chip (R7-2 ruling): the save moment's one home is
/// now the header session·project control (its Save sibling and the
/// changes popup's revert-all ride the SAME controller-supplied actions
/// this row used to render — see
/// `session_control::SessionProjectControl`), so this row carries only the
/// project-wide "Debug active · N · Clear all" chip, and only while debug
/// overrides are actually set. Renders nothing otherwise.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn TreePanelActions(panes: Vec<UiPaneView>, on_action: EventHandler<UiAction>) -> Element {
    let Some(editor) = panes.iter().find_map(|pane| match &pane.body {
        UiViewContent::ProjectEditor(editor) => Some((**editor).clone()),
        _ => None,
    }) else {
        return rsx! {};
    };
    let debug_overrides = editor.debug_overrides;
    if debug_overrides == 0 {
        return rsx! {};
    }
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:gap-1.5",
            crate::app::project::project_pane::DebugActiveChip { count: debug_overrides, on_action }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roster_preserves_the_ratified_homes_and_is_total_over_views() {
        for spec in VIEWS {
            for side in [DockSide::Left, DockSide::Right] {
                let roster = roster(spec.view, side);
                assert!(!roster.is_empty(), "{:?} {side:?} roster empty", spec.view);
                for panel in roster {
                    assert_eq!(panel.side(), side, "{panel:?} rostered off its home side");
                }
            }
        }
        assert_eq!(
            roster(WorkbenchView::Nodes, DockSide::Left),
            &[PanelId::Tree]
        );
        assert_eq!(
            roster(WorkbenchView::Graph, DockSide::Left),
            &[PanelId::Tree]
        );
        assert_eq!(
            roster(WorkbenchView::Nodes, DockSide::Right),
            &[PanelId::Device, PanelId::Outputs]
        );
        assert_eq!(
            roster(WorkbenchView::Graph, DockSide::Right),
            &[PanelId::Props, PanelId::Device]
        );
    }

    #[test]
    fn defaults_open_rostered_panels_only() {
        for spec in VIEWS {
            let docks = defaults(spec.view);
            for (side, open) in [(DockSide::Left, docks.left), (DockSide::Right, docks.right)] {
                if let Some(panel) = open {
                    assert!(
                        roster(spec.view, side).contains(&panel),
                        "{:?} default {panel:?} missing from its roster",
                        spec.view
                    );
                }
            }
        }
        assert_eq!(
            defaults(WorkbenchView::Nodes),
            DockState {
                left: Some(PanelId::Tree),
                right: Some(PanelId::Device),
            }
        );
        assert_eq!(
            defaults(WorkbenchView::Graph),
            DockState {
                left: Some(PanelId::Tree),
                right: Some(PanelId::Props),
            }
        );
    }

    #[test]
    fn memory_seeds_per_view_from_defaults_and_toggles_radio_per_side() {
        let mut memory = PanelMemory::default();
        assert_eq!(
            memory.view(WorkbenchView::Nodes),
            defaults(WorkbenchView::Nodes)
        );

        // Pressing the open panel collapses its side; the other view's
        // memory is untouched (per-view memory).
        memory.view_mut(WorkbenchView::Nodes).toggle(PanelId::Tree);
        assert_eq!(memory.view(WorkbenchView::Nodes).left, None);
        assert_eq!(
            memory.view(WorkbenchView::Graph),
            defaults(WorkbenchView::Graph)
        );

        // Pressing a collapsed side's panel reopens it; pressing another
        // panel on an open side is a radio swap.
        memory.view_mut(WorkbenchView::Nodes).toggle(PanelId::Tree);
        assert_eq!(memory.view(WorkbenchView::Nodes).left, Some(PanelId::Tree));
        memory
            .view_mut(WorkbenchView::Nodes)
            .toggle(PanelId::Outputs);
        assert_eq!(
            memory.view(WorkbenchView::Nodes).right,
            Some(PanelId::Outputs)
        );
        memory
            .view_mut(WorkbenchView::Nodes)
            .toggle(PanelId::Outputs);
        assert_eq!(memory.view(WorkbenchView::Nodes).right, None);
    }

    #[test]
    fn view_table_routes_round_trip() {
        for spec in VIEWS {
            assert_eq!(view_for_route(spec.route_view), spec.view);
        }
        // Play short-circuits before the workbench, so it stays
        // unclaimed and falls back to the default view.
        assert_eq!(
            view_for_route(crate::router::ProjectView::Play),
            WorkbenchView::default()
        );
    }

    /// The views are Nodes and Graph; the old Map view's route lands on
    /// the default view.
    #[test]
    fn the_views_are_nodes_and_graph() {
        assert_eq!(
            VIEWS.iter().map(|spec| spec.label).collect::<Vec<_>>(),
            ["Nodes", "Graph"]
        );
        assert_eq!(
            view_for_route(crate::router::ProjectView::Mapping),
            WorkbenchView::default()
        );
    }

    /// The device lens addresses every view: tabs were hidden there by a
    /// pre-`/device/<uid>` carve-out once.
    #[test]
    fn device_lens_addresses_every_view() {
        let hrefs = WorkbenchHrefs::for_lens(&crate::router::StudioRoute::parse("/device/devx"))
            .expect("a device route is a lens");
        assert_eq!(
            hrefs.href(WorkbenchView::Nodes).as_deref(),
            Some("/device/devx")
        );
        assert_eq!(
            hrefs.href(WorkbenchView::Graph).as_deref(),
            Some("/device/devx/graph")
        );
    }

    #[test]
    fn hrefs_only_on_a_lens() {
        assert!(WorkbenchHrefs::for_lens(&crate::router::StudioRoute::Home).is_none());
    }
}
