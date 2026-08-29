//! The Studio web shell: Dioxus wiring over the core `StudioActor`.
//!
//! All update logic (the pull loop, command queue, preemption, timeouts,
//! backoff, log cap, change-gating) lives in `lpa-client` / `lpa-studio-core`
//! after M7. This module keeps only browser concerns: install the global
//! `log::` sink and the JS-console mirror hook, spawn the actor, drive a
//! `Signal<UiStudioView>` from its change-gated view channel, run a timer that
//! enqueues `RefreshTick` commands at the core-owned cadence, forward UI
//! actions as `Action` commands, and render.
//!
//! # JS-console mirroring (P4)
//!
//! The controller's `on_entry` hook — installed here before the actor spawns
//! — is the **single** mirroring point: every entry entering the core log
//! ring (hand-built drafts, batch-recorded producer drafts, and drained
//! `log::` records) reaches the browser console exactly once, independent of
//! the console pane's display filter. The old view-diff mirror here and the
//! raw-serial-line mirror in `browser_serial_client_io.rs` are gone.

use core::cell::{Cell, RefCell};
use core::time::Duration;
use std::collections::BTreeSet;
use std::rc::Rc;

use crate::app::StudioShell;
use crate::app::layout::LocalStoreBanner;
use crate::app::layout::{
    ChromeModeToggle, ChromeProjectMenu, ChromeSessionControl, CloudAccountControl, SiteChrome,
    SiteSection, StudioSettingsPopover, VersionBadge,
};
use crate::app::project::ProjectDetailContent;
use crate::app::share::{
    ProjectShareControl, VisitorBannerHost, VisitorShareSlot, archive_project, use_visitor_session,
};
use crate::app::workbench;
use crate::base::{ToastHost, use_toast_provider};
use crate::cloud::SharedOpenState;
use crate::local_store::{self, LocalStoreStatus};
use crate::router::{self, StudioRoute};
use crate::unsaved_gate;
use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use lpa_studio_core::app::studio::studio_view_channel::CommandSender;
use lpa_studio_core::{
    DeviceTimers, HOME_NODE_ID, HomeOp, ProjectController, ProjectOp, STUDIO_LOG_SINK,
    SettingsCommand, StudioActor, StudioCommand, StudioController, UiAction,
    UiChromeSessionControl, UiLogEntry, UiLogLevel, UiStudioView, has_unsaved_work,
};
use lpc_cloud_api::share_link;
use lpc_history::PrefixedUid;

const STYLE: &str = include_str!("style.css");

/// The command surface the render body keeps: enqueue commands and read the
/// core-owned next-tick delay (cadence + backoff).
#[derive(Clone)]
struct StudioBridge {
    tx: CommandSender,
    delay: Rc<Cell<Duration>>,
}

#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn App() -> Element {
    #[cfg(feature = "stories")]
    if crate::stories::story_book::should_show_story_book() {
        return rsx! {
            style { "{STYLE}" }
            document::Stylesheet { href: asset!("/assets/tailwind.css") }
            crate::stories::story_book::StoryBook {}
        };
    }

    // Dev-only measurement page (GPU-preview discovery M1); never linked
    // from product navigation and absent from non-stories builds.
    #[cfg(all(feature = "stories", target_arch = "wasm32"))]
    if crate::exploration::preview_lab::should_show_preview_lab() {
        return rsx! {
            style { "{STYLE}" }
            document::Stylesheet { href: asset!("/assets/tailwind.css") }
            crate::exploration::preview_lab::PreviewLabPage {}
        };
    }

    // The board display-def editor: a standalone page, like the story
    // book — an early return before any hooks (route changes into/out of
    // it hard-reload — see the route listener below).
    if matches!(router::current_route(), StudioRoute::BoardEditor) {
        return rsx! {
            style { "{STYLE}" }
            document::Stylesheet { href: asset!("/assets/tailwind.css") }
            lpa_board_editor::BoardEditorPage {}
        };
    }

    // NOTE: Boards and Docs deliberately have NO early return — they are
    // sections of the running app, not standalone pages. An early return
    // here would run before the hooks below, which is why the surfaces
    // that do it can only be entered by a page load. See the render body.

    // The engine wasm every worker boots from, fetched once now rather than
    // N times after the first click (see `preload_engine_assets`). First
    // hook of the running app on purpose: the download should already be in
    // flight while the rest of the page wires itself up.
    #[cfg(target_arch = "wasm32")]
    use_hook(preload_engine_assets);

    // The shell loader's dismissal (index.html `__lpShell`): the first
    // effect after the first committed render, i.e. the moment real chrome
    // exists to look at. An effect, not a hook — a hook runs before the DOM
    // insert and would drop the overlay onto a still-empty page.
    #[cfg(target_arch = "wasm32")]
    use_effect(dismiss_shell_loader);

    let mut view = use_signal(UiStudioView::empty);
    // The OpenRouter connect return leg (`?code=…`): consumed synchronously
    // BEFORE the router reads the URL — it scrubs the query and restores the
    // pre-redirect path. The exchange itself runs async once the actor is up.
    #[cfg(target_arch = "wasm32")]
    let openrouter_callback = use_hook(|| {
        Rc::new(RefCell::new(
            crate::openrouter_oauth::take_pending_callback(),
        ))
    });
    // Transient connect-flow error, rendered by the settings section and the
    // agent empty state (a failed exchange must not die silently).
    #[cfg_attr(
        not(target_arch = "wasm32"),
        allow(unused_variables, reason = "only the wasm exchange task writes it")
    )]
    let openrouter_error = use_context_provider(|| Signal::new(None::<String>));
    // Who the cloud service says we are: one `whoami` per page load, in
    // context as a `Signal<CloudSession>` alongside a refresh handle. Nothing
    // renders it yet (P5/P6 do); an unreachable service leaves it quiet.
    crate::cloud::use_cloud_session_provider();
    // The one transient-confirmation slot (base/toast.rs). Provided here so
    // a line survives the surface that raised it: archiving navigates Home
    // and unmounts the menu row that asked for it, and the "Archived —
    // Restore from the Projects page." line still has to land.
    let toasts = use_toast_provider();
    // The route: parsed from the URL at boot, canonicalized once, then
    // kept in sync bidirectionally — the view loop below mirrors the LENS
    // into the URL (SDI: the URL is the focused document), and the
    // browser-navigation listener dispatches actions for back/forward and
    // in-app link clicks.
    //
    // `/p/<slug>-prjx` is BOTH the owner's editor address and the link a
    // visitor was handed (identity vision D1/D9) — which one it is depends
    // on a fact only the mounted library holds, so the route is resolved
    // against the library roster below (`library_uids`) rather than at
    // parse time.
    let boot_route = use_hook(router::boot_route);
    let shared_project = use_context_provider(|| Signal::new(router::PendingSharedProject(None)));
    let mut route = use_signal({
        let boot_route = boot_route.clone();
        move || boot_route
    });
    use_hook(move || {
        router::replace(&route.peek().clone());
    });
    // The library's `prj…` uids, latched from the last view whose library
    // had actually MOUNTED. `None` means the library has not spoken yet:
    // an empty roster from an unmounted store is not an answer, and
    // answering "we don't have it" from one would send an owner reloading
    // their own project down the visitor path.
    let library_uids = use_hook(|| Rc::new(RefCell::new(None::<BTreeSet<String>>)));
    // A `/p/<uid>` route waiting for that roster (boot, before the store
    // has attached). Resolved by the view loop on the first mounted
    // library; navigation mid-session answers straight from the latch.
    let pending_project_route = use_hook(|| Rc::new(RefCell::new(None::<PrefixedUid>)));
    use_hook({
        let waiting = Rc::clone(&pending_project_route);
        let boot_route = boot_route.clone();
        move || {
            if let StudioRoute::Project { uid, .. } = boot_route {
                *waiting.borrow_mut() = Some(uid);
            }
        }
    });
    // What the view currently shows, for the navigation listener: the
    // open project's (uid, slug), the tab's one runtime session, and the
    // route the lens binds. `saw_opening` guards the go-home fallback (the
    // boot-time home flash must not rewrite the URL that requested a
    // startup reopen).
    let open_ids_now = use_hook(|| Rc::new(RefCell::new(None::<(String, String)>)));
    // THE session (single-session web policy): navigation is studio OR
    // site, so leaving is a teardown, and the listener needs the
    // session's kind, name, teardown target and in-flight operation to
    // carry it out. This is the same bridge the editor-open flag used to
    // be — the listener runs outside the render body, so the facts it
    // judges by are latched here by the view loop rather than read from
    // the view signal.
    let session_now = use_hook(|| Rc::new(RefCell::new(None::<UiChromeSessionControl>)));
    // A nav-initiated teardown the actor hasn't finished yet. The
    // view→route reconciliation below must not rewrite the URL back to
    // the lens while it is set: the user CHOSE the site route, and the
    // session the teardown is ending is still in the snapshot the loop is
    // looking at.
    let leaving_session = use_hook(|| Rc::new(Cell::new(false)));
    let bound_route_now = use_hook(|| Rc::new(RefCell::new(None::<StudioRoute>)));
    let saw_opening = use_hook(|| Rc::new(Cell::new(false)));
    // A route-driven open we dispatched (startup / back-forward / hash nav)
    // that the actor hasn't started yet. While set, stale home views must
    // not trip the "open ended" fallback — the race: a queued RefreshTick's
    // home view can land between the navigation and the action starting.
    let pending_route_open = use_hook(|| Rc::new(Cell::new(false)));
    // Armed whenever the open project holds unsaved persisted edits; read
    // by the `beforeunload` listener installed below.
    let unsaved = use_hook(|| Rc::new(Cell::new(false)));
    let _unsaved_gate = use_hook({
        let flag = Rc::clone(&unsaved);
        move || unsaved_gate::install_unsaved_gate(flag)
    });

    // Install the global `log::` sink and the JS-console mirror hook, then
    // spawn the actor once and drive the view signal from its change-gated
    // channel.
    let loop_open_ids = Rc::clone(&open_ids_now);
    let loop_session = Rc::clone(&session_now);
    let loop_leaving = Rc::clone(&leaving_session);
    let loop_bound_route = Rc::clone(&bound_route_now);
    let loop_saw_opening = Rc::clone(&saw_opening);
    let loop_pending_route_open = Rc::clone(&pending_route_open);
    let loop_unsaved = Rc::clone(&unsaved);
    let loop_library_uids = Rc::clone(&library_uids);
    let loop_pending_project = Rc::clone(&pending_project_route);
    // The transient session's uid from the last view — the fork-at-save
    // toast (D7) fires on the transient→owned transition: same uid, marker
    // gone. Navigating away clears the open uid too, so a torn-down
    // session never toasts.
    let loop_last_transient = use_hook(|| Rc::new(RefCell::new(None::<String>)));
    let mut loop_toasts = toasts;
    let bridge = use_hook(move || {
        install_log_sink();
        let mut controller = StudioController::new(now_secs);
        controller.set_on_entry(log_to_js_console);
        // Device event trace (M0): persist lifecycle records across
        // refreshes and stream to a capture sink when the URL asks.
        crate::device_events_io::install(&mut controller);
        // Device-session deadlines (connect / readiness / request-idle /
        // request-total) run on browser timers; without this the core
        // default fires every deadline immediately.
        controller.set_device_timers(make_device_timers());
        // Crypto randomness for identity minting (`dev` uids). Host
        // builds keep the core's clock-derived fallback.
        #[cfg(target_arch = "wasm32")]
        controller.set_random(crate::library_host_opfs::random_bytes);
        // The LOCAL slug stamp the library dates packages with — the same
        // one the setup flow derives a device name's date from, so a board
        // named at provision and the project generated beside it agree
        // about what day it is.
        #[cfg(target_arch = "wasm32")]
        controller.set_local_stamp(crate::library_host_opfs::local_slug_stamp);
        // Layered settings (P4): the persisted user layer loads
        // synchronously before the actor spawns (settings are effective
        // before panes render), and user mutations write back through the
        // hook. The host layer (dev-settings.json) is fetched below.
        if let Some(json) = crate::settings_io::load_user_settings_json() {
            controller.load_user_settings_json(&json);
        }
        controller.set_on_user_settings(crate::settings_io::store_user_settings_json);
        // Node copy produces envelope text in core and writes it here
        // (core never touches `navigator.clipboard`).
        controller.set_on_copy_text(crate::clipboard::write_text);
        // Shader agent (P5/P8): run futures ride the browser's
        // single-threaded executor; the provider streams SSE over the
        // browser fetch transport — Anthropic or OpenAI-compatible per the
        // resolved settings. Both are wasm-only — host builds of this
        // crate only run unit tests and never spawn the actor.
        #[cfg(target_arch = "wasm32")]
        {
            use lpa_studio_core::AgentProviderConfig;
            controller.set_agent_spawner(wasm_bindgen_futures::spawn_local);
            controller.set_agent_provider_factory(|config| match config {
                AgentProviderConfig::Anthropic(config) => {
                    Box::new(lpa_agent::AnthropicProvider::new(
                        config.clone(),
                        lpa_agent::provider::WebFetchTransport,
                    ))
                }
                AgentProviderConfig::OpenAiCompat(config) => {
                    Box::new(lpa_agent::OpenAiCompatProvider::new(
                        config.clone(),
                        lpa_agent::provider::WebFetchTransport,
                    ))
                }
            });
            // Model discovery (P8): the settings dropdown's `/models`
            // listing over the same fetch transport, spawned through the
            // agent seam and reporting back as `ModelsLoaded`.
            controller.set_agent_models_fetcher(|config| match config {
                AgentProviderConfig::Anthropic(config) => {
                    let config = config.clone();
                    Box::pin(async move {
                        lpa_agent::list_anthropic_models(
                            &config,
                            &lpa_agent::provider::WebFetchTransport,
                        )
                        .await
                    })
                }
                AgentProviderConfig::OpenAiCompat(config) => {
                    let config = config.clone();
                    Box::pin(async move {
                        lpa_agent::list_openai_compat_models(
                            &config,
                            &lpa_agent::provider::WebFetchTransport,
                        )
                        .await
                    })
                }
            });
        }
        let (actor, handle) = StudioActor::new(controller, make_pull_timer);
        let mut view_rx = handle.view;
        let loop_tx = handle.tx.clone();
        spawn(async move {
            while let Some(next) = view_rx.recv().await {
                *loop_open_ids.borrow_mut() = next
                    .open_project_uid
                    .clone()
                    .zip(next.open_project_name.clone());
                // The fork-at-save moment (examples vision D7): the session
                // that was a transient view is suddenly an ordinary owned
                // one under the same uid — the explicit save installed it.
                // The URL heal rides the lens reconciliation below; this is
                // the one-line confirmation.
                {
                    let was_transient = loop_last_transient.borrow().clone();
                    if let Some(uid) = was_transient
                        && next.open_project_transient.is_none()
                        && next.open_project_uid.as_deref() == Some(uid.as_str())
                    {
                        loop_toasts.say(crate::app::share::visitor_session::SAVED_YOURS_LINE);
                    }
                    *loop_last_transient.borrow_mut() = next
                        .open_project_transient
                        .as_ref()
                        .and(next.open_project_uid.clone());
                }
                // Latch the library roster and answer any `/p/<uid>` route
                // that has been waiting for it (the boot case — navigation
                // mid-session reads the same latch synchronously).
                if let Some(home) = next.home.as_ref()
                    && home.library_available
                {
                    *loop_library_uids.borrow_mut() =
                        Some(home.projects.iter().map(|card| card.uid.clone()).collect());
                }
                let waiting = *loop_pending_project.borrow();
                if let Some(uid) = waiting
                    && resolve_project_route(
                        uid,
                        &loop_library_uids,
                        &loop_tx,
                        &loop_pending_route_open,
                        shared_project,
                        route,
                    )
                {
                    *loop_pending_project.borrow_mut() = None;
                }
                // The tab's one session, for the navigation listener's
                // studio-or-site policy.
                *loop_session.borrow_mut() = next.session.clone();
                // The editor is showing exactly when the view built the
                // pane layout (device-opened projects carry no library
                // uid, so pane presence — not project identity — is the
                // gate).
                let editor_showing = !next.panes.is_empty();
                // The nav teardown has landed the moment the editor it
                // was tearing down is gone; from here the reconciliation
                // below is free to speak about the URL again.
                if !editor_showing {
                    loop_leaving.set(false);
                }
                let bound = editor_showing.then(|| router::lens_route(&next)).flatten();
                // A NEW document took the lens this emission (none → some,
                // or a different session): that is a navigation the user
                // caused from wherever they are — an example opened from
                // Explore must land in the editor — so it rewrites the URL
                // even off the shell routes below.
                let bound_changed = match (&*loop_bound_route.borrow(), &bound) {
                    (None, Some(_)) => true,
                    (Some(previous), Some(next_bound)) => !previous.same_session(next_bound),
                    _ => false,
                };
                *loop_bound_route.borrow_mut() = bound.clone();
                let opening_now = next
                    .home
                    .as_ref()
                    .is_some_and(|home| home.opening.is_some());
                if opening_now || editor_showing {
                    loop_saw_opening.set(true);
                    // the dispatched open has started; fallbacks may judge it
                    loop_pending_route_open.set(false);
                }

                // view → route: the URL follows the LENS (SDI — the URL is
                // the focused document): lens on the sim + open project →
                // /p/<slug>-<uid>; lens on a device → /device/<uid>.
                let current = route.peek().clone();
                // A STEADY lens follows the URL only while a shell route
                // is what's rendered (the gallery routes, where a card
                // open resolves into the lens URL, and the lens routes,
                // where boot/slug/identity resolution lands). In any
                // other section — Home, Explore, Boards, Docs — the user
                // deliberately left the editor surface; yanking the URL
                // back would make those sections unreachable while a
                // lens is attached (seen live with `#/home` bouncing).
                // A lens CHANGE (`bound_changed`) rewrites from anywhere.
                let on_shell_route = matches!(
                    current,
                    StudioRoute::Devices
                        | StudioRoute::Projects
                        | StudioRoute::Project { .. }
                        | StudioRoute::Example { .. }
                        | StudioRoute::Device { .. }
                );
                // …and never while a nav teardown is in flight. Leaving
                // the studio is initiated BY navigation to a site route
                // (single-session policy), so between the dispatch and
                // the session actually ending there is an emission whose
                // lens still says "editor", on a route the user chose
                // deliberately — `/projects` is a shell route, so without
                // this the loop would push the user straight back into
                // the editor they just left.
                if editor_showing && !loop_leaving.get() && (on_shell_route || bound_changed) {
                    // `same_session`, not `!=`: play is a lens ZOOM on the
                    // same document, and the lens's own route always reads
                    // non-play — comparing by equality would rewrite the
                    // user straight back out of `/…/play`.
                    if let Some(target) = bound
                        && !target.same_session(&current)
                    {
                        if matches!(
                            current,
                            StudioRoute::Project { .. }
                                | StudioRoute::Example { .. }
                                | StudioRoute::Device { .. }
                        ) {
                            // boot/forward resolution on a lens route
                            // (uid → slug, identity landing): same place,
                            // no duplicate entries
                            router::replace(&target);
                        } else {
                            // an open from a page (gallery card, Explore
                            // example): a real navigation, so a real
                            // history entry (back returns to that page)
                            router::navigate(&target);
                        }
                        route.set(target);
                    }
                    // an unaddressable lens (no library identity yet, or a
                    // device whose identity has not landed) keeps the URL
                    // as-is
                } else if matches!(
                    current,
                    StudioRoute::Project { .. }
                        | StudioRoute::Example { .. }
                        | StudioRoute::Device { .. }
                ) {
                    // the editor went away: home without an in-flight open
                    // (after one started) means the open ended — the URL
                    // goes back to the gallery the cards live on
                    // (`/devices`, not the `/` landing: the core is
                    // showing the gallery view, so the landing stub would
                    // be the wrong body). The boot-time home flash (nothing started
                    // yet) keeps the route so the startup re-derivation
                    // can use it; a route-dispatched open still connecting
                    // (pending) keeps it too — the gallery's connect
                    // evidence renders the window honestly in place.
                    let open_ended = next.home.is_some()
                        && !opening_now
                        && loop_saw_opening.get()
                        && !loop_pending_route_open.get();
                    if open_ended {
                        router::replace(&StudioRoute::Devices);
                        route.set(StudioRoute::Devices);
                    }
                }

                // D10 — the address bar HEALS. The canonical link for a
                // project is `/p/<slugify(display name)>-<uid>`, so once
                // the open project's name is known (and again the moment a
                // rename lands) a stale slug, a case-mangled paste and a
                // bare uid all straighten out. This is the ONE place that
                // decides what the pretty half says.
                //
                // `replaceState` only: nothing navigates, no history entry
                // is spent, and `same_session` ignores the slug — so the
                // lens sync above never reads this rewrite as a move to
                // some other document and never fights it.
                let current = route.peek().clone();
                if let StudioRoute::Project { uid, view, .. } = &current
                    && next.open_project_uid.as_deref() == Some(uid.to_string().as_str())
                {
                    let canonical = StudioRoute::Project {
                        uid: *uid,
                        slug: next
                            .open_project_name
                            .as_deref()
                            .map(share_link::slugify)
                            .filter(|slug| !slug.is_empty()),
                        view: *view,
                    };
                    if canonical != current {
                        router::replace(&canonical);
                        route.set(canonical);
                    }
                }

                // Arm/disarm the unload gate from the snapshot that is
                // about to render, so the browser prompt always matches
                // what the save panel is showing.
                loop_unsaved.set(has_unsaved_work(&next.dirty));

                view.set(next);
            }
        });
        spawn(actor.run());
        // The host settings layer: a same-origin dev-settings.json fetch
        // (absent everywhere but the dev server / a future Electron host).
        let settings_tx = handle.tx.clone();
        spawn(async move {
            if let Some(host) = crate::settings_io::fetch_dev_settings().await {
                crate::settings_io::remember_host_layer(&host);
                settings_tx.send(StudioCommand::Settings(SettingsCommand::HostLayerLoaded(
                    host,
                )));
            }
        });
        // The OpenRouter code→key exchange (return leg captured above).
        // Success writes the key and switches the provider — Connect alone
        // makes the agent ready; failure lands in the transient signal.
        #[cfg(target_arch = "wasm32")]
        if let Some(callback) = openrouter_callback.borrow_mut().take() {
            use lpa_studio_core::app::settings::AgentProvider;
            let exchange_tx = handle.tx.clone();
            let mut error = openrouter_error;
            spawn(async move {
                match crate::openrouter_oauth::exchange(callback).await {
                    Ok(key) => {
                        exchange_tx.send(StudioCommand::Settings(
                            SettingsCommand::SetAgentOpenRouterApiKey(Some(key)),
                        ));
                        exchange_tx.send(StudioCommand::Settings(
                            SettingsCommand::SetAgentProvider(Some(AgentProvider::OpenRouter)),
                        ));
                    }
                    Err(message) => error.set(Some(message)),
                }
            });
        }
        StudioBridge {
            tx: handle.tx,
            delay: handle.delay,
        }
    });

    // route → actor: back/forward, in-app link clicks and manual URL edits
    // dispatch the matching action. Programmatic navigate/replace calls fire
    // no browser event, so everything arriving here is real user navigation.
    //
    // It is also where the single-session policy's other half lives:
    // navigation is studio OR site, so an arrival anywhere but a lens
    // route ENDS the tab's session ([`nav_session_plan`]) — silently,
    // with one line, never a prompt — and an operation in flight refuses
    // the move instead.
    let nav_bridge = bridge.clone();
    let nav_open_ids = Rc::clone(&open_ids_now);
    let nav_session = Rc::clone(&session_now);
    let nav_leaving = Rc::clone(&leaving_session);
    let nav_unsaved = Rc::clone(&unsaved);
    let nav_bound_route = Rc::clone(&bound_route_now);
    let nav_pending_route_open = Rc::clone(&pending_route_open);
    let nav_library_uids = Rc::clone(&library_uids);
    let nav_pending_project = Rc::clone(&pending_project_route);
    let mut nav_toasts = toasts;
    let _route_listener = use_hook(move || {
        router::install_route_listener(move |event| {
            // One decision, two moments: a click asks BEFORE the history
            // entry is written (a refusal there is invisible), a move
            // reports after the URL already changed (a refusal there has
            // to put it back).
            let target = match &event {
                router::NavEvent::ClickIntent(target) => target.clone(),
                router::NavEvent::Moved => router::current_route(),
            };
            let plan = nav_session_plan(nav_session.borrow().as_ref(), &target, nav_unsaved.get());
            if let router::NavEvent::ClickIntent(_) = event {
                return match plan {
                    NavSessionPlan::Refuse(line) => {
                        nav_toasts.warn(line);
                        false
                    }
                    // Everything else is settled on arrival, below: the
                    // click's own history write happens between the two
                    // calls, and acting twice would tear down twice.
                    _ => true,
                };
            }
            let new_route = target;
            let old = route.peek().clone();
            if new_route == old {
                return true;
            }
            match plan {
                NavSessionPlan::Refuse(line) => {
                    // Back/forward (or a manual edit) has already spent
                    // the entry, so nav is refused by putting the URL
                    // back. A PUSH, not a replace: after a back, pushing
                    // the entry the back consumed leaves history exactly
                    // as it was, and the forward stack it truncates is
                    // the one the back created.
                    router::navigate(&old);
                    nav_toasts.warn(line);
                    return true;
                }
                NavSessionPlan::Leave { teardown, said } => {
                    // The studio is being left: end the session. No
                    // prompt, on dirty or otherwise — the draft overlay
                    // is durable, so what is ending is the RUN, and the
                    // line below says so.
                    nav_leaving.set(true);
                    nav_bridge.tx.send(StudioCommand::Action(teardown));
                    nav_toasts.say(said);
                }
                NavSessionPlan::Keep => {
                    // This navigation supersedes any teardown still
                    // outstanding: the user is back in the studio (or
                    // never had a session), and the reconciliation below
                    // must be free to speak about the URL again even if
                    // the previous teardown never reported.
                    nav_leaving.set(false);
                }
            }
            route.set(new_route.clone());
            match &new_route {
                StudioRoute::Project { uid, .. } => {
                    // already the focused document? The uid is the whole
                    // comparison — a stale slug in the pasted link is the
                    // same project, and play is ignored on purpose:
                    // entering or leaving play must never re-open the
                    // session.
                    let uid_string = uid.to_string();
                    let already_bound = matches!(
                        &*nav_bound_route.borrow(),
                        Some(StudioRoute::Project { uid: bound, .. }) if bound == uid
                    ) || nav_open_ids
                        .borrow()
                        .as_ref()
                        .is_some_and(|(open, _)| *open == uid_string);
                    if !already_bound
                        && !resolve_project_route(
                            *uid,
                            &nav_library_uids,
                            &nav_bridge.tx,
                            &nav_pending_route_open,
                            shared_project,
                            route,
                        )
                    {
                        // the library has not mounted yet (a very early
                        // in-app link): hold the intent for the view loop
                        *nav_pending_project.borrow_mut() = Some(*uid);
                    }
                }
                StudioRoute::Example { slug, .. } => {
                    // An example's bare address resolves client-side (the
                    // parser only produces slugs the embedded table has) —
                    // no roster needed, so it dispatches straight into the
                    // same stateless open an Explore card uses (D2).
                    let already_bound = matches!(
                        &*nav_bound_route.borrow(),
                        Some(StudioRoute::Example { slug: bound, .. }) if bound == slug
                    );
                    if !already_bound
                        && let Some(example) =
                            lpa_studio_core::app::home::embedded_example_by_slug(slug)
                    {
                        nav_pending_route_open.set(true);
                        nav_bridge.tx.send(StudioCommand::Action(UiAction::from_op(
                            HOME_NODE_ID,
                            HomeOp::OpenExample {
                                id: example.id.to_string(),
                            },
                        )));
                    }
                }
                StudioRoute::Device { uid, play: _ } => {
                    let already_bound = matches!(
                        &*nav_bound_route.borrow(),
                        Some(StudioRoute::Device { uid: bound, .. }) if bound == uid
                    );
                    if !already_bound {
                        // attach the existing session for this uid, or
                        // granted-port connect (M1) + attach; the gallery's
                        // connect evidence renders the window honestly
                        nav_pending_route_open.set(true);
                        nav_bridge.tx.send(StudioCommand::Action(UiAction::from_op(
                            ProjectController::NODE_ID,
                            ProjectOp::OpenDeviceProject {
                                uid: Some(uid.clone()),
                            },
                        )));
                    }
                }
                StudioRoute::Devices
                | StudioRoute::Projects
                | StudioRoute::Home
                | StudioRoute::Explore
                | StudioRoute::Account
                | StudioRoute::Boards { .. }
                | StudioRoute::Docs { .. } => {
                    // The site sections. Setting the route signal above
                    // already re-rendered the body, and the session (if
                    // there was one) has just been told to end by the
                    // plan — there is nothing else a site route asks of
                    // the actor. Docs and Boards reached from studio mode
                    // don't even come through here: they open a new tab,
                    // which is how the session behind them keeps running
                    // (`site_chrome.rs`).
                }
                StudioRoute::Stories { .. } | StudioRoute::BoardEditor => {
                    // the story book and board editor mount
                    // on fresh page loads only (their early returns in App
                    // run before any hooks); reload to keep the hook order
                    // sound
                    router::hard_reload();
                }
            }
            // Only the click intent above can refuse; a move that has
            // already happened is never undone by a return value.
            true
        })
    });

    // The P6 visitor coordinator: who is looking at the open `/p/`
    // project, the strip banner's state, the pull loop, fork and discard.
    // Provided as context for the chrome slot and the banner host below.
    let _visitor_session = use_visitor_session(bridge.tx.clone(), view, route);

    // A `/p/` link whose uid the library does not hold (P3's pending
    // intent), consumed here: fetch → tracking copy in the library → the
    // ordinary open funnel; or the calm not-found line on Home.
    let shared_open_state = use_context_provider(|| Signal::new(SharedOpenState::Idle));
    let consume_bridge = bridge.clone();
    let consume_pending_route_open = Rc::clone(&pending_route_open);
    use_effect(move || {
        let Some(uid) = shared_project().0 else {
            return;
        };
        consume_shared_intent(
            uid,
            shared_open_state,
            shared_project,
            consume_bridge.tx.clone(),
            Rc::clone(&consume_pending_route_open),
        );
    });

    // The local project library: probed in the startup hook below (which
    // also attaches the library host and only then fires the connect
    // action).
    let mut store_status = use_signal(|| LocalStoreStatus::Initializing);

    // Startup ordering matters: the library must attach before the startup
    // open runs, or opens would go through the legacy (storeless) path on
    // first paint. The probe is awaited here; the sim still starts
    // (without persistence) if the store is unavailable.
    let startup_route = use_hook(|| route.peek().clone());
    let startup_bridge = bridge.clone();
    let startup_pending_route_open = Rc::clone(&pending_route_open);
    use_hook(move || {
        let startup_bridge = startup_bridge.clone();
        spawn(async move {
            local_store::request_persist();
            let status = local_store::init_local_store().await;
            #[cfg(target_arch = "wasm32")]
            if status == LocalStoreStatus::Ready {
                if let Some(host) = local_store::library_host() {
                    startup_bridge.tx.send(StudioCommand::AttachLibrary(
                        lpa_studio_core::app::studio::studio_command::LibraryAttachment(host),
                    ));
                }
                install_library_listeners(&startup_bridge.tx);
            }
            store_status.set(status);
            // Reload = re-derivation (D37): the pool died with the page,
            // so the route rebuilds its runtime — a device route connects
            // the granted port (M1) and attaches, with the
            // connecting/failed window rendering honestly on the gallery's
            // device card. A `/p/<uid>` route rebuilds too, but not from
            // here: it first has to learn whether this library HAS that
            // project, so the view loop resolves it against the roster.
            match &startup_route {
                StudioRoute::Device { uid, play: _ } => {
                    startup_pending_route_open.set(true);
                    startup_bridge
                        .tx
                        .send(StudioCommand::Action(UiAction::from_op(
                            ProjectController::NODE_ID,
                            ProjectOp::OpenDeviceProject {
                                uid: Some(uid.clone()),
                            },
                        )));
                }
                // A cold `/p/<slug>` load: the example is compiled in, so
                // no roster wait — open transiently the moment the library
                // host is attached (the open funnel needs its clock and
                // active slot, never its store).
                StudioRoute::Example { slug, .. } => {
                    if let Some(example) =
                        lpa_studio_core::app::home::embedded_example_by_slug(slug)
                    {
                        startup_pending_route_open.set(true);
                        startup_bridge
                            .tx
                            .send(StudioCommand::Action(UiAction::from_op(
                                HOME_NODE_ID,
                                HomeOp::OpenExample {
                                    id: example.id.to_string(),
                                },
                            )));
                    }
                }
                StudioRoute::Home
                | StudioRoute::Project { .. }
                | StudioRoute::Devices
                | StudioRoute::Projects
                | StudioRoute::Explore
                | StudioRoute::Account
                | StudioRoute::Stories { .. }
                | StudioRoute::Boards { .. }
                | StudioRoute::BoardEditor
                | StudioRoute::Docs { .. } => {}
            }
            // D32 auto-connect (M6): the load-time attach sweep — queued
            // AFTER the route dispatch, so a `/device/<uid>` reload's own
            // connect runs first and the sweep no-ops on the live session
            // (the core guard makes it idempotent). Attach + pull + show,
            // nothing else; failures land softly on card evidence.
            startup_bridge
                .tx
                .send(StudioCommand::Action(UiAction::from_op(
                    lpa_studio_core::DeviceController::NODE_ID,
                    lpa_studio_core::DeviceOp::AutoConnect,
                )));
            // Hotplug (M6): a granted port (re)appearing re-runs the
            // sweep; a departing port hastens the Gone classification
            // with a tick. Listener lifetime = page lifetime (forget).
            #[cfg(target_arch = "wasm32")]
            install_serial_hotplug(&startup_bridge.tx);
        });
    });

    let refresh_bridge = bridge.clone();
    let _refresh_task = use_future(move || {
        let refresh_bridge = refresh_bridge.clone();
        async move {
            loop {
                let delay = refresh_bridge.delay.get();
                TimeoutFuture::new(delay.as_millis() as u32).await;
                refresh_bridge.tx.send(StudioCommand::RefreshTick);
            }
        }
    });

    let action_bridge = bridge.clone();
    let action_unsaved = Rc::clone(&unsaved);
    let on_action = move |action: UiAction| {
        // The other way to lose unsaved work: opening a DIFFERENT project
        // replaces the one loaded on the sim. (Merely detaching the lens to
        // the gallery does not — the session keeps running, edits included —
        // so that path is deliberately not gated.)
        if action_unsaved.get() && unsaved_gate::action_replaces_loaded_project(&action) {
            let proceed = unsaved_gate::confirm_discarding_unsaved(
                "This project has unsaved changes. Opening another project will discard them.\n\nOpen anyway?",
            );
            if !proceed {
                return;
            }
        }
        action_bridge.tx.send(StudioCommand::Action(action));
    };

    // Settings popover gestures ride the same ordered command queue; the
    // actor applies them synchronously, like console commands.
    let settings_bridge = bridge.clone();
    let on_settings = move |command: SettingsCommand| {
        settings_bridge.tx.send(StudioCommand::Settings(command));
    };
    // The chat footer's model chip dispatches the same settings mutations
    // (SetAgentModel / RequestModels) from deep inside the node tree;
    // context spares threading a handler through every layer. Stories
    // provide no context, so the chip renders inert there.
    let chip_settings_bridge = bridge.clone();
    use_context_provider(|| {
        Callback::new(move |command: SettingsCommand| {
            chip_settings_bridge
                .tx
                .send(StudioCommand::Settings(command));
        })
    });

    // The URL's intent picks the frame: a PROJECT route whose project the
    // view hasn't reached yet renders the opening frame, not the gallery.
    // A device route never does — its connecting/failed window renders
    // honestly on the gallery's device card (the connect evidence). A
    // link to a project this library does NOT have never gets here: the
    // route resolution above lands it on Home with a pending intent.
    let current_view = view.read().clone();
    let current_route = route.read().clone();
    let opening_frame = matches!(
        current_route,
        StudioRoute::Project { .. } | StudioRoute::Example { .. }
    ) && !current_route.project_matches_view(&current_view);
    // Play mode (panel.md P12) is a zoom on the SAME session: the flag only
    // picks what the shell renders, and the toggle only rewrites the URL.
    let project_view = current_route.project_view();
    let play = current_route.is_play();
    let play_toggle = current_route
        .is_lens()
        .then(|| current_route.with_play(!play).path());
    // The workbench view tabs' targets: same-session view suffixes on the
    // current lens address, plain links like the play/patch toggles — one
    // slot per view-table row. Only the default view is addressable on a
    // device lens (no mapping address yet), so the other tabs hide there.
    let workbench_hrefs = current_route.is_lens().then(|| {
        workbench::WorkbenchHrefs::from_entries(workbench::VIEWS.iter().map(|spec| {
            let addressable = spec.view == workbench::WorkbenchView::default()
                || matches!(
                    &current_route,
                    StudioRoute::Project { .. } | StudioRoute::Example { .. }
                );
            (
                spec.view,
                addressable.then(|| current_route.with_view(spec.route_view).path()),
            )
        }))
    });
    // Workbench routes trade the scrolling-document page for a
    // full-height app frame: the docks and center scroll INTERNALLY.
    // Keyed off an actually-open editor so opening frames, galleries,
    // and bare-pane states keep the document layout.
    let editor_open = current_view
        .panes
        .iter()
        .any(|pane| matches!(&pane.body, lpa_studio_core::UiViewContent::ProjectEditor(_)));
    // Patch is a workbench view (R5), so it gets the app frame too; only
    // play still zooms out of the workbench.
    let workbench_route = current_route.is_lens() && !play && editor_open;

    // Sharing administers THE project in the address bar (D1 — the address
    // bar IS the link), so both its doors exist only on a project route.
    // Whether the pill actually draws is a further question only the
    // service can answer; see `app::share::ProjectShareControl`.
    let project_uid = match &current_route {
        StudioRoute::Project { uid, .. } => Some(*uid),
        _ => None,
    };
    // The ⋯ menu's "Sharing & access…" opens the SAME panel the pill does.
    // A popover owns its own open state, so the row bumps a request count
    // that re-keys the control and mounts it open — one bump per ask, so
    // asking again after closing reopens it.
    let mut share_request = use_signal(|| 0u32);
    let project_menu = project_uid.map(|uid| ChromeProjectMenu {
        on_share: EventHandler::new(move |()| share_request += 1),
        on_archive: EventHandler::new(move |()| {
            archive_project(uid, Some(toasts), move || {
                // We just archived the project this route addresses; the
                // link no longer resolves for anyone but its members, so
                // staying here would be a lie. Home, with a real history
                // entry — back returns to where the user was.
                let mut route = route;
                router::navigate(&StudioRoute::Home);
                route.set(StudioRoute::Home);
            });
        }),
    });

    // One shell for every section: the chrome renders at the same offset
    // whatever is below it, and switching sections swaps only the body —
    // the actor, runtime pool, and open sessions are untouched.
    // Shared by the chrome and the section body below: an EventHandler is
    // Copy, the raw closure is not.
    let on_action = EventHandler::new(on_action);
    // THE session·project control: this tab's ONE session (core's control
    // projection) paired with the SAME detail content the pane's [i]
    // renders, so device state and project state (unsaved / failed /
    // syncing) are visible on every view at every width. Presentation only
    // — zero new state; `None` off the lens routes, and `project` stays
    // `None` for a session with nothing open (the honest-empty segment).
    let session_control = current_route
        .is_lens()
        .then(|| current_view.session.clone())
        .flatten()
        .map(|session| ChromeSessionControl {
            session,
            project: current_view.panes.iter().find_map(|pane| match &pane.body {
                lpa_studio_core::UiViewContent::ProjectEditor(editor) => {
                    Some(ProjectDetailContent::new(editor, pane.status.clone()))
                }
                _ => None,
            }),
            on_action,
            initially_open: false,
        });
    // The chrome's narrow ladder keys off the control's presence (a bar
    // carrying it stops fitting sooner); the version chip's fold below
    // follows the same rung.
    let session_control_present = session_control.is_some();
    let section = match &current_route {
        // `/` is Home: no tab lights — the logo wears the underline.
        StudioRoute::Home => SiteSection::Home,
        // Explicit, not the catch-all: `/devices` must light the Devices
        // tab (a catch-all once carried it and silently stopped when lens
        // routes moved to Session — G3 finding).
        StudioRoute::Devices => SiteSection::Devices,
        StudioRoute::Projects => SiteSection::Projects,
        StudioRoute::Explore => SiteSection::Explore,
        StudioRoute::Boards { .. } => SiteSection::Boards,
        StudioRoute::Docs { .. } => SiteSection::Docs,
        // Like Session: no tab lights. The avatar in the right cluster is
        // the account page's current-place marker.
        StudioRoute::Account => SiteSection::Account,
        // Lens routes light NO tab — the header session·project control is
        // the current-place marker (single-session policy). The other
        // catch-all routes (stories, the standalone editors) never render
        // this chrome.
        _ => SiteSection::Session,
    };
    let settings = current_view.settings.clone();

    // The workbench keeps a modest desktop inset (the workbench frame draws
    // no box of its own now — see `app::workbench`), and below the fold
    // breakpoint the frame bleeds this padding back out so its summon strip
    // reads as a full-width toolbar under the site chrome; the chrome itself
    // keeps the inset.
    let main_class = if workbench_route {
        "tw:mx-auto tw:flex tw:h-dvh tw:min-h-0 tw:w-[min(1520px,100%)] tw:flex-col tw:px-3 tw:pb-2 tw:pt-2 tw:max-[820px]:px-[10px] tw:max-[820px]:pb-0 tw:max-[820px]:pt-1"
    } else {
        "tw:mx-auto tw:min-h-screen tw:w-[min(1520px,100%)] tw:px-7 tw:pb-16 tw:pt-7 tw:max-[880px]:px-[18px] tw:max-[880px]:pb-[72px] tw:max-[880px]:pt-[18px]"
    };
    rsx! {
        style { "{STYLE}" }
        document::Stylesheet { href: asset!("/assets/tailwind.css") }
        main { class: "{main_class}",
            SiteChrome {
                section,
                project_menu,
                session_control,
                // The play zoom, a plain hash link (the route listener sees
                // no new document). Patch is a workbench VIEW now (R5) —
                // the band tab carries it, so the chrome has no patch
                // toggle to fold.
                play_toggle: play_toggle
                    .map(|href| ChromeModeToggle { href, active: play }),
                tight: workbench_route,
                if let Some(uid) = project_uid {
                    // First in the right cluster, ahead of the gear and
                    // the avatar (spike §1-A). Re-keyed on the request
                    // count so the ⋯ row can mount it open, and on the
                    // uid so moving to another project re-asks the
                    // service instead of showing the last one's roster.
                    ProjectShareControl {
                        key: "{uid}-{share_request}",
                        uid,
                        initially_open: share_request() > 0,
                    }
                    // The same slot, visitor variant (P6, spike §2-D).
                    // Each door self-gates on the service's answer —
                    // members get the pill above, link-holders get this
                    // one, and never both.
                    VisitorShareSlot {}
                }
                // The build chip is an inspector, not a control: it is the
                // first utility to fold — with the crowded bar's <900 rung
                // on lens routes, with the phone rung elsewhere.
                span {
                    class: if session_control_present { "tw:hidden tw:@min-[900px]:flex" } else { "tw:hidden tw:@min-[560px]:flex" },
                    VersionBadge {}
                }
                StudioSettingsPopover { settings, on_settings }
                // Last of the chrome's children, so the account slot sits
                // exactly where the spike puts it: after the settings
                // trigger, before SiteChrome's own ⋯ button. Inert until
                // the cloud session context says otherwise. On /account
                // the slot wears the you're-here underline — no nav tab
                // lights there, so the slot is the section's marker
                // (G1 ruling 2026-08-07).
                CloudAccountControl { on_account: section == SiteSection::Account }
            }
            LocalStoreBanner { status: store_status.read().clone() }
            // The visitor strip (P6, spike §3-A): full-width under the
            // chrome, only for an open tracking copy without write.
            // Self-gating — renders nothing for members and non-project
            // routes.
            VisitorBannerHost {}
            match current_route {
                StudioRoute::Home => rsx! {
                    crate::app::HomePage {
                        on_action,
                        home: current_view.home.clone().map(|home| *home),
                    }
                },
                StudioRoute::Account => rsx! {
                    crate::app::AccountPage {}
                },
                StudioRoute::Explore => rsx! {
                    crate::app::ExplorePage {
                        home: current_view.home.clone().map(|home| *home),
                        on_action,
                    }
                },
                StudioRoute::Boards { board } => rsx! {
                    // The detected OS drives per-bridge driver warnings
                    // (plan D5) — detected here at the platform edge;
                    // lpa-boards stays platform-blind.
                    lpa_boards::BoardsCatalogPage { os: detect_host_os(), initial_board: board }
                },
                StudioRoute::Docs { page, anchor } => rsx! {
                    // The section gets the app's real dispatcher: the
                    // `open-in-studio` embed runs the same `OpenExample`
                    // flow a gallery card does, into the user's own
                    // library. Docs SIMS never come through here — they
                    // are leased controllers of their own (D2).
                    crate::app::DocsPage { page, anchor, on_studio_action: on_action }
                },
                StudioRoute::Projects => rsx! {
                    StudioShell {
                        view: current_view,
                        running: false,
                        gallery: crate::app::layout::ShellGallery::Projects,
                        opening_frame,
                        play,
                        project_view,
                        workbench_hrefs: workbench_hrefs.clone(),
                        on_action,
                    }
                },
                // Devices (`#/`) and the lens routes: the shell's default
                // gallery page is Devices.
                _ => rsx! {
                    StudioShell {
                        view: current_view,
                        running: false,
                        opening_frame,
                        play,
                        project_view,
                        workbench_hrefs: workbench_hrefs.clone(),
                        on_action,
                    }
                },
            }
            // Last, and outside every section: one line at the page's
            // bottom for acts with no other visible consequence (a link on
            // the clipboard, an access level flipped, a project archived).
            ToastHost {}
        }
    }
}

/// What a navigation does to the tab's ONE runtime session.
///
/// The whole studio-or-site policy, as a value: the route listener turns
/// it into dispatches and toasts, and the tests below read it directly.
enum NavSessionPlan {
    /// The session (if any) survives this arrival.
    Keep,
    /// Refused — an operation is in flight, and this line names it.
    Refuse(String),
    /// The studio is being left: run `teardown`, then say `said`.
    Leave { teardown: UiAction, said: String },
}

/// Decide what arriving at `target` does to `session` (ruling R8-4).
///
/// **Leaving a lens route ends the session.** With one session per tab,
/// the running sim or attached board IS the studio; a site section is
/// somewhere else, and a session nobody is looking at is a worker (or a
/// held serial port) burning down a laptop battery behind a docs page.
/// So navigation is studio OR site, and this is the seam that enforces
/// it.
///
/// **It never prompts.** The draft overlay is durable — what ends is the
/// RUN, not the work — so a dirty project is not a reason to interrogate
/// anyone (the same property `unsaved_gate::action_replaces_loaded_project`
/// establishes for the lens-detach family, which this replaces). `dirty`
/// only decides whether the line PROMISES the draft; promising one that
/// was never written would be its own small lie.
///
/// **The one refusal is an operation in flight.** A deploy or a flash
/// cannot be torn down honestly halfway, so the move is refused with the
/// operation named — the same shape as the install funnel's refusal
/// (P1's `enforce_single_session`), so the two places the policy bites
/// speak with one voice.
fn nav_session_plan(
    session: Option<&UiChromeSessionControl>,
    target: &StudioRoute,
    dirty: bool,
) -> NavSessionPlan {
    let Some(session) = session else {
        return NavSessionPlan::Keep;
    };
    // Every lens route is the studio, including the zooms (play, patch,
    // mapping) and another document entirely: moving the lens between
    // documents is the install funnel's business, and P1 already refuses
    // THAT with the operation named.
    if target.is_lens() {
        return NavSessionPlan::Keep;
    }
    // The story book and the standalone board editor reload the page
    // (their early returns in `App` run before any hooks), so the session
    // dies with the document whatever we dispatch here.
    if matches!(
        target,
        StudioRoute::Stories { .. } | StudioRoute::BoardEditor
    ) {
        return NavSessionPlan::Keep;
    }
    if let Some(operation) = session.busy.as_deref() {
        return NavSessionPlan::Refuse(format!(
            "{operation} in progress — finish or cancel it before leaving"
        ));
    }
    NavSessionPlan::Leave {
        teardown: session_teardown(session),
        said: session_stopped_line(session, dirty),
    }
}

/// The action that ends `session`: the sim's own stop verb, or the
/// board's disconnect aimed by the card key the control carries — the
/// same op the card's danger-zone row dispatches, so leaving the studio
/// and clicking Disconnect are literally the same teardown.
fn session_teardown(session: &UiChromeSessionControl) -> UiAction {
    let op = if session.sim {
        lpa_studio_core::DeviceOp::StopSimulator
    } else {
        lpa_studio_core::DeviceOp::DisconnectDevice {
            target: lpa_studio_core::DeviceTarget::card(session.key.clone()),
        }
    };
    UiAction::from_op(lpa_studio_core::DeviceController::NODE_ID, op)
}

/// The one line a teardown-by-nav leaves behind (ruled copy, R8-4).
///
/// The draft clause is conditional: with nothing unsaved there is no
/// draft to reassure anyone about, and a promise made on every stop is a
/// promise nobody reads.
fn session_stopped_line(session: &UiChromeSessionControl, dirty: bool) -> String {
    let stopped = if session.sim {
        "Simulator stopped".to_string()
    } else {
        format!("{} disconnected", session.name)
    };
    if dirty {
        format!("{stopped} — your edits are saved as a draft")
    } else {
        stopped
    }
}

/// Act on a `/p/<uid>` route, once the library roster can say whether this
/// is the user's own project or somebody else's link (identity vision
/// D1/D9 — the two cases share one address, and only the library tells
/// them apart).
///
/// A HIT opens through the same funnel a gallery card's click uses
/// (`HomeOp::OpenPackage`, keyed by uid): create or reuse the sim session
/// and push the head (D19), or re-attach when the sim already runs this
/// project — the core's open flow decides that from its own loaded-project
/// record (the D37 invariant, `studio_controller.rs`), so this must keep
/// handing it a key that record recognizes.
///
/// A MISS is a visitor: hold the uid as a pending intent, land on Home, and
/// leave the URL exactly as the sender wrote it. Nothing consumes the
/// intent this round — the fetch/offer/copy flow is a later one.
///
/// Returns whether the roster could answer at all; `false` means the
/// library has not mounted yet and the caller should hold the intent.
fn resolve_project_route(
    uid: PrefixedUid,
    library_uids: &RefCell<Option<BTreeSet<String>>>,
    tx: &CommandSender,
    pending_route_open: &Rc<Cell<bool>>,
    mut shared_project: Signal<router::PendingSharedProject>,
    mut route: Signal<StudioRoute>,
) -> bool {
    let uid_string = uid.to_string();
    let Some(in_library) = library_uids
        .borrow()
        .as_ref()
        .map(|uids| uids.contains(&uid_string))
    else {
        return false;
    };
    if in_library {
        pending_route_open.set(true);
        tx.send(StudioCommand::Action(UiAction::from_op(
            HOME_NODE_ID,
            HomeOp::OpenPackage { key: uid_string },
        )));
    } else {
        shared_project.set(router::PendingSharedProject(Some(uid)));
        route.set(StudioRoute::Home);
    }
    true
}

/// Consume one pending shared-project intent (P6): fetch the project as a
/// tracking copy into the library, then open it through the same funnel a
/// gallery card uses. A refusal lands in `state` for Home's one quiet
/// line; the URL stays exactly as the sender wrote it, so a reload
/// retries.
#[cfg(target_arch = "wasm32")]
fn consume_shared_intent(
    uid: PrefixedUid,
    mut state: Signal<SharedOpenState>,
    mut shared_project: Signal<router::PendingSharedProject>,
    tx: CommandSender,
    pending_route_open: Rc<Cell<bool>>,
) {
    state.set(SharedOpenState::Opening);
    spawn(async move {
        match crate::cloud::shared_open::open_shared_into_library(uid).await {
            Ok(summary) => {
                state.set(SharedOpenState::Idle);
                pending_route_open.set(true);
                tx.send(StudioCommand::Action(UiAction::from_op(
                    HOME_NODE_ID,
                    HomeOp::OpenPackage {
                        key: summary.uid.to_string(),
                    },
                )));
            }
            Err(failure) => state.set(failure),
        }
        // Consumed either way: a fresh navigation (or reload) re-arms it.
        shared_project.set(router::PendingSharedProject(None));
    });
}

/// Host builds never navigate; the intent simply clears.
#[cfg(not(target_arch = "wasm32"))]
fn consume_shared_intent(
    _uid: PrefixedUid,
    mut state: Signal<SharedOpenState>,
    mut shared_project: Signal<router::PendingSharedProject>,
    _tx: CommandSender,
    _pending_route_open: Rc<Cell<bool>>,
) {
    state.set(SharedOpenState::Idle);
    shared_project.set(router::PendingSharedProject(None));
}

/// The pull loop's per-request progress-deadline timer on wasm: a `setTimeout`
/// via `gloo_timers`. The actor calls this to build each pull's quiet-gap
/// deadline; native callers would pass a `sleep`-backed factory instead.
/// The OS the page runs on, for per-bridge driver guidance (plan D5).
/// User-agent sniffing is exactly the right tool here: the answer only
/// picks which driver instructions to show.
fn detect_host_os() -> lpa_boards::HostOs {
    #[cfg(target_arch = "wasm32")]
    {
        let user_agent = web_sys::window()
            .and_then(|window| window.navigator().user_agent().ok())
            .unwrap_or_default();
        if user_agent.contains("Mac") {
            lpa_boards::HostOs::MacOs
        } else if user_agent.contains("Win") {
            lpa_boards::HostOs::Windows
        } else if user_agent.contains("Linux") || user_agent.contains("X11") {
            lpa_boards::HostOs::Linux
        } else {
            lpa_boards::HostOs::Other
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        lpa_boards::HostOs::Other
    }
}

pub(crate) fn make_pull_timer(delay: Duration) -> TimeoutFuture {
    TimeoutFuture::new(delay.as_millis() as u32)
}

/// DeviceSession timers on wasm: the same `setTimeout` future, boxed for
/// the session's injected factory (the `make_pull_timer` pattern).
pub(crate) fn make_device_timers() -> DeviceTimers {
    DeviceTimers::new(|delay| Box::pin(TimeoutFuture::new(delay.as_millis() as u32)))
}

/// Warm the browser engine's assets once, at page load.
///
/// Every worker this page ever boots — the simulator's and each preview
/// pool member's — fetches the SAME multi-MB `fw_browser` wasm. Left to the
/// first boot, that download starts only once the user has already clicked,
/// and on a cold, throttled connection it is most of what a boot spends its
/// budget on.
///
/// The shell loader (index.html) usually starts the wasm's download even
/// earlier — the moment the APP wasm's bytes finish, before this app exists
/// to run anything — and `engine_cache` adopts that in-flight response
/// rather than fetching twice. This preload is the demand that makes the
/// adoption happen at page load (and the whole story, shell absent): one
/// fire-and-forget compile into the page cache, so the first click's boot
/// pays for nothing.
///
/// Fire-and-forget by design: a failure here is silent and costs nothing —
/// the boot demands the same asset again through the same cache. The URLs
/// come from `resolved_engine_urls`, the same pre-boot manifest resolution
/// the workers boot from (falling back to `BrowserWorkerOptions`'s unhashed
/// defaults when the manifest fetch in `index.html` hasn't landed or
/// failed), so there is one place to change them.
///
/// The wasm goes through `warm_engine_cache` (boot protocol v2): one
/// streaming fetch with byte progress, one `WebAssembly.compile`, and the
/// compiled `Module` is what every worker instantiates from. The glue JS is
/// still `import()`ed per worker, so for it a plain read into the HTTP
/// cache is the whole job. (Dev caveat: a `dx serve` rebuild invalidates
/// the cache entries mid-session — harmless, the next demand re-fetches.)
/// Tell the shell loader (index.html) the app has rendered: call
/// `window.__lpShell.done()` if it is there. Tolerant of its absence —
/// stories, tests, and any document predating the shell script have no
/// overlay to dismiss (and the shell's own MutationObserver on `#main`
/// backstops surfaces that never get here).
#[cfg(target_arch = "wasm32")]
fn dismiss_shell_loader() {
    use wasm_bindgen::{JsCast, JsValue};

    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(shell) = js_sys::Reflect::get(&window, &JsValue::from_str("__lpShell")) else {
        return;
    };
    if let Ok(done) = js_sys::Reflect::get(&shell, &JsValue::from_str("done"))
        && let Some(done) = done.dyn_ref::<js_sys::Function>()
    {
        let _ = done.call0(&shell);
    }
}

#[cfg(target_arch = "wasm32")]
fn preload_engine_assets() {
    use lpa_link::providers::browser_worker::{resolved_engine_urls, warm_engine_cache};

    wasm_bindgen_futures::spawn_local(async move {
        let options = resolved_engine_urls().await;
        warm_engine_cache(&options.fw_browser_wasm_path);
        let module_url = options.fw_browser_module_path;
        // The body has to be consumed for the response to land in the
        // cache — a `Response` whose stream is never read warms nothing.
        match gloo_net::http::Request::get(&module_url).send().await {
            Ok(response) => {
                if let Err(error) = response.binary().await {
                    log::debug!("engine preload: reading {module_url}: {error}");
                }
            }
            Err(error) => log::debug!("engine preload: fetching {module_url}: {error}"),
        }
    });
}

/// M6 (D32): the `navigator.serial` hotplug listeners. A `connect`
/// event (a granted port re-appearing) re-runs the auto-connect sweep;
/// a `disconnect` sends a tick so the Gone classification lands without
/// waiting for the next cadence beat.
#[cfg(target_arch = "wasm32")]
fn install_serial_hotplug(tx: &CommandSender) {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::Closure;

    let connect_tx = tx.clone();
    let on_connect = Closure::wrap(Box::new(move || {
        connect_tx.send(StudioCommand::Action(UiAction::from_op(
            lpa_studio_core::DeviceController::NODE_ID,
            lpa_studio_core::DeviceOp::AutoConnect,
        )));
    }) as Box<dyn FnMut()>);
    let tick_tx = tx.clone();
    let on_disconnect = Closure::wrap(Box::new(move || {
        tick_tx.send(StudioCommand::RefreshTick);
    }) as Box<dyn FnMut()>);
    let installed = lpa_link::providers::browser_serial_esp32::install_serial_events(
        on_connect.as_ref().unchecked_ref(),
        on_disconnect.as_ref().unchecked_ref(),
    );
    if installed {
        on_connect.forget();
        on_disconnect.forget();
    }
}

/// Wire the cross-tab library refresh triggers (M4b): a BroadcastChannel
/// message from another tab's catalog transaction / save / close, and
/// this tab becoming visible again, both enqueue a coalescable
/// `LibraryChanged`; `pagehide` best-effort-flushes open project stores.
/// Installed once at startup; the closures live for the page.
#[cfg(target_arch = "wasm32")]
fn install_library_listeners(tx: &CommandSender) {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::Closure;

    match web_sys::BroadcastChannel::new(crate::library_host_opfs::LIBRARY_CHANNEL) {
        Ok(channel) => {
            let ping_tx = tx.clone();
            let on_message = Closure::wrap(Box::new(move |_event: web_sys::MessageEvent| {
                ping_tx.send(StudioCommand::LibraryChanged);
            }) as Box<dyn FnMut(_)>);
            channel.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
            on_message.forget();
            // keep the receiving channel alive for the page lifetime
            core::mem::forget(channel);
        }
        Err(e) => log::warn!("BroadcastChannel unavailable, no cross-tab refresh: {e:?}"),
    }

    let Some(window) = web_sys::window() else {
        return;
    };
    if let Some(document) = window.document() {
        let visible_tx = tx.clone();
        let document_for_check = document.clone();
        let on_visible = Closure::wrap(Box::new(move |_event: web_sys::Event| {
            if document_for_check.visibility_state() == web_sys::VisibilityState::Visible {
                visible_tx.send(StudioCommand::LibraryChanged);
            }
        }) as Box<dyn FnMut(_)>);
        if let Err(e) = document.add_event_listener_with_callback(
            "visibilitychange",
            on_visible.as_ref().unchecked_ref(),
        ) {
            log::warn!("visibilitychange listener failed: {e:?}");
        }
        on_visible.forget();
    }

    let on_pagehide = Closure::wrap(Box::new(move |_event: web_sys::Event| {
        if let Some(host) = local_store::opfs_library_host() {
            host.flush_open_projects_best_effort();
        }
    }) as Box<dyn FnMut(_)>);
    if let Err(e) =
        window.add_event_listener_with_callback("pagehide", on_pagehide.as_ref().unchecked_ref())
    {
        log::warn!("pagehide listener failed: {e:?}");
    }
    on_pagehide.forget();
}

/// The controller's log-stamping clock on wasm: seconds since the Unix epoch
/// from `Date.now()`. Core takes the closure so it stays platform-free.
#[cfg(target_arch = "wasm32")]
pub(crate) fn now_secs() -> f64 {
    js_sys::Date::now() / 1000.0
}

/// Host builds of this crate only run unit tests and never spawn the actor,
/// so the clock stub mirrors the JS-console stubs below.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn now_secs() -> f64 {
    0.0
}

/// Install the studio `log::Log` sink as the global logger, before the actor
/// spawns, so `log::` macros anywhere on the wasm side are captured and later
/// drained into the console ring by the actor.
///
/// The max level is the *capture* floor (`Info` — mirroring core's
/// `LogFilter` default), so producers below it never format or queue
/// output. The global console UI that used to move this floor retired
/// with M7′ P2; the floor is fixed until a per-device level control
/// lands. An already-installed logger is tolerated with a JS-console
/// warning, never a panic.
fn install_log_sink() {
    match log::set_logger(&STUDIO_LOG_SINK) {
        // `Info` mirrors `LogFilter::default().min_level` in core.
        Ok(()) => log::set_max_level(capture_level_for(UiLogLevel::Info)),
        Err(_) => console_warn("studio log sink not installed: a global logger is already set"),
    }
}

/// Map the console's display threshold to the global `log::` max level that
/// gates producers. The floor is inclusive: a `min_level` of `Info` captures
/// `Info` and above, dropping `Debug`/`Trace` at the macro.
fn capture_level_for(min_level: UiLogLevel) -> log::LevelFilter {
    match min_level {
        UiLogLevel::Trace => log::LevelFilter::Trace,
        UiLogLevel::Debug => log::LevelFilter::Debug,
        UiLogLevel::Info => log::LevelFilter::Info,
        UiLogLevel::Warn => log::LevelFilter::Warn,
        UiLogLevel::Error => log::LevelFilter::Error,
    }
}

/// Mirror one ring entry to the JS console (the controller `on_entry` hook).
fn log_to_js_console(log: &UiLogEntry) {
    let message = console_line(log);
    match log.level {
        UiLogLevel::Trace | UiLogLevel::Debug => console_debug(&message),
        UiLogLevel::Info => console_info(&message),
        UiLogLevel::Warn => console_warn(&message),
        UiLogLevel::Error => console_error(&message),
    }
}

/// The mirrored line, rebuilt from the structured entry: origin label plus
/// detail (module path, endpoint id, transport label) when present, then the
/// message. Severity is conveyed by the console method, not the text.
fn console_line(log: &UiLogEntry) -> String {
    match log.source.detail.as_deref() {
        Some(detail) => format!("[{}/{detail}] {}", log.source.origin.label(), log.message),
        None => format!("[{}] {}", log.source.origin.label(), log.message),
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(js_namespace = console, js_name = debug)]
    fn console_debug(message: &str);

    #[wasm_bindgen::prelude::wasm_bindgen(js_namespace = console, js_name = info)]
    fn console_info(message: &str);

    #[wasm_bindgen::prelude::wasm_bindgen(js_namespace = console, js_name = warn)]
    fn console_warn(message: &str);

    #[wasm_bindgen::prelude::wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(message: &str);
}

#[cfg(not(target_arch = "wasm32"))]
fn console_debug(_message: &str) {}

#[cfg(not(target_arch = "wasm32"))]
fn console_info(_message: &str) {}

#[cfg(not(target_arch = "wasm32"))]
fn console_warn(_message: &str) {}

#[cfg(not(target_arch = "wasm32"))]
fn console_error(_message: &str) {}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::{UiChromeSessionStatus, UiLogOrigin, UiLogSource};

    /// The tab's session, as the view loop latches it for the listener.
    fn session(sim: bool, busy: Option<&str>) -> UiChromeSessionControl {
        UiChromeSessionControl {
            key: if sim { "sim" } else { "dev7k2" }.to_string(),
            sim,
            name: if sim { "Sim" } else { "Attic strip" }.to_string(),
            board: sim.then(|| "ESP32-C6".to_string()),
            status: UiChromeSessionStatus::Run,
            busy: busy.map(str::to_string),
            stat_line: None,
        }
    }

    fn project_route() -> StudioRoute {
        StudioRoute::Project {
            uid: "prj0000000000000000".parse().expect("a project uid"),
            slug: Some("mini-dome".to_string()),
            view: router::ProjectView::Workspace,
        }
    }

    /// The plan's central claim: a site route ENDS the tab's session, and
    /// the sim's teardown is `StopSimulator` (never a detach).
    #[test]
    fn leaving_the_studio_stops_the_sim() {
        let plan = nav_session_plan(Some(&session(true, None)), &StudioRoute::Projects, false);

        let NavSessionPlan::Leave { teardown, said } = plan else {
            panic!("a site route must end the session");
        };
        assert!(matches!(
            teardown.op_as::<lpa_studio_core::DeviceOp>(),
            Some(lpa_studio_core::DeviceOp::StopSimulator)
        ));
        // Nothing unsaved: no draft to promise.
        assert_eq!(said, "Simulator stopped");
    }

    /// Every site section, not just the galleries the old detach arm
    /// covered — a session running behind a docs page is exactly the
    /// thing the single-session policy is for.
    #[test]
    fn every_site_route_ends_the_session() {
        for target in [
            StudioRoute::Home,
            StudioRoute::Devices,
            StudioRoute::Projects,
            StudioRoute::Explore,
            StudioRoute::Account,
            StudioRoute::Boards { board: None },
            StudioRoute::Docs {
                page: None,
                anchor: None,
            },
        ] {
            assert!(
                matches!(
                    nav_session_plan(Some(&session(true, None)), &target, false),
                    NavSessionPlan::Leave { .. }
                ),
                "{} kept the session alive",
                target.path()
            );
        }
    }

    /// The draft clause is evidence, not decoration: it appears only when
    /// there is unsaved work the draft is holding.
    #[test]
    fn the_draft_clause_follows_the_dirty_flag() {
        let dirty = nav_session_plan(Some(&session(true, None)), &StudioRoute::Home, true);
        let NavSessionPlan::Leave { said, .. } = dirty else {
            panic!("leaving");
        };
        assert_eq!(said, "Simulator stopped — your edits are saved as a draft");

        let hardware = nav_session_plan(Some(&session(false, None)), &StudioRoute::Home, true);
        let NavSessionPlan::Leave { said, teardown } = hardware else {
            panic!("leaving");
        };
        assert_eq!(
            said,
            "Attic strip disconnected — your edits are saved as a draft"
        );
        // The board is named by the card key the control carries — the
        // same key the card's own Disconnect row hands the op.
        assert!(matches!(
            teardown.op_as::<lpa_studio_core::DeviceOp>(),
            Some(lpa_studio_core::DeviceOp::DisconnectDevice { target })
                if target.card_key() == Some("dev7k2")
        ));
    }

    /// The only refusal (R8-4): an operation in flight, named, with both
    /// ways out of it.
    #[test]
    fn an_operation_in_flight_refuses_the_move() {
        let plan = nav_session_plan(
            Some(&session(false, Some("Deploy"))),
            &StudioRoute::Explore,
            true,
        );

        let NavSessionPlan::Refuse(line) = plan else {
            panic!("a busy session must refuse the move");
        };
        assert_eq!(
            line,
            "Deploy in progress — finish or cancel it before leaving"
        );
    }

    /// A busy session still moves freely INSIDE the studio: play, patch
    /// and mapping are zooms on the same session, and refusing them would
    /// lock the user out of watching the very operation they started.
    #[test]
    fn the_guard_never_blocks_movement_inside_the_studio() {
        for target in [
            project_route(),
            project_route().with_play(true),
            project_route().with_view(router::ProjectView::Patch),
            StudioRoute::Device {
                uid: "dev7k2".to_string(),
                play: false,
            },
        ] {
            assert!(
                matches!(
                    nav_session_plan(Some(&session(true, Some("Deploy"))), &target, true),
                    NavSessionPlan::Keep
                ),
                "{} was not treated as the studio",
                target.path()
            );
        }
    }

    /// With nothing running there is nothing to end and nothing to say —
    /// browsing the site must stay silent.
    #[test]
    fn a_tab_with_no_session_navigates_silently() {
        for target in [
            StudioRoute::Home,
            StudioRoute::Projects,
            StudioRoute::Docs {
                page: Some("intro".to_string()),
                anchor: None,
            },
            project_route(),
        ] {
            assert!(matches!(
                nav_session_plan(None, &target, true),
                NavSessionPlan::Keep
            ));
        }
    }

    /// The story book and the board editor reload the page, so the
    /// session dies with the document — dispatching a teardown into a
    /// tab that is about to be replaced would only race the reload.
    #[test]
    fn the_reloading_routes_are_left_to_the_reload() {
        for target in [
            StudioRoute::Stories { story_id: None },
            StudioRoute::BoardEditor,
        ] {
            assert!(matches!(
                nav_session_plan(Some(&session(true, None)), &target, false),
                NavSessionPlan::Keep
            ));
        }
    }

    #[test]
    fn console_line_renders_origin_label_without_detail() {
        let entry = UiLogEntry::new(0.0, UiLogLevel::Info, UiLogOrigin::Studio, "connected");

        assert_eq!(console_line(&entry), "[studio] connected");
    }

    #[test]
    fn console_line_renders_origin_and_detail() {
        let entry = UiLogEntry::new(
            0.0,
            UiLogLevel::Debug,
            UiLogSource::with_detail(UiLogOrigin::Device, "fw_core::server"),
            "boot ok",
        );

        assert_eq!(console_line(&entry), "[device/fw_core::server] boot ok");
    }

    /// `style.css` is `include_str!`'d as raw bytes and injected into a
    /// `<style>` tag — nothing in the build ever PARSES it, and a browser
    /// silently discards every rule after a syntax error. A single dropped
    /// `}` in a merge once deleted the whole debug treatment while `just
    /// check`, `just test` and all 11 CI jobs stayed green; only a human
    /// looking at the screen caught it (2026-08-02).
    ///
    /// So parse it here with a real CSS parser, not a hand-rolled brace
    /// counter: this catches malformed selectors, bad at-rules and unclosed
    /// blocks alike, and fails with the offending line.
    #[test]
    fn embedded_stylesheet_parses_as_valid_css() {
        use lightningcss::stylesheet::{ParserOptions, StyleSheet};
        use std::sync::{Arc, RwLock};

        let warnings = Arc::new(RwLock::new(Vec::new()));
        let options = ParserOptions {
            error_recovery: true,
            warnings: Some(Arc::clone(&warnings)),
            ..ParserOptions::default()
        };

        // A hard parse error (the unclosed-block case) surfaces as Err.
        let sheet = StyleSheet::parse(STYLE, options)
            .unwrap_or_else(|error| panic!("style.css failed to parse: {error}"));

        // Rules the parser recovered from — exactly what a browser silently
        // drops. `error_recovery` keeps parsing so one break reports every
        // consequence, not just the first.
        let warnings = warnings.read().expect("warning lock");
        assert!(
            warnings.is_empty(),
            "style.css has {} parse error(s) a browser would silently drop:\n{}",
            warnings.len(),
            warnings
                .iter()
                .map(|warning| format!("  {warning}"))
                .collect::<Vec<_>>()
                .join("\n")
        );

        assert!(
            !sheet.rules.0.is_empty(),
            "style.css parsed to zero rules — the stylesheet is not reaching the app"
        );

        // Syntax alone is not enough. A dropped `}` does NOT make the file
        // invalid — under CSS nesting every following rule silently becomes
        // a CHILD of the preceding one, which parses cleanly and means
        // something entirely different. That is exactly how the 2026-08-02
        // regression erased the debug treatment with CI green.
        //
        // This codebase never nests style rules (no `&`, nothing but
        // `@media`/`@keyframes`/`@supports` containers), so a style rule
        // holding style rules is a lost brace, not an intention.
        fn assert_unnested(rules: &lightningcss::rules::CssRuleList<'_>, path: &str) {
            use lightningcss::rules::CssRule;

            for rule in &rules.0 {
                match rule {
                    CssRule::Style(style) => {
                        assert!(
                            style.rules.0.is_empty(),
                            "style.css:{} — a style rule ({path}) contains {} nested rule(s). \
                             This codebase does not nest style rules, so a `}}` is missing \
                             above and every nested rule is silently inert in the browser.",
                            style.loc.line + 1,
                            style.rules.0.len()
                        );
                    }
                    CssRule::Media(media) => assert_unnested(&media.rules, "@media"),
                    CssRule::Supports(supports) => assert_unnested(&supports.rules, "@supports"),
                    CssRule::LayerBlock(layer) => assert_unnested(&layer.rules, "@layer"),
                    _ => {}
                }
            }
        }

        assert_unnested(&sheet.rules, "top level");
    }
}
