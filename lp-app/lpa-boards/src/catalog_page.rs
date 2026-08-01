//! `BoardsCatalogPage`: the public "what should I buy" page (`#/boards`).
//!
//! Boundary: catalog data in, nothing out. The page renders the embedded
//! display sidecars ([`crate::all_boards`]) through [`BoardDiagram`] and
//! knows nothing about projects, devices, or studio state. The host app
//! passes the detected [`HostOs`] so driver warnings (plan decision D5) stay
//! platform-blind here. Styling rides `lpb-cat-*` classes owned by the
//! consuming app's stylesheet.

use dioxus::prelude::*;

use crate::display_manifest::{BoardDisplayFile, SupportTier};
use crate::geometry::DiagramMode;
use crate::usb_bridge::{DriverNeedLevel, HostOs};
use crate::{BoardDiagram, all_boards};

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

/// The whole catalog page. `os` drives which driver warnings show — hosts
/// detect it at the platform edge and pass it in.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn BoardsCatalogPage(os: HostOs) -> Element {
    let mut sort_by = use_signal(|| SortBy::Recommended);
    let mut family_filter = use_signal(|| Option::<String>::None);

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
                    "first-class, tested every release"
                }
                span { class: "lpb-cat-def",
                    span { class: "lpb-cat-tier lpb-cat-tier--silver", "Silver" }
                    "supported, tested occasionally"
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
                    BoardCard { board: board.clone(), os }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn BoardCard(board: BoardDisplayFile, os: HostOs) -> Element {
    let price = if board.price_usd.fract() == 0.0 {
        format!("${:.0}", board.price_usd)
    } else {
        format!("${:.2}", board.price_usd)
    };
    let driver = board
        .usb_bridge
        .map(|bridge| (bridge, bridge.guidance(os)))
        .filter(|(_, guidance)| guidance.level == DriverNeedLevel::Warning);

    rsx! {
        article { class: "lpb-cat-card",
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
            div { class: "lpb-cat-info",
                div { class: "lpb-cat-title",
                    span { class: "lpb-cat-name", "{board.display_name}" }
                    span { class: "lpb-cat-mfr", "{board.manufacturer}" }
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
                if let Some((bridge, guidance)) = driver {
                    details { class: "lpb-cat-driver-warn",
                        summary {
                            span { class: "lpb-cat-driver-badge", "driver required" }
                            "{guidance.summary}"
                        }
                        ol {
                            for step in guidance.steps {
                                li { "{step}" }
                            }
                        }
                        span { class: "lpb-cat-driver-chip-note", "bridge: {bridge.display_name()}" }
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
