//! A "Your projects" gallery card.

use dioxus::prelude::*;
use lpa_studio_core::app::library::PackageHealth;
use lpa_studio_core::{
    ActionConfirmation, ControllerId, HOME_NODE_ID, HomeOp, PreviewSource, UiAction, UiPackageCard,
};

use lpa_studio_core::core::time_ago::time_ago;
use lpc_cloud_api::share_link::slugify;

use crate::router::canonical_share_path;

use crate::app::home::card_footer::{
    CardContextLine, CardGlassFooter, CardStatusGlyph, ContextTone, GlyphTone,
};
use crate::app::home::card_thumb::CardThumb;
use crate::app::home::gallery_preview::{ThumbMode, card_hover_handlers};
use crate::app::home::package_export::export_package_to_download;
use crate::base::{
    DetailPopover, DetailSection, PopoverPlacement, StudioIcon, StudioIconName, Toasts,
};
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
            class: package_card_class(opening, busy, blocked.is_some(), false),
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
            // The face is the art; the words are one slim glass bar —
            // title, status glyphs, and the ⋯ IN the bar (G1 feedback:
            // nothing floats on the picture). Everything deeper —
            // status in words, the actions — is the ⋯ popup's job (the
            // redesign's "second click"). The footer sits under the
            // stretched open link, so the whole face stays a door.
            CardGlassFooter {
                title: card.slug.clone(),
                context: face_context_line(blocked.as_ref(), opening),
                glyphs: face_status_glyphs(&card, blocked.is_some()),
                // Hover slides the bar up over the quiet facts — capped,
                // delayed, and skipped entirely while the card is already
                // saying something louder (blocked / opening).
                reveal: (blocked.is_none() && !opening).then(|| {
                    let live = live_presence_line(&card);
                    rsx! {
                        if let Some(edited) = edited_line.clone() {
                            p { class: "tw:m-0 tw:truncate tw:text-xs tw:text-muted-foreground",
                                "Edited {edited}"
                            }
                        }
                        if let Some(provenance) = card.provenance.clone() {
                            p { class: "tw:m-0 tw:truncate tw:text-xs tw:text-dim-foreground",
                                "{provenance}"
                            }
                        }
                        if let Some(live) = live {
                            p { class: live.class, "{live.text}" }
                        }
                    }
                }),
                trailing: rsx! {
                    PackageCardMenu {
                        card: card.clone(),
                        initially_open: menu_initially_open,
                        edited_line,
                        open_href: blocked.is_none().then(|| open_href.clone()),
                        opening,
                        on_action,
                    }
                },
            }
        }
    }
}

/// The face's single context line — only when the card demands
/// attention: the blocked headline (amber, matching the border) or
/// "Opening…". Quiet facts (edited stamp, provenance) read as noise
/// repeated across a grid (G1 feedback 2026-08-26) and live in the ⋯
/// popup instead; a quiet card wears a title-only bar.
fn face_context_line(blocked: Option<&(String, String)>, opening: bool) -> Option<CardContextLine> {
    if let Some((headline, _remedy)) = blocked {
        // amber = honest bad content (the roster precedent); never
        // violet, which means "bound" in this Studio. The remedy words
        // live in the ⋯ popup.
        return Some(CardContextLine {
            text: headline.clone(),
            tone: ContextTone::Attention,
        });
    }
    opening.then(|| CardContextLine {
        text: "Opening…".to_string(),
        tone: ContextTone::Working,
    })
}

/// The title row's status glyphs — the D28 runtime-presence facts
/// compressed to icons (words ride the tooltip and the ⋯ popup):
/// lightning = on the connected device (green only when current, the
/// D24 rule), the sim glyph = running in simulator, amber "!" = the
/// card is blocked. Both runtimes live = both glyphs ("Live in 2
/// places" stays a popup/tooltip phrasing).
fn face_status_glyphs(card: &UiPackageCard, blocked: bool) -> Vec<CardStatusGlyph> {
    if blocked {
        return vec![CardStatusGlyph {
            icon: StudioIconName::StepAttention,
            tone: GlyphTone::Attention,
            words: "This project can't be opened by this Studio.".to_string(),
        }];
    }
    let mut glyphs = Vec::new();
    if card.running_in_sim {
        glyphs.push(CardStatusGlyph {
            icon: StudioIconName::Simulator,
            tone: GlyphTone::Live,
            words: "Running in simulator".to_string(),
        });
    }
    glyphs
}

/// The card menu — the redesigned card's DEPTH surface ("the second
/// click"): a status section carrying in words everything the slim face
/// compressed to glyphs, then the actions — open in sim, put on an
/// empty device, push, rename, duplicate, export, delete. The rows are
/// `UiAction`s rendered in the shared menu-item context (export is a
/// web-side handler wearing the same classes) — one action vocabulary,
/// one look.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn PackageCardMenu(
    card: UiPackageCard,
    /// Open the menu immediately (stories only).
    #[props(default = false)]
    initially_open: bool,
    /// The formatted "…ago" stamp, computed by the card (fixed clock in
    /// stories).
    #[props(default)]
    edited_line: Option<String>,
    /// The card's open link (D37: opening is navigation) for the
    /// "Open in sim" row. `None` renders no row (blocked cards).
    #[props(default)]
    open_href: Option<String>,
    /// This card's open is in flight — the open row holds navigation.
    #[props(default = false)]
    opening: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let mut rename_value = use_signal(|| card.slug.clone());
    let rename_uid = card.uid.clone();
    let export_card = card.clone();
    // Copy link (G1 finding, 2026-08-29): the canonical `/p/<slug>-<uid>`
    // address, from the card's own identity. Cards carry the library slug
    // (dated); the cosmetic half heals on open, the uid is what matters.
    let link_name = card.slug.clone();
    let link_uid = card.uid.clone();
    let link_toasts = try_consume_context::<Toasts>();
    // Rename and duplicate both round-trip the manifest through the strict
    // reader, so on a package this Studio cannot read they would fail with
    // a parser complaint. A blocked card offers only what works on raw
    // bytes: export the files, or delete the package.
    let blocked = !card.health.is_openable();
    let duplicate = home_action(HomeOp::DuplicatePackage {
        uid: card.uid.clone(),
    });
    let delete = delete_package_action(&card);

    // "New project from this…" (module authoring unit, P5): only a
    // PATTERN project has an export to build a project around, so the row
    // is absent — not disabled — on everything else. A general project has
    // no answer to "from WHICH module", and a disabled row that can never
    // become enabled teaches nothing.
    let new_from =
        (!blocked && card.project_kind == PATTERN_KIND_LABEL && !card.exports.is_empty())
            .then(|| card.exports.clone());

    // The status facts, in words — everything the slim face compresses
    // away (card-overlay redesign). Derived here so the section renders
    // from the same helpers the face's tooltips use.
    let blocked_lines = card
        .health
        .blocked()
        .map(|(headline, remedy)| (headline.to_string(), remedy.to_string()));
    let upgrades_from = match card.health {
        PackageHealth::UpgradesOnOpen { found } => Some(found),
        _ => None,
    };
    let association = card.on_device.clone();
    let live = live_presence_line(&card);

    rsx! {
        DetailPopover {
            icon: StudioIconName::More,
            label: "Project actions".to_string(),
            placement: PopoverPlacement::BottomEnd,
            initially_open,
            // Compact trigger for the glass bar: the stock 32px toned
            // square crowded the slim footer (G1 feedback — "a little
            // big, wants a tiny bit more space around it"). No
            // preflight, so the class resets the UA button chrome.
            trigger: rsx! {
                span { class: "tw:inline-flex tw:items-center tw:justify-center",
                    StudioIcon { name: StudioIconName::More, size: 13 }
                }
            },
            trigger_class: CARD_MENU_TRIGGER_CLASS.to_string(),
            trigger_open_class: format!("{CARD_MENU_TRIGGER_CLASS} tw:bg-white/10 tw:text-strong-foreground"),
            // ---- status: the words behind the face's glyphs ----
            DetailSection {
                div { class: "tw:grid tw:gap-0.5",
                    if let Some((headline, remedy)) = blocked_lines {
                        // amber = honest bad content; never violet (bound)
                        p { class: "tw:m-0 tw:text-xs tw:font-semibold tw:text-status-attention-foreground",
                            "{headline}"
                        }
                        p { class: "tw:m-0 tw:text-xs tw:leading-normal tw:text-muted-foreground",
                            "{remedy}"
                        }
                    }
                    if let Some(found) = upgrades_from {
                        // a fact, not a warning: it opens, and opening it
                        // is what upgrades it
                        p { class: "tw:m-0 tw:text-xs tw:text-dim-foreground",
                            title: "Opening this project upgrades it to the current format and saves a version you can go back to.",
                            "Format {found} — upgrades when you open it"
                        }
                    }
                    if let Some(edited) = edited_line {
                        p { class: "tw:m-0 tw:text-xs tw:text-muted-foreground", "Edited {edited}" }
                    }
                    // Advisory board target (vision D3): a quiet fact, not
                    // a warning.
                    if let Some(target) = card.target.as_deref() {
                        p { class: "tw:m-0 tw:text-xs tw:text-muted-foreground",
                            "for {target_display_name(target)}"
                        }
                    }
                    if let Some(provenance) = card.provenance.clone() {
                        p { class: "tw:m-0 tw:text-xs tw:text-dim-foreground", "{provenance}" }
                    }
                    // the association parity line yields to the LIVE
                    // indication when the device is actually here
                    if let Some(device) = association {
                        p { class: "tw:m-0 tw:text-xs tw:text-status-good-foreground",
                            "On {device} ✓"
                        }
                    }
                    // D28 runtime presence, in full: the aggregate line
                    // spells out both places on its own second line —
                    // the popup has the room the face does not.
                    if let Some(live) = live {
                        p { class: live.class, "{live.text}" }
                        if let Some(places) = live.title {
                            p { class: "tw:m-0 tw:text-xs tw:text-muted-foreground", "{places}" }
                        }
                    }
                    // a fact, not a warning: the card stays clickable —
                    // the open's refusal notice explains
                    if card.open_elsewhere {
                        p { class: "tw:m-0 tw:text-xs tw:text-muted-foreground", "Open in another tab" }
                    }
                }
            }
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
                    // The crystallized open action (D36 prep): the same
                    // navigation as the card face, spelled out — projects
                    // always open on the sim, never a device takeover
                    // (D29). A real <a> for D37 nav.
                    if let Some(href) = open_href.clone() {
                        a {
                            class: "{menu_item_action_class()} tw:no-underline",
                            href: "{href}",
                            title: "Open this project in the simulator.",
                            onclick: move |event: MouseEvent| {
                                if opening {
                                    event.prevent_default();
                                }
                            },
                            span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center", aria_hidden: "true",
                                StudioIcon { name: StudioIconName::Play, size: 14 }
                            }
                            span { "Open in sim" }
                        }
                    }
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
                        title: "Copy this project's link — the same address the editor shows.",
                        onclick: move |_| {
                            crate::clipboard::write_text(
                                &crate::app::share::share_url::project_link_absolute(
                                    &link_name,
                                    &link_uid,
                                ),
                            );
                            if let Some(mut toasts) = link_toasts {
                                toasts.say("Link copied");
                            }
                        },
                        span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center", aria_hidden: "true",
                            StudioIcon { name: StudioIconName::ExternalLink, size: 14 }
                        }
                        span { "Copy link" }
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

/// The card menu's Delete action, wearing its confirmation.
///
/// INLINE (the armed two-click confirm), never the native dialog: a
/// `window.confirm` that is suppressed — automation-driven browsers
/// auto-dismiss it with `false` — makes the row a silent no-op (defect,
/// 2026-08-31: the Delete row "did nothing"), and the armed row is the
/// destructive-confirm language everywhere else (device Forget/Dismiss).
fn delete_package_action(card: &UiPackageCard) -> UiAction {
    home_action(HomeOp::DeletePackage {
        uid: card.uid.clone(),
    })
    .with_confirmation(
        ActionConfirmation::new(
            "Delete project",
            format!(
                "Delete \"{}\" and its history from your library?",
                card.slug
            ),
            "Delete",
        )
        .inline(),
    )
}

/// The glass bar's compact ⋯ trigger: a 20px quiet icon button (the
/// stock 32px icon-menu square overwhelms the slim footer). Resets UA
/// button chrome itself — Tailwind preflight is not loaded.
const CARD_MENU_TRIGGER_CLASS: &str = "tw:grid tw:h-5 tw:w-5 tw:flex-none tw:cursor-pointer tw:appearance-none tw:place-items-center tw:rounded tw:border-0 tw:bg-transparent tw:p-0 tw:text-muted-foreground tw:transition-colors tw:hover:bg-white/10 tw:hover:text-strong-foreground";

/// The card's treatment while an open runs. `busy` is a DIMMING, not a
/// disabling: the card still acts (it supersedes), so it keeps its
/// pointer cursor.
fn package_card_class(opening: bool, busy: bool, blocked: bool, dragging: bool) -> &'static str {
    // tw:relative anchors the stretched open link (see the card markup)
    if dragging {
        // Drag in flight: the pointer has left the card, so the ring is
        // pinned on rather than hover-gated, and the shadow says "lifted
        // off the page". The HTML5 drag image is a snapshot taken at
        // dragstart, so this styles the SOURCE — which is the card the eye
        // is tracking anyway while the ghost follows the cursor.
        return "tw:group tw:relative tw:cursor-grabbing tw:overflow-hidden tw:rounded-md tw:border tw:border-transparent tw:bg-card ux-ir-ring ux-ir-ring-inset ux-ir-ring-on ux-drag-chip";
    }
    if blocked {
        // amber edge, default cursor: the card is a statement, not a door
        "tw:group tw:relative tw:overflow-hidden tw:rounded-md tw:border tw:border-status-attention-border tw:bg-card"
    } else if opening {
        "tw:group tw:relative tw:cursor-wait tw:overflow-hidden tw:rounded-md tw:border tw:border-status-working-border tw:bg-card"
    } else if busy {
        "tw:group tw:relative tw:cursor-pointer tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card tw:opacity-60 tw:transition-opacity"
    } else {
        // Inset ring: the card clips its own overflow, so an outset ring
        // would be clipped away entirely (same trap .ux-glass-panel::after
        // documents). It paints at z-3, above the glass footer (z-2) and
        // the stretched open link (z-1), and takes no pointer events. The
        // resting border goes transparent on hover so the ring IS the edge
        // rather than a second line inside a grey one.
        "tw:group tw:relative tw:cursor-pointer tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card tw:transition-colors tw:hover:border-transparent ux-ir-ring ux-ir-ring-inset ux-card-lift"
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
        assert!(package_card_class(false, false, true, false).contains("status-attention-border"));
    }

    #[test]
    fn an_upgradable_card_stays_a_normal_card() {
        let mut upgradable = card(false);
        upgradable.health = PackageHealth::UpgradesOnOpen { found: 4 };
        assert!(upgradable.health.is_openable());
        assert_eq!(upgradable.health.blocked(), None);
        assert!(
            !package_card_class(false, false, false, false).contains("status-attention-border")
        );
    }

    #[test]
    fn a_card_in_flight_pins_the_ring_and_a_resting_one_does_not() {
        // The drag ghost is a snapshot taken at dragstart, so the SOURCE
        // card carries the in-flight light — and the pointer has left it,
        // so the ring cannot be hover-gated there.
        let dragging = package_card_class(false, false, false, true);
        assert!(dragging.contains("ux-ir-ring-on"), "{dragging}");
        assert!(dragging.contains("ux-drag-chip"), "{dragging}");
        for resting in [
            package_card_class(false, false, false, false),
            package_card_class(true, false, false, false),
            package_card_class(false, true, false, false),
            package_card_class(false, false, true, false),
        ] {
            assert!(!resting.contains("ux-ir-ring-on"), "{resting}");
        }
    }

    #[test]
    fn delete_confirms_inline_so_no_native_dialog_can_swallow_it() {
        // The armed two-click confirm runs entirely in the row; the native
        // `window.confirm` path silently no-ops wherever the browser
        // suppresses dialogs (2026-08-31 defect: the Delete row did
        // nothing in an automation-driven session).
        let delete = delete_package_action(&card(false));
        let meta = delete.meta();
        assert!(meta.destructive, "delete wears the danger dress");
        let confirmation = meta.confirmation.as_ref().expect("delete always asks");
        assert!(confirmation.inline, "armed confirm, never window.confirm");
        assert_eq!(confirmation.confirm_label, "Delete");
        assert!(
            confirmation.message.contains("2026-07-09-1421-basic"),
            "the confirmation names the project: {}",
            confirmation.message
        );
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
