//! A "Your projects" gallery card.

use std::cell::RefCell;

use dioxus::prelude::*;
use lpa_studio_core::app::library::PackageHealth;
use lpa_studio_core::{
    ActionConfirmation, ControllerId, DEPLOY_NODE_ID, DeployOp, DeviceTarget, HOME_NODE_ID, HomeOp,
    PreviewSource, SyncRelation, UiAction, UiPackageCard,
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
    /// Names of connected-EMPTY devices (state-flow model §1-A, settled
    /// 2026-07-26): each grows an explicit "⚡ Put on <name>" button on
    /// this card — the gallery IS the chooser for a freshly set-up board,
    /// and the target is always named, never guessed. Empty = no buttons.
    #[props(default)]
    /// The Connected-empty boards this card offers a one-click push to,
    /// as (card key, display name) — the key is the push target (M4).
    empty_devices: Vec<(String, String)>,
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
            class: package_card_class(opening, busy, blocked.is_some()),
            onmouseenter: hover_enter,
            onmouseleave: hover_leave,
            // drag a project onto a device card = the push-confirm sheet
            draggable: blocked.is_none(),
            ondragstart: {
                let uid = card.uid.clone();
                let draggable = blocked.is_none();
                move |_| {
                    if draggable {
                        set_dragged_project(uid.clone());
                    }
                }
            },
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
                trailing: rsx! {
                    PackageCardMenu {
                        card: card.clone(),
                        initially_open: menu_initially_open,
                        edited_line,
                        open_href: blocked.is_none().then(|| open_href.clone()),
                        opening,
                        empty_devices,
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
fn face_context_line(
    blocked: Option<&(String, String)>,
    opening: bool,
) -> Option<CardContextLine> {
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
    if let Some(connection) = card.connected_device.as_ref() {
        glyphs.push(CardStatusGlyph {
            icon: StudioIconName::Apply,
            tone: match connection.relation {
                SyncRelation::AtHead => GlyphTone::Good,
                SyncRelation::Behind | SyncRelation::Diverged => GlyphTone::Attention,
            },
            words: connected_line(&connection.device_name, connection.relation),
        });
    }
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
    /// Connected-empty boards offered a one-click push, as (card key,
    /// display name) — the former face chips, now rows (model §1-A: the
    /// explicit row IS the D11 consent; pushing to a blank board
    /// destroys nothing).
    #[props(default)]
    empty_devices: Vec<(String, String)>,
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

    // M8′ (dialog-free): the menu row IS the D11 consent, exactly like
    // the card's Push button — the push runs directly, progress on the
    // device card's Operation-in-flight lane.
    let push_to_device = card.connected_device.as_ref().map(|connection| {
        UiAction::from_op(
            ControllerId::new(DEPLOY_NODE_ID),
            DeployOp::PushProject {
                key: card.uid.clone(),
                target: DeviceTarget::card(&connection.device_key),
            },
        )
        .with_label(format!("Push to {}", connection.device_name))
        .with_summary("Push this project to the connected device.")
        .with_icon("upload")
    });

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
    let association = card
        .connected_device
        .is_none()
        .then(|| card.on_device.clone())
        .flatten();
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
                    // One "Put on <name>" per connected-empty device
                    // (model §1-A): the row IS the D11 consent, and
                    // pushing to a blank board destroys nothing.
                    for (device_key, device_name) in empty_devices.iter().filter(|_| !blocked).cloned() {
                        button {
                            class: menu_item_action_class(),
                            r#type: "button",
                            title: "Put this project on \"{device_name}\" — it's empty and ready.",
                            onclick: {
                                let key = card.uid.clone();
                                move |_| {
                                    on_action.call(UiAction::from_op(
                                        ControllerId::new(DEPLOY_NODE_ID),
                                        DeployOp::PushProject {
                                            key: key.clone(),
                                            target: DeviceTarget::card(&device_key),
                                        },
                                    ));
                                }
                            },
                            span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center", aria_hidden: "true",
                                StudioIcon { name: StudioIconName::Apply, size: 14 }
                            }
                            span { "Put on \"{device_name}\"" }
                        }
                    }
                    if !blocked {
                        if let Some(push) = push_to_device {
                            ActionButton {
                                action: push,
                                running: false,
                                variant: ActionButtonVariant::MenuItem,
                                on_action,
                            }
                        }
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

/// The glass bar's compact ⋯ trigger: a 20px quiet icon button (the
/// stock 32px icon-menu square overwhelms the slim footer). Resets UA
/// button chrome itself — Tailwind preflight is not loaded.
const CARD_MENU_TRIGGER_CLASS: &str = "tw:grid tw:h-5 tw:w-5 tw:flex-none tw:cursor-pointer tw:appearance-none tw:place-items-center tw:rounded tw:border-0 tw:bg-transparent tw:p-0 tw:text-muted-foreground tw:transition-colors tw:hover:bg-white/10 tw:hover:text-strong-foreground";

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

thread_local! {
    /// The project uid mid-drag (HTML5 dataTransfer is awkward through
    /// Dioxus; a same-page hand-off cell is all card→card drag needs).
    static DRAGGED_PROJECT: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub(crate) fn set_dragged_project(uid: String) {
    DRAGGED_PROJECT.with(|cell| *cell.borrow_mut() = Some(uid));
}

pub(crate) fn take_dragged_project() -> Option<String> {
    DRAGGED_PROJECT.with(|cell| cell.borrow_mut().take())
}

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
const LIVE_LINE_ATTENTION: &str = "tw:m-0 tw:truncate tw:text-xs tw:text-status-working-foreground";

/// The card's runtime-presence line (D28, full semantics):
///
/// - live device only → the D24 connected line (green only when current —
///   green = good; behind/diverged read as facts needing attention);
/// - sim only → "Running in simulator" (load-as-push always runs the
///   head, so the sim is current: green);
/// - BOTH → the aggregate "Live in 2 places" (the pool cap makes 2 the
///   max for now), amber whenever the device side needs attention, with
///   the tooltip spelling the places out.
fn live_presence_line(card: &UiPackageCard) -> Option<LivePresenceLine> {
    match (card.connected_device.as_ref(), card.running_in_sim) {
        (Some(connection), true) => Some(LivePresenceLine {
            text: "Live in 2 places".to_string(),
            class: match connection.relation {
                SyncRelation::AtHead => LIVE_LINE_GOOD,
                SyncRelation::Behind | SyncRelation::Diverged => LIVE_LINE_ATTENTION,
            },
            title: Some(format!(
                "{} · running in simulator",
                connected_line(&connection.device_name, connection.relation)
            )),
        }),
        (Some(connection), false) => Some(LivePresenceLine {
            text: connected_line(&connection.device_name, connection.relation),
            class: match connection.relation {
                SyncRelation::AtHead => LIVE_LINE_GOOD,
                SyncRelation::Behind | SyncRelation::Diverged => LIVE_LINE_ATTENTION,
            },
            title: None,
        }),
        (None, true) => Some(LivePresenceLine {
            text: "Running in simulator".to_string(),
            class: LIVE_LINE_GOOD,
            title: None,
        }),
        (None, false) => None,
    }
}

/// The D24 connected indication: green only when the device is current
/// (green = good); behind/diverged read as facts needing attention.
fn connected_line(device_name: &str, relation: SyncRelation) -> String {
    match relation {
        SyncRelation::AtHead => format!("On {device_name} — connected ✓"),
        SyncRelation::Behind => format!("On {device_name} — behind your copy"),
        SyncRelation::Diverged => format!("On {device_name} — edited elsewhere"),
    }
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
    use lpa_studio_core::UiCardConnection;

    use super::*;

    fn card(connected: Option<SyncRelation>, running_in_sim: bool) -> UiPackageCard {
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
            connected_device: connected.map(|relation| UiCardConnection {
                device_key: "runtime-1".to_string(),
                device_name: "Porch sign".to_string(),
                relation,
            }),
            running_in_sim,
            target: None,
            health: PackageHealth::Ready,
        }
    }

    #[test]
    fn a_blocked_card_offers_only_the_remedies_that_work_on_raw_files() {
        // The gallery's contract after P3: an unopenable project is still
        // here, still named, still exportable and deletable.
        let mut blocked = card(None, false);
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
        let mut upgradable = card(None, false);
        upgradable.health = PackageHealth::UpgradesOnOpen { found: 4 };
        assert!(upgradable.health.is_openable());
        assert_eq!(upgradable.health.blocked(), None);
        assert!(!package_card_class(false, false, false).contains("status-attention-border"));
    }

    #[test]
    fn both_runtimes_aggregate_to_live_in_2_places() {
        // D28 aggregate: one line, not two — the pool cap makes 2 the max
        let line = live_presence_line(&card(Some(SyncRelation::AtHead), true)).unwrap();
        assert_eq!(line.text, "Live in 2 places");
        assert_eq!(line.class, LIVE_LINE_GOOD, "both places current = good");
        assert_eq!(
            line.title.as_deref(),
            Some("On Porch sign — connected ✓ · running in simulator"),
            "the tooltip spells the places out"
        );
    }

    #[test]
    fn a_behind_device_turns_the_aggregate_amber() {
        let line = live_presence_line(&card(Some(SyncRelation::Behind), true)).unwrap();
        assert_eq!(line.text, "Live in 2 places");
        assert_eq!(
            line.class, LIVE_LINE_ATTENTION,
            "one place needing attention colors the aggregate"
        );
    }

    #[test]
    fn single_runtimes_keep_their_own_lines() {
        let device = live_presence_line(&card(Some(SyncRelation::Behind), false)).unwrap();
        assert_eq!(device.text, "On Porch sign — behind your copy");
        assert_eq!(device.class, LIVE_LINE_ATTENTION);
        assert_eq!(device.title, None);

        let sim = live_presence_line(&card(None, true)).unwrap();
        assert_eq!(sim.text, "Running in simulator");
        assert_eq!(sim.class, LIVE_LINE_GOOD);

        assert_eq!(live_presence_line(&card(None, false)), None);
    }
}
