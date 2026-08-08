use dioxus::prelude::*;
use lpa_studio_core::{
    DirtySummary, ExportSeverity, NodeCardDrawer, NodeUiOp, UiAction, UiConfigSlot,
    UiNodeDirtyState, UiNodeSection, UiNodeTabBody, UiNodeView, UiPendingEdit, UiSlotRecord,
};

use crate::app::affordance::affordance_pane_tone;
use crate::app::layout::{PaneCollapse, RichObjectPane};
use crate::app::node::face::{face_space_badge, node_ui_action, space_badge_title};
use crate::app::node::slot_edit_actions::node_clear_debug_action;
use crate::app::node::{
    NodeChildren, NodeDetailPopover, NodeFaceBody, ProducedProducts, ProducedValues,
    SlotRecordEditor,
};
use crate::base::{HelpLink, Platform, StudioIcon, StudioIconName, node_kind_icon};

/// Which surface treatment a dirty node pane wears — the D7 tint experiment,
/// story-selectable pending the user's P5 pick.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NodeDirtyTint {
    /// Dirty tint on the header strip only, plus the state chip (live
    /// default).
    #[default]
    HeaderOnly,
    /// Dirty tint re-mixed into the whole pane surface (header + body).
    FullSurface,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NodePane(
    view: UiNodeView,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    /// The editor-level pending-edit list, threaded to every pane's detail
    /// popover (which filters it to its own node) and down through nested
    /// child panes.
    #[props(default)]
    pending_edits: Vec<UiPendingEdit>,
    #[props(default)] dirty_tint: NodeDirtyTint,
    /// Pin this face control's label-trigger detail popover open on first render
    /// (stories).
    #[props(default = None)]
    face_detail_open_control: Option<String>,
    /// Platform for the face code editor's shortcut hints; stories pin it
    /// for deterministic captures.
    #[props(default = None)]
    face_platform: Option<Platform>,
    /// Panel gestures raised by a module face (reset, auto-save).
    #[props(default = None)]
    module_panel: Option<EventHandler<crate::app::module::PanelGesture>>,
) -> Element {
    let mut active_tab = use_signal(|| 0_usize);
    let mut collapsed = use_signal(|| view.collapsed);
    let active_index = active_tab().min(view.tabs.len().saturating_sub(1));
    let active_body = view.tabs.get(active_index).map(|tab| tab.body.clone());
    let dirty = view.header.dirty;
    // The debug channel is separate from the dirty rollup (D7/D8): it marks
    // the card, it never washes the header.
    let debug_overrides = view.header.debug_overrides;
    // The rollup: the merged affordance tones the header (P6 — no count
    // chips; the detail trigger is the whole announcement). RichObjectPane
    // pins that composition.
    let tone = affordance_pane_tone(view.header.affordance(), view.header.status.kind);
    let surface_class = pane_surface_tint_class(dirty_tint, dirty);
    // "Not supported on this device": there is no runtime here, so the pane
    // shows NO body — params, products and slots would all describe
    // something that does not exist, and editing them could not take
    // effect. The whole body region becomes hazard-striped red instead.
    let unsupported = view.header.unsupported;
    let unsupported_kind = view.header.kind.clone();
    let header = view.header.clone();
    let title = view.header.title.clone();
    let tabs = view.tabs.clone();
    let focused = view.focused;
    let select_action = view.action.clone();
    let select_kind = view.header.kind.clone();
    // The node KIND, right-aligned before the ⓘ like the device card's
    // transport label (P2b item 3) — small muted lowercase identity text,
    // not status.
    let kind_label = view.header.kind.clone();
    let focus_action = view.action.clone();
    let issues = view.issues.clone();
    let header_actions = view.header_actions.clone();
    // Face + drawers replace the generic tab/section body when the
    // controller supplies a kind-specific face (shader/fixture/playlist
    // today); every other kind keeps the classic sections fallback.
    let face = view.face.clone();
    // The dimensionality badge (spike §4B): a producer's DECLARED space,
    // beside the kind label. Absent wherever there is nothing declared — a
    // fixture states a consume policy, not a space, and a card with no
    // space section has nothing to say here at all.
    let space_badge = view.face.as_ref().and_then(face_space_badge);
    // A module card's face carries two things the popup wants: the export
    // designation row (module authoring unit, P3) and the provenance line.
    let module_face = match view.face.as_ref() {
        Some(lpa_studio_core::UiNodeFace::Module(face)) => Some(face.clone()),
        _ => None,
    };
    // The DISPLAY-only export chip on a designated module's header (D12 —
    // the chip never toggles anything; the popup owns the gesture).
    // `Some(worst)` is "this module is an export"; the severity inside is
    // this export's own lint verdict, so a card that would ship badly says
    // so where you can see it without opening anything (R-A).
    let export_chip: Option<Option<ExportSeverity>> = module_face
        .as_ref()
        .and_then(|face| face.export.as_ref())
        .filter(|export| export.designated)
        .map(|export| export.findings.iter().map(|finding| finding.severity).max());
    let face_sections = main_tab_sections(&view);
    // Disclosure state is core-owned (`NodeCardUiState`), keyed by the
    // node's address path — the header path carries it for panes and
    // nested child cards alike.
    let face_node = view.header.path.clone();
    // The node address the Debug section's per-node Clear targets, and its
    // core-owned disclosure bit (collapsed on a fresh card).
    let section_node = Some(view.header.path.clone());
    let debug_open = view.card_ui.debug_open;
    let face_card_ui = view.card_ui.clone();
    let add_node_menu = view.add_node_menu.clone();
    // The root card's exports/rig split for the column below (R-A);
    // `None` on every other card, which renders the plain column.
    let child_exports = view.exports.clone();

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-3",
            div { class: surface_class,
                RichObjectPane {
                    collapse: PaneCollapse {
                        collapsed: collapsed(),
                        expand_label: "Expand node".to_string(),
                        collapse_label: "Collapse node".to_string(),
                        on_toggle: EventHandler::new(move |()| collapsed.set(!collapsed())),
                    },
                    primary: rsx! {
                        if let Some(action) = select_action {
                            NodeSelectButton {
                                action,
                                focused,
                                kind: select_kind,
                                on_action,
                            }
                        }
                    },
                    title,
                    title_action: focus_action.clone(),
                    tone,
                    accent: focused,
                    actions: header_actions,
                    on_action,
                    trailing: rsx! {
                        NodeDebugMarker { count: debug_overrides }
                        if let Some(worst) = export_chip {
                            span {
                                class: export_chip_class(worst),
                                title: export_chip_title(worst),
                                "export"
                            }
                        }
                        if let Some(badge) = space_badge {
                            span {
                                class: SPACE_BADGE_CLASS,
                                title: space_badge_title(),
                                "{badge}"
                            }
                        }
                        if !kind_label.is_empty() {
                            span { class: "tw:self-center tw:whitespace-nowrap tw:pl-2 tw:pr-1 tw:text-[11px] tw:font-bold tw:lowercase tw:tracking-wide tw:text-dim-foreground",
                                "{kind_label}"
                            }
                        }
                        // The "?" on shader nodes: where a browsing user
                        // first meets the concept, the docs answer is one
                        // click away (help-link flywheel).
                        if kind_label.eq_ignore_ascii_case("shader") {
                            HelpLink {
                                href: crate::app::docs::docs_links::what_is_a_shader::HREF,
                                title: "What's a shader?",
                                class: "tw:self-center".to_string(),
                            }
                        }
                        // No runtime here, no tabs: every tab is a view
                        // onto state this build does not have.
                        if !unsupported && tabs.len() > 1 {
                            NodeTabs {
                                tabs: tabs.clone(),
                                active_index,
                                on_select: move |index| active_tab.set(index),
                            }
                        }
                    },
                    detail: rsx! {
                        NodeDetailPopover {
                            header,
                            pending_edits: pending_edits.clone(),
                            module: module_face.clone(),
                            on_action,
                        }
                    },
                    body: rsx! {
                        if unsupported {
                            NodeUnsupportedBody { kind: unsupported_kind.clone() }
                        } else {
                            if !issues.is_empty() {
                                ul { class: "tw:m-0 tw:grid tw:list-none tw:gap-1 tw:rounded-sm tw:border tw:border-status-error-border tw:bg-status-error-bg tw:p-3",
                                    for issue in issues.clone() {
                                        li { class: "tw:text-sm tw:text-status-error-foreground", "{issue}" }
                                    }
                                }
                            }
                            if let Some(face) = face.clone() {
                                NodeFaceBody {
                                    face,
                                    node: face_node.clone(),
                                    card_ui: face_card_ui.clone(),
                                    sections: face_sections.clone(),
                                    detail_open_control: face_detail_open_control.clone(),
                                    platform: face_platform,
                                    add_node_menu: add_node_menu.clone(),
                                    pending_edits: pending_edits.clone(),
                                    dirty_tint,
                                    module_panel,
                                    on_action,
                                }
                            } else {
                                match active_body {
                                    Some(UiNodeTabBody::Sections(sections)) => rsx! {
                                        div { class: "tw:-mx-4 tw:-mb-4 tw:grid tw:min-w-0",
                                            for (index, section) in sections.into_iter().enumerate() {
                                                NodeSection {
                                                    section,
                                                    first: index == 0,
                                                    focus_action: focus_action.clone(),
                                                    node: section_node.clone(),
                                                    debug_open,
                                                    on_action,
                                                    pending_edits: pending_edits.clone(),
                                                    dirty_tint,
                                                }
                                            }
                                        }
                                    },
                                    Some(UiNodeTabBody::Text { title, body }) => rsx! {
                                        section { class: "tw:grid tw:min-w-0 tw:gap-2",
                                            h4 { class: "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-heading", "{title}" }
                                            pre { class: "tw:m-0 tw:max-h-80 tw:overflow-auto tw:rounded-sm tw:border tw:border-border-subtle tw:bg-page tw:p-3 tw:text-xs tw:leading-normal tw:text-muted-foreground",
                                                code { "{body}" }
                                            }
                                        }
                                    },
                                    None => rsx! {
                                        p { class: "tw:m-0 tw:text-sm tw:text-subtle-foreground", "No node tabs are available." }
                                    },
                                }
                            }
                        }
                    },
                }
            }
            // Children always render OUTSIDE the pane as sibling cards —
            // the playlist face included: its view carries exactly the
            // ACTIVE child (the face derivation enforces the invariant —
            // one rendering of the active child, zero of the others).
            if !collapsed() && !view.children.is_empty() {
                NodeChildren {
                    items: view.children.clone(),
                    on_action,
                    pending_edits,
                    dirty_tint,
                    module_panel,
                    exports: child_exports,
                }
            }
        }
    }
}

/// The declared-space badge beside the kind label: a mono chip, quiet
/// enough to read as identity rather than status (it is neither a state nor
/// a warning — it is what this node IS).
const SPACE_BADGE_CLASS: &str = "tw:self-center tw:whitespace-nowrap tw:rounded-xs tw:border tw:border-border-subtle tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-[10px] tw:font-bold tw:tracking-wide tw:text-subtle-foreground";

/// The export chip's tone: sage while the export reads clean, and the
/// finding's own tone when it does not — the family colour says what kind
/// of thing this is, the tone says how it is doing (the same convention the
/// popup's lint rows use inside a sage section).
fn export_chip_class(worst: Option<ExportSeverity>) -> &'static str {
    // Written out per arm rather than composed: Tailwind's scanner reads
    // literals, so a class assembled at runtime never gets generated.
    match worst {
        None => {
            "tw:self-center tw:whitespace-nowrap tw:rounded-pill tw:border tw:px-1.5 tw:py-0.5 tw:text-[10px] tw:font-bold tw:lowercase tw:tracking-wide tw:border-status-export-border tw:bg-status-export-bg tw:text-status-export-foreground"
        }
        Some(ExportSeverity::Warning) => {
            "tw:self-center tw:whitespace-nowrap tw:rounded-pill tw:border tw:px-1.5 tw:py-0.5 tw:text-[10px] tw:font-bold tw:lowercase tw:tracking-wide tw:border-status-warning-border tw:bg-status-warning-bg tw:text-status-warning-foreground"
        }
        Some(ExportSeverity::Error) => {
            "tw:self-center tw:whitespace-nowrap tw:rounded-pill tw:border tw:px-1.5 tw:py-0.5 tw:text-[10px] tw:font-bold tw:lowercase tw:tracking-wide tw:border-status-error-border tw:bg-status-error-bg tw:text-status-error-foreground"
        }
    }
}

/// What the chip's tooltip says, which is where the severity is spelled
/// out in words.
fn export_chip_title(worst: Option<ExportSeverity>) -> &'static str {
    match worst {
        None => "This module is exported from the project.",
        Some(ExportSeverity::Warning) => {
            "This module is exported from the project, with a warning — see its ⓘ."
        }
        Some(ExportSeverity::Error) => {
            "This module is exported from the project, and would not load as an import — see its ⓘ."
        }
    }
}

/// The whole body of a node whose kind has no runtime on this device.
///
/// This REPLACES the node's body rather than dimming it. A node with no
/// runtime has no live params, products or slots — rendering them (even
/// ghosted) shows state that does not exist and invites edits that cannot
/// take effect. And it usually means the project does not work on this
/// device at all, so the body region wears the error family with diagonal
/// hazard stripes: the surface itself says "this is not ordinary content",
/// which no amount of tinted text inside a normal body can.
///
/// Deliberately NOT a bordered block inside the body — a box drawn inside
/// the pane's own box reads as one more section among many. `-mx-4 -mb-4`
/// bleeds this to the pane's edges (the pane's `overflow-hidden` clips it
/// into the rounded corners), so the treatment IS the body.
///
/// One headline and one link, generously spaced. The engine's detailed
/// message ("node kind Fluid is not included in this firmware build") stays
/// in the detail popover: it names a build, and the person reading this
/// needs to know which BOARD to use.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeUnsupportedBody(
    /// The node's human kind label, as the header and the picker spell it.
    kind: String,
) -> Element {
    rsx! {
        div {
            class: "tw:-mx-4 tw:-mb-4 tw:grid tw:min-w-0 tw:justify-items-center tw:gap-4 tw:border-t tw:border-status-error-border tw:bg-status-error-bg tw:bg-[image:var(--studio-status-error-stripes)] tw:px-6 tw:py-12 tw:text-center",
            p { class: "tw:m-0 tw:text-sm tw:leading-normal tw:text-status-error-foreground",
                "{kind} node isn't supported on this device."
            }
            // The hash router's route listener hard-reloads into the boards
            // catalog, so a plain anchor is the whole mechanism.
            a {
                class: "tw:text-xs tw:font-bold tw:text-status-error-foreground tw:underline tw:underline-offset-4 tw:opacity-80 tw:transition-opacity tw:hover:opacity-100",
                href: "/boards",
                "See supported boards"
            }
        }
    }
}

/// Selection indicator/toggle in the pane's primary-affordance slot, left of
/// the node name (D3).
///
/// Selecting a node focuses it (probes ride the focused node), so body
/// clicks stay inert and only this control dispatches the focus action —
/// editing another node's slots never steals the selection.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeSelectButton(
    action: UiAction,
    focused: bool,
    /// The node's kind label — its glyph doubles as the select control.
    kind: String,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let icon = node_kind_icon(&kind);
    let (class, label) = if focused {
        (
            "tw:inline-flex tw:h-8 tw:w-8 tw:shrink-0 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-selection-border tw:bg-transparent tw:p-0 tw:text-strong-foreground",
            "Node is selected; probes follow this node",
        )
    } else {
        (
            "tw:inline-flex tw:h-8 tw:w-8 tw:shrink-0 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-border-subtle tw:bg-transparent tw:p-0 tw:text-subtle-foreground tw:hover:border-accent-border tw:hover:text-accent",
            "Select this node so probes follow it",
        )
    };

    rsx! {
        button {
            class,
            r#type: "button",
            aria_label: label,
            aria_pressed: "{focused}",
            title: label,
            onclick: move |event| {
                event.stop_propagation();
                if let Some(handler) = on_action {
                    handler.call(action.clone());
                }
            },
            StudioIcon {
                name: icon,
                size: 15,
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NodeSection(
    section: UiNodeSection,
    #[props(default = false)] first: bool,
    #[props(default)] focus_action: Option<UiAction>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    /// The owning node's address path — the Debug section's per-node Clear
    /// dispatches `NodeClearDebugOp` against it, and its disclosure toggle
    /// keys `NodeCardUiState` on it.
    #[props(default = None)]
    node: Option<String>,
    /// Core-owned disclosure state for the Debug section
    /// (`NodeCardUiState::debug_open`); every other section ignores it.
    #[props(default = false)]
    debug_open: bool,
    /// The editor-level pending-edit list, threaded through extracted child
    /// sections into their nested panes' detail popovers.
    #[props(default)]
    pending_edits: Vec<UiPendingEdit>,
    #[props(default)] dirty_tint: NodeDirtyTint,
) -> Element {
    match section {
        UiNodeSection::ProducedProducts(products) => rsx! {
            section { class: section_class("tw:bg-card tw:p-0", first),
                ProducedProducts { products, focus_action, on_action }
            }
        },
        UiNodeSection::ProducedValues(values) => rsx! {
            section { class: section_class("tw:bg-card-subtle tw:px-4 tw:py-4", first),
                ProducedValues { values, on_action }
            }
        },
        UiNodeSection::ConfigSlots(slots) => rsx! {
            section { class: section_class("tw:bg-card tw:p-0", first),
                div { class: "lp-settings-section-header",
                    span { class: "lp-settings-section-label", "Settings" }
                }
                SlotRecordEditor {
                    record: UiSlotRecord::new(slots),
                    on_action,
                }
            }
        },
        UiNodeSection::DebugSlots(slots) => rsx! {
            DebugSlotsSection { slots, node, open: debug_open, on_action }
        },
        UiNodeSection::AssetSlots(assets) => rsx! {
            section { class: section_class("tw:bg-card tw:p-0", first),
                SlotRecordEditor {
                    record: UiSlotRecord::new(assets),
                    on_action,
                }
            }
        },
        UiNodeSection::Children(children) => rsx! {
            section { class: section_class("tw:bg-card tw:px-4 tw:py-4", first),
                NodeChildren {
                    items: children,
                    on_action,
                    pending_edits,
                    dirty_tint,
                }
            }
        },
    }
}

/// The node card's **Debug** section (D3/D4/D8 tier c): the node's
/// `SlotRole::Debug` rows, flattened by core, behind a **collapsed-by-default
/// disclosure** whose HEADER is the debug territory — hazard-striped and
/// labelled DEBUG whether or not anything is overridden, so a transient
/// control announces itself before it is touched (clean-transient
/// invisibility). Most of the time those controls are not wanted, so the rows
/// open on demand (G1 feedback); unmissability rides the always-visible
/// header, the card marker, and the global chip instead.
///
/// While collapsed the header still carries "N active · session only" and the
/// per-node **Clear** ([`lpa_studio_core::NodeClearDebugOp`]) — clearing never
/// requires expanding. Nothing here ever says "Revert" or "Reset" (D7).
///
/// Open state is CORE-OWNED, on the same path as the code/advanced drawers:
/// `NodeCardUiState::debug_open`, keyed by the node's address, mutated with
/// `NodeUiOp::SetDrawer { drawer: NodeCardDrawer::Debug, .. }` — so disclosure
/// survives re-renders and is e2e-drivable. The chevron follows the section
/// grammar's rotation (right = closed, down = open) even though the striped
/// header replaces `NodeCardSection`'s rail/collapsed-row pair.
///
/// **No state transition here may change a height.** The header reserves the
/// Clear button's box (`min-height` in the CSS), so the count/Clear appearing
/// does not reflow the card.
///
/// Every pixel of the treatment is the `.lp-debug-*` block in `style.css` —
/// the semantic side is `UiNodeSection::DebugSlots` in core.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn DebugSlotsSection(
    slots: Vec<UiConfigSlot>,
    /// The owning node's address path — both the Clear target and the
    /// disclosure's `NodeCardUiState` key. `None` (a caller with no address
    /// to give) renders the header as a plain label: no Clear, and no
    /// toggle, since there would be nowhere to store the bit.
    #[props(default = None)]
    node: Option<String>,
    /// Core-owned disclosure state (`NodeCardUiState::debug_open`).
    #[props(default = false)]
    open: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let active = slots
        .iter()
        .filter(|slot| slot.state.dirty != UiNodeDirtyState::Clean)
        .count();
    let clear = node
        .as_deref()
        .filter(|_| active > 0)
        .and_then(node_clear_debug_action);
    let mut class = String::from("lp-debug-section");
    if active > 0 {
        class.push_str(" lp-debug-section--active");
    }
    if open {
        class.push_str(" lp-debug-section--open");
    }
    let toggle_node = node.clone();
    let toggle_title = if open {
        "Collapse debug controls"
    } else {
        "Expand debug controls"
    };

    rsx! {
        section { class,
            div { class: "lp-debug-section-header",
                if let Some(node) = toggle_node {
                    button {
                        class: "lp-debug-section-toggle",
                        r#type: "button",
                        aria_expanded: "{open}",
                        aria_label: "{toggle_title}",
                        title: "{toggle_title}",
                        onclick: move |event| {
                            event.stop_propagation();
                            if let Some(handler) = on_action {
                                handler.call(node_ui_action(NodeUiOp::SetDrawer {
                                    node: node.clone(),
                                    drawer: NodeCardDrawer::Debug,
                                    open: !open,
                                }));
                            }
                        },
                        span { class: debug_chevron_class(open),
                            StudioIcon {
                                name: if open { StudioIconName::Expanded } else { StudioIconName::Collapsed },
                                size: 12,
                            }
                        }
                        span { class: "lp-debug-section-label", "Debug" }
                        span { class: "lp-debug-section-note",
                            if active > 0 { "{active} active · session only" } else { "session only" }
                        }
                    }
                } else {
                    span { class: "lp-debug-section-toggle",
                        span { class: "lp-debug-section-label", "Debug" }
                        span { class: "lp-debug-section-note",
                            if active > 0 { "{active} active · session only" } else { "session only" }
                        }
                    }
                }
                if let Some(action) = clear {
                    button {
                        class: "lp-debug-button",
                        r#type: "button",
                        title: "Clear every debug override on this node",
                        onclick: move |event| {
                            event.stop_propagation();
                            if let Some(handler) = on_action {
                                handler.call(action.clone());
                            }
                        },
                        "Clear"
                    }
                }
            }
            if open {
                SlotRecordEditor {
                    record: UiSlotRecord::new(slots),
                    on_action,
                }
            }
        }
    }
}

/// Chevron rotation for the Debug disclosure — the section grammar's
/// convention: the glyph previews the state the press leads to.
fn debug_chevron_class(open: bool) -> &'static str {
    if open {
        "lp-debug-section-chevron lp-debug-section-chevron--open"
    } else {
        "lp-debug-section-chevron"
    }
}

/// The node card's debug marking (D8 tier b): a hazard pill in the header's
/// trailing slot while the node's subtree carries an active override. Not a
/// `PaneChrome` chip — the rich-object pane renders none by convention — and
/// deliberately not the header wash, which stays the dirty/status rollup's
/// (D7: a debug override is not pending work).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeDebugMarker(count: usize) -> Element {
    if count == 0 {
        return rsx! {};
    }
    rsx! {
        span {
            class: "lp-debug-marker",
            title: "This node carries {count} active debug override(s) — session only, never saved",
            "debug {count}"
        }
    }
}

/// The main tab's sections — the face's advanced drawer hosts them
/// unchanged (today's slot-row view behind the last lid).
fn main_tab_sections(view: &UiNodeView) -> Vec<UiNodeSection> {
    view.tabs
        .iter()
        .find_map(|tab| match &tab.body {
            UiNodeTabBody::Sections(sections) => Some(sections.clone()),
            UiNodeTabBody::Text { .. } => None,
        })
        .unwrap_or_default()
}

fn section_class(body_class: &'static str, first: bool) -> String {
    if first {
        format!("tw:min-w-0 {body_class}")
    } else {
        format!("tw:min-w-0 tw:border-t tw:border-border-muted {body_class}")
    }
}

/// Wrapper class around the pane for the D7 full-surface variant: a
/// `display: contents` wrapper that re-mixes the pane's card tokens with the
/// dominant dirty status color so the whole pane (header and body) tints.
///
/// The override targets the Tailwind-level `--tw-color-card*` tokens: the
/// Studio `--studio-color-*` custom properties are substituted into them at
/// `:root`, so re-declaring the Studio tokens on a wrapper never reaches
/// descendants' `bg-card*` utilities.
///
/// `HeaderOnly` (the live default) never re-mixes; `FullSurface` re-mixes on
/// dirty panes and resets a clean pane back to the base surface tokens so a
/// clean child nested inside a dirty parent's body is not tinted.
fn pane_surface_tint_class(variant: NodeDirtyTint, dirty: DirtySummary) -> &'static str {
    match variant {
        NodeDirtyTint::HeaderOnly => "tw:contents",
        NodeDirtyTint::FullSurface => {
            if dirty.failed > 0 {
                "tw:contents tw:[--tw-color-card:color-mix(in_oklab,var(--studio-status-error-bg)_55%,var(--studio-color-surface))] tw:[--tw-color-card-subtle:color-mix(in_oklab,var(--studio-status-error-bg)_55%,var(--studio-color-surface-subtle))] tw:[--tw-color-card-muted:color-mix(in_oklab,var(--studio-status-error-bg)_55%,var(--studio-color-surface-muted))]"
            } else if dirty.persisted > 0 {
                "tw:contents tw:[--tw-color-card:color-mix(in_oklab,var(--studio-status-warning-bg)_55%,var(--studio-color-surface))] tw:[--tw-color-card-subtle:color-mix(in_oklab,var(--studio-status-warning-bg)_55%,var(--studio-color-surface-subtle))] tw:[--tw-color-card-muted:color-mix(in_oklab,var(--studio-status-warning-bg)_55%,var(--studio-color-surface-muted))]"
            } else {
                "tw:contents tw:[--tw-color-card:var(--studio-color-surface)] tw:[--tw-color-card-subtle:var(--studio-color-surface-subtle)] tw:[--tw-color-card-muted:var(--studio-color-surface-muted)]"
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeTabs(
    tabs: Vec<lpa_studio_core::UiNodeTab>,
    active_index: usize,
    on_select: EventHandler<usize>,
) -> Element {
    rsx! {
        div { class: "tw:flex tw:h-full tw:items-stretch tw:overflow-hidden tw:border-l tw:border-border-muted tw:bg-card-muted", role: "tablist",
            for (index, tab) in tabs.into_iter().enumerate() {
                button {
                    class: if index == active_index {
                        "tw:min-h-full tw:border-0 tw:border-r tw:border-border-muted tw:bg-card-subtle tw:px-4 tw:text-xs tw:font-bold tw:text-strong-foreground"
                    } else {
                        "tw:min-h-full tw:border-0 tw:border-r tw:border-border-muted tw:bg-transparent tw:px-4 tw:text-xs tw:font-bold tw:text-muted-foreground tw:hover:bg-card-subtle tw:hover:text-strong-foreground"
                    },
                    r#type: "button",
                    role: "tab",
                    aria_selected: "{index == active_index}",
                    onclick: move |event| {
                        event.stop_propagation();
                        on_select.call(index);
                    },
                    "{tab.label}"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirty(persisted: usize, failed: usize) -> DirtySummary {
        DirtySummary { persisted, failed }
    }

    #[test]
    fn header_tone_rides_the_shared_affordance_merge() {
        use lpa_studio_core::{UiNodeHeader, UiStatus};

        use crate::app::layout::PaneTone;

        let tone = |status: UiStatus, dirty: DirtySummary| {
            let header = UiNodeHeader::new("Clock", "Clock", "/clock")
                .with_status(status)
                .with_dirty(dirty);
            affordance_pane_tone(header.affordance(), header.status.kind)
        };

        // A clean node keeps its runtime status tone on the wash.
        assert_eq!(
            tone(UiStatus::good("Running"), DirtySummary::clean()),
            PaneTone::Good
        );
        // Dirty precedence: failed > unsaved.
        assert_eq!(
            tone(UiStatus::good("Running"), dirty(2, 1)),
            PaneTone::Error
        );
        assert_eq!(
            tone(UiStatus::good("Running"), dirty(2, 0)),
            PaneTone::Warning
        );
        // D7: debug overrides never enter the summary, so a debug-only node
        // keeps its runtime tone — no wash at all.
        assert_eq!(
            tone(UiStatus::good("Running"), DirtySummary::clean()),
            PaneTone::Good
        );
        // An error status is never masked by a dirty wash.
        assert_eq!(
            tone(UiStatus::error("Failed"), dirty(1, 0)),
            PaneTone::Error
        );
    }

    #[test]
    fn surface_tint_applies_only_in_full_surface_variant_on_dirty_panes() {
        assert_eq!(
            pane_surface_tint_class(NodeDirtyTint::HeaderOnly, dirty(2, 0)),
            "tw:contents"
        );

        let unsaved = pane_surface_tint_class(NodeDirtyTint::FullSurface, dirty(2, 0));
        assert!(unsaved.contains("--studio-status-warning-bg"));
        let failed = pane_surface_tint_class(NodeDirtyTint::FullSurface, dirty(1, 1));
        assert!(failed.contains("--studio-status-error-bg"));

        let clean = pane_surface_tint_class(NodeDirtyTint::FullSurface, DirtySummary::clean());
        assert!(!clean.contains("color-mix"));
        assert!(clean.contains("--tw-color-card:var(--studio-color-surface)"));
    }
}
