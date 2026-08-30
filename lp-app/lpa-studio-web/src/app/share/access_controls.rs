//! The sharing controls themselves — the URL hero, the general-access
//! segment and its description line, the people list and its add row
//! (spike `project-share` §1-A + §2-B, gate rulings G1/G2 — visual
//! reference only, never imported).
//!
//! Link-first, in the order the relationship panel's Where and Access
//! sections mount them:
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
//! # One home, since the pill retired
//!
//! These were the body of the chrome's standalone Share pill
//! (`SharePillPopover` / `SharePanel`). The pill and its panel retired with
//! relationship-control P5 — the project segment's popover is the one door
//! now — and the controls moved wholesale into that popover's Where and
//! Access sections ([`super::project_relationship_panel`], vision D9),
//! same controls, same gate-approved strings. Nothing here paints a
//! popover or knows a trigger; they are `pub(crate)` pieces the panel
//! composes.
//!
//! # Shape
//!
//! Everything here is **pure** — props in, events out — so the stories
//! mount the three access states and the awkward people set with no cloud
//! service, no session and no context. [`super::project_roster`] is the
//! live half that fills these props and answers the events.
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
use dioxus_icons::lucide::{Link2, Plus};
use lpc_cloud_api::{Access, MemberRole};

use crate::app::share::share_person::SharePerson;
use crate::app::share::share_url::ShareUrl;
use crate::base::{InlineButtonTone, inline_text_button_class};
use crate::core::outline_action_class;

/// The hero: the whole link on one line, and the one button that matters.
/// The relationship panel's Where section mounts it — the link is *where
/// this document lives*, not a footer button.
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
pub(crate) fn AccessSegment(
    access: Access,
    busy: bool,
    on_access: Option<EventHandler<Access>>,
) -> Element {
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
pub(crate) fn AccessDescription(access: Access) -> Element {
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
///
/// `on_remove: None` makes the list **read-only** — the relationship
/// panel's Member state shows you who else is on a project you do not
/// administer, and a Remove button there would refuse every click.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn PeopleList(
    people: Vec<SharePerson>,
    on_remove: Option<EventHandler<String>>,
) -> Element {
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
                // Same rule as the owner row above: no `on_remove`, no
                // button. A read-only roster states who is here; it does
                // not offer a verb it cannot carry out.
                if let Some(on_remove) = on_remove {
                    button {
                        class: inline_text_button_class(InlineButtonTone::Neutral, false),
                        r#type: "button",
                        title: "Remove {person.email} from this project",
                        onclick: move |_| on_remove.call(email.clone()),
                        "Remove"
                    }
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
pub(crate) fn AddPersonRow(on_add: Option<EventHandler<String>>, adding: bool) -> Element {
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
                class: outline_action_class(false),
                r#type: "submit",
                "Add"
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

/// The URL hero's box: the terminal surface, because it holds an address.
const URL_HERO_CLASS: &str = "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:rounded-sm tw:border tw:border-border tw:bg-terminal tw:px-2.5 tw:py-2";
/// The one filled button in the panel — the link IS the share.
const COPY_BUTTON_CLASS: &str = "tw:inline-flex tw:flex-none tw:cursor-pointer tw:items-center tw:gap-1.5 tw:rounded-sm tw:border tw:border-accent-border tw:bg-accent tw:px-2.5 tw:py-1.5 tw:text-accent-foreground tw:transition-colors tw:hover:bg-accent-hover";
/// The three-way segment's frame.
const SEGMENT_CLASS: &str =
    "tw:flex tw:min-w-0 tw:overflow-hidden tw:rounded-sm tw:border tw:border-border-strong";
/// One segment button, before its state tone.
const SEGMENT_BUTTON_BASE: &str = "tw:min-w-0 tw:flex-1 tw:cursor-pointer tw:border-0 tw:border-r tw:border-border-muted tw:px-1.5 tw:py-2 tw:transition-colors tw:last:border-r-0";
/// The pending-invitation badge: warn-toned, because it is a promise the
/// service has not been able to keep yet.
const INVITED_BADGE_CLASS: &str = "tw:inline-flex tw:flex-none tw:rounded-pill tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:px-1.5 tw:py-px tw:font-mono tw:text-[8.5px] tw:font-bold tw:uppercase tw:tracking-wide tw:text-status-warning-foreground";
/// The dashed add row, at the list's bottom — the idiom `device_card`'s
/// entry cards mirror (P4: kept bespoke on purpose; it is the pattern
/// source, not a one-off).
const ADD_ROW_CLASS: &str = "tw:flex tw:w-full tw:min-w-0 tw:cursor-pointer tw:items-center tw:gap-2 tw:rounded-sm tw:border tw:border-dashed tw:border-border-strong tw:bg-transparent tw:px-2.5 tw:py-2 tw:text-left tw:text-subtle-foreground tw:transition-colors tw:hover:border-dim-foreground tw:hover:text-foreground";
/// The same row, unfolded into its email box.
const ADD_INPUT_CLASS: &str = "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:rounded-sm tw:border tw:border-border tw:bg-card-muted tw:px-2.5 tw:py-1.5";

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
        for class in [COPY_BUTTON_CLASS, ADD_ROW_CLASS] {
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
