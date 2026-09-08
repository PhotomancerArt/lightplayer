//! The target menu: Desktop and the boards, as a popover (D44, D41, PD16).
//!
//! Two surfaces open the same menu, so the menu is one component and the
//! two decisions that differ are props:
//!
//! - **The Devices page's add slot** — the quiet second verb "start a board
//!   here ▾". Its rows are tagged with what this build would run them as
//!   (`sim`, from core's capability table), and picking one dispatches
//!   [`SimCreateOp`]: mint the record, power it on, and the card appears in
//!   the grid beside the slot that made it.
//! - **A project's Hardware row** — the same two groups with **no tag at
//!   all**: `target` names hardware, and "a board is just a board" (D41).
//!   Emu/sim is something a *device* is, not something a project declares.
//!   Its scope is wider, too: every catalog board is a legitimate target,
//!   and the ones Studio has no runtime manifest for are listed disabled
//!   with the reason rather than silently missing (Q10).
//!
//! # Why a popover, again
//!
//! The same reason [`device_pick_popover`](super::device_pick_popover)
//! gives: the add slot lives in the roster grid, and a list that grows with
//! the catalog would re-lay the whole grid out every time it opened.
//! [`PopoverButton`] floats the panel in the browser's top layer, so the
//! slot is the same height open or shut.
//!
//! # Keyboard
//!
//! [`PopoverButton`] gives Escape-to-close, a dismissing backdrop and focus
//! back on the trigger; the rows are real buttons, so Tab and Enter work
//! without help. Up and Down rove between them — a menu of one-line rows is
//! the one place the board picker's missing roving is actually felt, since
//! there is nothing else in the panel to Tab through.

use dioxus::prelude::*;
use lpa_studio_core::{
    HomeOp, SimCreateOp, TargetChoice, TargetGroup, TargetOffer, TargetScope, UiAction,
    target_offer,
};
use wasm_bindgen::JsCast;

use super::device_pick_popover::BoardSwatch;
use super::package_card::home_action;
use crate::base::{
    OPTION_CARD_CHECK_CLASS, PopoverButton, PopoverCloseHandle, PopoverPlacement, StudioIcon,
    StudioIconName,
};

/// The add slot's second verb, verbatim (spike 2a). Lowercase because it
/// is a quiet aside beside the CTA, not a second button.
pub(crate) const SLOT_VERB_LABEL: &str = "start a board here";

/// The add slot's quiet second verb and its menu (spike 2a).
///
/// Deliberately quiet beside "It's connected": the common case is a board
/// on the desk, and this is the other way a card can appear.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn TargetPickPopover(
    /// Stories only: mount the panel open (a capture cannot click).
    #[props(default = false)]
    initially_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        PopoverButton {
            class: target_trigger_class().to_string(),
            open_class: target_trigger_class().to_string(),
            trigger: rsx! {
                span { "{SLOT_VERB_LABEL}" }
                span { class: trigger_caret_class(), "\u{25be}" }
            },
            label: "Start a board here".to_string(),
            title: "Start a runtime of a board — or of this computer — in this tab.".to_string(),
            popup_class: TARGET_POPUP_CLASS.to_string(),
            chrome_class: "ux-popover-chrome-neutral".to_string(),
            placement: PopoverPlacement::BottomMiddle,
            layer_keeps_layout: true,
            initially_open,
            TargetPickMenu {
                offer: target_offer(TargetScope::Runnable),
                show_tags: true,
                on_pick: move |board_id: String| {
                    on_action.call(SimCreateOp::action_for(board_id));
                },
            }
        }
    }
}

/// The Hardware row's trigger and menu (spike 3D): the project's current
/// target, and the same two groups behind it.
///
/// `uid` is the library package the write addresses; picking dispatches
/// [`HomeOp::SetPackageTarget`], which patches the open project's manifest
/// through its own handle.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn HardwarePickPopover(
    uid: String,
    /// The project's current target — the catalog board id, or `None` for
    /// Desktop, which is what an absent `target` has always meant.
    #[props(default)]
    target: Option<String>,
    /// Stories only: mount the panel open.
    #[props(default = false)]
    initially_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let selected = target
        .clone()
        .unwrap_or_else(|| lpa_studio_core::DESKTOP_BOARD_ID.to_string());
    let label = lpa_studio_core::board_display_name(&selected);

    rsx! {
        PopoverButton {
            class: hardware_trigger_class().to_string(),
            open_class: hardware_trigger_class().to_string(),
            trigger: rsx! {
                BoardSwatch { board_id: Some(selected.clone()) }
                span { class: trigger_label_class(), "{label}" }
                span { class: trigger_caret_class(), "\u{25be}" }
            },
            label: "Choose this project's hardware".to_string(),
            title: "The hardware this project runs on.".to_string(),
            popup_class: TARGET_POPUP_CLASS.to_string(),
            chrome_class: "ux-popover-chrome-neutral".to_string(),
            placement: PopoverPlacement::BottomEnd,
            layer_keeps_layout: true,
            initially_open,
            TargetPickMenu {
                offer: target_offer(TargetScope::Everything),
                // D41: nothing here says emu or sim. A board is just a
                // board; what a DEVICE is is the device's business.
                show_tags: false,
                show_ids: true,
                selected: selected.clone(),
                on_pick: move |board_id: String| {
                    on_action
                        .call(
                            home_action(HomeOp::SetPackageTarget {
                                uid: uid.clone(),
                                target: Some(board_id),
                            }),
                        );
                },
            }
        }
    }
}

/// The menu itself: one group head per [`TargetGroup`], then its rows.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn TargetPickMenu(
    offer: TargetOffer,
    /// Tag each row with what this build would run it as (the add slot).
    #[props(default = false)]
    show_tags: bool,
    /// Put the catalog id under the name (the Hardware row — `target` is a
    /// value people paste into manifests, so the row shows the value).
    #[props(default = false)]
    show_ids: bool,
    /// The row already chosen, when the menu has one.
    #[props(default)]
    selected: Option<String>,
    on_pick: EventHandler<String>,
) -> Element {
    let close = try_consume_context::<PopoverCloseHandle>();
    let menu_id = use_hook(next_menu_id);
    // Which pickable row the arrow keys are on. Only the ENABLED rows are
    // reachable: a row the build cannot run is a statement, not a stop.
    let mut cursor = use_signal(|| 0usize);
    let pickable: Vec<String> = offer
        .choices
        .iter()
        .filter(|choice| choice.runnable)
        .map(|choice| choice.board_id.clone())
        .collect();
    let last = pickable.len().saturating_sub(1);

    rsx! {
        div {
            class: "tw:grid tw:min-w-0 tw:py-1.5",
            onkeydown: move |event: KeyboardEvent| {
                let next = match event.key() {
                    Key::ArrowDown => (cursor() + 1).min(last),
                    Key::ArrowUp => cursor().saturating_sub(1),
                    _ => return,
                };
                event.prevent_default();
                cursor.set(next);
                focus_row(&menu_id, next);
            },
            for group in [TargetGroup::Desktop, TargetGroup::Boards] {
                {
                    let rows: Vec<TargetChoice> = offer.group(group).cloned().collect();
                    rsx! {
                        if !rows.is_empty() {
                            p { class: group_head_class(), "{group.label()}" }
                            for choice in rows {
                                {
                                    let row = pickable
                                        .iter()
                                        .position(|id| id == &choice.board_id);
                                    let picked = selected.as_deref()
                                        == Some(choice.board_id.as_str());
                                    let board_id = choice.board_id.clone();
                                    rsx! {
                                        button {
                                            key: "{choice.board_id}",
                                            id: row.map(|row| format!("{menu_id}-row-{row}")),
                                            class: row_class(choice.runnable, picked),
                                            r#type: "button",
                                            disabled: !choice.runnable,
                                            title: choice.unavailable().unwrap_or(""),
                                            onfocus: move |_| {
                                                if let Some(row) = row {
                                                    cursor.set(row);
                                                }
                                            },
                                            onclick: move |event: MouseEvent| {
                                                event.stop_propagation();
                                                on_pick.call(board_id.clone());
                                                if let Some(mut close) = close {
                                                    close.close();
                                                }
                                            },
                                            BoardSwatch { board_id: Some(choice.board_id.clone()) }
                                            span { class: "tw:grid tw:min-w-0 tw:flex-1 tw:text-left",
                                                span { class: row_title_class(), "{choice.title}" }
                                                if show_ids {
                                                    span { class: row_sub_class(), "{choice.board_id}" }
                                                }
                                            }
                                            if let Some(reason) = choice.unavailable() {
                                                span { class: row_reason_class(), "{reason}" }
                                            } else if show_tags {
                                                span { class: row_tag_class(), "{choice.backing.tag()}" }
                                            }
                                            if picked {
                                                span { class: OPTION_CARD_CHECK_CLASS, aria_hidden: "true",
                                                    StudioIcon { name: StudioIconName::StepComplete, size: 10 }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // A2: only rendered once something in the list is emulated.
            // Inert text explaining a choice nobody has is noise.
            if let Some(hint) = offer.hint {
                p { class: hint_class(), "{hint}" }
            }
        }
    }
}

/// Move DOM focus to the `index`-th pickable row of `menu_id`.
///
/// By id rather than by a held handle: the rows are re-rendered whenever
/// the offer changes, and a stale element handle focusing nothing is worse
/// than a lookup that misses.
fn focus_row(menu_id: &str, index: usize) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    if let Some(element) = document.get_element_by_id(&format!("{menu_id}-row-{index}"))
        && let Ok(element) = element.dyn_into::<web_sys::HtmlElement>()
    {
        let _ = element.focus();
    }
}

/// A per-mount id, so two menus on one page (the add slot and a settings
/// row) cannot fight over the same row ids.
fn next_menu_id() -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    format!("ux-target-menu-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// The menu panel: narrow, because every row is one line of text.
const TARGET_POPUP_CLASS: &str = "tw:grid tw:w-[300px] tw:max-w-[calc(100vw-80px)] tw:min-w-0 tw:overflow-hidden tw:whitespace-normal tw:rounded-md tw:border tw:text-sm tw:text-muted-foreground";

/// The add slot's second verb: a text affordance under the primary CTA, in
/// the remembered line's voice — the slot already has one button, and a
/// second chip beside it would read as two equal offers.
fn target_trigger_class() -> &'static str {
    "ux-focus-ring tw:inline-flex tw:cursor-pointer tw:items-center tw:gap-1 tw:appearance-none tw:border-0 tw:bg-transparent tw:p-0 tw:text-xs tw:text-subtle-foreground tw:underline tw:decoration-dotted tw:hover:text-strong-foreground"
}

/// The Hardware row's trigger: the settings rows' own scale, bordered like
/// the name field beside it so the row reads as editable.
fn hardware_trigger_class() -> &'static str {
    "tw:inline-flex tw:h-[22px] tw:min-w-0 tw:max-w-full tw:cursor-pointer tw:items-center tw:gap-1.5 tw:rounded tw:border tw:border-border tw:bg-transparent tw:px-1.5 tw:text-left tw:text-xs tw:text-muted-foreground tw:hover:border-border-strong tw:hover:text-strong-foreground"
}

fn trigger_label_class() -> &'static str {
    "tw:min-w-0 tw:flex-1 tw:truncate"
}

fn trigger_caret_class() -> &'static str {
    "tw:flex-none tw:text-[10px] tw:text-subtle-foreground"
}

/// A group head: the quietest thing in the panel — it separates, it does
/// not compete with the rows.
fn group_head_class() -> &'static str {
    "tw:m-0 tw:px-2.5 tw:pt-1.5 tw:pb-1 tw:text-[10px] tw:font-bold tw:tracking-wide tw:text-dim-foreground tw:uppercase"
}

/// One menu row. A row the build cannot run is dimmed and inert; it stays
/// in the list because a project may legitimately be FOR that board.
fn row_class(runnable: bool, picked: bool) -> &'static str {
    match (runnable, picked) {
        (false, _) => {
            "tw:relative tw:flex tw:w-full tw:min-w-0 tw:cursor-not-allowed tw:items-center tw:gap-2 tw:border-0 tw:bg-transparent tw:px-2.5 tw:py-1.5 tw:text-left tw:opacity-50"
        }
        (true, false) => {
            "ux-focus-ring tw:relative tw:flex tw:w-full tw:min-w-0 tw:cursor-pointer tw:items-center tw:gap-2 tw:border-0 tw:bg-transparent tw:px-2.5 tw:py-1.5 tw:text-left tw:hover:bg-card-raised"
        }
        (true, true) => {
            "ux-focus-ring tw:relative tw:flex tw:w-full tw:min-w-0 tw:cursor-pointer tw:items-center tw:gap-2 tw:border-0 tw:bg-card-raised tw:px-2.5 tw:py-1.5 tw:text-left"
        }
    }
}

fn row_title_class() -> &'static str {
    "tw:min-w-0 tw:truncate tw:text-xs tw:font-semibold tw:text-strong-foreground"
}

fn row_sub_class() -> &'static str {
    "tw:min-w-0 tw:truncate tw:font-mono tw:text-[10px] tw:text-subtle-foreground"
}

/// The row tag (spike 2b's "a word"): the same lowercase word the runtime
/// band and the `?on=` grammar use.
fn row_tag_class() -> &'static str {
    "tw:flex-none tw:pr-3 tw:text-[10px] tw:font-medium tw:text-subtle-foreground"
}

/// Why a row is inert, said on the row rather than left to a guess.
fn row_reason_class() -> &'static str {
    "tw:flex-none tw:text-[10px] tw:text-dim-foreground"
}

fn hint_class() -> &'static str {
    "tw:m-0 tw:border-t tw:border-border-muted tw:px-2.5 tw:pt-2 tw:pb-1 tw:text-[11px] tw:leading-relaxed tw:text-subtle-foreground"
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The add slot's menu offers only what it can start, Desktop first —
    /// the renderer's claim about the offer it draws.
    #[test]
    fn the_slots_menu_leads_with_desktop_and_offers_only_startable_rows() {
        let offer = target_offer(TargetScope::Runnable);

        assert_eq!(offer.group(TargetGroup::Desktop).count(), 1);
        assert!(offer.choices.iter().all(|choice| choice.runnable));
        assert!(offer.group(TargetGroup::Boards).count() >= 5);
    }

    /// Every row a menu can render has a class, and the inert ones cannot
    /// be mistaken for pickable.
    #[test]
    fn an_unrunnable_row_is_inert_and_says_why() {
        assert!(row_class(false, false).contains("cursor-not-allowed"));
        assert!(row_class(true, false).contains("cursor-pointer"));
        assert!(row_class(true, true).contains("bg-card-raised"));

        let offer = target_offer(TargetScope::Everything);
        let inert: Vec<&TargetChoice> = offer
            .choices
            .iter()
            .filter(|choice| !choice.runnable)
            .collect();
        assert!(!inert.is_empty(), "Q10's two boards are in the wide scope");
        for choice in inert {
            assert_eq!(choice.unavailable(), Some("no hardware manifest yet"));
        }
    }

    /// Two menus on one page get different row ids, so an arrow key in the
    /// settings row cannot move focus inside the add slot's menu.
    #[test]
    fn each_mounted_menu_gets_its_own_row_ids() {
        assert_ne!(next_menu_id(), next_menu_id());
    }
}
