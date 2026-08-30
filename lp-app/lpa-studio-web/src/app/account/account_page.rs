//! `/account` — the profile page (spike §3: B's structure, A's content).
//!
//! Four groups of settings rows, holding only what actually exists:
//!
//! - **Identity** — the provider's photo (never ours, never stored), and
//!   the two name boxes. Two neutral boxes rather than one "full name"
//!   field because a family-first CJK name is not a parsing problem, it is
//!   a different order; save appears when a box differs from what loaded.
//! - **Account** — the login email with the connection's own label beside
//!   it, and the account id with its birthday. Facts, not controls.
//! - **Cloud sync** — the auto-publish driver's ledger: what it last
//!   concluded about the engine and about each project, failures included.
//!   Diagnostic rows only; the product-level share surface is a separate
//!   vision.
//! - **Sessions** — every browser this account is open in, with a way to
//!   close one, and a way to close them all.
//!
//! # Shape
//!
//! [`AccountPage`] is the live half: it reads the `CloudSession` context
//! (house rule — `try_consume_context`, inert without it), owns the two
//! name drafts, and owns the session list's fetch. Everything visible is
//! [`AccountPageBody`] / [`AccountSignInCard`], which take pure props, so
//! the stories render the awkward identities and the empty/loading states
//! without a service, a clock, or a context.
//!
//! # Why saving writes through instead of refreshing
//!
//! [`CloudSessionRefresh`] re-asks `whoami`, and its first act is to put
//! the session back to `Pending` — which on this page would blink the
//! whole profile out to the sign-in card and back for every name edit. A
//! successful `UpdateMe` already answers with the updated record, so the
//! context is written through with it. The refresh handle is still used
//! where it belongs: "Sign out everywhere", which genuinely does mean
//! "ask again".

use dioxus::prelude::*;
use dioxus_icons::lucide::{Laptop, Smartphone, Tablet};
use lpa_studio_core::core::time_ago::time_ago;
use lpc_cloud_api::request::{ListSessions, RevokeSession, UpdateMe};
use lpc_cloud_api::{LoginOptionsInfo, MeInfo, SessionInfo};

use crate::app::layout::cloud_account::{AccountAvatar, AvatarFace, SignInPanel, end_session};
use crate::cloud::sync::sync_status::{
    self, ProjectSyncStatus, SyncOutcomeKind, SyncStatusSnapshot,
};
use crate::cloud::{CloudSession, CloudSessionRefresh, FetchCloudPort, account_memory};
use crate::core::outline_action_class;

/// The page at `/account`.
///
/// Inert without the `CloudSession` context: it renders the sign-in card
/// with nothing to click, which is also what a story would see.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn AccountPage() -> Element {
    let Some(mut session) = try_consume_context::<Signal<CloudSession>>() else {
        return rsx! {
            AccountSignInCard { options: None, next: "/account".to_string() }
        };
    };
    let refresh = try_consume_context::<CloudSessionRefresh>();

    // ---- the two name drafts -------------------------------------------
    //
    // Seeded from the loaded record and re-seeded whenever it changes (a
    // save's write-through, a switch). `use_memo` means a re-render that
    // did not move the names does not clobber what is being typed.
    let mut given = use_signal(String::new);
    let mut family = use_signal(String::new);
    let mut save_status = use_signal(|| SaveStatus::Idle);
    let loaded_names = use_memo(move || {
        session().me().map(|me| {
            (
                me.given_name.clone().unwrap_or_default(),
                me.family_name.clone().unwrap_or_default(),
            )
        })
    });
    use_effect(move || {
        if let Some((loaded_given, loaded_family)) = loaded_names() {
            given.set(loaded_given);
            family.set(loaded_family);
        }
    });

    // ---- the session list ----------------------------------------------
    //
    // Keyed on the ACCOUNT, not the whole session: a name edit writes a new
    // `MeInfo` through the context, and re-listing sessions because someone
    // fixed a typo would be a wasted round trip. `reload` is what a revoke
    // bumps.
    let account = use_memo(move || session().me().map(|me| me.uid.to_string()));
    let mut reload = use_signal(|| 0u32);
    let mut sessions = use_signal(|| SessionsPane::Loading);
    use_effect(move || {
        let _ = reload.read();
        if account().is_none() {
            return;
        }
        spawn(async move {
            sessions.set(
                match lpa_cloud_client::call(&FetchCloudPort::new(), ListSessions).await {
                    Ok(list) => SessionsPane::Ready(list.sessions),
                    Err(error) => {
                        log::warn!("could not list sessions: {error}");
                        SessionsPane::Unavailable
                    }
                },
            );
        });
    });

    // ---- the cloud sync ledger -----------------------------------------
    //
    // The auto-publish driver keeps a per-tab ledger of trip conclusions
    // (`sync_status`); this page is its one reader. Polled rather than
    // subscribed: the driver is not a component and must never depend on
    // the UI runtime, and a once-a-second copy on a page someone opened to
    // diagnose sync is the cheapest honest wiring.
    let mut sync = use_signal(SyncStatusSnapshot::default);
    use_future(move || async move {
        loop {
            sync.set(sync_status::snapshot());
            #[cfg(target_arch = "wasm32")]
            gloo_timers::future::TimeoutFuture::new(1_000).await;
            #[cfg(not(target_arch = "wasm32"))]
            break;
        }
    });

    let on_save = EventHandler::new(move |update: UpdateMe| {
        save_status.set(SaveStatus::Saving);
        spawn(async move {
            match lpa_cloud_client::call(&FetchCloudPort::new(), update).await {
                Ok(updated) => {
                    // Write-through (see the module docs): the page must not
                    // blink through `Pending` for a name edit.
                    let current = session.peek().clone();
                    if let CloudSession::SignedIn { options, .. } = current {
                        account_memory::remember(&updated, js_sys::Date::now());
                        session.set(CloudSession::SignedIn {
                            me: updated,
                            options,
                        });
                    }
                    save_status.set(SaveStatus::Saved);
                }
                Err(error) => {
                    log::warn!("could not save names: {error}");
                    save_status.set(SaveStatus::Failed);
                }
            }
        });
    });

    let on_revoke = EventHandler::new(move |id: String| {
        spawn(async move {
            if let Err(error) =
                lpa_cloud_client::call(&FetchCloudPort::new(), RevokeSession { id }).await
            {
                log::warn!("could not revoke session: {error}");
            }
            // Re-list either way: the server is the authority on what is
            // still open, including when it just refused us.
            reload += 1;
        });
    });

    let on_sign_out_everywhere = refresh.map(|mut refresh| {
        let listed = sessions;
        EventHandler::new(move |()| {
            let others = listed.peek().others();
            spawn(async move {
                for id in others {
                    if let Err(error) =
                        lpa_cloud_client::call(&FetchCloudPort::new(), RevokeSession { id }).await
                    {
                        log::warn!("could not revoke session: {error}");
                    }
                }
                // This browser last, so a failure part-way through still
                // leaves you signed in somewhere you can retry from.
                end_session().await;
                refresh.refresh();
            });
        })
    });

    match session() {
        CloudSession::SignedIn { me, .. } => rsx! {
            AccountPageBody {
                me,
                given: given(),
                family: family(),
                save_status: save_status(),
                sessions: sessions(),
                sync: sync(),
                now_secs: crate::web_app::now_secs(),
                on_given: EventHandler::new(move |value: String| {
                    given.set(value);
                    save_status.set(SaveStatus::Idle);
                }),
                on_family: EventHandler::new(move |value: String| {
                    family.set(value);
                    save_status.set(SaveStatus::Idle);
                }),
                on_save,
                on_revoke,
                on_sign_out_everywhere,
            }
        },
        // Boot: the page holds the card's shape and shimmers rather than
        // offering to sign in a beat before showing a profile — the same
        // ruling the chrome's pending pill answers to.
        CloudSession::Pending => rsx! {
            AccountSignInCard { pending: true, options: None, next: "/account".to_string() }
        },
        CloudSession::Anonymous { options } => rsx! {
            AccountSignInCard { options, next: "/account".to_string() }
        },
        // Unreachable: still the invitation, minus anywhere to go. A viewer
        // who cannot reach the service is told nothing is here, not that
        // something broke.
        CloudSession::Unreachable => rsx! {
            AccountSignInCard { options: None, next: "/account".to_string() }
        },
    }
}

/// Where a name edit stands. Only ever one at a time — the page has one
/// Save button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SaveStatus {
    /// Nothing in flight; the note reads as guidance.
    #[default]
    Idle,
    /// `UpdateMe` is out.
    Saving,
    /// It landed, and nothing has been typed since.
    Saved,
    /// It did not land. The draft is kept — retyping a name to retry would
    /// be a punishment for the network's behavior.
    Failed,
}

/// The session list, as the page has it.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum SessionsPane {
    /// `ListSessions` is out (the first paint).
    #[default]
    Loading,
    /// The server's answer, newest first (its own ordering).
    Ready(Vec<SessionInfo>),
    /// The call did not land. The group says so rather than claiming this
    /// is the only session open.
    Unavailable,
}

impl SessionsPane {
    /// The ids of every session that is NOT this browser — what "sign out
    /// everywhere" revokes before ending this one.
    pub fn others(&self) -> Vec<String> {
        match self {
            SessionsPane::Ready(sessions) => sessions
                .iter()
                .filter(|session| !session.current)
                .map(|session| session.id.clone())
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// The signed-in page. Pure: every value and every gesture is a prop.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn AccountPageBody(
    me: MeInfo,
    /// The given-name draft (may differ from `me` — that is the dirty
    /// condition).
    given: String,
    /// The family-name draft.
    family: String,
    #[props(default)] save_status: SaveStatus,
    #[props(default)] sessions: SessionsPane,
    /// The auto-publish driver's per-tab ledger (empty under stories that
    /// do not stage one).
    #[props(default)]
    sync: SyncStatusSnapshot,
    /// Epoch seconds, for the "signed in …" phrasing. A prop so a capture
    /// is not a function of when it ran.
    #[props(default = 0.0)]
    now_secs: f64,
    /// Absent under stories ⇒ the boxes are read-only and Save never shows.
    #[props(default)]
    on_given: Option<EventHandler<String>>,
    #[props(default)] on_family: Option<EventHandler<String>>,
    #[props(default)] on_save: Option<EventHandler<UpdateMe>>,
    #[props(default)] on_revoke: Option<EventHandler<String>>,
    #[props(default)] on_sign_out_everywhere: Option<EventHandler<()>>,
) -> Element {
    let edit = name_edit(&me, &given, &family);
    let saving = save_status == SaveStatus::Saving;
    rsx! {
        div { class: PAGE_CLASS,
            header { class: "tw:grid tw:gap-0.5",
                h1 { class: "tw:m-0 tw:text-[15px] tw:font-extrabold tw:text-strong-foreground",
                    "Your profile"
                }
                p { class: "tw:m-0 tw:text-[11.5px] tw:text-dim-foreground",
                    "How you appear on projects you publish."
                }
            }

            // ---- Identity --------------------------------------------
            section { class: "tw:grid tw:gap-2",
                h2 { class: GROUP_TITLE_CLASS, "Identity" }
                div { class: ROWS_CLASS,
                    div { class: ROW_CLASS,
                        span { class: KEY_CLASS, "Photo" }
                        span { class: "{VALUE_CLASS} tw:flex tw:items-center tw:gap-2.5",
                            AccountAvatar { face: AvatarFace::of_me(&me), size: 34 }
                            span { class: "tw:min-w-0 tw:text-[11px] tw:font-medium tw:text-dim-foreground",
                                "from your sign-in provider — updates when it does; not stored here"
                            }
                        }
                    }
                    NameRow {
                        label: "Given name",
                        id: "account-given-name",
                        value: given,
                        placeholder: "—",
                        disabled: saving,
                        on_input: on_given,
                    }
                    NameRow {
                        label: "Family name",
                        id: "account-family-name",
                        value: family,
                        placeholder: "—",
                        disabled: saving,
                        on_input: on_family,
                    }
                    div { class: ROW_CLASS,
                        span { class: KEY_CLASS, aria_hidden: "true", "" }
                        span { class: "{VALUE_CLASS} tw:flex tw:items-center tw:gap-2.5",
                            // Save APPEARS on dirty (spike §3 caption, A's
                            // behavior): a permanently greyed button is
                            // furniture, and this page is mostly read.
                            if let Some(update) = edit.clone()
                                && let Some(on_save) = on_save
                            {
                                button {
                                    class: outline_action_class(false),
                                    r#type: "button",
                                    disabled: saving,
                                    onclick: move |_| on_save.call(update.clone()),
                                    // `style.css` resets `button { font: inherit }`
                                    // UNLAYERED, which beats every (layered)
                                    // Tailwind font utility on the button itself
                                    // — so the type lives on this span.
                                    span { class: "tw:text-xs tw:font-bold", "Save" }
                                }
                            }
                            span { class: "tw:min-w-0 tw:text-[11px] tw:font-medium tw:text-dim-foreground",
                                {save_note(save_status)}
                            }
                        }
                    }
                }
            }

            // ---- Account ---------------------------------------------
            section { class: "tw:grid tw:gap-2",
                h2 { class: GROUP_TITLE_CLASS, "Account" }
                div { class: ROWS_CLASS,
                    div { class: ROW_CLASS,
                        span { class: KEY_CLASS, "Email" }
                        span {
                            class: "{VALUE_DIM_CLASS} tw:truncate",
                            title: "{me.email}",
                            "{me.email}"
                        }
                        span { class: ACTION_CLASS,
                            // The connection's own label, in the mono pill
                            // grammar — never a hard-coded "Google" (§4).
                            span { class: BADGE_CLASS, "{me.provider_label}" }
                        }
                    }
                    div { class: ROW_CLASS,
                        span { class: KEY_CLASS, "Account id" }
                        span { class: "{VALUE_DIM_CLASS} tw:truncate tw:font-mono tw:text-[11px]",
                            "{me.uid} · since {short_date(me.created_at)}"
                        }
                    }
                }
            }

            // ---- Cloud sync ------------------------------------------
            //
            // Diagnostic, not product: the auto-publish engine works in
            // silence by design, and this group is where that silence can
            // be interrogated — the driver's own facts, then the newest
            // conclusion per project, failures included. The larger
            // share/publish surface is a separate vision; nothing here
            // offers a control.
            section { class: "tw:grid tw:gap-2",
                h2 { class: GROUP_TITLE_CLASS, "Cloud sync" }
                div { class: ROWS_CLASS,
                    div { class: ROW_CLASS,
                        span { class: KEY_CLASS, "Auto-publish" }
                        span { class: VALUE_DIM_CLASS, {sync_engine_note(&sync, now_secs)} }
                    }
                    for row in sync.rows.iter() {
                        SyncStatusRow { key: "{row.uid}", row: row.clone(), now_secs }
                    }
                }
            }

            // ---- Sessions --------------------------------------------
            section { class: "tw:grid tw:gap-2",
                h2 { class: GROUP_TITLE_CLASS, "Sessions" }
                div { class: ROWS_CLASS,
                    match &sessions {
                        SessionsPane::Loading => rsx! {
                            div { class: ROW_CLASS,
                                span { class: KEY_CLASS, aria_hidden: "true", "" }
                                span {
                                    class: "tw:h-3 tw:w-40 tw:animate-pulse tw:rounded-full tw:bg-card-muted",
                                    aria_hidden: "true",
                                }
                            }
                        },
                        SessionsPane::Unavailable => rsx! {
                            div { class: ROW_CLASS,
                                span { class: KEY_CLASS, aria_hidden: "true", "" }
                                span { class: VALUE_DIM_CLASS,
                                    "Could not reach the service for your session list."
                                }
                            }
                        },
                        SessionsPane::Ready(list) => rsx! {
                            for session in list.iter() {
                                SessionRow {
                                    key: "{session.id}",
                                    session: session.clone(),
                                    now_secs,
                                    on_revoke,
                                }
                            }
                        },
                    }
                    // The last row is the group's footer whatever the list
                    // did: "sign out everywhere" is how you recover from a
                    // list you cannot read.
                    div { class: ROW_CLASS,
                        span { class: KEY_CLASS, aria_hidden: "true", "" }
                        span { class: VALUE_DIM_CLASS,
                            {session_lifetime_note(&sessions)}
                        }
                        span { class: ACTION_CLASS,
                            if let Some(on_sign_out_everywhere) = on_sign_out_everywhere {
                                button {
                                    class: DANGER_LINK_CLASS,
                                    r#type: "button",
                                    title: "Ends every session on this account, including this browser",
                                    onclick: move |_| on_sign_out_everywhere.call(()),
                                    span { class: "tw:text-[11.5px] tw:font-bold", "Sign out everywhere" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One name box. Its own component because the label, the box, and the
/// `font: inherit` workaround belong together.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NameRow(
    label: String,
    id: String,
    value: String,
    placeholder: String,
    disabled: bool,
    on_input: Option<EventHandler<String>>,
) -> Element {
    rsx! {
        div { class: ROW_CLASS,
            label { class: KEY_CLASS, r#for: "{id}", "{label}" }
            // The box's type comes from this wrapper — see `VALUE_CLASS`.
            span { class: VALUE_CLASS,
                input {
                    id: "{id}",
                    class: INPUT_CLASS,
                    r#type: "text",
                    value: "{value}",
                    placeholder: "{placeholder}",
                    autocomplete: "off",
                    spellcheck: "false",
                    disabled,
                    readonly: on_input.is_none(),
                    oninput: move |event| {
                        if let Some(on_input) = on_input {
                            on_input.call(event.value());
                        }
                    },
                }
            }
        }
    }
}

/// The Auto-publish row's sentence: what the driver knows about itself.
///
/// Every branch names the fact a diagnosis needs first: whether there is
/// an account to sync with, whether the sign-in sweep ever saw the
/// library, and how much it offered when it did.
fn sync_engine_note(sync: &SyncStatusSnapshot, now_secs: f64) -> String {
    if !sync.engine.signed_in {
        return "waiting for sign-in — nothing syncs until the account is known".to_string();
    }
    match &sync.engine.last_sweep {
        None => "on — projects publish as they are created and saved".to_string(),
        Some(sweep) if sweep.host_missing => {
            "the project library was not ready at sign-in, so nothing was offered — reload to retry"
                .to_string()
        }
        Some(sweep) => format!(
            "sign-in sweep offered {} project{} {}",
            sweep.offered,
            if sweep.offered == 1 { "" } else { "s" },
            time_ago(now_secs, sweep.at_ms / 1000.0),
        ),
    }
}

/// One project's newest sync conclusion.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SyncStatusRow(row: ProjectSyncStatus, now_secs: f64) -> Element {
    let badge = match row.kind {
        SyncOutcomeKind::Refused | SyncOutcomeKind::Denied => {
            format!(
                "{BADGE_CLASS} tw:border-status-error-border tw:bg-status-error-bg tw:text-status-error-foreground"
            )
        }
        SyncOutcomeKind::Retrying => {
            format!(
                "{BADGE_CLASS} tw:border-status-warning-border tw:bg-status-warning-bg tw:text-status-warning-foreground"
            )
        }
        SyncOutcomeKind::Published | SyncOutcomeKind::Pushed => {
            format!(
                "{BADGE_CLASS} tw:border-status-good-border tw:bg-status-good-bg tw:text-status-good-foreground"
            )
        }
        SyncOutcomeKind::NothingSaved | SyncOutcomeKind::Skipped => BADGE_CLASS.to_string(),
    };
    rsx! {
        div { class: ROW_CLASS,
            span { class: "{KEY_CLASS} tw:truncate", title: "{row.name}", "{row.name}" }
            span {
                class: "{VALUE_DIM_CLASS} tw:truncate",
                // The full sentence survives truncation as the tooltip —
                // the detail is the diagnosis.
                title: "{row.detail}",
                "{row.detail} · {time_ago(now_secs, row.at_ms / 1000.0)}"
            }
            span { class: ACTION_CLASS,
                span { class: badge, "{row.kind.label()}" }
            }
        }
    }
}

/// One open session.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SessionRow(
    session: SessionInfo,
    now_secs: f64,
    on_revoke: Option<EventHandler<String>>,
) -> Element {
    let agent = session.user_agent.clone().unwrap_or_default();
    let summary = user_agent_summary(&agent);
    // Only the calling session earns a key: naming the others "Browser"
    // down the column would be a label repeated for nothing.
    let key = if session.current { "This browser" } else { "" };
    let id = session.id.clone();
    rsx! {
        div { class: ROW_CLASS,
            span { class: KEY_CLASS, "{key}" }
            span {
                class: if session.current { format!("{VALUE_CLASS} tw:flex tw:items-center tw:gap-2.5") } else { format!("{VALUE_DIM_CLASS} tw:flex tw:items-center tw:gap-2.5") },
                // The raw string is the tooltip: the summary is a guess, and
                // the guess should never be the only thing you can see.
                title: if agent.is_empty() { String::new() } else { agent.clone() },
                span { class: "tw:flex tw:flex-none tw:text-dim-foreground",
                    match device_kind(&agent) {
                        DeviceKind::Phone => rsx! {
                            Smartphone { size: 14 }
                        },
                        DeviceKind::Tablet => rsx! {
                            Tablet { size: 14 }
                        },
                        DeviceKind::Computer => rsx! {
                            Laptop { size: 14 }
                        },
                    }
                }
                span { class: "tw:min-w-0 tw:truncate",
                    "{summary} · signed in {time_ago(now_secs, session.created_at)}"
                }
            }
            span { class: ACTION_CLASS,
                if session.current {
                    span { class: "{BADGE_CLASS} tw:border-status-good-border tw:bg-status-good-bg tw:text-status-good-foreground",
                        "current"
                    }
                } else if let Some(on_revoke) = on_revoke {
                    button {
                        class: LINK_BUTTON_CLASS,
                        r#type: "button",
                        title: "End this session",
                        onclick: move |_| on_revoke.call(id.clone()),
                        span { class: "tw:text-[11.5px] tw:font-bold", "Sign out" }
                    }
                }
            }
        }
    }
}

/// The signed-out / boot / unreachable face of `/account`.
///
/// Not a 404 and not an error: `/account` is a real address someone can be
/// handed or can bookmark, and the honest answer to "who are you" from a
/// browser with no session is an invitation.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn AccountSignInCard(
    /// How this deployment signs people in. `None` ⇒ the invitation without
    /// a door (unreachable service, or a story with no fixtures) — better
    /// than a button that goes nowhere.
    #[props(default)]
    options: Option<LoginOptionsInfo>,
    /// Where to come back to after the provider (always `/account` live).
    #[props(default = "/account".to_string())]
    next: String,
    /// `whoami` is still in flight: hold the card's shape and shimmer.
    #[props(default = false)]
    pending: bool,
) -> Element {
    rsx! {
        div { class: "tw:mx-auto tw:grid tw:w-full tw:max-w-[380px] tw:content-start tw:gap-3 tw:pt-10",
            div { class: "tw:grid tw:gap-1 tw:text-center",
                h1 { class: "tw:m-0 tw:text-[15px] tw:font-extrabold tw:text-strong-foreground",
                    "Sign in to see your account"
                }
                p { class: "tw:m-0 tw:text-[11.5px] tw:text-dim-foreground",
                    "Your profile, your sessions, and the projects you publish live on your LightPlayer account."
                }
            }
            div { class: CARD_CLASS,
                if pending {
                    div { class: "tw:grid tw:gap-2 tw:p-3", aria_hidden: "true",
                        span { class: "tw:h-8 tw:animate-pulse tw:rounded-sm tw:bg-card-muted" }
                        span { class: "tw:h-3 tw:w-2/3 tw:animate-pulse tw:rounded-full tw:bg-card-muted" }
                    }
                } else if let Some(options) = options {
                    // The chrome's §4 chooser body, reused whole — the same
                    // rows, minus its own title, because the heading above
                    // already made the ask.
                    SignInPanel { options, next, header: false }
                } else {
                    p { class: "tw:m-0 tw:p-3 tw:text-center tw:text-[11.5px] tw:text-dim-foreground",
                        "Sign-in is unavailable right now."
                    }
                }
            }
        }
    }
}

/// The edit a Save would send, or `None` when nothing differs from the
/// loaded record — which is also the dirty test the Save button asks.
///
/// Blank and absent are the same thing here (a cleared box clears the
/// field), so a record with no family name and an empty box is clean.
pub fn name_edit(me: &MeInfo, given: &str, family: &str) -> Option<UpdateMe> {
    let edited = UpdateMe {
        given_name: name_field(given),
        family_name: name_field(family),
    };
    let loaded = UpdateMe {
        given_name: name_field(me.given_name.as_deref().unwrap_or_default()),
        family_name: name_field(me.family_name.as_deref().unwrap_or_default()),
    };
    (edited != loaded).then_some(edited)
}

/// A name box's value as the vocabulary carries it: trimmed, and `None`
/// when there is nothing left.
fn name_field(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// What the line beside Save says.
fn save_note(status: SaveStatus) -> &'static str {
    match status {
        SaveStatus::Idle => "Names start from your provider; edits stay on LightPlayer.",
        SaveStatus::Saving => "Saving…",
        SaveStatus::Saved => "Saved.",
        SaveStatus::Failed => "Could not save — your edit is still here; try again.",
    }
}

/// The sessions group's footer line.
///
/// The lifetime is read off the current session (`expires_at - created_at`)
/// rather than assumed: the TTL is the deployment's configuration, and a
/// self-hosted service that sets its own must not be described by ours.
fn session_lifetime_note(sessions: &SessionsPane) -> String {
    let days = match sessions {
        SessionsPane::Ready(list) => list
            .iter()
            .find(|session| session.current)
            .map(|session| ((session.expires_at - session.created_at) / 86_400.0).round())
            .filter(|days| *days >= 1.0),
        _ => None,
    };
    match days {
        Some(days) => format!("{days}-day sessions."),
        None => "Includes this browser.".to_string(),
    }
}

/// `Aug 6, 2026` from f64 epoch seconds (UTC).
///
/// Howard Hinnant's civil-from-days, the same arithmetic the studio
/// controller's slug stamp uses — no date crate, and no dependency on the
/// browser's locale machinery for a string this small.
pub fn short_date(epoch_secs: f64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = (epoch_secs as i64).div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let name = MONTHS.get((month - 1) as usize).copied().unwrap_or("???");
    format!("{name} {day}, {year}")
}

/// Which glyph a session's row wears.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeviceKind {
    Computer,
    Phone,
    Tablet,
}

/// The device family a user-agent string admits to.
fn device_kind(agent: &str) -> DeviceKind {
    if agent.contains("iPad") || agent.contains("Tablet") {
        DeviceKind::Tablet
    } else if agent.contains("iPhone") || agent.contains("Mobile") || agent.contains("Android") {
        DeviceKind::Phone
    } else {
        DeviceKind::Computer
    }
}

/// "Chrome · Android" from a user-agent string.
///
/// A handful of substring checks and nothing more: user-agent strings are a
/// pile of historical lies (every browser claims to be Mozilla, Edge claims
/// to be Chrome), the answer is only ever a recall clue — "was that my
/// phone?" — and the raw string rides along as the row's tooltip for the
/// cases this gets wrong. A parsing crate for a two-word hint would be a
/// dependency, a bundle, and a maintenance stream we do not need.
///
/// Order matters: the impostors are checked before the browsers they claim
/// to be, and Android before the Linux it is built on.
pub fn user_agent_summary(agent: &str) -> String {
    if agent.trim().is_empty() {
        return "Unknown browser".to_string();
    }
    let browser = if agent.contains("Edg/") || agent.contains("Edge/") {
        Some("Edge")
    } else if agent.contains("OPR/") || agent.contains("Opera") {
        Some("Opera")
    } else if agent.contains("Firefox/") {
        Some("Firefox")
    } else if agent.contains("Chrome/") || agent.contains("Chromium/") {
        Some("Chrome")
    } else if agent.contains("Safari/") {
        Some("Safari")
    } else {
        None
    };
    let os = if agent.contains("Android") {
        Some("Android")
    } else if agent.contains("iPhone") || agent.contains("iPad") || agent.contains("iOS") {
        Some("iOS")
    } else if agent.contains("Mac OS X") || agent.contains("Macintosh") {
        Some("macOS")
    } else if agent.contains("Windows") {
        Some("Windows")
    } else if agent.contains("CrOS") {
        Some("ChromeOS")
    } else if agent.contains("Linux") || agent.contains("X11") {
        Some("Linux")
    } else {
        None
    };
    match (browser, os) {
        (Some(browser), Some(os)) => format!("{browser} · {os}"),
        (Some(browser), None) => browser.to_string(),
        (None, Some(os)) => os.to_string(),
        (None, None) => "Unknown browser".to_string(),
    }
}

/// The page column: the spike's ~620px settings measure, centred, with the
/// groups' own rhythm between them.
const PAGE_CLASS: &str =
    "tw:mx-auto tw:grid tw:w-full tw:max-w-[640px] tw:content-start tw:gap-6 tw:pt-1";
/// A group's uppercase title.
const GROUP_TITLE_CLASS: &str = "tw:m-0 tw:text-[11px] tw:font-extrabold tw:uppercase tw:tracking-[0.1em] tw:text-subtle-foreground";
/// The bordered card the rows divide.
const ROWS_CLASS: &str = "tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card";
/// One settings row.
const ROW_CLASS: &str = "tw:flex tw:min-h-[46px] tw:items-center tw:gap-3.5 tw:border-b tw:border-border-subtle tw:px-3.5 tw:py-2.5 tw:last:border-b-0";
/// The row's label column.
const KEY_CLASS: &str = "tw:w-[122px] tw:flex-none tw:text-[11.5px] tw:font-bold tw:text-subtle-foreground tw:max-[560px]:w-[84px]";
/// The row's value column. It carries the type for its whole subtree on
/// purpose: `style.css` resets `input { font: inherit }` UNLAYERED, so a
/// name box takes its font from this span and ignores font utilities of its
/// own — and the rest of the rows then match it for free.
const VALUE_CLASS: &str = "tw:min-w-0 tw:flex-1 tw:text-[12.5px] tw:font-semibold";
/// The same column, said quietly: a fact rather than an answer you gave.
const VALUE_DIM_CLASS: &str =
    "tw:min-w-0 tw:flex-1 tw:text-[12.5px] tw:font-medium tw:text-subtle-foreground";
/// The row's trailing control/badge column.
const ACTION_CLASS: &str = "tw:flex tw:flex-none tw:items-center";
/// A name box.
const INPUT_CLASS: &str = "tw:h-7 tw:w-full tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-muted tw:px-2 tw:text-strong-foreground tw:disabled:text-dim-foreground";
/// A quiet in-row action.
const LINK_BUTTON_CLASS: &str = "tw:cursor-pointer tw:rounded-xs tw:border-0 tw:bg-transparent tw:px-1.5 tw:py-1 tw:text-subtle-foreground tw:transition-colors tw:hover:bg-background-wash tw:hover:text-accent";
/// The same, in the refusal tone.
const DANGER_LINK_CLASS: &str = "tw:cursor-pointer tw:rounded-xs tw:border-0 tw:bg-transparent tw:px-1.5 tw:py-1 tw:text-status-error-foreground tw:transition-colors tw:hover:bg-status-error-bg";
/// The mono pill grammar (provider label, `current`).
const BADGE_CLASS: &str = "tw:inline-flex tw:items-center tw:gap-1.5 tw:rounded-pill tw:border tw:border-border tw:bg-card-muted tw:px-2 tw:py-0.5 tw:font-mono tw:text-[9.5px] tw:font-bold tw:uppercase tw:tracking-[0.04em] tw:text-subtle-foreground";
/// The signed-out card's box (the chrome chooser's popup, at rest).
const CARD_CLASS: &str = "tw:overflow-hidden tw:rounded-md tw:border tw:border-status-neutral-border tw:bg-card tw:text-sm tw:text-muted-foreground";

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_history::{PrefixedUid, UidPrefix};

    fn me(given: Option<&str>, family: Option<&str>) -> MeInfo {
        MeInfo {
            uid: PrefixedUid::mint(UidPrefix::User, &[1u8; 16]),
            email: "yona@example.com".to_string(),
            display_name: "Yona Appletree".to_string(),
            given_name: given.map(str::to_string),
            family_name: family.map(str::to_string),
            picture_url: None,
            provider_label: "Google".to_string(),
            created_at: 1_754_400_000.0,
        }
    }

    fn session(id: &str, current: bool, created_at: f64) -> SessionInfo {
        SessionInfo {
            id: id.to_string(),
            created_at,
            expires_at: created_at + 30.0 * 86_400.0,
            user_agent: None,
            current,
        }
    }

    #[test]
    fn an_untouched_record_is_not_dirty() {
        let me = me(Some("Yona"), Some("Appletree"));
        assert_eq!(name_edit(&me, "Yona", "Appletree"), None);
    }

    /// Surrounding whitespace is not an edit — a stray space would
    /// otherwise light Save and then save nothing visible.
    #[test]
    fn whitespace_is_not_an_edit() {
        let me = me(Some("Yona"), Some("Appletree"));
        assert_eq!(name_edit(&me, "  Yona ", "Appletree  "), None);
    }

    /// A mononym: an empty box and an absent field are the same state, so
    /// Cher's blank family box is clean.
    #[test]
    fn blank_and_absent_are_the_same_field() {
        let me = me(Some("Cher"), None);
        assert_eq!(name_edit(&me, "Cher", ""), None);
        assert_eq!(name_edit(&me, "Cher", "   "), None);
        assert_eq!(
            name_edit(&me, "Cher", "Sarkisian"),
            Some(UpdateMe {
                given_name: Some("Cher".to_string()),
                family_name: Some("Sarkisian".to_string()),
            })
        );
    }

    /// Clearing a box clears the field, and the edit says so.
    #[test]
    fn clearing_a_box_sends_none() {
        let me = me(Some("Yona"), Some("Appletree"));
        assert_eq!(
            name_edit(&me, "Yona", ""),
            Some(UpdateMe {
                given_name: Some("Yona".to_string()),
                family_name: None,
            })
        );
    }

    /// Family-first is an ordering, not a parse: both boxes travel exactly
    /// as typed.
    #[test]
    fn a_family_first_name_travels_as_typed() {
        let me = me(Some("山田"), Some("太郎"));
        assert_eq!(name_edit(&me, "山田", "太郎"), None);
        assert_eq!(
            name_edit(&me, "山田", "花子"),
            Some(UpdateMe {
                given_name: Some("山田".to_string()),
                family_name: Some("花子".to_string()),
            })
        );
    }

    #[test]
    fn user_agents_read_as_a_browser_and_an_os() {
        assert_eq!(
            user_agent_summary(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
                 (KHTML, like Gecko) Version/17.4 Safari/605.1.15"
            ),
            "Safari · macOS"
        );
        assert_eq!(
            user_agent_summary(
                "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) \
                 Chrome/124.0.0.0 Mobile Safari/537.36"
            ),
            "Chrome · Android"
        );
        assert_eq!(
            user_agent_summary(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) \
                 Chrome/124.0.0.0 Safari/537.36 Edg/124.0.0.0"
            ),
            "Edge · Windows"
        );
        assert_eq!(
            user_agent_summary(
                "Mozilla/5.0 (X11; Linux x86_64; rv:126.0) Gecko/20100101 Firefox/126.0"
            ),
            "Firefox · Linux"
        );
    }

    /// The edge that has to be right: an unstamped session (the edge did
    /// not capture one, or a pre-P2 row) still renders a row.
    #[test]
    fn an_unknown_agent_still_says_something() {
        assert_eq!(user_agent_summary(""), "Unknown browser");
        assert_eq!(user_agent_summary("   "), "Unknown browser");
        assert_eq!(user_agent_summary("curl/8.4.0"), "Unknown browser");
    }

    #[test]
    fn device_kinds_pick_the_glyph() {
        assert_eq!(
            device_kind("Mozilla/5.0 (iPhone; CPU iPhone OS 17_4 like Mac OS X)"),
            DeviceKind::Phone
        );
        assert_eq!(
            device_kind("Mozilla/5.0 (iPad; CPU OS 17_4 like Mac OS X)"),
            DeviceKind::Tablet
        );
        assert_eq!(
            device_kind("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)"),
            DeviceKind::Computer
        );
        assert_eq!(device_kind(""), DeviceKind::Computer);
    }

    #[test]
    fn dates_read_as_a_person_would_write_them() {
        // 2026-08-06T00:00:00Z
        assert_eq!(short_date(1_785_974_400.0), "Aug 6, 2026");
        assert_eq!(short_date(0.0), "Jan 1, 1970");
        // A leap day, because the arithmetic is the whole point.
        assert_eq!(short_date(1_709_164_800.0), "Feb 29, 2024");
    }

    /// The lifetime line is read off the current session, never assumed.
    #[test]
    fn the_lifetime_note_comes_from_the_current_session() {
        let ready = SessionsPane::Ready(vec![
            session("a", true, 1_754_400_000.0),
            session("b", false, 1_754_000_000.0),
        ]);
        assert_eq!(session_lifetime_note(&ready), "30-day sessions.");
        // No current session to read (or no list at all): say the thing
        // that is true regardless.
        assert_eq!(
            session_lifetime_note(&SessionsPane::Unavailable),
            "Includes this browser."
        );
        assert_eq!(
            session_lifetime_note(&SessionsPane::Ready(vec![session(
                "b",
                false,
                1_754_000_000.0
            )])),
            "Includes this browser."
        );
    }

    /// "Sign out everywhere" revokes the others; this browser's cookie is
    /// dropped by the logout that follows, not by a revoke.
    #[test]
    fn sign_out_everywhere_lists_only_the_other_sessions() {
        let pane = SessionsPane::Ready(vec![
            session("a", true, 1.0),
            session("b", false, 2.0),
            session("c", false, 3.0),
        ]);
        assert_eq!(pane.others(), vec!["b".to_string(), "c".to_string()]);
        assert!(SessionsPane::Loading.others().is_empty());
        assert!(SessionsPane::Unavailable.others().is_empty());
    }
}
