//! A "Your projects" gallery card.

use dioxus::prelude::*;
use lpa_studio_core::app::library::PackageHealth;
use lpa_studio_core::{
    ActionConfirmation, ControllerId, HOME_NODE_ID, HomeOp, PreviewSource, UiAction, UiPackageCard,
};

use lpa_studio_core::core::time_ago::time_ago;
use lpc_cloud_api::share_link::slugify;

use crate::router::canonical_share_path;

use crate::app::home::card_thumb::CardThumb;
use crate::app::home::gallery_preview::{ThumbMode, card_hover_handlers};
use crate::app::home::package_export::export_package_to_download;
use crate::base::{DetailPopover, DetailSection, PopoverPlacement, StudioIcon, StudioIconName};
use crate::core::{ActionButton, ActionButtonVariant, menu_item_action_class, quiet_action_class};

/// One package card: thumbnail, name, meta, and the card menu. Clicking the
/// card opens the copy the card *is* — the library head, pushed to the
/// simulator (D13).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn PackageCard(
    card: UiPackageCard,
    /// This card's open is in flight.
    #[props(default = false)]
    opening: bool,
    /// Some other open is in flight. The card reads busy — but it still
    /// takes clicks: the newest click wins (D4), and a click that looks
    /// ignored is the failure this whole change exists to remove.
    #[props(default = false)]
    busy: bool,
    /// Fixed clock for stories; `None` uses the platform clock.
    #[props(default)]
    now_secs: Option<f64>,
    /// Open the card menu immediately (stories only).
    #[props(default = false)]
    menu_initially_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let now = now_secs.unwrap_or_else(platform_now_secs);
    let edited_line = card.last_saved_at.map(|at| time_ago(now, at));
    // A project this Studio cannot open is still ON SCREEN — that is the
    // point (P3). It just says what is wrong instead of pretending to be
    // openable: no open link, no push, no drag, and a menu cut down to the
    // two remedies that work on raw files (export, delete).
    let blocked = card
        .health
        .blocked()
        .map(|(headline, remedy)| (headline.to_string(), remedy.to_string()));
    let upgrades_from = match card.health {
        PackageHealth::UpgradesOnOpen { found } => Some(found),
        _ => None,
    };
    // the slug IS the title; the thumbnail initial skips its date stamp

    // The card's open link IS the project's share link (identity vision
    // D1/D9): one address, so "copy what the address bar says" is the
    // whole share gesture. The slug is recomputed from the display name
    // rather than trusted from the library, so a rename shows up in the
    // link the moment the card does.
    let open_href = canonical_share_path(&slugify(&card.slug), &card.uid);

    // A live preview leases a runtime and LOADS the project. On a package
    // this Studio cannot read that is a guaranteed failure, so the blocked
    // card keeps its seeded placeholder — and hovers no preview either.
    let source = blocked
        .is_none()
        .then(|| PreviewSource::ProjectUid(card.uid.clone()));
    // Hover-to-play: pointing at a card is what buys live rendering. The
    // stretched open link is a CHILD of this article, so entering it is
    // still entering the card. Touch devices never send these, so a tap
    // still just follows the link.
    let (hover_enter, hover_leave) = card_hover_handlers(source.as_ref());

    rsx! {
        article {
            class: package_card_class(opening, busy, blocked.is_some()),
            onmouseenter: hover_enter,
            onmouseleave: hover_leave,
            // Opening a card is NAVIGATION, so it is a real <a> to the
            // project route (D37: the URL points at a runtime — a project
            // always opens on the sim, never a device takeover): plain
            // click rides the route listener → open path, and cmd/middle-click
            // "open in new tab" works natively. The link stretches over
            // the card (absolute overlay) instead of wrapping it, so the
            // card menu isn't interactive-inside-interactive markup; the
            // menu floats above it (z-order).
            if blocked.is_none() {
                a {
                    class: "tw:absolute tw:inset-0 tw:z-[1]",
                    href: "{open_href}",
                    aria_label: "Open {card.slug}",
                    onclick: move |event| {
                        // Only this card's OWN in-flight open holds the
                        // navigation: any other card's click supersedes it
                        // (D4), which is exactly what following the link
                        // does — the route listener dispatches the new open.
                        if opening {
                            event.prevent_default();
                        }
                    },
                }
            }
            CardThumb {
                seed: card.uid.clone(),
                label: card.slug.clone(),
                source,
                // Poster-first, like the example shelf: the library page is
                // for finding a project, not for watching twelve of them.
                mode: ThumbMode::PosterFirst,
            }
            div { class: "tw:flex tw:items-start tw:justify-between tw:gap-2 tw:p-3",
                div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
                    p { class: "tw:m-0 tw:truncate tw:text-sm tw:font-semibold tw:text-strong-foreground",
                        "{card.slug}"
                    }
                    if let Some((headline, remedy)) = blocked.clone() {
                        // amber = honest bad content (the roster precedent);
                        // never violet, which means "bound" in this Studio
                        p { class: "tw:m-0 tw:text-xs tw:font-semibold tw:text-status-attention-foreground",
                            "{headline}"
                        }
                        p { class: "tw:m-0 tw:text-xs tw:leading-normal tw:text-muted-foreground",
                            "{remedy}"
                        }
                    } else if opening {
                        p { class: "tw:m-0 tw:text-xs tw:text-status-working-foreground", "Opening…" }
                    } else {
                        if let Some(found) = upgrades_from {
                            // a fact, not a warning: it opens, and opening
                            // it is what upgrades it
                            p { class: "tw:m-0 tw:text-xs tw:text-dim-foreground",
                                title: "Opening this project upgrades it to the current format and saves a version you can go back to.",
                                "Format {found} — upgrades when you open it"
                            }
                        }
                        if let Some(edited) = edited_line {
                            p { class: "tw:m-0 tw:text-xs tw:text-muted-foreground", "Edited {edited}" }
                        }
                        // Advisory board target (vision D3): a quiet fact,
                        // not a warning — mismatch tint is P06's job, in
                        // mismatch context only.
                        if let Some(target) = card.target.as_deref() {
                            p { class: "tw:m-0 tw:truncate tw:text-xs tw:text-muted-foreground",
                                "for {target_display_name(target)}"
                            }
                        }
                        if let Some(provenance) = card.provenance.clone() {
                            p { class: "tw:m-0 tw:truncate tw:text-xs tw:text-dim-foreground", "{provenance}" }
                        }
                        // the remembered-association parity line
                        if let Some(device) = card.on_device.clone() {
                            p { class: "tw:m-0 tw:truncate tw:text-xs tw:text-status-good-foreground",
                                "On {device} ✓"
                            }
                        }
                        // D28: the runtime-presence chip. A pointer,
                        // deliberately inert: no runtime grab from a
                        // project card (D29's never-a-surprise-takeover);
                        // the runtime card itself sits one glance up in
                        // the roster.
                        if let Some(live) = live_presence_line(&card) {
                            p { class: live.class, title: live.title, "{live.text}" }
                        }
                        // a fact, not a warning: neutral chip; the card stays
                        // clickable — the open's refusal notice explains
                        if card.open_elsewhere {
                            p { class: "tw:m-0 tw:text-xs tw:text-muted-foreground",
                                span { class: "tw:inline-block tw:rounded tw:border tw:border-border tw:px-1.5 tw:py-px",
                                    "Open in another tab"
                                }
                            }
                        }
                    }
                }
                span {
                    class: "tw:relative tw:z-[2]",
                    PackageCardMenu {
                        card: card.clone(),
                        initially_open: menu_initially_open,
                        on_action,
                    }
                }
            }
            // the crystallized open action (D36 prep): same navigation as
            // the bare card click, spelled out — projects always open on
            // the sim, never a device takeover (D29). Beside it, one
            // "Put on <name>" per connected-empty device (model §1-A):
            // the explicit button IS the D11 consent, and pushing to a
            // blank board destroys nothing — one click, no confirm.
            div { class: "tw:flex tw:flex-wrap tw:gap-1 tw:px-3 tw:pb-3",
                // Both chooser buttons wear the SAME quiet-chip treatment
                // (2026-07-26 walk: the anchor's UA underline read as a
                // link, the push button's accent tint didn't match — a
                // matched pair now; the <a> stays an <a> for D37 nav).
                if blocked.is_none() {
                    a {
                        class: "{quiet_action_class()} tw:relative tw:z-[2] tw:no-underline",
                        href: "{open_href}",
                        title: "Open this project in the simulator.",
                        onclick: move |event| {
                            if opening {
                                event.prevent_default();
                            }
                        },
                        span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center", aria_hidden: "true",
                            StudioIcon { name: StudioIconName::Play, size: 14 }
                        }
                        span { "Open in sim" }
                    }
                } else {
                    // The remedies, spelled out on the card itself: export
                    // reads raw files, so it works on exactly the packages
                    // that need rescuing.
                    button {
                        class: "{quiet_action_class()} tw:relative tw:z-[2]",
                        r#type: "button",
                        title: "Download this project as a zip archive — the files are exported as they are.",
                        onclick: {
                            let export_card = card.clone();
                            move |_| export_package_to_download(&export_card)
                        },
                        span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center", aria_hidden: "true",
                            StudioIcon { name: StudioIconName::Download, size: 14 }
                        }
                        span { "Download zip" }
                    }
                }
            }
        }
    }
}

/// The card menu: rename form plus duplicate / export / delete rows. The
/// rows are `UiAction`s rendered in the shared menu-item context (export is
/// a web-side handler wearing the same classes) — one action vocabulary,
/// one look.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn PackageCardMenu(
    card: UiPackageCard,
    /// Open the menu immediately (stories only).
    #[props(default = false)]
    initially_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let mut rename_value = use_signal(|| card.slug.clone());
    let rename_uid = card.uid.clone();
    let export_card = card.clone();
    // Rename and duplicate both round-trip the manifest through the strict
    // reader, so on a package this Studio cannot read they would fail with
    // a parser complaint. A blocked card offers only what works on raw
    // bytes: export the files, or delete the package.
    let blocked = !card.health.is_openable();
    let duplicate = home_action(HomeOp::DuplicatePackage {
        uid: card.uid.clone(),
    });
    let delete = home_action(HomeOp::DeletePackage {
        uid: card.uid.clone(),
    })
    .with_confirmation(ActionConfirmation::new(
        "Delete project",
        format!(
            "Delete \"{}\" and its history from your library?",
            card.slug
        ),
        "Delete",
    ));

    // "New project from this…" (module authoring unit, P5): only a
    // PATTERN project has an export to build a project around, so the row
    // is absent — not disabled — on everything else. A general project has
    // no answer to "from WHICH module", and a disabled row that can never
    // become enabled teaches nothing.
    let new_from =
        (!blocked && card.project_kind == PATTERN_KIND_LABEL && !card.exports.is_empty())
            .then(|| card.exports.clone());

    rsx! {
        DetailPopover {
            icon: StudioIconName::More,
            label: "Project actions".to_string(),
            placement: PopoverPlacement::BottomEnd,
            initially_open,
            if let Some(exports) = new_from.clone() {
                DetailSection { title: Some("New project from this\u{2026}".to_string()),
                    NewFromPatternForm { uid: card.uid.clone(), exports, on_action }
                }
            }
            if !blocked {
                DetailSection { title: Some("Rename".to_string()),
                    form {
                        class: "tw:flex tw:gap-2",
                        onsubmit: move |event| {
                            event.prevent_default();
                            let name = rename_value.read().trim().to_string();
                            if !name.is_empty() {
                                on_action.call(home_action(HomeOp::RenamePackage {
                                    uid: rename_uid.clone(),
                                    name,
                                }));
                            }
                        },
                        input {
                            class: "tw:min-w-0 tw:flex-1 tw:rounded tw:border tw:border-border tw:bg-terminal tw:px-2 tw:py-1 tw:text-sm tw:text-strong-foreground",
                            value: "{rename_value}",
                            oninput: move |event| rename_value.set(event.value()),
                        }
                        button { class: quiet_action_class(), r#type: "submit", "Rename" }
                    }
                }
            }
            DetailSection {
                div { class: "tw:grid tw:gap-0.5",
                    if !blocked {
                        ActionButton {
                            action: duplicate,
                            running: false,
                            variant: ActionButtonVariant::MenuItem,
                            on_action,
                        }
                    }
                    button {
                        class: menu_item_action_class(),
                        r#type: "button",
                        title: "Download this project as a zip archive.",
                        onclick: move |_| export_package_to_download(&export_card),
                        span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center", aria_hidden: "true",
                            StudioIcon { name: StudioIconName::Download, size: 14 }
                        }
                        span { "Download zip" }
                    }
                    ActionButton {
                        action: delete,
                        running: false,
                        variant: ActionButtonVariant::MenuItem,
                        on_action,
                    }
                }
            }
        }
    }
}

/// The display label a pattern project's kind reads as (core's
/// `package_manifest::kind_label`).
const PATTERN_KIND_LABEL: &str = "Pattern";

/// The inline "New project from this…" form (the Rename precedent: a form
/// in the menu, never a dialog).
///
/// One name field, prefilled from the export — `fire-project`, not
/// "Untitled": the thing you are starting from is the thing worth naming
/// it after. A FAMILY (more than one export) grows a select ahead of it,
/// because "from this" is ambiguous the moment a package ships two
/// modules; a single-export package never sees the control.
///
/// The prefill follows the selected export until you type over it, at
/// which point your name wins and stops moving under you.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NewFromPatternForm(
    uid: String,
    exports: Vec<String>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let mut selected = use_signal(|| exports.first().cloned().unwrap_or_default());
    let mut typed = use_signal(|| Option::<String>::None);
    let export = selected.read().clone();
    let name = typed
        .read()
        .clone()
        .unwrap_or_else(|| default_project_name(&export));
    let submit_name = name.clone();

    rsx! {
        form {
            class: "tw:grid tw:gap-2",
            onsubmit: move |event| {
                event.prevent_default();
                let name = submit_name.trim().to_string();
                if !name.is_empty() {
                    on_action.call(home_action(HomeOp::CreateFromPattern {
                        uid: uid.clone(),
                        export: selected.read().clone(),
                        name,
                    }));
                }
            },
            if exports.len() > 1 {
                select {
                    class: "tw:min-w-0 tw:rounded tw:border tw:border-border tw:bg-terminal tw:px-2 tw:py-1 tw:text-sm tw:text-strong-foreground",
                    value: "{export}",
                    onchange: move |event| selected.set(event.value()),
                    for name in exports.iter().cloned() {
                        // `selected` mirrors the bound value onto the option:
                        // the select's own `value` lands before its options
                        // mount, so it alone cannot restore the selection.
                        option {
                            key: "{name}",
                            value: "{name}",
                            selected: name == export,
                            "{name}"
                        }
                    }
                }
            }
            div { class: "tw:flex tw:gap-2",
                input {
                    class: "tw:min-w-0 tw:flex-1 tw:rounded tw:border tw:border-border tw:bg-terminal tw:px-2 tw:py-1 tw:text-sm tw:text-strong-foreground",
                    value: "{name}",
                    oninput: move |event| typed.set(Some(event.value())),
                }
                button { class: quiet_action_class(), r#type: "submit", "Create" }
            }
        }
    }
}

/// The prefilled name for a new project built around `export`.
fn default_project_name(export: &str) -> String {
    format!("{export}-project")
}

/// Friendly display form of a project's advisory `target` (vendor/product
/// board id) for the "for \<board\>" card badge: the catalog's
/// `display_name` when the id is a known board, else the raw id verbatim —
/// advisory metadata may name a board this build's catalog doesn't carry
/// (a future board, a typo'd id), and the badge should still say something
/// rather than disappear.
fn target_display_name(target: &str) -> &str {
    lpa_boards::all_boards()
        .iter()
        .find(|board| board.board_id == target)
        .map(|board| board.display_name.as_str())
        .unwrap_or(target)
}

pub(crate) fn home_action(op: HomeOp) -> UiAction {
    UiAction::from_op(ControllerId::new(HOME_NODE_ID), op)
}

/// The card's treatment while an open runs. `busy` is a DIMMING, not a
/// disabling: the card still acts (it supersedes), so it keeps its
/// pointer cursor.
fn package_card_class(opening: bool, busy: bool, blocked: bool) -> &'static str {
    // tw:relative anchors the stretched open link (see the card markup)
    if blocked {
        // amber edge, default cursor: the card is a statement, not a door
        "tw:relative tw:overflow-hidden tw:rounded-md tw:border tw:border-status-attention-border tw:bg-card"
    } else if opening {
        "tw:relative tw:cursor-wait tw:overflow-hidden tw:rounded-md tw:border tw:border-status-working-border tw:bg-card"
    } else if busy {
        "tw:relative tw:cursor-pointer tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card tw:opacity-60 tw:transition-opacity"
    } else {
        "tw:relative tw:cursor-pointer tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card tw:transition-colors tw:hover:border-border-strong"
    }
}

// The card→card drag hand-off (drop a project onto a device card = the
// push-confirm sheet) went with M2 of the device-model rebuild: there is
// no drop target left. The rebuilt device model re-adds both halves.

/// One rendered runtime-presence line (the D28 chip family).
#[derive(Debug, PartialEq, Eq)]
struct LivePresenceLine {
    text: String,
    class: &'static str,
    /// Tooltip spelling out the places on the aggregate line; `None` when
    /// the single line already says everything.
    title: Option<String>,
}

const LIVE_LINE_GOOD: &str = "tw:m-0 tw:truncate tw:text-xs tw:text-status-good-foreground";

/// The card's runtime-presence line (D28): "Running in simulator" while
/// the sim runs this project's head — load-as-push always runs the head,
/// so the sim is current (green).
///
/// ⚠️ The DEVICE lines (the D24 connected line and the "Live in 2 places"
/// aggregate) went with M2 of the device-model rebuild, along with the
/// `connected_device` connection the card carried. The rebuilt device
/// model re-adds them.
fn live_presence_line(card: &UiPackageCard) -> Option<LivePresenceLine> {
    card.running_in_sim.then(|| LivePresenceLine {
        text: "Running in simulator".to_string(),
        class: LIVE_LINE_GOOD,
        title: None,
    })
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn platform_now_secs() -> f64 {
    js_sys::Date::now() / 1000.0
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn platform_now_secs() -> f64 {
    0.0
}

/// P02: the "for \<board\>" badge resolves a known catalog id to its
/// display name, and falls back to the raw string for an id the catalog
/// doesn't carry — advisory metadata should still render *something*
/// rather than vanish.
#[cfg(test)]
mod target_display_name_tests {
    use super::target_display_name;

    #[test]
    fn known_board_resolves_to_its_display_name() {
        assert_eq!(
            target_display_name("espressif/esp32-c6-devkitc-1"),
            "ESP32-C6-DevKitC-1"
        );
    }

    #[test]
    fn unknown_board_falls_back_to_the_raw_id() {
        assert_eq!(
            target_display_name("acme/future-board-9000"),
            "acme/future-board-9000"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(running_in_sim: bool) -> UiPackageCard {
        UiPackageCard {
            uid: "prj1".to_string(),
            kind: "Module".to_string(),
            project_kind: "General".to_string(),
            exports: Vec::new(),
            slug: "2026-07-09-1421-basic".to_string(),
            last_saved_at: None,
            provenance: None,
            on_device: None,
            open_elsewhere: false,
            running_in_sim,
            target: None,
            health: PackageHealth::Ready,
        }
    }

    #[test]
    fn a_blocked_card_offers_only_the_remedies_that_work_on_raw_files() {
        // The gallery's contract after P3: an unopenable project is still
        // here, still named, still exportable and deletable.
        let mut blocked = card(false);
        blocked.health = PackageHealth::Blocked {
            headline: "Format 3 — too old for this Studio".to_string(),
            remedy: "Export a copy or delete it.".to_string(),
        };
        assert!(!blocked.health.is_openable());
        assert_eq!(
            blocked.health.blocked().map(|(headline, _)| headline),
            Some("Format 3 — too old for this Studio")
        );
        assert!(package_card_class(false, false, true).contains("status-attention-border"));
    }

    #[test]
    fn an_upgradable_card_stays_a_normal_card() {
        let mut upgradable = card(false);
        upgradable.health = PackageHealth::UpgradesOnOpen { found: 4 };
        assert!(upgradable.health.is_openable());
        assert_eq!(upgradable.health.blocked(), None);
        assert!(!package_card_class(false, false, false).contains("status-attention-border"));
    }

    #[test]
    fn the_sim_line_appears_exactly_while_the_sim_runs_this_project() {
        let sim = live_presence_line(&card(true)).expect("the sim line");
        assert_eq!(sim.text, "Running in simulator");
        assert_eq!(sim.class, LIVE_LINE_GOOD, "the sim always runs the head");
        assert_eq!(sim.title, None);

        assert_eq!(live_presence_line(&card(false)), None);
    }
}
