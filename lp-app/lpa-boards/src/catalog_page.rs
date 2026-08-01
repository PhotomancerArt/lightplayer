//! `BoardsCatalogPage`: the public "what should I buy" page (`#/boards`).
//!
//! Boundary: catalog data in, nothing out. The page renders the embedded
//! display sidecars ([`crate::all_boards`]) through [`BoardDiagram`] and
//! knows nothing about projects, devices, or studio state. The host app
//! passes the detected [`HostOs`] so driver warnings (plan decision D5) stay
//! platform-blind here. Styling rides `lpb-cat-*` / `lpb-det-*` classes
//! owned by the consuming app's stylesheet.
//!
//! Detail is in-page selection, not routing: clicking a card swaps the grid
//! for [`BoardDetail`], and the hash is mirrored to
//! `#/boards/<vendor>/<product>` via `replaceState` (fires no events) so
//! board pages stay deep-linkable — the host passes the initial selection
//! parsed from the URL.

use dioxus::prelude::*;

use crate::display_manifest::{BoardDisplayFile, SupportTier};
use crate::geometry::DiagramMode;
use crate::usb_bridge::{DriverGuidance, DriverNeedLevel, HostOs};
use crate::{BoardDiagram, all_boards, board_by_id};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortBy {
    Recommended,
    Price,
    Name,
}

fn tier_rank(tier: SupportTier) -> u8 {
    match tier {
        SupportTier::Gold => 0,
        SupportTier::Silver => 1,
        SupportTier::Bronze => 2,
    }
}

fn tier_label(tier: SupportTier) -> &'static str {
    match tier {
        SupportTier::Gold => "gold",
        SupportTier::Silver => "silver",
        SupportTier::Bronze => "bronze",
    }
}

fn price_label(price_usd: f64) -> String {
    if price_usd.fract() == 0.0 {
        format!("${price_usd:.0}")
    } else {
        format!("${price_usd:.2}")
    }
}

/// The driver guidance worth surfacing for a board on `os`, if any.
fn driver_warning(board: &BoardDisplayFile, os: HostOs) -> Option<DriverGuidance> {
    board
        .usb_bridge
        .map(|bridge| bridge.guidance(os))
        .filter(|guidance| guidance.level == DriverNeedLevel::Warning)
}

/// `family` value → the human SoC name shown on its filter chip, taken from
/// the first board carrying the family.
fn family_chips() -> Vec<(String, String)> {
    let mut chips: Vec<(String, String)> = Vec::new();
    for board in all_boards() {
        if !chips.iter().any(|(family, _)| *family == board.family) {
            chips.push((board.family.clone(), board.soc.clone()));
        }
    }
    chips
}

/// Mirror the selection into the URL without firing any events — board
/// detail stays deep-linkable while the page keeps SPA behavior.
fn mirror_selection_hash(selected: Option<&str>) {
    #[cfg(target_arch = "wasm32")]
    {
        let hash = match selected {
            Some(board_id) => format!("#/boards/{board_id}"),
            None => "#/boards".to_string(),
        };
        if let Some(history) = web_sys::window().and_then(|window| window.history().ok()) {
            let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&hash));
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = selected;
}

/// The whole catalog page. `os` drives which driver warnings show — hosts
/// detect it at the platform edge and pass it in. `initial_board` is the
/// deep-linked selection parsed from the URL, if any.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn BoardsCatalogPage(os: HostOs, #[props(default)] initial_board: Option<String>) -> Element {
    let mut sort_by = use_signal(|| SortBy::Recommended);
    let mut family_filter = use_signal(|| Option::<String>::None);
    let mut selected =
        use_signal(|| initial_board.filter(|board_id| board_by_id(board_id).is_some()));

    if let Some(board_id) = selected() {
        let board = board_by_id(&board_id).expect("selection validated on set");
        return rsx! {
            div { class: "lpb-cat-page",
                button {
                    class: "lpb-det-back",
                    onclick: move |_| {
                        selected.set(None);
                        mirror_selection_hash(None);
                    },
                    "← All boards"
                }
                BoardDetail { board: board.clone(), os }
            }
        };
    }

    let mut boards: Vec<&'static BoardDisplayFile> = all_boards()
        .iter()
        .filter(|board| {
            family_filter()
                .map(|family| board.family == family)
                .unwrap_or(true)
        })
        .collect();
    match sort_by() {
        SortBy::Recommended => boards.sort_by(|a, b| {
            tier_rank(a.tier)
                .cmp(&tier_rank(b.tier))
                .then(a.price_usd.total_cmp(&b.price_usd))
        }),
        SortBy::Price => boards.sort_by(|a, b| a.price_usd.total_cmp(&b.price_usd)),
        SortBy::Name => boards.sort_by(|a, b| a.display_name.cmp(&b.display_name)),
    }

    rsx! {
        div { class: "lpb-cat-page",
            header { class: "lpb-cat-header",
                h1 { "Supported boards" }
                p { class: "lpb-cat-sub",
                    "Every drawing renders from the same checked-in metadata that drives provisioning and pin discovery — no hand-drawn images."
                }
            }
            div { class: "lpb-cat-tier-legend",
                span { class: "lpb-cat-def",
                    span { class: "lpb-cat-tier lpb-cat-tier--gold", "Gold" }
                    "recommended first — full feature set"
                }
                span { class: "lpb-cat-def",
                    span { class: "lpb-cat-tier lpb-cat-tier--silver", "Silver" }
                    "supported — limits apply (features or testing)"
                }
                span { class: "lpb-cat-def",
                    span { class: "lpb-cat-tier lpb-cat-tier--bronze", "Bronze" }
                    "community-verified, should work"
                }
            }
            div { class: "lpb-cat-controls",
                span { class: "lpb-cat-group",
                    b { "Sort" }
                    select {
                        class: "lpb-cat-select",
                        onchange: move |event| {
                            sort_by.set(match event.value().as_str() {
                                "price" => SortBy::Price,
                                "name" => SortBy::Name,
                                _ => SortBy::Recommended,
                            });
                        },
                        option { value: "rec", "Recommended" }
                        option { value: "price", "Price ↑" }
                        option { value: "name", "Name" }
                    }
                }
                span { class: "lpb-cat-group",
                    b { "SoC" }
                    span { class: "lpb-cat-chiprow",
                        button {
                            class: "lpb-cat-fchip",
                            "aria-pressed": if family_filter().is_none() { "true" } else { "false" },
                            onclick: move |_| family_filter.set(None),
                            "All"
                        }
                        for (family, soc) in family_chips() {
                            button {
                                class: "lpb-cat-fchip",
                                "aria-pressed": if family_filter().as_deref() == Some(family.as_str()) { "true" } else { "false" },
                                onclick: {
                                    let family = family.clone();
                                    move |_| family_filter.set(Some(family.clone()))
                                },
                                "{soc}"
                            }
                        }
                    }
                }
            }
            div { class: "lpb-cat-grid",
                for board in boards {
                    BoardCard {
                        board: board.clone(),
                        os,
                        on_open: {
                            let board_id = board.board_id.clone();
                            move |_| {
                                selected.set(Some(board_id.clone()));
                                mirror_selection_hash(Some(&board_id));
                            }
                        },
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn BoardCard(board: BoardDisplayFile, os: HostOs, on_open: EventHandler<()>) -> Element {
    let price = price_label(board.price_usd);
    let driver = driver_warning(&board, os);

    rsx! {
        article { class: "lpb-cat-card",
            button { class: "lpb-cat-open", onclick: move |_| on_open.call(()),
                div { class: "lpb-cat-figure",
                    span { class: "lpb-cat-tier lpb-cat-tier--{tier_label(board.tier)}",
                        "{tier_label(board.tier)}"
                    }
                    span { class: "lpb-cat-price", "{price}" }
                    BoardDiagram {
                        board: board.clone(),
                        mode: DiagramMode::Plain,
                        scale: 0.6,
                        labels: false,
                    }
                }
                div { class: "lpb-cat-title",
                    span { class: "lpb-cat-name", "{board.display_name}" }
                    span { class: "lpb-cat-mfr", "{board.manufacturer}" }
                }
            }
            div { class: "lpb-cat-info",
                div { class: "lpb-cat-specs",
                    span { class: "lpb-cat-spec lpb-cat-spec--soc", "{board.soc}" }
                    span { class: "lpb-cat-spec", "{board.flash} flash" }
                    if let Some(psram) = &board.psram {
                        span { class: "lpb-cat-spec", "{psram} psram" }
                    }
                    for capability in board.capabilities.iter() {
                        span { class: "lpb-cat-spec", "{capability}" }
                    }
                    if let Some(guidance) = driver {
                        span {
                            class: "lpb-cat-driver-chip",
                            title: "{guidance.summary} — open the board for setup steps",
                            "DRIVER REQUIRED · {os.display_name()}"
                        }
                    }
                }
                p { class: "lpb-cat-blurb", "{board.blurb}" }
                if let Some(note) = &board.support_note {
                    p { class: "lpb-cat-support-note", "{note}" }
                }
                div { class: "lpb-cat-links",
                    for url in board.purchase_urls.iter() {
                        a {
                            class: "lpb-cat-buy",
                            href: "{url.href}",
                            target: "_blank",
                            rel: "noopener",
                            "{url.label} ↗"
                        }
                    }
                }
            }
        }
    }
}

/// One board's detail view: full pinout, specs, driver setup for the
/// current OS, and the board's notes.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn BoardDetail(board: BoardDisplayFile, os: HostOs) -> Element {
    let price = price_label(board.price_usd);
    let bridge = board.usb_bridge;
    let notes: Vec<_> = board
        .notes
        .iter()
        .filter(|note| note.os.is_none_or(|note_os| note_os.matches(os)))
        .cloned()
        .collect();

    rsx! {
        div { class: "lpb-det",
            header { class: "lpb-det-header",
                div { class: "lpb-det-title",
                    h1 { "{board.display_name}" }
                    span { class: "lpb-cat-mfr", "{board.manufacturer}" }
                    span { class: "lpb-cat-tier lpb-cat-tier--{tier_label(board.tier)}",
                        "{tier_label(board.tier)}"
                    }
                    span { class: "lpb-cat-price", "{price}" }
                }
                div { class: "lpb-cat-specs",
                    span { class: "lpb-cat-spec lpb-cat-spec--soc", "{board.soc}" }
                    span { class: "lpb-cat-spec", "{board.flash} flash" }
                    if let Some(psram) = &board.psram {
                        span { class: "lpb-cat-spec", "{psram} psram" }
                    }
                    for capability in board.capabilities.iter() {
                        span { class: "lpb-cat-spec", "{capability}" }
                    }
                }
                p { class: "lpb-cat-blurb", "{board.blurb}" }
                if let Some(note) = &board.support_note {
                    p { class: "lpb-cat-support-note", "{note}" }
                }
                div { class: "lpb-cat-links",
                    for url in board.purchase_urls.iter() {
                        a {
                            class: "lpb-cat-buy",
                            href: "{url.href}",
                            target: "_blank",
                            rel: "noopener",
                            "{url.label} ↗"
                        }
                    }
                }
            }
            div { class: "lpb-det-figure",
                BoardDiagram {
                    board: board.clone(),
                    mode: DiagramMode::Caps,
                    u: 13.0,
                    scale: 1.35,
                }
            }
            if let Some(bridge) = bridge {
                {
                    let guidance = bridge.guidance(os);
                    let warning = guidance.level == DriverNeedLevel::Warning;
                    rsx! {
                        section {
                            class: if warning { "lpb-det-section lpb-det-driver lpb-det-driver--warning" } else { "lpb-det-section lpb-det-driver" },
                            h2 { "USB driver — {os.display_name()}" }
                            p { class: "lpb-det-driver-summary",
                                if warning {
                                    span { class: "lpb-cat-driver-chip", "DRIVER REQUIRED" }
                                }
                                "{guidance.summary} (bridge: {bridge.display_name()})"
                            }
                            if !guidance.steps.is_empty() {
                                ol { class: "lpb-det-steps",
                                    for step in guidance.steps {
                                        li { "{step}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !notes.is_empty() {
                section { class: "lpb-det-section",
                    h2 { "Notes" }
                    ul { class: "lpb-det-notes",
                        for note in notes.iter() {
                            li {
                                if let Some(note_os) = note.os {
                                    span { class: "lpb-det-note-tag",
                                        "{note_os.display_name()}"
                                        if let Some(version) = &note.os_version {
                                            " {version}"
                                        }
                                    }
                                }
                                "{note.text}"
                            }
                        }
                    }
                }
            }
        }
    }
}
