//! The Share pill and the panel it opens (spike `project-share` §1-A + §2-B,
//! gate rulings G1/G2 — visual reference only, never imported).
//!
//! Link-first, top to bottom:
//!
//! 1. **The URL**, because the address bar is the product (D1/D13). It is
//!    the hero, not a footer button: copying the link is the share, and
//!    everything below is only *administration* of what that link grants.
//! 2. **General access** as a three-way segment — `Restricted` /
//!    `Anyone can view` / `Anyone can edit` — with one line saying what the
//!    chosen level means in plain words. `Anyone can edit` wears the warn
//!    palette, pressed segment included: the uid IS the capability, so
//!    handing out the link is handing out write access and the control
//!    should look like it.
//! 3. **People**, orthogonal to the segment (D4): members always read and
//!    write, and the segment says what everybody *else* can do. The
//!    add-person affordance sits at the list's BOTTOM (house rule:
//!    add-buttons at the insertion point).
//!
//! No footer: the hero owns Copy, and closing is the × or a click outside.
//!
//! # Shape
//!
//! Everything here is **pure** — props in, events out — so the stories
//! mount the three access states and the awkward people set with no cloud
//! service, no session and no context. `project_share_control` is the live
//! half that fills these props and answers the events.
//!
//! # Tailwind traps (crate README)
//!
//! This build ships Tailwind with **no preflight**, so every `<button>`
//! here carries an explicit `tw:bg-*`/`tw:border-*` — omitting one paints
//! the UA's `buttonface`, not "no background". And `style.css` resets
//! `button, input { font: inherit }` *unlayered*, which beats any layered
//! `tw:text-*`/`tw:font-*` utility placed on the control itself: every
//! label's type therefore lives on an inner `<span>`.

use dioxus::prelude::*;
use dioxus_icons::lucide::{Link2, Plus, UserRound, X};
use lpc_cloud_api::{Access, MemberRole};

use crate::app::share::share_person::SharePerson;
use crate::app::share::share_url::ShareUrl;
use crate::base::{PopoverButton, PopoverCloseHandle, PopoverPlacement};

/// The chrome's Share pill and its anchored panel.
///
/// Quiet by design (G1 ruling): the neutral chip family, with the accent
/// living only in the person glyph — the bar already carries the URL, so
/// this is a door, not an advertisement. Opening tints the border with the
/// accent.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SharePillPopover(
    /// What the panel's title calls this project.
    name: String,
    /// The canonical link, in the pieces the hero paints.
    url: ShareUrl,
    /// What holding the link grants right now.
    access: Access,
    /// The member rows (empty is legal — a project nobody has been added
    /// to still shows its owner once the service answers).
    #[props(default)]
    people: Vec<SharePerson>,
    /// A `SetAccess` is in flight; the segment stays interactive (the
    /// update is optimistic) but says so.
    #[props(default = false)]
    busy: bool,
    #[props(default)] on_access: Option<EventHandler<Access>>,
    #[props(default)] on_copy: Option<EventHandler<()>>,
    #[props(default)] on_add: Option<EventHandler<String>>,
    #[props(default)] on_remove: Option<EventHandler<String>>,
    /// Stories only: mount the panel open (capture cannot click). Also how
    /// the ⋯ menu's "Sharing & access…" row opens it — see
    /// `project_share_control`.
    #[props(default = false)]
    initially_open: bool,
) -> Element {
    rsx! {
        PopoverButton {
            class: SHARE_PILL_CLASS.to_string(),
            open_class: SHARE_PILL_OPEN_CLASS.to_string(),
            trigger: rsx! {
                span { class: "tw:flex tw:flex-none tw:text-accent",
                    UserRound { size: 13 }
                }
                // The word carries its own type: `style.css` resets
                // `button { font: inherit }` unlayered, which beats any
                // font utility on the button itself.
                span { class: "tw:text-[11.5px] tw:font-bold", "Share" }
            },
            label: "Share".to_string(),
            title: format!("Sharing and access for \"{name}\""),
            popup_class: SHARE_POPUP_CLASS.to_string(),
            // The bordered-chip family, like the avatar trigger it sits
            // beside — this pill has a border at rest, so the neutral
            // chrome's raised fill reads as the same control, opened.
            chrome_class: "ux-popover-chrome-neutral".to_string(),
            placement: PopoverPlacement::BottomEnd,
            // Glyph PLUS label: the top-layer copy must keep the trigger's
            // own box or the bar would shift on open.
            layer_keeps_layout: true,
            initially_open,
            SharePanel {
                name,
                url,
                access,
                people,
                busy,
                on_access,
                on_copy,
                on_add,
                on_remove,
            }
        }
    }
}

/// The panel's body. Pure — stories mount it without a popover.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SharePanel(
    name: String,
    url: ShareUrl,
    access: Access,
    #[props(default)] people: Vec<SharePerson>,
    #[props(default = false)] busy: bool,
    #[props(default)] on_access: Option<EventHandler<Access>>,
    #[props(default)] on_copy: Option<EventHandler<()>>,
    #[props(default)] on_add: Option<EventHandler<String>>,
    #[props(default)] on_remove: Option<EventHandler<String>>,
    /// Stories only: render the add row already unfolded into its input.
    #[props(default = false)]
    adding: bool,
) -> Element {
    let close = try_consume_context::<PopoverCloseHandle>();
    rsx! {
        // One explicit grid wrapper: the popover primitive nests children
        // in its own content div, so the panel class never reaches them.
        div { class: "tw:grid tw:min-w-0 tw:gap-2.5 tw:p-3.5",
            div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2",
                strong { class: "tw:min-w-0 tw:truncate tw:text-[12.5px] tw:font-bold tw:text-strong-foreground",
                    "Share \"{name}\""
                }
                if let Some(mut close) = close {
                    button {
                        class: PANEL_CLOSE_CLASS,
                        r#type: "button",
                        aria_label: "Close",
                        onclick: move |_| close.close(),
                        X { size: 13 }
                    }
                }
            }
            ShareUrlHero { url: url.clone(), on_copy }
            p { class: "tw:m-0 tw:px-0.5 tw:text-[10px] tw:leading-snug tw:text-dim-foreground",
                "Same link as the address bar — copying either works."
            }
            AccessSegment { access, busy, on_access }
            AccessDescription { access }
            span { class: GROUP_HEADER_CLASS, "People" }
            PeopleList { people, on_remove }
            AddPersonRow { on_add, adding }
        }
    }
}

/// The hero: the whole link on one line, and the one button that matters.
/// Shared with the visitor popover (P6) — same link, same powers, same box.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ShareUrlHero(url: ShareUrl, on_copy: Option<EventHandler<()>>) -> Element {
    let absolute = url.absolute();
    rsx! {
        div { class: URL_HERO_CLASS,
            // One mono line, truncated from the right: the slug is what a
            // human recognizes, so it must survive a narrow panel.
            span {
                class: "tw:min-w-0 tw:flex-1 tw:truncate tw:font-mono tw:text-[11px] tw:font-semibold tw:text-subtle-foreground",
                title: "{absolute}",
                span { class: "tw:text-dim-foreground", "{url.origin}" }
                span { class: "tw:text-dim-foreground", "/p/" }
                span { class: "tw:text-heading", "{url.slug}" }
                span { class: "tw:text-dim-foreground", "{url.uid_segment()}" }
            }
            button {
                class: COPY_BUTTON_CLASS,
                r#type: "button",
                title: "Copy this link",
                onclick: move |_| {
                    if let Some(on_copy) = on_copy {
                        on_copy.call(());
                    }
                },
                Link2 { size: 12 }
                span { class: "tw:text-[11px] tw:font-bold", "Copy" }
            }
        }
    }
}

/// The three-way general-access control (D4: `none | view | edit`).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AccessSegment(access: Access, busy: bool, on_access: Option<EventHandler<Access>>) -> Element {
    rsx! {
        div { class: SEGMENT_CLASS, role: "group", aria_label: "General access",
            for level in [Access::None, Access::View, Access::Edit] {
                button {
                    key: "{segment_label(level)}",
                    class: segment_button_class(level, level == access),
                    r#type: "button",
                    aria_pressed: if level == access { "true" } else { "false" },
                    disabled: busy && level == access,
                    onclick: move |_| {
                        if let Some(on_access) = on_access
                            && level != access
                        {
                            on_access.call(level);
                        }
                    },
                    span { class: "tw:text-[11px] tw:font-semibold tw:leading-tight",
                        "{segment_label(level)}"
                    }
                }
            }
        }
    }
}

/// The line under the segment: what the chosen level actually means, in
/// the spike's words (gate-approved — do not paraphrase).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AccessDescription(access: Access) -> Element {
    let tone = if access == Access::Edit {
        "tw:text-status-warning-foreground"
    } else {
        "tw:text-dim-foreground"
    };
    let strong = if access == Access::Edit {
        "tw:font-semibold"
    } else {
        "tw:font-semibold tw:text-muted-foreground"
    };
    rsx! {
        p { class: "tw:m-0 tw:min-h-[28px] tw:px-0.5 tw:text-[10.5px] tw:leading-snug {tone}",
            match access {
                Access::None => rsx! {
                    span { class: "{strong}", "Only people added below" }
                    " can open the link. Everyone else sees nothing — not even that it exists."
                },
                Access::View => rsx! {
                    "Opens "
                    span { class: "{strong}", "running" }
                    " for anyone with the link — no account needed. Only the people below can save changes."
                },
                Access::Edit => rsx! {
                    "Anyone holding the link can "
                    span { class: "{strong}", "edit and save" }
                    ". The link is the only key — share it accordingly."
                },
            }
        }
    }
}

/// The member rows. An empty list is a real state (the service has not
/// answered yet, or the roster is genuinely only you and the row is still
/// loading) and renders as one quiet line rather than a headed void.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PeopleList(people: Vec<SharePerson>, on_remove: Option<EventHandler<String>>) -> Element {
    if people.is_empty() {
        return rsx! {
            p { class: "tw:m-0 tw:px-0.5 tw:text-[10.5px] tw:leading-snug tw:text-dim-foreground",
                "Nobody else has been added yet."
            }
        };
    }
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
            for person in people.iter() {
                PersonRow { key: "{person.email}", person: person.clone(), on_remove }
            }
        }
    }
}

/// One person: face, who they are, and what they may do about it.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PersonRow(person: SharePerson, on_remove: Option<EventHandler<String>>) -> Element {
    let email = person.email.clone();
    let owner = person.role == MemberRole::Owner;
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2.5 tw:px-0.5 tw:py-1",
            span {
                class: "tw:inline-flex tw:h-7 tw:w-7 tw:flex-none tw:select-none tw:items-center tw:justify-center tw:overflow-hidden tw:rounded-full tw:border tw:font-bold",
                style: "{initials_style(person.hue())}",
                aria_hidden: "true",
                "{person.initials()}"
            }
            span { class: "tw:grid tw:min-w-0 tw:flex-1 tw:gap-px",
                span { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5",
                    span { class: "tw:min-w-0 tw:truncate tw:text-xs tw:font-semibold tw:text-foreground",
                        "{person.headline()}"
                    }
                    if person.you {
                        span { class: "tw:flex-none tw:text-xs tw:text-dim-foreground", "(you)" }
                    }
                    if person.pending {
                        span { class: INVITED_BADGE_CLASS, "invited" }
                    }
                }
                if let Some(secondary) = person.secondary() {
                    span { class: "tw:min-w-0 tw:truncate tw:text-[10.5px] tw:text-dim-foreground",
                        "{secondary}"
                    }
                }
            }
            if owner {
                // The owner is fixed: a control that refuses every click is
                // worse than a label that never pretended to be one.
                span { class: "tw:flex-none tw:px-1.5 tw:text-[11px] tw:font-semibold tw:text-dim-foreground",
                    "Owner"
                }
            } else {
                span { class: "tw:flex-none tw:px-1 tw:text-[11px] tw:font-semibold tw:text-subtle-foreground",
                    "Editor"
                }
                button {
                    class: ROW_ACTION_CLASS,
                    r#type: "button",
                    title: "Remove {person.email} from this project",
                    onclick: move |_| {
                        if let Some(on_remove) = on_remove {
                            on_remove.call(email.clone());
                        }
                    },
                    span { class: "tw:text-[11px] tw:font-semibold", "Remove" }
                }
            }
        }
    }
}

/// The add affordance, at the list's BOTTOM: a dashed row that unfolds
/// into one email box. Membership is keyed by email, so an address that has
/// never signed in is a legal answer — it lands as a pending invitation the
/// service resolves at that person's first login.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AddPersonRow(on_add: Option<EventHandler<String>>, adding: bool) -> Element {
    let mut open = use_signal(|| adding);
    let mut draft = use_signal(String::new);
    if !open() {
        return rsx! {
            button {
                class: ADD_ROW_CLASS,
                r#type: "button",
                onclick: move |_| open.set(true),
                Plus { size: 13 }
                span { class: "tw:text-[11.5px] tw:font-semibold", "Add people by email…" }
            }
        };
    }
    let mut submit = move || {
        let email = draft.peek().trim().to_string();
        if email.is_empty() {
            open.set(false);
            return;
        }
        if let Some(on_add) = on_add {
            on_add.call(email);
        }
        draft.set(String::new());
        open.set(false);
    };
    rsx! {
        form {
            class: ADD_INPUT_CLASS,
            onsubmit: move |event| {
                event.prevent_default();
                submit();
            },
            input {
                class: "tw:min-w-0 tw:flex-1 tw:border-0 tw:bg-transparent tw:p-0 tw:outline-none",
                // The box's own type has to beat `style.css`'s unlayered
                // `input { font: inherit }`, which a utility class cannot —
                // so it is inline.
                style: "font: 600 12px/1.3 var(--studio-font-sans); color: var(--studio-color-text);",
                r#type: "email",
                autofocus: true,
                placeholder: "name@example.com",
                value: "{draft}",
                oninput: move |event| draft.set(event.value()),
                onkeydown: move |event| {
                    if event.key() == Key::Escape {
                        draft.set(String::new());
                        open.set(false);
                    }
                },
            }
            button {
                class: ADD_SUBMIT_CLASS,
                r#type: "submit",
                span { class: "tw:text-[11px] tw:font-bold", "Add" }
            }
        }
    }
}

/// The segment's labels — the spike's words, in `Access` order.
fn segment_label(access: Access) -> &'static str {
    match access {
        Access::None => "Restricted",
        Access::View => "Anyone can view",
        Access::Edit => "Anyone can edit",
    }
}

/// One segment button's class. `Anyone can edit` is warn-toned when
/// pressed (post-gate refinement): the pressed state is the one that says
/// "the link is write access right now", and it must not read like a
/// friendly accent confirmation.
fn segment_button_class(access: Access, pressed: bool) -> String {
    let state = match (access, pressed) {
        (Access::Edit, true) => {
            "tw:bg-status-warning-bg tw:text-status-warning-foreground tw:hover:text-status-warning-foreground"
        }
        (_, true) => "tw:bg-accent-wash tw:text-accent tw:hover:text-accent",
        (Access::Edit, false) => {
            "tw:bg-transparent tw:text-subtle-foreground tw:hover:bg-background-wash tw:hover:text-status-warning-foreground"
        }
        (_, false) => {
            "tw:bg-transparent tw:text-subtle-foreground tw:hover:bg-background-wash tw:hover:text-foreground"
        }
    };
    format!("{SEGMENT_BUTTON_BASE} {state}")
}

/// Inline paint for a people-row avatar: the hue washed into the surface.
/// Inline because the value is data-driven — Tailwind only ships classes it
/// can see in the source.
fn initials_style(hue: u16) -> String {
    format!(
        "font-size: 10px; \
         border-color: color-mix(in srgb, hsl({hue} 60% 62%) 55%, transparent); \
         background: color-mix(in srgb, hsl({hue} 60% 62%) 22%, var(--studio-color-surface-subtle)); \
         color: color-mix(in srgb, hsl({hue} 60% 62%) 88%, white 6%);"
    )
}

/// The pill at rest: the neutral chip family, accent only in the glyph.
const SHARE_PILL_CLASS: &str = "tw:inline-flex tw:flex-none tw:cursor-pointer tw:items-center tw:gap-1.5 tw:rounded-pill tw:border tw:border-status-neutral-border tw:bg-status-neutral-bg tw:px-3 tw:py-1.5 tw:text-status-neutral-foreground tw:transition-colors tw:hover:border-accent-border tw:hover:text-strong-foreground";
/// The pill while open: the accent border tint the spike's `.sharebtn.open`
/// wears. Same box, so opening cannot nudge the bar.
const SHARE_PILL_OPEN_CLASS: &str = "tw:inline-flex tw:flex-none tw:cursor-pointer tw:items-center tw:gap-1.5 tw:rounded-pill tw:border tw:border-accent-border tw:bg-accent-wash tw:px-3 tw:py-1.5 tw:text-strong-foreground";
/// The panel, at the spike's 348px. Plain `w-[…]`, the shipped ⋯ menu's
/// idiom: `.ux-popover-panel` already caps every panel at
/// `calc(100vw - 24px)`.
const SHARE_POPUP_CLASS: &str = "tw:grid tw:w-[348px] tw:min-w-0 tw:rounded-md tw:border tw:border-border-strong tw:bg-card-subtle tw:text-sm tw:text-muted-foreground tw:shadow-lg";
/// The header's ×.
const PANEL_CLOSE_CLASS: &str = "tw:ml-auto tw:inline-flex tw:h-5 tw:w-5 tw:flex-none tw:cursor-pointer tw:items-center tw:justify-center tw:rounded-sm tw:border tw:border-transparent tw:bg-transparent tw:p-0 tw:text-dim-foreground tw:transition-colors tw:hover:bg-card-raised tw:hover:text-strong-foreground";
/// The URL hero's box: the terminal surface, because it holds an address.
const URL_HERO_CLASS: &str = "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:rounded-sm tw:border tw:border-border tw:bg-terminal tw:px-2.5 tw:py-2";
/// The one filled button in the panel — the link IS the share.
const COPY_BUTTON_CLASS: &str = "tw:inline-flex tw:flex-none tw:cursor-pointer tw:items-center tw:gap-1.5 tw:rounded-sm tw:border tw:border-accent-border tw:bg-accent tw:px-2.5 tw:py-1.5 tw:text-accent-foreground tw:transition-colors tw:hover:bg-accent-hover";
/// The three-way segment's frame.
const SEGMENT_CLASS: &str =
    "tw:flex tw:min-w-0 tw:overflow-hidden tw:rounded-sm tw:border tw:border-border-strong";
/// One segment button, before its state tone.
const SEGMENT_BUTTON_BASE: &str = "tw:min-w-0 tw:flex-1 tw:cursor-pointer tw:border-0 tw:border-r tw:border-border-muted tw:px-1.5 tw:py-2 tw:transition-colors tw:last:border-r-0";
/// The uppercase mini-header over a group.
const GROUP_HEADER_CLASS: &str =
    "tw:px-0.5 tw:pt-0.5 tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground";
/// The pending-invitation badge: warn-toned, because it is a promise the
/// service has not been able to keep yet.
const INVITED_BADGE_CLASS: &str = "tw:inline-flex tw:flex-none tw:rounded-pill tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:px-1.5 tw:py-px tw:font-mono tw:text-[8.5px] tw:font-bold tw:uppercase tw:tracking-wide tw:text-status-warning-foreground";
/// A quiet per-row verb (Remove).
const ROW_ACTION_CLASS: &str = "tw:flex-none tw:cursor-pointer tw:rounded-sm tw:border tw:border-transparent tw:bg-transparent tw:px-1.5 tw:py-1 tw:text-subtle-foreground tw:transition-colors tw:hover:bg-card-raised tw:hover:text-strong-foreground";
/// The dashed add row, at the list's bottom.
const ADD_ROW_CLASS: &str = "tw:flex tw:w-full tw:min-w-0 tw:cursor-pointer tw:items-center tw:gap-2 tw:rounded-sm tw:border tw:border-dashed tw:border-border-strong tw:bg-transparent tw:px-2.5 tw:py-2 tw:text-left tw:text-subtle-foreground tw:transition-colors tw:hover:border-dim-foreground tw:hover:text-foreground";
/// The same row, unfolded into its email box.
const ADD_INPUT_CLASS: &str = "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:rounded-sm tw:border tw:border-border tw:bg-card-muted tw:px-2.5 tw:py-1.5";
/// The box's submit.
const ADD_SUBMIT_CLASS: &str = "tw:flex-none tw:cursor-pointer tw:rounded-sm tw:border tw:border-accent-border tw:bg-transparent tw:px-2 tw:py-1 tw:text-accent tw:transition-colors tw:hover:bg-accent-wash";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_segment_says_the_spikes_words() {
        assert_eq!(segment_label(Access::None), "Restricted");
        assert_eq!(segment_label(Access::View), "Anyone can view");
        assert_eq!(segment_label(Access::Edit), "Anyone can edit");
    }

    /// The pressed `Anyone can edit` segment must be WARN, never the accent
    /// every other pressed segment wears (post-gate refinement).
    #[test]
    fn pressed_edit_is_warn_and_pressed_view_is_accent() {
        let edit = segment_button_class(Access::Edit, true);
        assert!(edit.contains("tw:bg-status-warning-bg"));
        assert!(!edit.contains("tw:text-accent"));

        let view = segment_button_class(Access::View, true);
        assert!(view.contains("tw:bg-accent-wash"));
        assert!(view.contains("tw:text-accent"));
    }

    /// No preflight: every one of these buttons must name its own
    /// background, or the browser paints `buttonface` (crate README).
    #[test]
    fn every_button_class_names_a_background() {
        for class in [
            SHARE_PILL_CLASS,
            SHARE_PILL_OPEN_CLASS,
            PANEL_CLOSE_CLASS,
            COPY_BUTTON_CLASS,
            ROW_ACTION_CLASS,
            ADD_ROW_CLASS,
            ADD_SUBMIT_CLASS,
        ] {
            assert!(class.contains("tw:bg-"), "no background in `{class}`");
        }
        for access in [Access::None, Access::View, Access::Edit] {
            for pressed in [false, true] {
                let class = segment_button_class(access, pressed);
                assert!(class.contains("tw:bg-"), "no background in `{class}`");
            }
        }
    }
}
