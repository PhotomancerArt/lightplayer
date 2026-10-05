//! The device's access panel (spike `access-panel-tidy`, concept 4B).
//!
//! It opens from the card's Connections group ("Access · open ›") into a
//! detail card in the top layer, so the card keeps its height. Two
//! sections, as every detail card has: the access itself, then the keys.
//! Only a link that holds author sees it: USB (the trusted link) or a
//! Bluetooth link at author — the board answers the list at that tier only.
//!
//! Top: "Access", one sentence saying what it is, then Play and Author,
//! each Anyone or Password.
//! Password shows a box with a random password already in it, selected
//! when you click in, so typing replaces it; ↻ rolls another; it saves when
//! you click away (or press Enter). Anyone saves at once, no confirm. While
//! Author is Anyone, Play follows it.
//!
//! The second section is one line: "Your browsers & account · always get in ·
//! N of 16 · added by USB", which opens the keys, grouped
//! ([`super::access_key_group_row`]). "This isn't enterprise banking
//! software" (Yona): nothing else.

use dioxus::prelude::*;
use lpa_studio_core::{
    AccessCommand, AccessTier, DeviceAccessChange, DeviceId, UiAccessPanel, UiPasswordLine,
};

use super::access_fields::HELP_CLASS;
use super::access_key_group_row::{AccessKeyGroupRow, ICON_TILE_CLASS};
use super::share_words::fresh_share_words;
use crate::base::{DetailSection, StudioIcon, StudioIconName};

/// The panel body (the popover's content, and the stories' subject).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DeviceAccessPanel(
    panel: UiAccessPanel,
    on_access: EventHandler<AccessCommand>,
    /// Stories only: open the keys list.
    #[props(default)]
    keys_open_preview: bool,
    /// Stories only: the key group (by first salt) whose trash can starts
    /// armed.
    #[props(default)]
    armed_preview: Option<[u8; 16]>,
    /// Stories only: the words a fresh box starts with.
    #[props(default)]
    words_preview: Option<String>,
) -> Element {
    let device = panel.device;
    let busy = panel.writing || panel.ble_enabled.is_none();
    let mut keys_open = use_signal(|| keys_open_preview);
    let author_anyone = panel.author.is_anyone();
    rsx! {
        DetailSection { title: "Access".to_string(),
            div { class: "tw:grid tw:min-w-0 tw:gap-1.5 tw:pb-1",
                p { class: "tw:m-0 tw:mb-0.5 tw:text-xs tw:leading-snug tw:text-muted-foreground",
                    "Control who can play and author over Bluetooth and Wi‑Fi."
                }
                PasswordRow {
                    key: "play-{line_key(&panel.play)}",
                    device,
                    tier: AccessTier::Play,
                    line: panel.play.clone(),
                    locked: busy || author_anyone,
                    words: words_preview.clone(),
                    on_access,
                }
                PasswordRow {
                    key: "author-{line_key(&panel.author)}",
                    device,
                    tier: AccessTier::Edit,
                    line: panel.author.clone(),
                    locked: busy,
                    words: words_preview.clone(),
                    on_access,
                }
                if let Some(notice) = panel.notice.clone() {
                    p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-status-good-foreground", "{notice}" }
                }
                if panel.ble_enabled.is_none() {
                    p { class: HELP_CLASS, "Reading the device's list…" }
                }
                if panel.writing {
                    p { class: HELP_CLASS, "Writing to the device…" }
                }
                if let Some(error) = panel.error.clone() {
                    p { class: "tw:m-0 tw:text-xs tw:leading-relaxed tw:text-status-error-foreground", "{error}" }
                }
            }
        }
        DetailSection {
            button {
                class: "tw:flex tw:w-full tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-2.5 tw:border-0 tw:bg-transparent tw:p-0 tw:py-1 tw:text-left ux-focus-ring",
                r#type: "button",
                aria_expanded: "{keys_open()}",
                onclick: move |_| {
                    let was = keys_open();
                    keys_open.set(!was);
                },
                span { class: "{ICON_TILE_CLASS} tw:border-status-neutral-border tw:bg-status-neutral-bg tw:text-status-neutral-foreground",
                    StudioIcon { name: StudioIconName::AccessLaptop, size: 15 }
                }
                span { class: "tw:grid tw:min-w-0 tw:flex-1 tw:gap-px",
                    span { class: "tw:text-[13px] tw:font-bold tw:text-strong-foreground", "Your browsers & account" }
                    span { class: "tw:truncate tw:text-[11px] tw:text-dim-foreground",
                        "always get in · {panel.used} of {panel.capacity} · added by USB"
                    }
                }
                span { class: if keys_open() { "tw:inline-flex tw:rotate-90 tw:text-dim-foreground" } else { "tw:inline-flex tw:text-dim-foreground" },
                    StudioIcon { name: StudioIconName::Collapsed, size: 14 }
                }
            }
            if keys_open() {
                // Caps its own height (the `DetailPopover` convention — see
                // `pending_edit_section.rs`'s `PendingEditList`): an uncapped
                // list grows the panel past the viewport on a device with
                // many keys, and the popover's trigger-vs-panel geometry
                // only hides the collapsed "Access · open ›" row when the
                // panel fully covers it — a panel clamped back across the
                // trigger without covering it left that row painted over
                // the list's own rows
                // (ticket 2026-10-02-access-popover-overlap-and-indistinct-keys).
                ul { class: "tw:m-0 tw:grid tw:max-h-80 tw:list-none tw:overflow-y-auto tw:p-0 tw:sm:ml-[40px]",
                    for group in panel.keys.clone() {
                        AccessKeyGroupRow {
                            key: "{group.salts[0]:?}",
                            armed_preview: armed_preview == group.salts.first().copied(),
                            group,
                            device,
                            busy: panel.writing,
                            on_access,
                        }
                    }
                }
            }
        }
    }
}

/// One line, Play or Author: its name, Anyone | Password, and the box.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PasswordRow(
    device: DeviceId,
    tier: AccessTier,
    line: UiPasswordLine,
    /// Busy, or Play while Author is Anyone.
    locked: bool,
    words: Option<String>,
    on_access: EventHandler<AccessCommand>,
) -> Element {
    let saved = match &line {
        UiPasswordLine::Shown(password) => password.clone(),
        _ => String::new(),
    };
    let start = match &line {
        UiPasswordLine::Shown(password) => password.clone(),
        UiPasswordLine::SetElsewhere | UiPasswordLine::NotSet => String::new(),
        UiPasswordLine::Anyone | UiPasswordLine::FollowsAuthor => {
            words.clone().unwrap_or_else(fresh_share_words)
        }
    };
    let mut draft = use_signal(|| start);
    let mut just_focused = use_signal(|| false);
    let anyone = line.is_anyone();
    let name = match tier {
        AccessTier::Play => "Play",
        AccessTier::Edit => "Author",
    };
    let set = move |password: Option<String>| {
        on_access.call(AccessCommand::Change {
            device,
            change: DeviceAccessChange::SetPassword { tier, password },
        });
    };
    let saved_for_commit = saved.clone();
    let commit = move || {
        let typed = draft.read().clone();
        if !typed.trim().is_empty() && typed != saved_for_commit {
            set(Some(typed));
        }
    };
    let commit_on_blur = commit.clone();
    let commit_on_enter = commit.clone();
    let commit_on_save = commit;
    let dirty = !anyone && !draft.read().trim().is_empty() && *draft.read() != saved;
    let note = match (&line, draft.read().is_empty()) {
        (UiPasswordLine::SetElsewhere, true) => {
            Some("Set from another browser, so it can't be shown — type a new one to replace it.")
        }
        (UiPasswordLine::NotSet, true) => {
            Some("No password on the device yet — type one, or only your browsers get in.")
        }
        _ => None,
    };
    let placeholder = match line {
        UiPasswordLine::SetElsewhere => "type a new one",
        _ => "type a password",
    };
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2.5",
            span { class: "tw:w-[46px] tw:flex-none tw:text-[13px] tw:font-bold tw:text-strong-foreground", "{name}" }
            div { class: if locked { "{SEGMENTS_CLASS} tw:opacity-50" } else { "{SEGMENTS_CLASS}" },
                role: "group",
                aria_label: "Who nearby can {name.to_lowercase()}",
                button {
                    class: segment_class(anyone),
                    r#type: "button",
                    aria_pressed: "{anyone}",
                    disabled: locked,
                    onclick: move |_| if !anyone { set(None) },
                    "Anyone"
                }
                button {
                    class: segment_class(!anyone),
                    r#type: "button",
                    aria_pressed: "{!anyone}",
                    disabled: locked,
                    onclick: move |_| {
                        if anyone {
                            let mut typed = draft.read().clone();
                            if typed.trim().is_empty() {
                                typed = fresh_share_words();
                                draft.set(typed.clone());
                            }
                            set(Some(typed));
                        }
                    },
                    "Password"
                }
            }
            if anyone {
                span { class: "tw:min-w-0 tw:text-xs tw:leading-tight tw:text-dim-foreground",
                    if line == UiPasswordLine::FollowsAuthor { "follows Author" } else { "no password" }
                }
            }
        }
        if !anyone {
            div { class: "tw:ml-[56px] tw:flex tw:min-w-0 tw:items-center tw:gap-1",
                input {
                    class: "tw:min-w-0 tw:flex-1 tw:rounded tw:border tw:border-border-strong tw:bg-terminal tw:px-2 tw:py-1.5 tw:font-mono tw:text-[13px] tw:text-strong-foreground",
                    r#type: "text",
                    aria_label: "{name} password",
                    placeholder,
                    spellcheck: "false",
                    autocomplete: "off",
                    disabled: locked,
                    value: "{draft}",
                    oninput: move |event| draft.set(event.value()),
                    // Type to replace it: the whole password is selected
                    // when the box takes focus.
                    onfocus: move |_| {
                        select_focused_input();
                        just_focused.set(true);
                    },
                    // The focusing click's mouseup would drop the selection.
                    onmouseup: move |event| {
                        if just_focused() {
                            event.prevent_default();
                            just_focused.set(false);
                        }
                    },
                    onblur: move |_| commit_on_blur(),
                    onkeydown: move |event| {
                        if event.key() == Key::Enter {
                            commit_on_enter();
                        }
                    },
                }
                if dirty {
                    button {
                        class: "tw:flex-none tw:cursor-pointer tw:appearance-none tw:rounded tw:border-0 tw:bg-white/10 tw:px-2 tw:py-1.5 tw:text-xs tw:font-bold tw:text-strong-foreground ux-focus-ring",
                        r#type: "button",
                        disabled: locked,
                        onclick: move |_| commit_on_save(),
                        "Save"
                    }
                }
                button {
                    class: "tw:inline-flex tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:rounded tw:border-0 tw:bg-transparent tw:p-1.5 tw:text-muted-foreground tw:hover:text-strong-foreground ux-focus-ring",
                    r#type: "button",
                    title: "Another random one",
                    aria_label: "Another random password",
                    disabled: locked,
                    onclick: move |_| draft.set(fresh_share_words()),
                    StudioIcon { name: StudioIconName::AccessRegenerate, size: 14 }
                }
            }
            if let Some(note) = note {
                p { class: "tw:m-0 tw:ml-[56px] tw:text-[11.5px] tw:leading-snug tw:text-subtle-foreground", "{note}" }
            }
        }
    }
}

/// A key that changes when the device's answer does, so a row's box
/// starts over from what the device now holds.
fn line_key(line: &UiPasswordLine) -> String {
    match line {
        UiPasswordLine::Anyone => "anyone".to_string(),
        UiPasswordLine::FollowsAuthor => "follows".to_string(),
        UiPasswordLine::SetElsewhere => "elsewhere".to_string(),
        UiPasswordLine::NotSet => "unset".to_string(),
        // The length and a checksum, not the password.
        UiPasswordLine::Shown(password) => format!(
            "shown-{}-{}",
            password.len(),
            password.bytes().fold(0u32, |sum, b| sum
                .wrapping_mul(31)
                .wrapping_add(u32::from(b)))
        ),
    }
}

const SEGMENTS_CLASS: &str =
    "tw:flex tw:flex-none tw:overflow-hidden tw:rounded tw:border tw:border-border-strong";

fn segment_class(pressed: bool) -> &'static str {
    if pressed {
        "tw:cursor-pointer tw:appearance-none tw:border-0 tw:bg-white/10 tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:text-strong-foreground tw:disabled:cursor-not-allowed ux-focus-ring"
    } else {
        "tw:cursor-pointer tw:appearance-none tw:border-0 tw:bg-transparent tw:px-2 tw:py-1 tw:text-xs tw:font-semibold tw:text-muted-foreground tw:hover:text-strong-foreground tw:disabled:cursor-not-allowed ux-focus-ring"
    }
}

/// Select the focused input's whole text.
fn select_focused_input() {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        if let Some(input) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.active_element())
            .and_then(|element| element.dyn_into::<web_sys::HtmlInputElement>().ok())
        {
            input.select();
        }
    }
}
