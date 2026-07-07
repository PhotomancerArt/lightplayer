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

use core::cell::Cell;
use core::time::Duration;
use std::rc::Rc;

use crate::app::StudioShell;
use crate::app::layout::LocalStoreBanner;
use crate::local_store::{self, LocalStoreStatus};
use crate::studio_url;
use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use lpa_studio_core::app::studio::studio_view_channel::CommandSender;
use lpa_studio_core::{
    ConsoleCommand, STUDIO_LOG_SINK, StudioActor, StudioCommand, StudioController, UiAction,
    UiLogEntry, UiLogLevel, UiStudioView,
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
    // Install the global `log::` sink and the JS-console mirror hook, then
    // spawn the actor once and drive the view signal from its change-gated
    // channel.
    let bridge = use_hook(|| {
        install_log_sink();
        let mut controller = StudioController::new(now_secs);
        controller.set_on_entry(log_to_js_console);
        let (actor, handle) = StudioActor::new(controller, make_pull_timer);
        let mut view_rx = handle.view;
        spawn(async move {
            while let Some(next) = view_rx.recv().await {
                view.set(next);
            }
        });
        spawn(actor.run());
        StudioBridge {
            tx: handle.tx,
            delay: handle.delay,
        }
    });

    // The local project store: mounted in the startup hook below (which
    // also attaches the library and only then fires the connect action).
    let mut store_status = use_signal(|| LocalStoreStatus::Initializing);
    let on_store_retry = move |_| {
        spawn(async move {
            store_status.set(local_store::init_local_store().await);
        });
    };

    // Startup ordering matters: the library must attach before the connect
    // action runs, or the demo would load through the legacy (storeless)
    // path on first paint. The store mount is awaited here; the sim still
    // starts (without persistence) if the store is unavailable.
    let startup_intent = use_hook(studio_url::read_connection_intent);
    let startup_bridge = bridge.clone();
    use_hook(move || {
        let startup_bridge = startup_bridge.clone();
        spawn(async move {
            local_store::request_persist();
            let status = local_store::init_local_store().await;
            #[cfg(target_arch = "wasm32")]
            if status == LocalStoreStatus::Ready {
                if let Some(library) = local_store::library_store() {
                    startup_bridge.tx.send(StudioCommand::AttachLibrary(
                        lpa_studio_core::app::studio::studio_command::LibraryAttachment(library),
                    ));
                }
            }
            store_status.set(status);
            if let Some(action) = startup_intent.and_then(|intent| intent.startup_action()) {
                startup_bridge.tx.send(StudioCommand::Action(action));
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
        studio_url::update_for_action(&action);
        action_bridge.tx.send(StudioCommand::Action(action));
    };

    // Console toolbar gestures ride the same ordered command queue as
    // actions; the actor applies them synchronously and never coalesces them.
    let console_bridge = bridge.clone();
    let on_console = move |command: ConsoleCommand| {
        console_bridge.tx.send(StudioCommand::Console(command));
    };

    rsx! {
        style { "{STYLE}" }
        document::Stylesheet { href: asset!("/assets/tailwind.css") }
        div { class: "tw:mx-auto tw:w-[min(1520px,100%)] tw:px-7 tw:pt-4 tw:max-[880px]:px-[18px]",
            LocalStoreBanner {
                status: store_status.read().clone(),
                on_retry: on_store_retry,
            }
        }
        StudioShell {
            view: view.read().clone(),
            running: false,
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
/// `Debug` is the runtime max (bumping to `Trace` is a cheap follow-up; this
/// avoids paying for hot-path `trace!` in dependencies) — the console pane's
/// filter is the *display* gate. An already-installed logger is tolerated
/// with a JS-console warning, never a panic.
fn install_log_sink() {
    match log::set_logger(&STUDIO_LOG_SINK) {
        Ok(()) => log::set_max_level(log::LevelFilter::Debug),
        Err(_) => console_warn("studio log sink not installed: a global logger is already set"),
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
