//! Stories for the chrome's account surface.
//!
//! The live control renders from the `CloudSession` context, which stories
//! never provide — so these mount the presentational halves with fixtures:
//! the slot's three signed-out/boot/signed-in visuals, the §4 sign-in
//! chooser body, and the identity dropdown body (which a story can show
//! open, unlike the popover it normally lives in).
//!
//! The "photo" fixtures use an inline `data:` image so a capture never
//! depends on a provider CDN.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;
use lpc_cloud_api::{DevChoice, DevPickerOptions, LoginOptionsInfo, MeInfo, OidcOption};
use lpc_history::{PrefixedUid, UidPrefix};

use crate::app::layout::cloud_account::{
    ACCOUNT_HERE_WRAP, AccountAvatar, AccountMenu, AvatarFace, PendingPill, SignInLink, SignInMenu,
    SignInPanel,
};
use crate::cloud::account_memory::RememberedAccount;

#[story(
    description = "Signed out (§1A ruling): one quiet word in the secondary nav family's treatment, last before the ⋯ menu. Next to it, the boot state — a shimmer pill holding the slot's shape while whoami is in flight, so \"Sign in\" never pops into an avatar."
)]
pub(crate) fn slot_signed_out_and_pending() -> Element {
    rsx! {
        div { class: SLOT_ROW,
            SignInLink { href: "/auth/google?next=%2Fprojects".to_string() }
            PendingPill {}
        }
    }
}

#[story(
    description = "On /account the slot wears the tabs' you're-here underline (G1 ruling 2026-08-07): no nav tab lights there, so the slot that opens the page marks the place, the way the logo is Home's tab. Left: on /account. Right: the same slot anywhere else. Both against the header's border line the bar must land on."
)]
pub(crate) fn slot_on_account_underline() -> Element {
    rsx! {
        div { class: "tw:flex tw:min-h-[46px] tw:items-center tw:justify-end tw:gap-2 tw:border-b tw:border-border-subtle tw:px-4 tw:pb-2.5",
            span { class: ACCOUNT_HERE_WRAP,
                span { class: AVATAR_TRIGGER,
                    AccountAvatar { face: AvatarFace::of_me(&yona()), size: 26 }
                }
            }
            span { class: "tw:flex tw:flex-none",
                span { class: AVATAR_TRIGGER,
                    AccountAvatar { face: AvatarFace::of_me(&crew()), size: 26 }
                }
            }
        }
    }
}

#[story(
    description = "The signed-in slot: a 28px avatar button, photo when the provider has one and initials on a per-account hue when it does not (also the fallback when the photo fails to load). Sizes 26 (trigger), 34 (dropdown header) and 26 (switch rows)."
)]
pub(crate) fn avatar_faces() -> Element {
    rsx! {
        div { class: SLOT_ROW,
            span { class: AVATAR_TRIGGER,
                AccountAvatar { face: AvatarFace::of_me(&yona()), size: 26 }
            }
            span { class: AVATAR_TRIGGER,
                AccountAvatar { face: AvatarFace::of_me(&crew()), size: 26 }
            }
            AccountAvatar { face: AvatarFace::of_me(&yona()), size: 34 }
            AccountAvatar { face: AvatarFace::of_me(&crew()), size: 34 }
            AccountAvatar { face: AvatarFace::of_me(&maria()), size: 26 }
            AccountAvatar { face: AvatarFace::of_me(&yamada()), size: 26 }
        }
    }
}

#[story(
    description = "The §4 chooser on a deployment with one external connection: sign-in is rendered from the server's login options, never from a hard-coded Google, and the fine print says who does NOT need an account."
)]
pub(crate) fn sign_in_panel_provider_only() -> Element {
    panel(rsx! {
        SignInPanel {
            options: LoginOptionsInfo {
                oidc: vec![google()],
                dev_picker: None,
            },
            next: "/projects".to_string(),
        }
    })
}

#[story(
    description = "The same chooser on a local dev deployment: the passwordless profile picker rides alongside the external connection, each row a one-click session for a seeded account."
)]
pub(crate) fn sign_in_panel_dev_picker() -> Element {
    panel(rsx! {
        SignInPanel {
            options: LoginOptionsInfo {
                oidc: vec![google()],
                dev_picker: Some(DevPickerOptions {
                    start_path: "/auth/dev".to_string(),
                    choices: vec![
                        DevChoice {
                            email: "lightatplay@gmail.com".to_string(),
                            display_name: "Yona Appletree".to_string(),
                        },
                        DevChoice {
                            email: "crew@zookdome.org".to_string(),
                            display_name: "Zook Dome-Crew".to_string(),
                        },
                    ],
                }),
            },
            next: "/projects".to_string(),
        }
    })
}

#[story(
    label = "Sign-in popover, open in the bar",
    description = "The chooser as it actually ships: the real popover, mounted open at the end of a chrome-width row. This is the story that catches what a panel-only fixture cannot — the trigger's open treatment (the ⋯ menu's quiet chrome, not a bright pill) and the panel's placement and width against the bar's right edge."
)]
pub(crate) fn sign_in_popover_open() -> Element {
    rsx! {
        // Room for the panel under the bar; the popover paints in the top
        // layer, so the frame only has to hold the trigger's row.
        div { class: "tw:min-h-[360px]",
            div { class: "tw:flex tw:items-center tw:justify-end tw:gap-2 tw:border-b tw:border-border-subtle tw:pb-2.5",
                SignInMenu {
                    options: dev_options(),
                    next: "/projects".to_string(),
                    initially_open: true,
                }
            }
        }
    }
}

#[story(
    description = "A fresh local server: dev auth on, no OIDC connection configured, and nobody signed in yet — so the picker exists with an empty list. The group says so and names the door, rather than showing a headed void with nothing to click."
)]
pub(crate) fn sign_in_panel_no_profiles() -> Element {
    panel(rsx! {
        SignInPanel {
            options: LoginOptionsInfo {
                oidc: Vec::new(),
                dev_picker: Some(DevPickerOptions {
                    start_path: "/auth/dev".to_string(),
                    choices: Vec::new(),
                }),
            },
            next: "/".to_string(),
        }
    })
}

#[story(
    description = "The identity dropdown with one account (spike §2A): identity header, Profile, add-another, sign out — the ⋯ menu's grammar at 236px."
)]
pub(crate) fn dropdown_single_account() -> Element {
    menu(rsx! {
        AccountMenu {
            me: yona(),
            accounts: Vec::new(),
            options: Some(prod_options()),
            next: "/projects".to_string(),
        }
    })
}

#[story(
    description = "The same dropdown grown multi-account (spike §2C): the switch group appears between Profile and sign-out whenever this browser remembers other accounts. Rows are local memory of past sign-ins; clicking one re-auths through that account's provider."
)]
pub(crate) fn dropdown_switch_group() -> Element {
    menu(rsx! {
        AccountMenu {
            me: yona(),
            accounts: vec![remembered(&crew(), "Google"), remembered(&photomancer(), "Google")],
            options: Some(prod_options()),
            next: "/projects".to_string(),
        }
    })
}

#[story(
    label = "Dropdown, awkward identity",
    description = "The spike's awkward set: a long Spanish name with a very long domain, a family-first CJK name, and a mononym. Every name and email row must ellipsize inside 236px rather than widening the menu."
)]
pub(crate) fn dropdown_awkward_identity() -> Element {
    menu(rsx! {
        AccountMenu {
            me: maria(),
            accounts: vec![remembered(&yamada(), "Dev"), remembered(&cher(), "Google")],
            options: Some(dev_options()),
            next: "/sim/zook-dome".to_string(),
        }
    })
}

fn google() -> OidcOption {
    OidcOption {
        id: "google".to_string(),
        label: "Google".to_string(),
        start_path: "/auth/google".to_string(),
    }
}

fn prod_options() -> LoginOptionsInfo {
    LoginOptionsInfo {
        oidc: vec![google()],
        dev_picker: None,
    }
}

fn dev_options() -> LoginOptionsInfo {
    LoginOptionsInfo {
        oidc: vec![google()],
        dev_picker: Some(DevPickerOptions {
            start_path: "/auth/dev".to_string(),
            choices: vec![DevChoice {
                email: "yamada.taro@gmail.com".to_string(),
                display_name: "山田 太郎".to_string(),
            }],
        }),
    }
}

/// A deterministic stand-in for a provider photo — no CDN in a capture.
const PHOTO: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 64 64'%3E%3Crect width='64' height='64' fill='rgb(20,50,63)'/%3E%3Ccircle cx='32' cy='25' r='11' fill='rgb(123,224,178)'/%3E%3Cellipse cx='32' cy='58' rx='20' ry='16' fill='rgb(29,92,80)'/%3E%3C/svg%3E";

fn person(given: &str, family: &str, email: &str, photo: bool, provider: &str, seed: u8) -> MeInfo {
    let display_name = [given, family]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    MeInfo {
        uid: PrefixedUid::mint(UidPrefix::User, &[seed; 16]),
        email: email.to_string(),
        display_name,
        given_name: (!given.is_empty()).then(|| given.to_string()),
        family_name: (!family.is_empty()).then(|| family.to_string()),
        picture_url: photo.then(|| PHOTO.to_string()),
        provider_label: provider.to_string(),
        created_at: 1_752_000_000_000.0,
    }
}

fn yona() -> MeInfo {
    person(
        "Yona",
        "Appletree",
        "lightatplay@gmail.com",
        true,
        "Google",
        1,
    )
}

fn photomancer() -> MeInfo {
    person(
        "Photomancer",
        "Art",
        "photomancer.art@gmail.com",
        true,
        "Google",
        2,
    )
}

fn crew() -> MeInfo {
    person("Zook", "Dome-Crew", "crew@zookdome.org", false, "Google", 3)
}

fn maria() -> MeInfo {
    person(
        "María-Guadalupe Fernanda",
        "de los Ángeles Rodríguez-Vásquez",
        "maria.guadalupe.fernanda.rodriguez.vasquez@extremely-long-domain.example.org",
        true,
        "Google",
        4,
    )
}

fn yamada() -> MeInfo {
    person("山田", "太郎", "yamada.taro@gmail.com", false, "Dev", 5)
}

fn cher() -> MeInfo {
    person("Cher", "", "cher@gmail.com", false, "Google", 6)
}

fn remembered(me: &MeInfo, provider: &str) -> RememberedAccount {
    RememberedAccount {
        email: me.email.clone(),
        display_name: me.display_name.clone(),
        picture_url: me.picture_url.clone(),
        provider_label: provider.to_string(),
        last_seen: 1_754_000_000_000.0,
    }
}

/// The chrome's right cluster, in miniature.
const SLOT_ROW: &str = "tw:flex tw:flex-wrap tw:items-center tw:gap-3 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-4";
/// The avatar trigger's own chrome, around a story-mounted face.
const AVATAR_TRIGGER: &str = "tw:inline-flex tw:h-7 tw:w-7 tw:flex-none tw:items-center tw:justify-center tw:overflow-hidden tw:rounded-full tw:border tw:border-status-neutral-border tw:bg-status-neutral-bg tw:p-0";

/// The §4 chooser's panel box.
fn panel(children: Element) -> Element {
    rsx! {
        div { class: "tw:w-[min(288px,calc(100vw-24px))] tw:overflow-hidden tw:rounded-md tw:border tw:border-status-neutral-border tw:bg-card tw:text-sm tw:text-muted-foreground tw:shadow-lg",
            {children}
        }
    }
}

/// The identity dropdown's panel box (the spike's 236px).
fn menu(children: Element) -> Element {
    rsx! {
        div { class: "tw:grid tw:w-[min(236px,calc(100vw-24px))] tw:gap-0.5 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-1.5 tw:text-sm tw:text-muted-foreground tw:shadow-lg",
            {children}
        }
    }
}
