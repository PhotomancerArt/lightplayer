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
use std::rc::Rc;

use crate::app::StudioShell;
use crate::app::layout::LocalStoreBanner;
use crate::local_store::{self, LocalStoreStatus};
use crate::router::{self, StudioRoute};
use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use lpa_studio_core::app::studio::studio_view_channel::CommandSender;
use lpa_studio_core::{
    ConsoleCommand, DeviceController, DeviceOp, HOME_NODE_ID, HomeOp, STUDIO_LOG_SINK, StudioActor,
    StudioCommand, StudioController, UiAction, UiLogEntry, UiLogLevel, UiStudioView,
};

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

    let mut view = use_signal(UiStudioView::empty);
    // The route: parsed from the URL at boot (with the legacy `?project=`
    // mapping), canonicalized once, then kept in sync bidirectionally —
    // the view loop below mirrors actor state into the URL, and the
    // browser-navigation listener dispatches actions for back/forward.
    let mut route = use_signal(router::boot_route);
    use_hook(move || router::replace(&route.peek().clone()));
    // The (uid, slug) the view currently shows (for the navigation
    // listener) and whether an open ever started this session (the
    // boot-time home flash must not rewrite the URL that requested a
    // startup reopen).
    let open_ids_now = use_hook(|| Rc::new(RefCell::new(None::<(String, String)>)));
    let saw_opening = use_hook(|| Rc::new(Cell::new(false)));
    // A route-driven open we dispatched (startup / back-forward / hash nav)
    // that the actor hasn't started yet. While set, stale home views must
    // not trip the "open ended" fallback — the race: a queued RefreshTick's
    // home view can land between the navigation and the action starting.
    let pending_route_open = use_hook(|| Rc::new(Cell::new(false)));

    // Install the global `log::` sink and the JS-console mirror hook, then
    // spawn the actor once and drive the view signal from its change-gated
    // channel.
    let loop_open_ids = Rc::clone(&open_ids_now);
    let loop_saw_opening = Rc::clone(&saw_opening);
    let loop_pending_route_open = Rc::clone(&pending_route_open);
    let bridge = use_hook(move || {
        install_log_sink();
        let mut controller = StudioController::new(now_secs);
        controller.set_on_entry(log_to_js_console);
        let (actor, handle) = StudioActor::new(controller, make_pull_timer);
        let mut view_rx = handle.view;
        spawn(async move {
            while let Some(next) = view_rx.recv().await {
                *loop_open_ids.borrow_mut() = next
                    .open_project_uid
                    .clone()
                    .zip(next.open_project_slug.clone());
                let opening_now = next
                    .home
                    .as_ref()
                    .is_some_and(|home| home.opening.is_some());
                if opening_now || next.open_project_uid.is_some() {
                    loop_saw_opening.set(true);
                    // the dispatched open has started; fallbacks may judge it
                    loop_pending_route_open.set(false);
                }

                // view → route: the URL follows the actor's state; the
                // slug is the user-facing key the URL carries
                let current = route.peek().clone();
                if let Some(slug) = &next.open_project_slug {
                    if !current.project_matches_view(&next) {
                        let target = StudioRoute::Project { key: slug.clone() };
                        if matches!(current, StudioRoute::Home) {
                            // a gallery open: a real navigation, so a real
                            // history entry (back returns to the gallery)
                            router::navigate(&target);
                        } else {
                            // boot/forward resolution: no duplicate entries
                            router::replace(&target);
                        }
                        route.set(target);
                    }
                } else if matches!(current, StudioRoute::Project { .. }) {
                    // the project went away: panes showing means a failed
                    // open or a bridge flow took over; home without an
                    // in-flight open (after one started) means the open
                    // ended unsuccessfully — either way the URL goes home.
                    // The boot-time home flash (nothing started yet) keeps
                    // the route so the startup reopen can use it.
                    let has_panes = !next.panes.is_empty();
                    let open_ended = next.home.is_some()
                        && !opening_now
                        && loop_saw_opening.get()
                        && !loop_pending_route_open.get();
                    if has_panes || open_ended {
                        router::replace(&StudioRoute::Home);
                        route.set(StudioRoute::Home);
                    }
                }

                view.set(next);
            }
        });
        spawn(actor.run());
        StudioBridge {
            tx: handle.tx,
            delay: handle.delay,
        }
    });

    // route → actor: back/forward and manual hash edits dispatch the
    // matching action. Programmatic navigate/replace calls fire no browser
    // events, so everything arriving here is real user navigation.
    let nav_bridge = bridge.clone();
    let nav_open_ids = Rc::clone(&open_ids_now);
    let nav_pending_route_open = Rc::clone(&pending_route_open);
    let _route_listener = use_hook(move || {
        router::install_route_listener(move || {
            let new_route = router::current_route();
            let old = route.peek().clone();
            if new_route == old {
                return;
            }
            route.set(new_route.clone());
            match &new_route {
                StudioRoute::Home => {
                    if nav_open_ids.borrow().is_some() {
                        // back to the gallery = full return: the gallery
                        // only renders when the link is idle
                        nav_bridge.tx.send(StudioCommand::Action(UiAction::from_op(
                            DeviceController::NODE_ID,
                            DeviceOp::DisconnectDevice,
                        )));
                    }
                }
                StudioRoute::Project { key } => {
                    let already_open = nav_open_ids
                        .borrow()
                        .as_ref()
                        .is_some_and(|(uid, slug)| uid == key || slug == key);
                    if !already_open {
                        nav_pending_route_open.set(true);
                        nav_bridge.tx.send(StudioCommand::Action(UiAction::from_op(
                            HOME_NODE_ID,
                            HomeOp::OpenPackage { key: key.clone() },
                        )));
                    }
                }
                StudioRoute::Stories { .. } => {
                    // the story book mounts on fresh page loads only (its
                    // early return in App runs before any hooks); reload to
                    // keep the hook order sound
                    router::hard_reload();
                }
            }
        })
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
            if let StudioRoute::Project { key } = &startup_route {
                startup_pending_route_open.set(true);
                startup_bridge
                    .tx
                    .send(StudioCommand::Action(UiAction::from_op(
                        HOME_NODE_ID,
                        HomeOp::OpenPackage { key: key.clone() },
                    )));
            }
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
    let on_action = move |action: UiAction| {
        action_bridge.tx.send(StudioCommand::Action(action));
    };

    // Console toolbar gestures ride the same ordered command queue as
    // actions; the actor applies them synchronously and never coalesces them.
    let console_bridge = bridge.clone();
    let on_console = move |command: ConsoleCommand| {
        // The display threshold doubles as the *capture* floor: raise or lower
        // the global `log::` max level to match, so `debug!`/`trace!` producers
        // short-circuit inside the macro when hidden instead of formatting and
        // queuing output the console would only drop. Reveal below the current
        // floor is therefore forward-only, by design.
        if let ConsoleCommand::SetMinLevel(level) = command {
            log::set_max_level(capture_level_for(level));
        }
        console_bridge.tx.send(StudioCommand::Console(command));
    };

    // The URL's intent picks the frame: a project route whose project the
    // view hasn't reached yet renders the opening frame, not the gallery.
    let current_view = view.read().clone();
    let current_route = route.read().clone();
    let opening_frame = matches!(current_route, StudioRoute::Project { .. })
        && !current_route.project_matches_view(&current_view);

    rsx! {
        style { "{STYLE}" }
        document::Stylesheet { href: asset!("/assets/tailwind.css") }
        div { class: "tw:mx-auto tw:w-[min(1520px,100%)] tw:px-7 tw:pt-4 tw:max-[880px]:px-[18px]",
            LocalStoreBanner { status: store_status.read().clone() }
        }
        StudioShell {
            view: current_view,
            running: false,
            opening_frame,
            on_action,
            on_console,
        }
    }
}

/// The pull loop's per-request progress-deadline timer on wasm: a `setTimeout`
/// via `gloo_timers`. The actor calls this to build each pull's quiet-gap
/// deadline; native callers would pass a `sleep`-backed factory instead.
fn make_pull_timer(delay: Duration) -> TimeoutFuture {
    TimeoutFuture::new(delay.as_millis() as u32)
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
fn now_secs() -> f64 {
    js_sys::Date::now() / 1000.0
}

/// Host builds of this crate only run unit tests and never spawn the actor,
/// so the clock stub mirrors the JS-console stubs below.
#[cfg(not(target_arch = "wasm32"))]
fn now_secs() -> f64 {
    0.0
}

/// Install the studio `log::Log` sink as the global logger, before the actor
/// spawns, so `log::` macros anywhere on the wasm side are captured and later
/// drained into the console ring by the actor.
///
/// The initial max level matches the console filter's default threshold
/// (`Info`): the display threshold is also the *capture* floor, so producers
/// below it never format or queue output. `on_console` keeps the two in sync
/// as the user moves the filter. An already-installed logger is tolerated with
/// a JS-console warning, never a panic.
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
    use lpa_studio_core::{UiLogOrigin, UiLogSource};

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
}
