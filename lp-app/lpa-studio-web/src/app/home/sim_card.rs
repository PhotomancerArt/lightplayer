//! The live simulator's roster card — and the editor's docked runtime
//! surface (D43: the same card, grown).
//!
//! This was `device_card.rs`, one renderer for hardware AND the sim. The
//! device half went with M2 of the device-model rebuild; the sim's card is
//! what remains, with the M7′ card-as-control-panel anatomy unchanged
//! (ratified spike, `spikes/device-card-panel/`):
//!
//! - **Tint left edge** carries state — tone from the rich-object rollup
//!   (worst-actionable section), so state reads without color. No status
//!   circle.
//! - **D40 title bar**: sim glyph LEFT of the name, transport text label
//!   right, always-visible GROW ⤢ — the ONE editor entry
//!   (`ProjectOp::OpenSimProject`); body clicks are quiet.
//! - **Icon-tab row** below the title renders the rich-object sections per
//!   the ratified mapping ([`card_tabs`]); tab badges derive from the
//!   rollup families.
//!
//! Everything shown reads off the core view-model ([`SimCardState`] →
//! [`sim_rich_object`]), so the renderer can never drift from the
//! vocabulary.

use dioxus::prelude::*;
use lpa_studio_core::{
    CardSheet as CardSheetState, CardTab, CardTabView, CardUiOp, CardVerb, ControllerId, HomeOp,
    ProjectController, ProjectOp, RichObjectView, RichSection, RuntimeOp, SimDetailAffordance,
    SimRichInput, UiAction, UiSimCard, UiStatusKind, card_tabs, sim_rich_object,
};
use lpa_studio_core::{UiLogEntry, UiLogLevel};

use crate::app::home::card_sheet::{
    CardSheet, CardSheetButton, CardSheetButtons, CardSheetMessage, CardSheetTitle, SheetButtonTone,
};
use crate::app::home::package_card::home_action;
use crate::app::home::sim_play_tab::PlayTabBody;
use crate::base::{NodeKindIcon, StudioIcon, StudioIconName};
use crate::core::{ActionButton, ActionButtonVariant, StatusChip, chip_status};

/// A card-resident sheet (D41) the card can open: THE destructive-confirm
/// pattern for card actions. One at a time; the title bar is always
/// spared.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SimCardSheet {
    /// A destructive confirm: title/message/verb come from the action's
    /// own [`lpa_studio_core::ActionConfirmation`]; Confirm dispatches it.
    Confirm(UiAction),
}

/// What a rendered affordance row does. A sheet row carries a display-only
/// action (label/icon/destructive chrome) that never dispatches — the
/// sheet's own confirm carries the flow — plus the CORE sheet-state
/// ([`CardSheetState`]) it opens: clicking dispatches
/// `HomeOp::CardUi(OpenSheet)` so the open sheet lives in core, survives
/// the card ⇄ pane growth, and is e2e-drivable (device-lifecycle P2b).
#[derive(Clone, PartialEq)]
enum CardRowAction {
    Dispatch(UiAction),
    Sheet(CardSheetState, UiAction),
}

/// Reconstruct a confirm sheet's wired action from its core [`CardVerb`]
/// at render (the plan's "web maps verb→action" — core state stays a pure
/// value, never a boxed op).
fn verb_to_action(verb: &CardVerb) -> UiAction {
    match verb {
        CardVerb::StopSim => stop_simulator_action(),
    }
}

/// Project the core sheet-state onto the web render enum (the confirm arm
/// reconstructs its action via [`verb_to_action`]).
fn sheet_to_web(sheet: &CardSheetState) -> SimCardSheet {
    match sheet {
        CardSheetState::Confirm(verb) => SimCardSheet::Confirm(verb_to_action(verb)),
    }
}

/// Dispatch a tab selection through core (`HomeOp::CardUi`), keyed by the
/// card's identity.
fn select_tab_action(card_key: &str, tab: CardTab) -> UiAction {
    home_action(HomeOp::CardUi(CardUiOp::SelectTab {
        card: card_key.to_string(),
        tab,
    }))
}

/// Dispatch opening a card-resident sheet through core.
fn open_sheet_action(card_key: &str, sheet: CardSheetState) -> UiAction {
    home_action(HomeOp::CardUi(CardUiOp::OpenSheet {
        card: card_key.to_string(),
        sheet,
    }))
}

/// Dispatch closing the open sheet through core.
fn close_sheet_action(card_key: &str) -> UiAction {
    home_action(HomeOp::CardUi(CardUiOp::CloseSheet {
        card: card_key.to_string(),
    }))
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn SimCard(
    card: UiSimCard,
    /// D43 pane mode: the SAME card grown into the editor's right-side
    /// column — tall body, ⇲ shrinks back to the gallery. The console is
    /// the Console tab here exactly as at card scale (G1b ruling 7
    /// retired round 3.5's permanent bottom region and the ambient
    /// strip).
    #[props(default = false)]
    pane: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let status_line = card.state.status_line();

    // The rich-object view: sections wired to concrete rows here (the
    // one identity→action hop — a row dispatches or opens a card sheet),
    // rollup tone for the edge, then the ratified sections→tabs grouping.
    let sections: Vec<RichSection<CardRowAction>> = sim_rich_object(&SimRichInput {
        state: card.state,
        project_name: card.project.as_ref().map(|chip| chip.name.as_str()),
        // D4: "as ESP32-S3 DevKitC-1" under the status line
        board_id: card.board_id.as_deref(),
    })
    .sections
    .into_iter()
    .map(wire_sim_card_section)
    .collect();
    let view = RichObjectView::new(sections);
    let edge_tone = view.rollup().tone;
    // The ▶ tab exists exactly when the card holds a project: that is the
    // one condition under which there is something honest to draw. A
    // runtime with nothing on it gets no picture tab — its front-door
    // body says so instead.
    let tabs = card_tabs(view, card.project.is_some());

    // The grow control dispatches the editor-attach op; with nothing
    // loaded it renders disabled (the control is always VISIBLE — D40 —
    // never a dead click).
    let grow_action = card.project.is_some().then(open_sim_project_action);

    // Tab + open sheet are CORE-OWNED view-state (device-lifecycle P2b):
    // they ride `card.ui`, keyed by identity, so they survive the card ⇄
    // pane growth and are e2e-drivable. The card key threads every
    // interaction back to core via `HomeOp::CardUi`.
    let card_key = card.identity_key().to_string();
    // a state change may drop the selected tab: fall back to Details (the
    // front door) rather than a blank body. Console is an ordinary tab in
    // BOTH modes (G1b ruling 7 reversed round 3.5's pane exclusion).
    let active_tab = tabs
        .iter()
        .find(|tab| tab.tab == card.ui.tab)
        .map_or(CardTab::Details, |tab| tab.tab);
    // The open sheet projected to the render enum (confirm arm rebuilt
    // from its verb). `None` = no sheet.
    let active_sheet = card.ui.sheet.as_ref().map(sheet_to_web);

    let edge_style = format!(
        "--edge-tint: var(--studio-status-{}-text);",
        status_family(edge_tone)
    );

    rsx! {
        article {
            class: sim_card_class(pane),
            style: "{edge_style}",
            title: "{status_line}",
            // D40 title bar: sim glyph · name · transport label · the
            // always-visible grow control.
            header { class: "tw:flex tw:min-h-9 tw:flex-none tw:items-center tw:gap-2 tw:border-b tw:border-border tw:bg-terminal tw:py-1.5 tw:pl-3 tw:pr-1.5",
                span { class: "tw:inline-flex tw:flex-none tw:items-center tw:text-muted-foreground",
                    title: "Simulator",
                    StudioIcon { name: StudioIconName::Simulator, size: 14 }
                }
                p { class: "tw:m-0 tw:min-w-0 tw:truncate tw:text-sm tw:font-semibold tw:text-strong-foreground",
                    "Simulator"
                }
                span { class: "tw:ml-auto tw:flex-none tw:text-[11px] tw:font-bold tw:uppercase tw:tracking-wide tw:text-dim-foreground",
                    "Simulator"
                }
                if pane {
                    // D43: the grown pane's ⇲ shrinks back to the
                    // gallery — the same route + detach pair the shell
                    // wordmark uses (the hash may not change).
                    a {
                        class: "{grow_button_class()} tw:no-underline",
                        href: "/devices",
                        title: "Back to the gallery",
                        aria_label: "Shrink the simulator back to the gallery",
                        onclick: move |_| {
                            on_action.call(UiAction::from_op(
                                ProjectController::NODE_ID,
                                ProjectOp::DetachLens,
                            ));
                        },
                        StudioIcon { name: StudioIconName::Shrink, size: 14 }
                    }
                } else {
                    button {
                        class: grow_button_class(),
                        r#type: "button",
                        disabled: grow_action.is_none(),
                        title: if grow_action.is_some() { "Open in the editor" } else { "Nothing to open in the editor yet" },
                        aria_label: "Open the simulator's project in the editor",
                        onclick: {
                            let grow_action = grow_action.clone();
                            move |_| {
                                if let Some(action) = &grow_action {
                                    on_action.call(action.clone());
                                }
                            }
                        },
                        StudioIcon { name: StudioIconName::Grow, size: 14 }
                    }
                }
            }
            // NO hero strip. The D12/P05 strip re-simulated the project in
            // the browser and presented the result as the runtime's face —
            // which the 2026-08-05 G2 ruling called what it was:
            // dishonest, and letterboxed besides. What the runtime is
            // actually doing lives on the ▶ tab now, drawn from frames it
            // published; the project's identity rides that tab's meta row.
            //
            // GRID STACK, not an absolute overlay (2026-07-31). The tab
            // content and the sheet occupy the same grid cell, so the
            // region is as tall as the TALLER of them — a sheet can grow
            // the card. The previous scheme positioned overlays absolutely
            // and compensated with a hand-maintained per-sheet min-height
            // table, which produced a recurring class of clipping bugs.
            // Content-driven height deletes the class.
            div { class: if pane { "ux-card-stack tw:min-h-0 tw:flex-1" } else { "ux-card-stack" },
                div { class: if pane { "tw:relative tw:flex tw:min-h-0 tw:flex-col" } else { "tw:relative tw:flex tw:flex-col" },
                    // the icon-tab row (below the title bar — spike anatomy)
                    div {
                        class: "tw:flex tw:flex-none tw:gap-0.5 tw:border-b tw:border-border tw:bg-terminal tw:px-1.5 tw:py-1",
                        role: "tablist",
                        for tab_view in tabs.iter() {
                            {tab_button(tab_view, active_tab, &card_key, on_action)}
                        }
                    }
                    div { class: if pane { "tw:grid tw:min-h-0 tw:flex-1 tw:content-start tw:gap-1.5 tw:overflow-y-auto tw:p-3" } else { "tw:grid tw:content-start tw:gap-1.5 tw:p-3" },
                        match active_tab {
                            CardTab::Play => rsx! {
                                PlayTabBody {
                                    card: card.clone(),
                                    // The same editor entry the title-bar ⤢
                                    // dispatches (G1: the picture carries its
                                    // own way in).
                                    open_action: grow_action.clone(),
                                    on_action,
                                }
                            },
                            CardTab::Details => rsx! {
                                {details_tab_body(&tabs, on_action, &card_key)}
                            },
                            CardTab::Console => rsx! {
                                {console_tab_body(&card.console_tail)}
                            },
                            _ => rsx! {
                                {sections_tab_body(&tabs, active_tab, on_action, &card_key)}
                            },
                        }
                    }
                    // No permanent pane console region and no ambient
                    // strip (G1b ruling 7, amending D42/round 3.5): the
                    // console is the Console TAB in both modes.
                }
                if let Some(active_sheet) = active_sheet.as_ref() {
                    {sim_card_sheet_view(active_sheet, &card_key, on_action)}
                }
            }
        }
    }
}

fn console_tab_body(tail: &[UiLogEntry]) -> Element {
    if tail.is_empty() {
        return rsx! {
            p { class: "tw:m-0 tw:font-mono tw:text-xs tw:text-dim-foreground",
                "No console output yet."
            }
            {trace_footer()}
        };
    }
    rsx! {
        div { class: "ux-console-lines",
            for entry in tail {
                div { class: console_line_class(entry.level), "{entry.message}" }
            }
        }
        {trace_footer()}
    }
}

/// The device-trace export affordance (M0): copies the lifecycle event
/// trace — including the PREVIOUS session's, which survives the refresh
/// that "fixed" the jank — as JSONL to the clipboard.
fn trace_footer() -> Element {
    rsx! {
        div { class: "tw:mt-1.5 tw:flex tw:justify-end",
            button {
                class: "ux-card-op-copy",
                r#type: "button",
                title: "Copy the runtime lifecycle event trace (this session and the previous one) as JSONL",
                onclick: move |_| crate::device_events_io::copy_trace(),
                "Copy device trace"
            }
        }
    }
}

/// Severity tint for a console line (the shared status families).
fn console_line_class(level: UiLogLevel) -> &'static str {
    match level {
        UiLogLevel::Trace | UiLogLevel::Debug => "ux-console-line-dim",
        UiLogLevel::Info => "",
        UiLogLevel::Warn => "ux-console-line-warn",
        UiLogLevel::Error => "ux-console-line-error",
    }
}

/// One icon tab: selection wears the card tint (`.ux-device-tab` — the
/// Danger tab the error family), the badge dot the per-tab announcement.
fn tab_button<A>(
    tab_view: &CardTabView<A>,
    active_tab: CardTab,
    card_key: &str,
    on_action: EventHandler<UiAction>,
) -> Element {
    let tab = tab_view.tab;
    let label = tab.label();
    let card_key = card_key.to_string();
    let badge_style = tab_view.badge.map(|badge| {
        format!(
            "background: var(--studio-status-{}-text);",
            status_family(badge)
        )
    });
    rsx! {
        button {
            class: if tab == CardTab::Danger { "ux-device-tab ux-device-tab-danger" } else { "ux-device-tab" },
            r#type: "button",
            role: "tab",
            aria_selected: tab == active_tab,
            title: "{label}",
            aria_label: "{label}",
            onclick: move |event| {
                event.stop_propagation();
                on_action.call(select_tab_action(&card_key, tab));
            },
            StudioIcon { name: tab_icon(tab), size: 14 }
            if let Some(badge_style) = badge_style {
                span { class: "ux-device-tab-badge", style: "{badge_style}" }
            }
        }
    }
}

/// The Details front door: the Health section with the status line up
/// front. The project's identity rides the ▶ tab's meta row
/// (honest-device preview P3) rather than a row here.
fn details_tab_body(
    tabs: &[CardTabView<CardRowAction>],
    on_action: EventHandler<UiAction>,
    card_key: &str,
) -> Element {
    let sections = tabs
        .iter()
        .find(|tab| tab.tab == CardTab::Details)
        .map(|tab| tab.sections.as_slice())
        .unwrap_or_default();
    rsx! {
        for section in sections.iter() {
            for line in section.lines.iter() {
                if line.label == "status" {
                    // the headline: tinted like the edge (the spike's
                    // status line), never a bare kv row.
                    p { class: "tw:m-0 tw:truncate tw:text-xs tw:font-semibold",
                        style: "color: var(--edge-tint);",
                        "{line.value}"
                    }
                } else {
                    // the §3a explain-the-situation line WRAPS — truncating
                    // an explanation defeats it.
                    p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-subtle-foreground",
                        "{line.value}"
                    }
                }
            }
            if let Some(chip) = section.chip.as_ref() {
                div { class: "tw:mt-1",
                    StatusChip { status: chip_status(chip) }
                }
            }
            for row in section.affordances.iter() {
                div { class: "tw:mt-1",
                    {row_button(row, ActionButtonVariant::Quiet, on_action, card_key)}
                }
            }
        }
    }
}

/// A plain tab's body: the tab's sections as compact fact rows +
/// advisory chip + affordances. Danger rows render as destructive menu
/// rows (inspector-row convention); other affordances as quiet chips.
fn sections_tab_body(
    tabs: &[CardTabView<CardRowAction>],
    active_tab: CardTab,
    on_action: EventHandler<UiAction>,
    card_key: &str,
) -> Element {
    let sections = tabs
        .iter()
        .find(|tab| tab.tab == active_tab)
        .map(|tab| tab.sections.as_slice())
        .unwrap_or_default();
    let menu_rows = active_tab == CardTab::Danger;
    rsx! {
        for section in sections {
            {fact_section(section, menu_rows, on_action, card_key)}
        }
    }
}

/// One section as compact fact rows + advisory chip + affordance rows.
fn fact_section(
    section: &RichSection<CardRowAction>,
    menu_rows: bool,
    on_action: EventHandler<UiAction>,
    card_key: &str,
) -> Element {
    rsx! {
        if !section.lines.is_empty() {
            dl { class: "tw:m-0 tw:grid tw:min-w-0 tw:gap-1 tw:text-xs",
                for line in section.lines.iter() {
                    div { class: "tw:grid tw:min-w-0 tw:grid-cols-[72px_minmax(0,1fr)] tw:gap-2",
                        dt { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground",
                            "{line.label}"
                        }
                        dd { class: "tw:m-0 tw:min-w-0 tw:font-mono tw:text-muted-foreground tw:break-words",
                            "{line.value}"
                        }
                    }
                }
            }
        }
        if let Some(chip) = section.chip.as_ref() {
            div { StatusChip { status: chip_status(chip) } }
        }
        for row in section.affordances.iter() {
            div {
                {row_button(
                    row,
                    if menu_rows { ActionButtonVariant::MenuItem } else { ActionButtonVariant::Quiet },
                    on_action,
                    card_key,
                )}
            }
        }
    }
}

/// One affordance row: dispatch rows fire `on_action`; sheet rows open
/// their card sheet through core (`HomeOp::CardUi`), keyed by the card's
/// identity. The display action carries only the button's label/icon
/// chrome.
fn row_button(
    row: &CardRowAction,
    variant: ActionButtonVariant,
    on_action: EventHandler<UiAction>,
    card_key: &str,
) -> Element {
    match row {
        CardRowAction::Dispatch(action) => rsx! {
            ActionButton { action: action.clone(), running: false, variant, on_action }
        },
        CardRowAction::Sheet(kind, display) => {
            let kind = kind.clone();
            let card_key = card_key.to_string();
            rsx! {
                ActionButton {
                    action: display.clone(),
                    running: false,
                    variant,
                    on_action: move |_| on_action.call(open_sheet_action(&card_key, kind.clone())),
                }
            }
        }
    }
}

/// Render the active card sheet (D41).
fn sim_card_sheet_view(
    active: &SimCardSheet,
    card_key: &str,
    on_action: EventHandler<UiAction>,
) -> Element {
    let card_key = card_key.to_string();
    match active {
        SimCardSheet::Confirm(action) => rsx! {
            ConfirmSheet { action: action.clone(), card_key, on_action }
        },
    }
}

/// The same action without its confirmation gate — the confirm sheet WAS
/// the gate, so Confirm dispatches through [`ActionButton`]/directly
/// without the native `confirm()` firing on top.
fn strip_confirmation(action: UiAction) -> UiAction {
    let meta = lpa_studio_core::ActionMeta {
        confirmation: None,
        ..action.meta().clone()
    };
    action.with_meta(meta)
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ConfirmSheet(action: UiAction, card_key: String, on_action: EventHandler<UiAction>) -> Element {
    let meta = action.meta().clone();
    let confirmation = meta.confirmation.clone().unwrap_or_else(|| {
        // defensive: a confirm sheet over an unconfirmed action still
        // reads sensibly (label as title, summary as body)
        lpa_studio_core::ActionConfirmation::new(
            meta.label.clone(),
            meta.summary.clone(),
            meta.label.clone(),
        )
    });
    let tone = if meta.destructive {
        SheetButtonTone::Destructive
    } else {
        SheetButtonTone::Primary
    };
    let confirmed = strip_confirmation(action);
    rsx! {
        CardSheet {
            on_dismiss: {
                let card_key = card_key.clone();
                move |_| on_action.call(close_sheet_action(&card_key))
            },
            CardSheetTitle { text: confirmation.title.clone() }
            CardSheetMessage { text: confirmation.message.clone() }
            CardSheetButtons {
                CardSheetButton {
                    label: "Cancel",
                    tone: SheetButtonTone::Quiet,
                    onclick: {
                        let card_key = card_key.clone();
                        move |_| on_action.call(close_sheet_action(&card_key))
                    },
                }
                CardSheetButton {
                    label: confirmation.confirm_label.clone(),
                    tone,
                    onclick: move |_| {
                        on_action.call(close_sheet_action(&card_key));
                        on_action.call(confirmed.clone());
                    },
                }
            }
        }
    }
}

fn tab_icon(tab: CardTab) -> StudioIconName {
    match tab {
        // ▶ goes to the tab that actually plays something. Details wears
        // the info glyph (G1b rename — the front door is what we KNOW
        // about the runtime, not its settings).
        CardTab::Play => StudioIconName::Play,
        CardTab::Project => StudioIconName::NodeKind(NodeKindIcon::Module),
        CardTab::Details => StudioIconName::Info,
        CardTab::Performance => StudioIconName::Performance,
        CardTab::Console => StudioIconName::Console,
        CardTab::Danger => StudioIconName::Danger,
    }
}

/// The status family's token name — the edge tint and badge dots ride the
/// shared `--studio-status-*` families (attention-orange for health;
/// yellow/purple stay node meanings, never borrowed here).
fn status_family(tone: UiStatusKind) -> &'static str {
    match tone {
        UiStatusKind::Neutral => "neutral",
        UiStatusKind::Working => "working",
        UiStatusKind::Good => "good",
        UiStatusKind::Warning => "warning",
        UiStatusKind::Attention => "attention",
        UiStatusKind::Error => "error",
    }
}

/// The sim card's grow (the D29 grammar's sim arm, runtime-pool P4):
/// re-attach the editor lens to the sim session and open what it runs.
fn open_sim_project_action() -> UiAction {
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        ProjectOp::OpenSimProject,
    )
}

/// Stop the simulator, from the sim card's Danger tab (runtime-pool P3's
/// destroy op). Confirmation states the honest cost: the worker dies, and
/// applied-but-unsaved edits live on it — anything not saved to the
/// library is gone.
pub(crate) fn stop_simulator_action() -> UiAction {
    UiAction::from_op(
        ControllerId::new(RuntimeOp::NODE_ID),
        RuntimeOp::StopSimulator,
    )
    .with_confirmation(lpa_studio_core::ActionConfirmation::new(
        "Stop simulator",
        "Stop the simulator? Anything not saved to your library is discarded.",
        "Stop",
    ))
}

/// Map one sim section's affordance identities onto card rows (the
/// stop-simulator confirm rides the D41 sheet).
fn wire_sim_card_section(section: RichSection<SimDetailAffordance>) -> RichSection<CardRowAction> {
    RichSection {
        title: section.title,
        tone: section.tone,
        lines: section.lines,
        chip: section.chip,
        affordances: section
            .affordances
            .iter()
            .map(|affordance| match affordance {
                SimDetailAffordance::OpenEditor => CardRowAction::Dispatch(
                    open_sim_project_action()
                        .with_label("Open in editor")
                        .with_summary("Open the loaded project in the editor.")
                        .with_icon("grow"),
                ),
                SimDetailAffordance::StopSimulator => CardRowAction::Sheet(
                    CardSheetState::Confirm(CardVerb::StopSim),
                    strip_confirmation(stop_simulator_action()),
                ),
            })
            .collect(),
        weight: section.weight,
    }
}

fn sim_card_class(pane: bool) -> String {
    let grown = if pane {
        " tw:flex tw:min-h-[560px] tw:flex-col"
    } else {
        ""
    };
    format!(
        "tw:group tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card \
         ux-device-edge{grown}"
    )
}

/// The grow control's chrome.
fn grow_button_class() -> &'static str {
    "tw:inline-flex tw:h-6 tw:w-7 tw:flex-none tw:cursor-pointer tw:items-center tw:justify-center tw:rounded tw:border-0 tw:bg-transparent tw:text-dim-foreground tw:hover:text-strong-foreground tw:disabled:cursor-default tw:disabled:opacity-40"
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::SimCardState;

    use super::*;

    /// The Danger row's confirm is the D41 sheet, so the row itself must
    /// carry no native `confirm()` gate — two gates for one verb was the
    /// double-prompt defect.
    #[test]
    fn the_stop_row_opens_the_sheet_without_a_second_gate() {
        let section = wire_sim_card_section(RichSection {
            title: "Danger zone".to_string(),
            tone: UiStatusKind::Neutral,
            lines: Vec::new(),
            chip: None,
            affordances: vec![SimDetailAffordance::StopSimulator],
            weight: lpa_studio_core::RichWeight::Danger,
        });
        match &section.affordances[0] {
            CardRowAction::Sheet(CardSheetState::Confirm(verb), display) => {
                assert_eq!(*verb, CardVerb::StopSim);
                assert!(display.meta().confirmation.is_none());
            }
            _ => panic!("the stop row rides the confirm sheet"),
        }
        // …and the sheet's own action DOES carry the confirmation copy.
        assert!(
            verb_to_action(&CardVerb::StopSim)
                .meta()
                .confirmation
                .is_some()
        );
    }

    /// Every tab the sim can show has a glyph — a tab row with a hole in
    /// it is how a state becomes unreachable.
    #[test]
    fn every_tab_has_a_glyph() {
        for tab in [
            CardTab::Play,
            CardTab::Details,
            CardTab::Project,
            CardTab::Performance,
            CardTab::Console,
            CardTab::Danger,
        ] {
            let _ = tab_icon(tab);
            assert!(!tab.label().is_empty());
        }
    }

    #[test]
    fn the_card_state_drives_the_edge_tone_family() {
        assert_eq!(status_family(SimCardState::Running.tone()), "good");
        assert_eq!(status_family(SimCardState::Empty.tone()), "good");
    }
}
