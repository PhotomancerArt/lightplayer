//! The chrome's account surface: one quiet word when signed out, one face
//! when signed in.
//!
//! Rendered from [`CloudSession`] and nothing else (spike `cloud-login`
//! §1A, §2A-grown-into-C, §4 — visual reference only, never imported).
//! Four states, in the one slot at the end of the right-hand cluster:
//!
//! - **Pending** — a shimmer pill the size of the word. The boot ruling is
//!   that "Sign in" must never pop into an avatar a beat later, so the slot
//!   holds its shape while `whoami` is in flight.
//! - **Anonymous** — "Sign in" in the secondary nav family's treatment.
//!   With exactly one way in it is a plain link straight to the provider;
//!   with more (a dev picker, a second connection) the same word opens the
//!   §4 chooser. Which of those it is comes from the server's
//!   [`LoginOptionsInfo`], never from a hard-coded "Google".
//! - **SignedIn** — a 28px avatar opening the identity dropdown: who you
//!   are, Profile, the accounts this browser remembers, sign out.
//! - **Unreachable** (or no context at all — stories) — nothing. A share
//!   link viewer on a machine that cannot reach the service came here to
//!   watch lights, not to be told about accounts.
//!
//! # Why sign-in links leave the app
//!
//! Every sign-in, switch, and add-account target is a *full page
//! navigation* through the provider (spike §5 lean ruling: switching is a
//! re-auth, and the provider's own picker does the work). The router
//! intercepts same-origin anchor clicks to keep the runtime pool alive
//! (`router::in_app_link_url`), so these links cancel that interception
//! explicitly and set `location.href` — see [`AuthLink`].

use dioxus::prelude::*;
use dioxus_icons::lucide::{LogOut, UserRound, UserRoundPlus};
use lpc_cloud_api::{DevChoice, LoginOptionsInfo, MeInfo};

use crate::app::layout::site_chrome::{
    GROUP_HEADER_CLASS, NAV_MENU_ITEM_IDLE, NAV_TAB_SECONDARY_IDLE,
};
use crate::base::{PopoverButton, PopoverCloseHandle, PopoverPlacement};
use crate::cloud::account_memory::{self, RememberedAccount};
use crate::cloud::{CloudSession, CloudSessionRefresh};

/// The chrome's account slot. Inert (renders nothing) without the
/// [`CloudSession`] context — the house rule stories rely on.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn CloudAccountControl(
    /// True on the /account page: the slot wears the tabs' you're-here
    /// underline. /account has no nav tab, so the slot that opens it
    /// marks the place, the way the logo does at Home (G1 ruling
    /// 2026-08-07).
    #[props(default = false)]
    on_account: bool,
) -> Element {
    let Some(session) = try_consume_context::<Signal<CloudSession>>() else {
        return rsx! {};
    };
    let refresh = try_consume_context::<CloudSessionRefresh>();
    // The switch group's rows, read from localStorage once per session
    // change rather than per render — this component re-renders with the
    // whole chrome, and the remembered list only moves when the session
    // does (the provider writes it on entering `SignedIn`).
    let remembered = use_memo(move || match session() {
        CloudSession::SignedIn { me, .. } if !me.anonymous => {
            other_accounts(&account_memory::load(), &me.email)
        }
        _ => Vec::new(),
    });
    // Recomputed per render: the chrome re-renders on every route change,
    // so `next` always names the page the user is looking at.
    let next = current_path();
    let inner = match session() {
        CloudSession::Pending => Some(rsx! {
            PendingPill {}
        }),
        // A GUEST session (examples vision D8) is real to the sync engine
        // but must not read as an account: the chrome keeps the signed-out
        // affordance — signing in is still the door this slot offers.
        CloudSession::SignedIn { me, options } if me.anonymous => {
            options.and_then(|options| match sign_in_affordance(&options, &next) {
                SignInAffordance::Direct(href) => Some(rsx! {
                    SignInLink { href }
                }),
                SignInAffordance::Chooser => Some(rsx! {
                    SignInMenu { options, next }
                }),
                SignInAffordance::Nothing => None,
            })
        }
        CloudSession::Anonymous { options } => {
            // Options that never landed: an affordance pointing nowhere
            // is worse than none (P4's `Anonymous { options: None }`).
            options.and_then(|options| match sign_in_affordance(&options, &next) {
                SignInAffordance::Direct(href) => Some(rsx! {
                    SignInLink { href }
                }),
                SignInAffordance::Chooser => Some(rsx! {
                    SignInMenu { options, next }
                }),
                SignInAffordance::Nothing => None,
            })
        }
        CloudSession::SignedIn { me, options } => Some(rsx! {
            AccountDropdown {
                me,
                accounts: remembered(),
                options,
                next,
                on_sign_out: refresh.map(|refresh| EventHandler::new(move |()| sign_out(refresh))),
            }
        }),
        CloudSession::Unreachable => None,
    };
    // No wrapper when the slot renders nothing: an empty span would still
    // claim a gap in the chrome's flex row.
    match inner {
        None => rsx! {},
        Some(inner) => {
            let class = if on_account {
                ACCOUNT_HERE_WRAP
            } else {
                "tw:flex tw:flex-none"
            };
            rsx! {
                span { class: "{class}", {inner} }
            }
        }
    }
}

/// The account slot's you're-here underline on /account: the tabs'
/// underline bar under whatever the slot renders, landing on the header's
/// border line like `LOGO_HOME_ACTIVE_WRAP` does for the logo at Home.
/// The offset suits the slot's 28px controls. `pub(crate)` for the
/// story that shows it against the header border.
pub(crate) const ACCOUNT_HERE_WRAP: &str = "tw:relative tw:flex tw:flex-none tw:after:absolute tw:after:inset-x-0 tw:after:-bottom-[12px] tw:after:h-0.5 tw:after:rounded-full tw:after:bg-[linear-gradient(90deg,var(--studio-spectrum))] tw:after:content-['']";

/// The boot shimmer: the slot at its signed-out size, holding its shape
/// while `whoami` is in flight.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PendingPill() -> Element {
    rsx! {
        span {
            class: "tw:inline-block tw:h-7 tw:w-[58px] tw:flex-none tw:animate-pulse tw:rounded-full tw:bg-card-muted",
            aria_hidden: "true",
        }
    }
}

/// The quiet word (§1A ruling): the secondary nav family's treatment, one
/// link, straight to the deployment's single connection.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SignInLink(href: String) -> Element {
    rsx! {
        AuthLink {
            href,
            class: NAV_TAB_SECONDARY_IDLE.to_string(),
            title: "Sign in to publish and share your own work".to_string(),
            "Sign in"
        }
    }
}

/// The same quiet word, opening the §4 chooser — for deployments with more
/// than one way in (a dev picker, a second connection).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SignInMenu(
    options: LoginOptionsInfo,
    next: String,
    /// Stories only: mount the chooser open (capture cannot click).
    #[props(default = false)]
    initially_open: bool,
) -> Element {
    rsx! {
        PopoverButton {
            class: SIGN_IN_TRIGGER_CLASS.to_string(),
            open_class: SIGN_IN_TRIGGER_OPEN_CLASS.to_string(),
            // The word carries its own type on the span; the button's
            // `font: inherit` reset (style.css, base layer) picks it up, so
            // the chooser's trigger matches the plain link's "Sign in".
            trigger: rsx! {
                span { class: "tw:text-xs tw:font-medium", "Sign in" }
            },
            label: "Sign in".to_string(),
            title: "Sign in to publish and share your own work".to_string(),
            popup_class: SIGN_IN_POPUP_CLASS.to_string(),
            // The QUIET chrome, like the ⋯ menu this word sits beside: the
            // merged outline then fills the open trigger with the terminal
            // surface. (Neutral is the bordered-chip family — the version
            // badge and the settings gear; on a borderless text trigger its
            // raised fill reads as a bright pill glued to the bar.)
            chrome_class: "ux-popover-chrome-quiet".to_string(),
            placement: PopoverPlacement::BottomEnd,
            // The trigger is text in its own padded box, not a lone glyph.
            layer_keeps_layout: true,
            initially_open,
            SignInPanel { options, next }
        }
    }
}

/// The chooser's body (spike §4): one row per configured connection, then
/// the dev picker's profiles, then the line that says who this is *not*
/// for. Pure — stories mount it without a popover.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SignInPanel(
    options: LoginOptionsInfo,
    next: String,
    /// The panel's own "Sign in" title. Off where the surface already said
    /// it — the `/account` page (P6) headlines the ask itself, and two
    /// headings stacked reads as a mistake.
    #[props(default = true)]
    header: bool,
) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-3 tw:p-3",
            if header {
                div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
                    strong { class: "tw:text-sm tw:text-strong-foreground", "Sign in" }
                    span { class: "tw:text-xs tw:font-bold tw:text-subtle-foreground",
                        "Publishing and sharing use your account"
                    }
                }
            }
            if !options.oidc.is_empty() {
                div { class: "tw:grid tw:min-w-0 tw:gap-1.5",
                    for option in options.oidc.iter() {
                        AuthLink {
                            key: "{option.id}",
                            href: sign_in_href(&option.start_path, &next, None),
                            class: PROVIDER_BUTTON_CLASS.to_string(),
                            "Continue with {option.label}"
                        }
                    }
                }
            }
            if let Some(picker) = options.dev_picker.as_ref() {
                div { class: "tw:grid tw:min-w-0 tw:gap-1.5",
                    span { class: LABEL_CLASS, "Pick a profile" }
                    // A dev server nobody has signed into yet has a picker
                    // and no profiles. Say so, and name the door: an empty
                    // group under a label reads as a surface that failed to
                    // load, and on a deployment with no OIDC connection
                    // there is otherwise nothing here to act on.
                    if picker.choices.is_empty() {
                        p { class: "tw:m-0 tw:text-[11px] tw:leading-snug tw:text-dim-foreground",
                            "No profiles yet — the first sign-in creates one:"
                        }
                        code { class: "tw:min-w-0 tw:font-mono tw:text-[10.5px] tw:text-subtle-foreground tw:select-all tw:[overflow-wrap:anywhere]",
                            "{picker.start_path}?email=you@example.com"
                        }
                    }
                    for choice in picker.choices.iter() {
                        AuthLink {
                            key: "{choice.email}",
                            href: sign_in_href(&picker.start_path, &next, Some(&choice.email)),
                            class: PICK_ROW_CLASS.to_string(),
                            AccountAvatar { face: AvatarFace::of_dev_choice(choice), size: 28 }
                            span { class: "tw:grid tw:min-w-0 tw:text-left",
                                span { class: "tw:truncate tw:text-xs tw:font-bold tw:text-strong-foreground",
                                    "{choice.display_name}"
                                }
                                span { class: "tw:truncate tw:text-[10.5px] tw:text-dim-foreground",
                                    "{choice.email}"
                                }
                            }
                        }
                    }
                }
            }
            p { class: "tw:m-0 tw:text-[11px] tw:leading-snug tw:text-dim-foreground",
                "Viewers of your links never need an account."
            }
        }
    }
}

/// The signed-in slot: the avatar button and the identity dropdown it
/// opens.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AccountDropdown(
    me: MeInfo,
    accounts: Vec<RememberedAccount>,
    options: Option<LoginOptionsInfo>,
    next: String,
    on_sign_out: Option<EventHandler<()>>,
) -> Element {
    let face = AvatarFace::of_me(&me);
    rsx! {
        PopoverButton {
            class: AVATAR_TRIGGER_CLASS.to_string(),
            open_class: AVATAR_TRIGGER_OPEN_CLASS.to_string(),
            trigger: rsx! {
                AccountAvatar { face: face.clone(), size: 26 }
            },
            label: "Account".to_string(),
            title: format!("{} — {}", me.display_name, me.email),
            popup_class: ACCOUNT_POPUP_CLASS.to_string(),
            chrome_class: "ux-popover-chrome-neutral".to_string(),
            placement: PopoverPlacement::BottomEnd,
            AccountMenu {
                me,
                accounts,
                options,
                next,
                on_sign_out,
            }
        }
    }
}

/// The identity dropdown's body (spike §2A grown into §2C): who you are,
/// Profile, the switch group when this browser remembers other accounts,
/// add-another, sign out. Pure but for the sign-out handler — stories mount
/// it open with fixtures and no handler (the row then renders inert).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn AccountMenu(
    me: MeInfo,
    /// The remembered accounts OTHER than `me` (see [`other_accounts`]).
    #[props(default)]
    accounts: Vec<RememberedAccount>,
    /// How this deployment signs people in — the switch and add-account
    /// rows' targets. Absent ⇒ those rows have nowhere to go and are
    /// omitted.
    #[props(default)]
    options: Option<LoginOptionsInfo>,
    /// The page to come back to after a re-auth.
    #[props(default = "/".to_string())]
    next: String,
    /// Absent under stories ⇒ the sign-out row renders inert.
    #[props(default)]
    on_sign_out: Option<EventHandler<()>>,
) -> Element {
    let close = try_consume_context::<PopoverCloseHandle>();
    let switch_rows: Vec<(RememberedAccount, String)> = options
        .as_ref()
        .map(|options| {
            accounts
                .iter()
                .filter_map(|account| {
                    switch_href(options, account, &next).map(|href| (account.clone(), href))
                })
                .collect()
        })
        .unwrap_or_default();
    let add_href = options
        .as_ref()
        .and_then(|options| add_account_href(options, &next));
    rsx! {
        // One explicit grid wrapper: the popover primitive nests children
        // in its own content div, so the panel's classes never reach them.
        div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
            // Identity header — not a control, just who you are.
            div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2.5 tw:px-2 tw:py-1.5",
                AccountAvatar { face: AvatarFace::of_me(&me), size: 34 }
                span { class: "tw:grid tw:min-w-0 tw:gap-px",
                    span { class: "tw:truncate tw:text-xs tw:font-bold tw:text-strong-foreground",
                        "{me.display_name}"
                    }
                    span { class: "tw:truncate tw:text-[11px] tw:text-dim-foreground", "{me.email}" }
                }
            }
            MenuDivider {}
            a {
                class: "{NAV_MENU_ITEM_IDLE} tw:flex tw:items-center tw:gap-2.5",
                href: "/account",
                onclick: move |_| {
                    if let Some(mut close) = close {
                        close.close();
                    }
                },
                UserRound { size: 14 }
                span { class: "tw:min-w-0 tw:truncate", "Profile" }
            }
            // "Your published projects" belongs here too — it waits on the
            // publish/share round that gives it something to list.
            if !switch_rows.is_empty() {
                span { class: GROUP_HEADER_CLASS, "Switch account" }
                for (account , href) in switch_rows.iter() {
                    AuthLink {
                        key: "{account.email}",
                        href: href.clone(),
                        class: SWITCH_ROW_CLASS.to_string(),
                        title: format!(
                            "Sign in as {} ({})",
                            account.display_name, account.provider_label,
                        ),
                        AccountAvatar { face: AvatarFace::of_account(account), size: 26 }
                        span { class: "tw:grid tw:min-w-0 tw:text-left",
                            span { class: "tw:truncate tw:text-[11.5px] tw:font-semibold tw:text-muted-foreground",
                                "{account.display_name}"
                            }
                            span { class: "tw:truncate tw:text-[10.5px] tw:text-dim-foreground",
                                "{account.email}"
                            }
                        }
                    }
                }
            }
            if let Some(href) = add_href {
                AuthLink {
                    href,
                    class: format!("{NAV_MENU_ITEM_IDLE} tw:flex tw:items-center tw:gap-2.5"),
                    UserRoundPlus { size: 14 }
                    span { class: "tw:min-w-0 tw:truncate", "Add another account…" }
                }
            }
            MenuDivider {}
            button {
                class: "{NAV_MENU_ITEM_IDLE} tw:flex tw:w-full tw:cursor-pointer tw:items-center tw:gap-2.5 tw:border-0 tw:bg-transparent tw:text-left",
                r#type: "button",
                onclick: move |_| {
                    if let Some(on_sign_out) = on_sign_out {
                        on_sign_out.call(());
                    }
                    if let Some(mut close) = close {
                        close.close();
                    }
                },
                LogOut { size: 14 }
                // The row's type lives on the span; the button's `font:
                // inherit` reset (style.css, base layer) picks it up.
                span { class: "tw:min-w-0 tw:truncate tw:text-xs tw:font-semibold", "Sign out" }
            }
        }
    }
}

/// The dropdown's hairline rule between groups.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn MenuDivider() -> Element {
    rsx! {
        span {
            class: "tw:mx-0.5 tw:my-1 tw:h-px tw:bg-border-subtle",
            aria_hidden: "true",
        }
    }
}

/// A face: the provider's photo when there is one, initials on a
/// deterministic hue otherwise — and initials again if the photo 404s or
/// the provider's CDN refuses us (never a broken-image glyph).
///
/// Photos are hotlinked with `referrerpolicy="no-referrer"`: the bytes are
/// the provider's, we store none of them, and the provider learns nothing
/// about which page you were on.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn AccountAvatar(face: AvatarFace, size: u32) -> Element {
    let mut photo_failed = use_signal(|| false);
    let box_style = format!("width: {size}px; height: {size}px;");
    let photo = face.picture_url.clone().filter(|_| !photo_failed());
    match photo {
        Some(url) => rsx! {
            span {
                class: "tw:inline-flex tw:flex-none tw:items-center tw:justify-center tw:overflow-hidden tw:rounded-full tw:bg-card-raised",
                style: "{box_style}",
                img {
                    class: "tw:h-full tw:w-full tw:object-cover",
                    src: "{url}",
                    alt: "",
                    referrerpolicy: "no-referrer",
                    onerror: move |_| photo_failed.set(true),
                }
            }
        },
        None => rsx! {
            span {
                class: "tw:inline-flex tw:flex-none tw:select-none tw:items-center tw:justify-center tw:overflow-hidden tw:rounded-full tw:border tw:font-bold",
                style: "{box_style} {initials_style(face.hue, size)}",
                aria_hidden: "true",
                "{face.initials}"
            }
        },
    }
}

/// What an avatar needs, whoever it is for — the signed-in account, a
/// remembered one, or a dev-picker profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvatarFace {
    /// Provider-hosted photo, hotlinked; `None` ⇒ initials.
    pub picture_url: Option<String>,
    /// One or two letters.
    pub initials: String,
    /// Deterministic hue (0-359) for the initials treatment.
    pub hue: u16,
}

impl AvatarFace {
    pub fn of_me(me: &MeInfo) -> Self {
        Self {
            picture_url: me.picture_url.clone(),
            initials: initials(
                &me.display_name,
                me.given_name.as_deref(),
                me.family_name.as_deref(),
            ),
            hue: avatar_hue(&me.email),
        }
    }

    pub fn of_account(account: &RememberedAccount) -> Self {
        Self {
            picture_url: account.picture_url.clone(),
            initials: initials(&account.display_name, None, None),
            hue: avatar_hue(&account.email),
        }
    }

    pub fn of_dev_choice(choice: &DevChoice) -> Self {
        Self {
            picture_url: None,
            initials: initials(&choice.display_name, None, None),
            hue: avatar_hue(&choice.email),
        }
    }
}

/// A link that LEAVES the app (sign-in, switch, add account).
///
/// The router intercepts same-origin anchor clicks so in-app navigation
/// never reloads the page and kills the runtime pool. These targets are
/// server routes, not app routes: the click cancels that interception (the
/// listener runs at the window, after component handlers) and assigns
/// `location.href` itself. The `href` stays real so hover, copy-link and
/// cmd-click all behave.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AuthLink(
    href: String,
    class: String,
    #[props(default = String::new())] title: String,
    children: Element,
) -> Element {
    let target = href.clone();
    rsx! {
        a {
            class: "{class}",
            href: "{href}",
            title: "{title}",
            onclick: move |event: Event<MouseData>| {
                event.prevent_default();
                leave_app(&target);
            },
            {children}
        }
    }
}

/// Hand the browser a full page navigation (host builds: nothing to do).
fn leave_app(href: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        if let Err(error) = window.location().set_href(href) {
            log::warn!("sign-in navigation refused: {error:?}");
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = href;
}

/// The page a sign-in should return to.
///
/// Read from the location rather than through `router::current_route()`:
/// the router folds every path it does not know into `/`, and `/account`
/// (P6) would silently become the landing page.
#[cfg(target_arch = "wasm32")]
fn current_path() -> String {
    web_sys::window()
        .and_then(|window| window.location().pathname().ok())
        .filter(|path| path.starts_with('/'))
        .unwrap_or_else(|| "/".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn current_path() -> String {
    "/".to_string()
}

/// End the session server-side, then re-ask who we are.
///
/// The refresh runs whatever the response was: the service is the authority
/// on whether the cookie is gone, and a failed logout that left us signed in
/// should show us still signed in.
fn sign_out(mut refresh: CloudSessionRefresh) {
    spawn(async move {
        end_session().await;
        refresh.refresh();
    });
}

/// Ask the server to drop THIS browser's session cookie.
///
/// Shared with the `/account` page's "Sign out everywhere" (P6), which ends
/// the other sessions first and then this one. Failures are logged, never
/// raised: the caller refreshes afterwards either way, and the service is
/// the authority on whether the cookie is actually gone.
pub async fn end_session() {
    if let Err(error) = gloo_net::http::Request::post("/auth/logout").send().await {
        log::warn!("sign-out request failed: {error}");
    }
}

/// What the signed-out slot should render, given the server's options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignInAffordance {
    /// Exactly one way in: the word is a link straight to it.
    Direct(String),
    /// More than one: the word opens the §4 chooser.
    Chooser,
    /// No connection is configured — nothing to offer, so nothing shown.
    Nothing,
}

/// One connection and no dev picker ⇒ no chooser at all (Q10 ruling).
pub fn sign_in_affordance(options: &LoginOptionsInfo, next: &str) -> SignInAffordance {
    match (options.oidc.as_slice(), options.dev_picker.as_ref()) {
        ([only], None) => SignInAffordance::Direct(sign_in_href(&only.start_path, next, None)),
        ([], None) => SignInAffordance::Nothing,
        _ => SignInAffordance::Chooser,
    }
}

/// `{start_path}?[email=…&]next=…`. The server's path is taken verbatim;
/// only the values this client supplies are encoded.
pub fn sign_in_href(start_path: &str, next: &str, email: Option<&str>) -> String {
    match email {
        Some(email) => format!(
            "{start_path}?email={}&next={}",
            encode_query_value(email),
            encode_query_value(next)
        ),
        None => format!("{start_path}?next={}", encode_query_value(next)),
    }
}

/// Where "Add another account…" goes.
///
/// A dev picker wins when there is one: on a dev deployment that IS the
/// add-account flow. Otherwise a single connection is unambiguous. Several
/// connections and no picker leave the question open — the row is omitted
/// rather than guessing which door another account came through (the
/// chooser popover for that case belongs to the signed-out surface).
pub fn add_account_href(options: &LoginOptionsInfo, next: &str) -> Option<String> {
    if let Some(picker) = options.dev_picker.as_ref() {
        return Some(sign_in_href(&picker.start_path, next, None));
    }
    match options.oidc.as_slice() {
        [only] => Some(sign_in_href(&only.start_path, next, None)),
        _ => None,
    }
}

/// Where a switch row goes: back through the provider that account signs in
/// with (spike §5 — one session at a time, so a switch is a re-auth).
///
/// The dev picker takes an `email` so a dev switch is one click; an OIDC
/// connection gets none — Google's own account picker is forced server-side
/// and a hint we cannot verify would be a lie in the URL bar.
pub fn switch_href(
    options: &LoginOptionsInfo,
    account: &RememberedAccount,
    next: &str,
) -> Option<String> {
    if let Some(picker) = options.dev_picker.as_ref() {
        let is_dev_account = account.provider_label.eq_ignore_ascii_case("dev")
            || picker
                .choices
                .iter()
                .any(|choice| choice.email == account.email);
        if is_dev_account {
            return Some(sign_in_href(&picker.start_path, next, Some(&account.email)));
        }
    }
    let matched = options
        .oidc
        .iter()
        .find(|option| option.label.eq_ignore_ascii_case(&account.provider_label))
        .or(match options.oidc.as_slice() {
            [only] => Some(only),
            _ => None,
        })?;
    Some(sign_in_href(&matched.start_path, next, None))
}

/// The remembered accounts that are not the one signed in — the switch
/// group's rows (empty ⇒ no group at all).
pub fn other_accounts(
    accounts: &[RememberedAccount],
    current_email: &str,
) -> Vec<RememberedAccount> {
    accounts
        .iter()
        .filter(|account| account.email != current_email)
        .cloned()
        .collect()
}

/// One or two letters for a face without a photo: the given/family initials
/// when the provider gave us names, the display name's first two words
/// otherwise (a remembered account carries only the joined name). Never
/// empty — a blank circle reads as a rendering fault.
pub fn initials(display_name: &str, given: Option<&str>, family: Option<&str>) -> String {
    let first = |value: Option<&str>| {
        value
            .and_then(|value| value.chars().find(|c| c.is_alphanumeric()))
            .map(|c| c.to_uppercase().to_string())
    };
    let from_names = format!(
        "{}{}",
        first(given).unwrap_or_default(),
        first(family).unwrap_or_default()
    );
    if !from_names.is_empty() {
        return from_names;
    }
    let from_words = display_name
        .split_whitespace()
        .take(2)
        .filter_map(|word| first(Some(word)))
        .collect::<String>();
    if !from_words.is_empty() {
        return from_words;
    }
    first(Some(display_name)).unwrap_or_else(|| "?".to_string())
}

/// A stable hue per account, so the same person keeps the same colored
/// circle everywhere (FNV-1a over the email — the identity the service
/// keys on).
pub fn avatar_hue(email: &str) -> u16 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in email.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (hash % 360) as u16
}

/// Inline paint for an initials avatar: the hue washed into the surface,
/// with the type scaled to the circle. Inline because both values are
/// data-driven — Tailwind only ships classes it can see in the source.
fn initials_style(hue: u16, size: u32) -> String {
    let font_size = (f64::from(size) * 0.36).round() as u32;
    format!(
        "font-size: {font_size}px; \
         border-color: color-mix(in srgb, hsl({hue} 60% 62%) 55%, transparent); \
         background: color-mix(in srgb, hsl({hue} 60% 62%) 22%, var(--studio-color-surface-subtle)); \
         color: color-mix(in srgb, hsl({hue} 60% 62%) 88%, white 6%);"
    )
}

/// Percent-encode one query VALUE (RFC 3986 unreserved set survives).
fn encode_query_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// The avatar trigger: the version chip's round 28px footprint, so the
/// right cluster keeps one geometry.
const AVATAR_TRIGGER_CLASS: &str = "tw:inline-flex tw:h-7 tw:w-7 tw:flex-none tw:cursor-pointer tw:items-center tw:justify-center tw:overflow-hidden tw:rounded-full tw:border tw:border-status-neutral-border tw:bg-status-neutral-bg tw:p-0";
const AVATAR_TRIGGER_OPEN_CLASS: &str = "tw:inline-flex tw:h-7 tw:w-7 tw:flex-none tw:cursor-pointer tw:items-center tw:justify-center tw:overflow-hidden tw:rounded-full tw:border tw:border-border-strong tw:bg-card-raised tw:p-0";
/// The identity dropdown, at the spike's 236px.
///
/// Plain `w-[…]`, the shipped ⋯ menu's idiom (`OVERFLOW_POPUP_CLASS`): the
/// primitive's own `.ux-popover-panel` already caps every panel at
/// `calc(100vw - 24px)`, and a second viewport clamp inside the width fought
/// the measured layout.
/// Material-free (P4): the merged-outline popover already paints
/// background/border/shadow.
const ACCOUNT_POPUP_CLASS: &str = "tw:grid tw:w-[236px] tw:min-w-0 tw:gap-0.5 tw:rounded-md tw:border tw:p-1.5 tw:text-sm tw:text-muted-foreground";
/// The signed-out chooser (§4): the ⋯ menu's 288px, because provider rows
/// carry copy. No `overflow-hidden` — the merged outline draws this panel's
/// chrome, and clipping the body only hides a layout fault instead of
/// showing it. Material-free (P4) for the same reason.
const SIGN_IN_POPUP_CLASS: &str =
    "tw:grid tw:w-[288px] tw:min-w-0 tw:rounded-md tw:border tw:text-sm tw:text-muted-foreground";
/// The quiet word as a popover trigger — the secondary tab's treatment on a
/// `button` instead of an `a`.
///
/// Two things a tab class never had to say, because tabs are anchors:
/// `bg-transparent` (this build imports Tailwind's theme and utilities but
/// NO preflight, so an unstyled button paints the UA's light `buttonface` —
/// which is what read as a bright pill glued into the bar), and a
/// TRANSPARENT border at rest, so opening — which lights that border —
/// cannot resize the button and nudge the row.
const SIGN_IN_TRIGGER_CLASS: &str = "tw:cursor-pointer tw:rounded-sm tw:border tw:border-transparent tw:bg-transparent tw:px-2.5 tw:py-1.5 tw:text-xs tw:font-medium tw:text-subtle-foreground/70 tw:no-underline tw:transition-colors tw:hover:bg-background-wash tw:hover:text-strong-foreground";
/// The same trigger while open: the ⋯ menu's quiet open treatment (the
/// merged outline paints over this in the top layer; it is what shows for
/// the frame before the first measurement lands).
const SIGN_IN_TRIGGER_OPEN_CLASS: &str = "tw:cursor-pointer tw:rounded-sm tw:border tw:border-border-strong tw:bg-terminal tw:px-2.5 tw:py-1.5 tw:text-xs tw:font-medium tw:text-strong-foreground tw:no-underline";
/// One connection row in the chooser.
const PROVIDER_BUTTON_CLASS: &str = "tw:flex tw:min-w-0 tw:items-center tw:justify-center tw:gap-2 tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-muted tw:px-3 tw:py-2 tw:text-xs tw:font-bold tw:text-strong-foreground tw:no-underline tw:transition-colors tw:hover:border-selection-border tw:hover:text-strong-foreground";
/// One dev-picker profile row.
const PICK_ROW_CLASS: &str = "tw:flex tw:min-w-0 tw:items-center tw:gap-2.5 tw:rounded-sm tw:border tw:border-border-subtle tw:bg-transparent tw:px-2 tw:py-1.5 tw:no-underline tw:transition-colors tw:hover:border-selection-border tw:hover:bg-card-raised";
/// One switch-account row: the menu row's rhythm at avatar height.
const SWITCH_ROW_CLASS: &str = "tw:flex tw:min-w-0 tw:items-center tw:gap-2.5 tw:rounded-sm tw:px-2 tw:py-1 tw:no-underline tw:transition-colors tw:hover:bg-card-raised";
const LABEL_CLASS: &str = "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground";

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_cloud_api::{DevPickerOptions, OidcOption};

    fn google() -> OidcOption {
        OidcOption {
            id: "google".to_string(),
            label: "Google".to_string(),
            start_path: "/auth/google".to_string(),
        }
    }

    fn github() -> OidcOption {
        OidcOption {
            id: "github".to_string(),
            label: "GitHub".to_string(),
            start_path: "/auth/github".to_string(),
        }
    }

    fn dev_picker() -> DevPickerOptions {
        DevPickerOptions {
            start_path: "/auth/dev".to_string(),
            choices: vec![DevChoice {
                email: "dev@example.com".to_string(),
                display_name: "Dev User".to_string(),
            }],
        }
    }

    fn options(oidc: Vec<OidcOption>, dev: Option<DevPickerOptions>) -> LoginOptionsInfo {
        LoginOptionsInfo {
            oidc,
            dev_picker: dev,
        }
    }

    fn account(email: &str, provider: &str) -> RememberedAccount {
        RememberedAccount {
            email: email.to_string(),
            display_name: "Someone".to_string(),
            picture_url: None,
            provider_label: provider.to_string(),
            last_seen: 0.0,
        }
    }

    /// Prod: one connection, so the word links straight through (Q10).
    #[test]
    fn a_single_connection_needs_no_chooser() {
        assert_eq!(
            sign_in_affordance(&options(vec![google()], None), "/projects"),
            SignInAffordance::Direct("/auth/google?next=%2Fprojects".to_string())
        );
    }

    #[test]
    fn a_dev_picker_or_a_second_connection_opens_the_chooser() {
        assert_eq!(
            sign_in_affordance(&options(vec![google()], Some(dev_picker())), "/"),
            SignInAffordance::Chooser
        );
        assert_eq!(
            sign_in_affordance(&options(vec![google(), github()], None), "/"),
            SignInAffordance::Chooser
        );
    }

    /// A deployment with no configured connection offers nothing — better
    /// than a word that leads nowhere.
    #[test]
    fn no_connection_shows_nothing() {
        assert_eq!(
            sign_in_affordance(&options(vec![], None), "/"),
            SignInAffordance::Nothing
        );
    }

    #[test]
    fn hrefs_encode_only_the_values() {
        assert_eq!(
            sign_in_href("/auth/dev", "/p/my project", Some("a+b@x.com")),
            "/auth/dev?email=a%2Bb%40x.com&next=%2Fp%2Fmy%20project"
        );
        assert_eq!(
            sign_in_href("/auth/google", "/", None),
            "/auth/google?next=%2F"
        );
    }

    /// The dev picker is the add-account flow on a dev deployment.
    #[test]
    fn add_account_prefers_the_dev_picker() {
        assert_eq!(
            add_account_href(&options(vec![google()], Some(dev_picker())), "/"),
            Some("/auth/dev?next=%2F".to_string())
        );
        assert_eq!(
            add_account_href(&options(vec![google()], None), "/"),
            Some("/auth/google?next=%2F".to_string())
        );
    }

    /// Several connections and no picker: which door a second account came
    /// through is unknowable, so the row is omitted rather than guessed.
    #[test]
    fn add_account_stays_silent_when_the_door_is_ambiguous() {
        assert_eq!(
            add_account_href(&options(vec![google(), github()], None), "/"),
            None
        );
        assert_eq!(add_account_href(&options(vec![], None), "/"), None);
    }

    #[test]
    fn a_dev_account_switches_through_the_picker_with_its_email() {
        let options = options(vec![google()], Some(dev_picker()));
        assert_eq!(
            switch_href(&options, &account("dev@example.com", "Dev"), "/projects"),
            Some("/auth/dev?email=dev%40example.com&next=%2Fprojects".to_string())
        );
        // A profile the picker does not list, but labelled Dev, still goes
        // through the picker (the seed list can change under us).
        assert_eq!(
            switch_href(&options, &account("other@example.com", "dev"), "/"),
            Some("/auth/dev?email=other%40example.com&next=%2F".to_string())
        );
    }

    /// An OIDC switch carries no email: Google forces its own picker, and a
    /// hint we cannot enforce would be a lie in the URL.
    #[test]
    fn an_oidc_account_switches_by_provider_label() {
        let both = options(vec![google(), github()], None);
        assert_eq!(
            switch_href(&both, &account("me@x.com", "GitHub"), "/"),
            Some("/auth/github?next=%2F".to_string())
        );
        // An unknown label with exactly one connection still resolves.
        assert_eq!(
            switch_href(
                &options(vec![google()], None),
                &account("me@x.com", "Whoever"),
                "/"
            ),
            Some("/auth/google?next=%2F".to_string())
        );
        // An unknown label with several connections does not.
        assert_eq!(
            switch_href(&both, &account("me@x.com", "Whoever"), "/"),
            None
        );
    }

    #[test]
    fn the_switch_group_never_lists_the_current_account() {
        let list = vec![account("a@x.com", "Google"), account("b@x.com", "Google")];
        let others = other_accounts(&list, "a@x.com");
        assert_eq!(others.len(), 1);
        assert_eq!(others[0].email, "b@x.com");
        assert!(other_accounts(&list[..1], "a@x.com").is_empty());
    }

    #[test]
    fn initials_prefer_the_provider_names() {
        assert_eq!(
            initials("Yona Appletree", Some("Yona"), Some("Appletree")),
            "YA"
        );
        assert_eq!(initials("Cher", Some("Cher"), Some("")), "C");
        assert_eq!(initials("山田 太郎", Some("山田"), Some("太郎")), "山太");
    }

    /// A remembered account carries only the joined name — the words stand
    /// in for the missing given/family split.
    #[test]
    fn initials_fall_back_to_the_display_name_words() {
        assert_eq!(initials("Yona Appletree", None, None), "YA");
        assert_eq!(initials("Zook Dome-Crew", None, None), "ZD");
        assert_eq!(initials("山田 太郎", None, None), "山太");
        assert_eq!(initials("Cher", None, None), "C");
    }

    /// Never a blank circle, whatever the service sent.
    #[test]
    fn initials_always_render_something() {
        assert_eq!(initials("", None, None), "?");
        assert_eq!(initials("  ", Some(""), None), "?");
    }

    #[test]
    fn a_hue_is_stable_per_email_and_in_range() {
        let hue = avatar_hue("yona@example.com");
        assert_eq!(hue, avatar_hue("yona@example.com"));
        assert!(hue < 360);
        assert_ne!(hue, avatar_hue("someone-else@example.com"));
    }
}
