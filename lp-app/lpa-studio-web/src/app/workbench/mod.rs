//! The studio workbench — the editor chrome (PanelDock model, one-band
//! presentation).
//!
//! An IntelliJ-shaped frame on the project editor routes: ONE chrome
//! band under the site header, then a full-height `[dock][center][dock]`
//! row. The **shell owns the docks**; the band carries the **view tabs**
//! (Nodes · Map) centered between the docks' tab segments; the **user
//! owns visibility** through those persistent tab rows. Nothing
//! rearranges itself:
//!
//! - Panels with FIXED homes — left: Tree (one panel, one ROLE — the
//!   view supplies the content: the project's node tree on Nodes, the
//!   fixture tree on Mapping, D10); right: Device · Outputs · Props
//!   (Mapping view only — the selection's properties, R4). The
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
//!   Device, the Mapping view opens the Tree + Props — so switching
//!   views feels like the view helping, never like the room rearranging.
//!   The memory is ephemeral by design (spike A2): losing it on reload
//!   costs one click.
//!
//! Design record: `spikes/studio-chrome/index.html` (rounds 1–3 ratified
//! 2026-08-12; rounds 4–6 — the band, the attached tabs, the Tree merge
//! — ratified 2026-08-14). The Mapping view's center mounts the unified
//! editor's coordinator
//! ([`crate::app::editor_shell::EditorShellCenter`]); the Tree's Mapping
//! body and the Outputs panel are that editor's rails, grown in place.

pub mod panels;
#[cfg(feature = "stories")]
pub(crate) mod workbench_stories;

use dioxus::prelude::*;
use lpa_mapping_editor::MapEditorSession;
use lpa_studio_core::{
    NodeId, ProjectEditorView, UiAction, UiDeviceCard, UiPaneView, UiPatchSurface, UiPatchTarget,
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
        view: WorkbenchView::Mapping,
        // "Map" (D9): a deliberate override of the round-2 gerund
        // ruling — the sibling-views posture reads best as short nouns.
        // Route strings are UNCHANGED (`/mapping`).
        label: "Map",
        route_view: crate::router::ProjectView::Mapping,
    },
];

/// The view a route suffix addresses: the [`VIEWS`] row that claims it,
/// or the default view (play and patch short-circuit before the
/// workbench mounts, so an unclaimed suffix means the workspace).
pub fn view_for_route(route_view: crate::router::ProjectView) -> WorkbenchView {
    VIEWS
        .iter()
        .find(|spec| spec.route_view == route_view)
        .map(|spec| spec.view)
        .unwrap_or_default()
}

/// The view tabs' targets, one slot per [`VIEWS`] row: `None` hides the
/// tab (a device lens has no mapping address yet). Stories default to
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
/// the Tree shows the assembly on Nodes, the fixture tree on Mapping).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelId {
    /// The structure tree (D10): the project's node tree on the Nodes
    /// view, the fixture → object → instance tree on the Mapping view.
    Tree,
    /// The lens session's device card (D43), docked.
    Device,
    /// box → port → wire-window cells (P2).
    Outputs,
    /// The selection's properties (R4, Figma prior art): the dived
    /// object's fields, or the selected fixture's placement facts.
    /// Mapping view only.
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
/// only where a canvas selection exists to describe (the Mapping view).
pub fn roster(view: WorkbenchView, side: DockSide) -> &'static [PanelId] {
    match (side, view) {
        (DockSide::Left, _) => &[PanelId::Tree],
        (DockSide::Right, WorkbenchView::Nodes) => &[PanelId::Device, PanelId::Outputs],
        (DockSide::Right, WorkbenchView::Mapping) => {
            &[PanelId::Props, PanelId::Outputs, PanelId::Device]
        }
    }
}

/// Each view's ratified dock defaults — what [`PanelMemory`] seeds a
/// view with on first visit. Nodes opens the Tree + Device; the Mapping
/// view opens the Tree + Props (the fixture tree and the object
/// properties: what actual mapping wants, R4 ruling).
pub fn defaults(view: WorkbenchView) -> DockState {
    match view {
        WorkbenchView::Nodes => DockState {
            left: Some(PanelId::Tree),
            right: Some(PanelId::Device),
        },
        WorkbenchView::Mapping => DockState {
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
    /// The view tabs' targets, one slot per [`VIEWS`] row.
    hrefs: WorkbenchHrefs,
    /// Stories only: preset panel memory (collapsed docks and the like).
    #[props(default)]
    initial_memory: Option<PanelMemory>,
    /// Stories only: preset the mobile fold's summoned panel.
    #[props(default)]
    initial_summoned: Option<PanelId>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let mut memory = use_signal(move || initial_memory.clone().unwrap_or_default());
    let docks = memory.read().view(view);
    // The Nodes view's route — threaded to the Props stack's
    // context-strip link (and any panel that addresses the workspace).
    let workspace_href = hrefs
        .href(WorkbenchView::default())
        .unwrap_or_else(|| "#".to_string());
    // The DIVE's shared state (R4): the focused fixture, its mapping
    // session (one selection/document for the canvas, the Fixtures tree,
    // and the Props pane), and the Props pane's commit bump (plain data
    // across the dock boundary; the session host applies on change).
    let dive_focused = use_signal(|| None::<NodeId>);
    let dive_session = use_signal(|| MapEditorSession::new(lpc_mapping::Map2dDoc::new()));
    let dive_commits = use_signal(|| 0u64);
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
                            dive_focused,
                            dive_session,
                            dive_commits,
                            workspace_href: workspace_href.clone(),
                            running,
                            now_secs,
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
                        WorkbenchView::Mapping => rsx! {
                            // The unified editor's coordinator: toolbar +
                            // arrange canvas + the editor.json prefetch
                            // (unified-editor P3/P4).
                            crate::app::editor_shell::EditorShellCenter {
                                surface: surface.clone(),
                                selection: patch_selection.clone(),
                                project_editor,
                                dive_focused,
                                dive_session,
                                dive_commits,
                                on_action,
                            }
                        },
                    }
                    if let Some(panel) = *summoned.read() {
                        // The summoned panel, replacing main below the fold.
                        div { class: "tw:absolute tw:inset-0 tw:z-10 tw:hidden tw:flex-col tw:bg-background tw:max-[820px]:flex",
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
                                    view,
                                    panes: panes.clone(),
                                    lens_card: lens_card.clone(),
                                    surface: surface.clone(),
                                    patch_selection: patch_selection.clone(),
                                    dive_focused,
                                    dive_session,
                                    dive_commits,
                                    workspace_href: workspace_href.clone(),
                                    running,
                                    now_secs,
                                    on_action,
                                }
                            }
                            if let Some(summary) = panel_footer(panel, view, surface.as_ref()) {
                                PanelFooter { summary }
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
                            dive_focused,
                            dive_session,
                            dive_commits,
                            workspace_href: workspace_href.clone(),
                            running,
                            now_secs,
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

/// One view tab: the nav-tab grammar (accent underline = you are here).
/// Deliberately the band's only PROMINENT text — bigger, mixed case, a
/// 2px underline against the panel tabs' quiet small-caps — so the
/// hierarchy reads view tabs > panel tabs at a glance (R4-1). Plain
/// links: the route listener turns a click into a same-session view
/// swap, exactly like the play/patch toggles.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ViewTab(label: &'static str, href: String, active: bool) -> Element {
    let class = if active {
        "tw:relative tw:px-3.5 tw:py-2 tw:text-sm tw:font-bold tw:tracking-tight tw:text-heading tw:no-underline tw:after:absolute tw:after:inset-x-2 tw:after:bottom-0 tw:after:h-[2.5px] tw:after:rounded-full tw:after:bg-accent tw:after:content-[''] tw:max-[1240px]:px-2.5 tw:max-[1240px]:text-[13px]"
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
    lens_card: Option<UiDeviceCard>,
    surface: Option<UiPatchSurface>,
    patch_selection: Option<UiPatchTarget>,
    dive_focused: Signal<Option<NodeId>>,
    dive_session: Signal<MapEditorSession>,
    dive_commits: Signal<u64>,
    /// The Nodes view's route — the Props stack's context-strip link.
    workspace_href: String,
    running: bool,
    now_secs: Option<f64>,
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
                    surface: surface.clone(),
                    patch_selection,
                    dive_focused,
                    dive_session,
                    dive_commits,
                    workspace_href,
                    running,
                    now_secs,
                    on_action,
                }
            }
            if let Some(summary) = panel_footer(panel, view, surface.as_ref()) {
                PanelFooter { summary }
            }
        }
    }
}

/// A panel's Finder-style summary footer (D12), pinned at the dock
/// bottom under the scrolling body. Declared per (panel, view) so any
/// panel can grow one; today only the Mapping view's Tree carries its
/// fixture totals (the line that used to sit atop the Fixtures panel).
fn panel_footer(
    panel: PanelId,
    view: WorkbenchView,
    surface: Option<&UiPatchSurface>,
) -> Option<String> {
    match (panel, view) {
        (PanelId::Tree, WorkbenchView::Mapping) => {
            let surface = surface?;
            let lamps: u32 = surface
                .fixtures
                .iter()
                .map(|fixture| fixture.patch.lamps)
                .sum();
            let instances: usize = surface
                .fixtures
                .iter()
                .map(|fixture| fixture.instances.len())
                .sum();
            Some(format!(
                "{} fixtures · {lamps} lamps · {instances} instances",
                surface.fixtures.len()
            ))
        }
        _ => None,
    }
}

/// The footer row itself: muted, mono, non-scrolling — the dock
/// composition owns it, panels only declare the line.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PanelFooter(summary: String) -> Element {
    rsx! {
        div { class: "tw:flex-none tw:border-t tw:border-border-subtle tw:bg-card-muted tw:px-2.5 tw:py-1 tw:font-mono tw:text-[10px] tw:text-dim-foreground",
            "{summary}"
        }
    }
}

/// One panel's BODY, dock- and summon-agnostic: the docks and the mobile
/// summon overlay render the same content through this one component.
/// Dispatch is `(view, panel)` — a panel is a role, and the VIEW picks
/// what fills it (D10): the Tree shows the project's node tree on Nodes
/// and the fixture tree on Mapping.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PanelBody(
    panel: PanelId,
    view: WorkbenchView,
    panes: Vec<UiPaneView>,
    lens_card: Option<UiDeviceCard>,
    surface: Option<UiPatchSurface>,
    patch_selection: Option<UiPatchTarget>,
    dive_focused: Signal<Option<NodeId>>,
    dive_session: Signal<MapEditorSession>,
    dive_commits: Signal<u64>,
    /// The Nodes view's route — the Props stack's context-strip link.
    workspace_href: String,
    running: bool,
    now_secs: Option<f64>,
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
        (PanelId::Tree, WorkbenchView::Mapping) => rsx! {
            div { class: "tw:grid tw:content-start tw:gap-2.5",
                TreePanelActions { panes: panes.clone(), on_action }
                // Today's fixture tree, re-housed whole: mixed grain
                // (effective instances + the dive's authored objects) is
                // DELIBERATE until the R5 patching plan splits the grains
                // (grain-follows-activity ruling).
                FixturesPanel {
                    surface,
                    selection: patch_selection,
                    dive: (*dive_focused.read()).map(|node| (node, dive_session)),
                    on_action,
                }
            }
        },
        (PanelId::Device, _) => rsx! {
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
        (PanelId::Props, _) => rsx! {
            panels::PropsPanel {
                surface,
                selection: patch_selection,
                dive_focused,
                dive_session,
                dive_commits,
                workspace_href,
                on_action,
            }
        },
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
/// now the header session·project control (the Save/↺ segments there ride
/// the SAME controller-supplied actions this row used to render — see
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
            roster(WorkbenchView::Mapping, DockSide::Left),
            &[PanelId::Tree]
        );
        assert_eq!(
            roster(WorkbenchView::Nodes, DockSide::Right),
            &[PanelId::Device, PanelId::Outputs]
        );
        assert_eq!(
            roster(WorkbenchView::Mapping, DockSide::Right),
            &[PanelId::Props, PanelId::Outputs, PanelId::Device]
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
            defaults(WorkbenchView::Mapping),
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
            memory.view(WorkbenchView::Mapping),
            defaults(WorkbenchView::Mapping)
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
        // Unclaimed suffixes (play/patch short-circuit before the
        // workbench) fall back to the default view.
        assert_eq!(
            view_for_route(crate::router::ProjectView::Patch),
            WorkbenchView::default()
        );
    }
}
