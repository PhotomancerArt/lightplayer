//! Route changes into the session recorder (`route` records).
//!
//! One catch-all — an effect on the `route` signal ([`use_route_recorder`])
//! records every change the page makes, from wherever it came — plus the
//! reason, when the change was the app's own idea. A programmatic site says
//! why just before its `route.set` ([`note_route_reason`]); the effect
//! consumes that reason with the change it explains. The one that matters
//! most is the kick back to `/devices` when an open ends
//! (`web_app.rs`, `open_ended`), whose reason names the evidence that
//! fired it.
//!
//! A navigation the app REFUSED (an unsaved-work prompt declined) never
//! changes the signal — the URL is put back and the route stays — so it is
//! recorded directly ([`record_route_rollback`]).

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use lpa_studio_core::DeviceEventKind;

use crate::router::StudioRoute;

thread_local! {
    static PENDING_REASON: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Say why the NEXT route change happens. Call just before `route.set`
/// (or a `navigate_push`); the recorder effect consumes it.
pub(crate) fn note_route_reason(reason: impl Into<String>) {
    PENDING_REASON.with(|slot| *slot.borrow_mut() = Some(reason.into()));
}

/// [`note_route_reason`], unless a more specific reason is already
/// waiting (a programmatic `navigate_push` lands in the same listener a
/// back button does).
pub(crate) fn note_route_reason_if_unset(reason: &str) {
    PENDING_REASON.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(reason.to_string());
        }
    });
}

fn take_route_reason() -> Option<String> {
    PENDING_REASON.with(|slot| slot.borrow_mut().take())
}

/// Record a navigation that was refused and rolled back: the user asked
/// for `attempted`, the app put the URL back on `restored`.
pub(crate) fn record_route_rollback(attempted: &StudioRoute, restored: &StudioRoute, why: &str) {
    crate::device_events_io::record(DeviceEventKind::Route {
        from: attempted.path(),
        to: restored.path(),
        reason: Some(format!("refused-nav: {why}")),
    });
}

/// The catch-all: record every change of the `route` signal, with the
/// pending reason when one was noted. The first run records the boot
/// route (`from` is empty).
pub(crate) fn use_route_recorder(route: Signal<StudioRoute>) {
    let previous: Rc<RefCell<Option<StudioRoute>>> = use_hook(|| Rc::new(RefCell::new(None)));
    use_effect(move || {
        let current = route();
        let before = previous.borrow_mut().replace(current.clone());
        let (from, reason) = match before {
            None => (String::new(), Some("boot".to_string())),
            Some(before) if before == current => return,
            Some(before) => (before.path(), take_route_reason()),
        };
        crate::device_events_io::record(DeviceEventKind::Route {
            from,
            to: current.path(),
            reason,
        });
    });
}
