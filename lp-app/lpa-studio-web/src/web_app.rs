//! The Studio web shell: Dioxus wiring over the core `StudioActor`.
//!
//! All update logic (the pull loop, command queue, preemption, timeouts,
//! backoff, log cap, change-gating) lives in `lpa-client` / `lpa-studio-core`
//! after M7. This module keeps only browser concerns: spawn the actor, drive a
//! `Signal<UiStudioView>` from its change-gated view channel, run a timer that
//! enqueues `RefreshTick` commands at the core-owned cadence, forward UI actions
//! as `Action` commands, render, and mirror new logs to the JS console.

use core::cell::Cell;
use core::time::Duration;
use std::rc::Rc;

use crate::app::StudioShell;
use crate::studio_url;
use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use lpa_studio_core::app::studio::studio_view_channel::CommandSender;
use lpa_studio_core::{
    StudioActor, StudioCommand, StudioController, UiAction, UiLogEntry, UiLogLevel, UiStudioView,
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
    // Spawn the actor once and drive the view signal from its change-gated
    // channel, mirroring newly-arrived logs to the JS console.
    let bridge = use_hook(|| {
        let (actor, handle) = StudioActor::new(StudioController::new(), make_pull_timer);
        let mut view_rx = handle.view;
        spawn(async move {
            while let Some(next) = view_rx.recv().await {
                for log in new_logs(&view.peek(), &next) {
                    log_to_js_console(&log);
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

    let startup_intent = use_hook(studio_url::read_connection_intent);
    let startup_bridge = bridge.clone();
    let _startup_task = use_future(move || {
        let startup_bridge = startup_bridge.clone();
        async move {
            if let Some(action) = startup_intent.and_then(|intent| intent.startup_action()) {
                startup_bridge.tx.send(StudioCommand::Action(action));
            }
        }
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

    rsx! {
        style { "{STYLE}" }
        document::Stylesheet { href: asset!("/assets/tailwind.css") }
        StudioShell {
            view: view.read().clone(),
            running: false,
            on_action,
        }
    }
}

/// The pull loop's per-request progress-deadline timer on wasm: a `setTimeout`
/// via `gloo_timers`. The actor calls this to build each pull's quiet-gap
/// deadline; native callers would pass a `sleep`-backed factory instead.
fn make_pull_timer(delay: Duration) -> TimeoutFuture {
    TimeoutFuture::new(delay.as_millis() as u32)
}

/// The log entries in `next` that arrived since `previous`, so the shell mirrors
/// only newly-appended logs to the JS console. Logs are appended into a bounded
/// core ring; the new entries are the suffix of `next.logs` after the last entry
/// shared with `previous.logs` (found by scanning back), which stays correct even
/// when the ring wraps.
fn new_logs(previous: &UiStudioView, next: &UiStudioView) -> Vec<UiLogEntry> {
    let Some(last_seen) = previous.logs.last() else {
        return next.logs.clone();
    };
    match next.logs.iter().rposition(|log| log == last_seen) {
        Some(index) => next.logs[index + 1..].to_vec(),
        None => next.logs.clone(),
    }
}

fn log_to_js_console(log: &UiLogEntry) {
    let message = format!("[{}] {}", log.source, log.message);
    match log.level {
        UiLogLevel::Debug => console_debug(&message),
        UiLogLevel::Info => console_info(&message),
        UiLogLevel::Warn => console_warn(&message),
        UiLogLevel::Error => console_error(&message),
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
    use lpa_studio_core::UiLogLevel;

    fn view_with_logs(messages: &[&str]) -> UiStudioView {
        let logs = messages
            .iter()
            .map(|message| UiLogEntry::new(UiLogLevel::Info, "studio", *message))
            .collect();
        UiStudioView::new(Vec::new(), logs)
    }

    #[test]
    fn new_logs_returns_appended_tail() {
        let previous = view_with_logs(&["a", "b"]);
        let next = view_with_logs(&["a", "b", "c", "d"]);

        let new = new_logs(&previous, &next);
        assert_eq!(new.len(), 2);
        assert_eq!(new[0].message, "c");
        assert_eq!(new[1].message, "d");
    }

    #[test]
    fn new_logs_handles_ring_wrap_by_anchoring_on_last_seen() {
        // The ring dropped "a"; the shared anchor "c" still locates the new tail.
        let previous = view_with_logs(&["a", "b", "c"]);
        let next = view_with_logs(&["b", "c", "d"]);

        let new = new_logs(&previous, &next);
        assert_eq!(new.len(), 1);
        assert_eq!(new[0].message, "d");
    }

    #[test]
    fn new_logs_returns_all_when_previous_empty() {
        let previous = view_with_logs(&[]);
        let next = view_with_logs(&["a", "b"]);

        assert_eq!(new_logs(&previous, &next).len(), 2);
    }
}
