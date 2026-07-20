//! Live gallery thumbnails: the web-side seam between home cards and the
//! core `PreviewHost` (`docs/adr/2026-07-16-preview-host.md`).
//!
//! The core actor's home state stays metadata-only (`UiPackageCard` /
//! `UiExampleCard` carry the uid / example id, which IS the preview
//! source); everything DOM-shaped — canvas mounting, generation remounts,
//! IntersectionObserver visibility, status polling — lives here, next to
//! the cards that consume it. [`use_thumb_preview`] is the whole consumer
//! surface: `CardThumb` calls it and renders the returned snapshot.
//!
//! # Host construction and lifetime
//!
//! One [`PreviewHost`] per page, constructed **lazily** the first time a
//! live thumb becomes visible (so story builds, which never pass a
//! source, never boot preview workers) and kept for the **app lifetime**:
//!
//! - the host's `run()` contract is construct-once / drive-once, and its
//!   pool workers each hold a WebGPU device request — re-booting the pool
//!   on every home visit would pay that cost repeatedly;
//! - with no leased slots (project open, gallery unmounted) the host idles
//!   at a ~100 ms poll with zero runtimes — there is nothing worth tearing
//!   down;
//! - future preview consumers (editor visual-module previews, preview-mode
//!   authoring) share the same host, which is the point of the service.
//!
//! Construction waits briefly for the app's local-store probe so
//! [`PreviewSource::ProjectUid`] leases get the library seam; if the store
//! never settles (OPFS unavailable) the host starts without it and project
//! leases fail-soft with a visible reason — exactly the state in which the
//! gallery renders no project cards anyway.

use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;
use lpa_studio_core::PreviewSource;

/// What the thumb's corner badge shows about its preview slot (fidelity-
/// tiers ADR: the granted tier and every failure are user-visible, never
/// silent). Stories inject these statically; live cards derive them from
/// the slot status.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    not(any(target_arch = "wasm32", feature = "stories")),
    allow(
        dead_code,
        reason = "constructed by the wasm live-preview path and the stories feature; host builds only run the unit tests"
    )
)]
pub(crate) enum ThumbPreviewBadge {
    /// Live on the GPU tier (presenting straight to the transferred
    /// canvas, zero readback).
    Gpu,
    /// Live on the CPU tier; `reason` says why the GPU request fell back
    /// (host-blitted `putImageData` frames — the UI never touches pixels).
    Cpu {
        /// The surfaced `tier_reason` (`None` when CPU was granted as
        /// asked — not the case today; gallery leases always request GPU).
        reason: Option<String>,
    },
    /// The preview gave up; the gradient stays and the chip carries the
    /// reason as its tooltip.
    Error {
        /// User-facing explanation from the slot status.
        reason: String,
    },
}

/// Per-render snapshot of one card thumb's live-preview state, produced by
/// [`use_thumb_preview`].
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ThumbPreview {
    /// Stable per-mount element id for the thumb frame — the
    /// IntersectionObserver target.
    pub frame_id: String,
    /// The live canvas layer to mount, when this thumb has a preview
    /// source (wasm builds only; `None` renders the static stack).
    pub canvas: Option<ThumbCanvas>,
    /// Badge derived from the live slot status (`None` until the slot
    /// goes live or fails).
    pub badge: Option<ThumbPreviewBadge>,
}

/// The mounted live `<canvas>` layer of a [`ThumbPreview`].
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ThumbCanvas {
    /// Generation-suffixed canvas element id: `transferControlToOffscreen`
    /// permanently consumes a canvas, so every recovery mounts a fresh
    /// element (preview-lab's generation discipline).
    pub id: String,
    /// A frame has reached this canvas — reveal it over the gradient.
    pub revealed: bool,
}

/// Monotonic thumb-frame ids (one per mounted `CardThumb`).
static NEXT_THUMB_ID: AtomicU64 = AtomicU64::new(0);

/// Drive one card thumb's live preview and snapshot it for rendering.
///
/// `source: None` (stories, non-wasm builds, cards without previews) is
/// fully inert: no host, no canvas, no observer. With a source, the thumb
/// leases a `PreviewHost` slot when it first scrolls into view, follows
/// visibility edges with `set_visible`, reveals the canvas after the first
/// presented frame, and recovers from errors by remounting a fresh canvas
/// generation and leasing again (bounded; then parks on an error badge).
pub(crate) fn use_thumb_preview(source: Option<PreviewSource>) -> ThumbPreview {
    let frame_id = use_hook(|| {
        let id = NEXT_THUMB_ID.fetch_add(1, Ordering::Relaxed);
        format!("gallery-thumb-{id}")
    });
    #[cfg(target_arch = "wasm32")]
    {
        wasm::use_live_thumb(frame_id, source)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Host builds of this crate run unit tests only and never mount a
        // live preview; the static stack still renders.
        let _ = source;
        ThumbPreview {
            frame_id,
            canvas: None,
            badge: None,
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::cell::RefCell;
    use std::rc::Rc;

    use dioxus::prelude::*;
    use gloo_timers::future::TimeoutFuture;
    use lpa_studio_core::{
        PreviewHost, PreviewHostConfig, PreviewProfile, PreviewSlotHandle, PreviewSlotRequest,
        PreviewSlotStatus, PreviewSource, PreviewTier,
    };
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::Closure;

    use super::{ThumbCanvas, ThumbPreview, ThumbPreviewBadge};

    /// Status/visibility poll cadence per thumb. Change detection rides
    /// `status_revision()`, so each tick is a couple of cheap reads.
    const THUMB_POLL_MS: u32 = 200;
    /// Substantive lease failures (deploy errors, worker loss, …) allowed
    /// before a thumb parks on an error badge instead of leasing again.
    const THUMB_ERROR_LIMIT: u8 = 2;
    /// Canvas-generation remounts allowed for the *expected* GPU staleness
    /// error (eviction/recycle consumed the transferred canvas) before the
    /// thumb parks. Bounds a hostile project's recycle flap.
    const THUMB_REMOUNT_LIMIT: u8 = 5;
    /// How long the host constructor waits for the app's local-store probe
    /// before starting without the library seam, in 100 ms polls.
    const LIBRARY_WAIT_POLLS: u32 = 50;

    /// The page-wide preview host (see the module docs for the lifetime
    /// decision).
    enum HostState {
        /// No live thumb has needed the host yet.
        Idle,
        /// The async constructor (library wait + boot) is in flight.
        Starting,
        /// Constructed; `run()` is being driven on a spawned task.
        Ready(Rc<PreviewHost>),
    }

    thread_local! {
        static HOST: RefCell<HostState> = const { RefCell::new(HostState::Idle) };
    }

    /// The shared preview host, kicking off its lazy construction on first
    /// call. `None` while construction is still in flight — callers poll.
    fn preview_host() -> Option<Rc<PreviewHost>> {
        HOST.with(|cell| {
            let mut state = cell.borrow_mut();
            match &*state {
                HostState::Ready(host) => Some(Rc::clone(host)),
                HostState::Starting => None,
                HostState::Idle => {
                    *state = HostState::Starting;
                    wasm_bindgen_futures::spawn_local(async {
                        // Give the app's startup probe (web_app.rs) time to
                        // attach the library; constructing with `None`
                        // would fail-soft every ProjectUid lease for the
                        // rest of the page.
                        let mut library = crate::local_store::library_host();
                        let mut polls = 0;
                        while library.is_none() && polls < LIBRARY_WAIT_POLLS {
                            TimeoutFuture::new(100).await;
                            library = crate::local_store::library_host();
                            polls += 1;
                        }
                        if library.is_none() {
                            log::warn!(
                                "gallery preview host: starting without a library \
                                 (project previews will surface an error)"
                            );
                        }
                        let host = Rc::new(PreviewHost::new(PreviewHostConfig::default(), library));
                        HOST.with(|cell| {
                            *cell.borrow_mut() = HostState::Ready(Rc::clone(&host));
                        });
                        // Drive-once contract: the host owns no executor.
                        // App-lifetime host — never shut down; page unload
                        // terminates the pool workers with the page.
                        host.run().await;
                    });
                    None
                }
            }
        })
    }

    /// Everything one live thumb owns outside the render path: the slot
    /// lease, the visibility observer, and recovery accounting. Shared
    /// (`Rc<RefCell<…>>`) between the poll loop, the observer callback,
    /// and the drop hook.
    #[derive(Default)]
    struct LiveThumbState {
        /// The leased slot; dropping it releases (DestroyRuntime).
        handle: Option<PreviewSlotHandle>,
        observer: Option<web_sys::IntersectionObserver>,
        /// Keeps the observer's JS callback alive for the thumb's life.
        observer_closure: Option<Closure<dyn FnMut(js_sys::Array)>>,
        /// Observer construction failed; treat the thumb as always
        /// visible instead of retrying every tick.
        observer_broken: bool,
        /// Latest observer edge (`None` until the first callback fires —
        /// leasing waits for it, so offscreen cards never deploy).
        visible: Option<bool>,
        /// Last consumed `status_revision` (cheap change detection).
        last_revision: Option<u64>,
        /// Substantive failures so far (parks at [`THUMB_ERROR_LIMIT`]).
        errors: u8,
        /// Canvas-generation remounts so far (parks at
        /// [`THUMB_REMOUNT_LIMIT`]).
        remounts: u8,
    }

    /// The wasm arm of [`super::use_thumb_preview`].
    pub(super) fn use_live_thumb(frame_id: String, source: Option<PreviewSource>) -> ThumbPreview {
        // Canvas element generation: bumped on every recovery so a fresh
        // element mounts (a GPU-tier canvas is consumed by its transfer).
        let generation = use_signal(|| 0_u32);
        let badge = use_signal(|| None::<ThumbPreviewBadge>);
        let revealed = use_signal(|| false);
        let state = use_hook(|| Rc::new(RefCell::new(LiveThumbState::default())));

        let tick_state = Rc::clone(&state);
        let tick_frame = frame_id.clone();
        let tick_source = source.clone();
        use_future(move || {
            let state = Rc::clone(&tick_state);
            let frame_id = tick_frame.clone();
            let source = tick_source.clone();
            async move {
                let Some(source) = source else {
                    return; // static thumb: nothing to drive
                };
                loop {
                    drive_thumb(&state, &frame_id, &source, generation, badge, revealed);
                    TimeoutFuture::new(THUMB_POLL_MS).await;
                }
            }
        });

        let drop_state = Rc::clone(&state);
        use_drop(move || {
            let mut state = drop_state.borrow_mut();
            if let Some(observer) = state.observer.take() {
                observer.disconnect();
            }
            state.observer_closure = None;
            // Dropping the handle releases the slot (DestroyRuntime).
            state.handle = None;
        });

        ThumbPreview {
            canvas: source.as_ref().map(|_| ThumbCanvas {
                id: thumb_canvas_id(&frame_id, generation()),
                revealed: revealed(),
            }),
            badge: badge(),
            frame_id,
        }
    }

    /// One poll tick: attach the observer once the frame exists, lease
    /// when first visible, and fold slot status into the render signals.
    fn drive_thumb(
        state_rc: &Rc<RefCell<LiveThumbState>>,
        frame_id: &str,
        source: &PreviewSource,
        mut generation: Signal<u32>,
        mut badge: Signal<Option<ThumbPreviewBadge>>,
        mut revealed: Signal<bool>,
    ) {
        let mut state = state_rc.borrow_mut();

        if state.observer.is_none() && !state.observer_broken {
            attach_observer(&mut state, state_rc, frame_id);
        }

        // Lease when the card is (or is assumed) visible and recovery
        // budget remains. The canvas for the current generation is already
        // mounted — the host's lease pipeline finds it by id.
        let assumed_visible = state.visible == Some(true) || state.observer_broken;
        if state.handle.is_none()
            && assumed_visible
            && state.errors < THUMB_ERROR_LIMIT
            && state.remounts < THUMB_REMOUNT_LIMIT
        {
            if let Some(host) = preview_host() {
                let handle = host.lease(PreviewSlotRequest {
                    source: source.clone(),
                    canvas_id: thumb_canvas_id(frame_id, *generation.peek()),
                    fps: None,
                    profile: PreviewProfile::default(),
                });
                state.last_revision = None;
                state.handle = Some(handle);
            }
        }

        // Snapshot the handle's observables before mutating the state.
        let (presented, revision, status) = {
            let Some(handle) = &state.handle else {
                return;
            };
            (
                handle.presented_frames() > 0,
                handle.status_revision(),
                handle.status(),
            )
        };
        if *revealed.peek() != presented {
            revealed.set(presented);
        }
        if state.last_revision == Some(revision) {
            return;
        }
        state.last_revision = Some(revision);
        match status {
            // Deploying keeps whatever the thumb showed; Suspended freezes
            // the canvas on its last frame (scroll-away) — both are
            // non-events for the overlay.
            PreviewSlotStatus::Deploying | PreviewSlotStatus::Suspended => {}
            PreviewSlotStatus::Live { tier, tier_reason } => {
                let next = match tier {
                    PreviewTier::Gpu => ThumbPreviewBadge::Gpu,
                    PreviewTier::Cpu => ThumbPreviewBadge::Cpu {
                        reason: tier_reason,
                    },
                };
                if badge.peek().as_ref() != Some(&next) {
                    badge.set(Some(next));
                }
            }
            PreviewSlotStatus::Error { reason } => {
                // Release first: never reuse a canvas that a GPU-tier slot
                // transferred. The host parks errored slots; recovery is
                // ours (remount a fresh generation, lease again).
                state.handle = None;
                state.last_revision = None;
                revealed.set(false);
                // The transferred-canvas error is the EXPECTED recovery
                // path after LRU eviction or a worker recycle (the host
                // words it "already transferred — remount"), so it spends
                // the remount budget, not the error budget. Matching on
                // the message is a P5 candidate for a typed status flag.
                let expected_remount = reason.contains("already transferred");
                if expected_remount {
                    state.remounts = state.remounts.saturating_add(1);
                } else {
                    state.errors = state.errors.saturating_add(1);
                }
                if state.errors >= THUMB_ERROR_LIMIT || state.remounts >= THUMB_REMOUNT_LIMIT {
                    log::warn!("gallery preview #{frame_id} gave up: {reason}");
                    badge.set(Some(ThumbPreviewBadge::Error { reason }));
                } else {
                    generation += 1; // fresh canvas element; re-lease next tick
                    if badge.peek().is_some() {
                        badge.set(None);
                    }
                }
            }
        }
    }

    /// Observe the thumb frame for visibility edges. The callback pushes
    /// edges straight onto the slot handle (`set_visible`) and records
    /// them for the lease gate. Retried each tick until the frame mounts;
    /// an unavailable IntersectionObserver degrades to always-visible.
    fn attach_observer(
        state: &mut LiveThumbState,
        state_rc: &Rc<RefCell<LiveThumbState>>,
        frame_id: &str,
    ) {
        let Some(element) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(frame_id))
        else {
            return; // not mounted yet; retry next tick
        };
        let callback_state = Rc::clone(state_rc);
        let closure = Closure::<dyn FnMut(js_sys::Array)>::new(move |entries: js_sys::Array| {
            let mut state = callback_state.borrow_mut();
            for entry in entries.iter() {
                let Ok(entry) = entry.dyn_into::<web_sys::IntersectionObserverEntry>() else {
                    continue;
                };
                let visible = entry.is_intersecting();
                state.visible = Some(visible);
                if let Some(handle) = &state.handle {
                    handle.set_visible(visible);
                }
            }
        });
        match web_sys::IntersectionObserver::new(closure.as_ref().unchecked_ref()) {
            Ok(observer) => {
                observer.observe(&element);
                state.observer = Some(observer);
                state.observer_closure = Some(closure);
            }
            Err(error) => {
                log::warn!("gallery preview: IntersectionObserver unavailable: {error:?}");
                state.observer_broken = true;
            }
        }
    }

    /// The generation-suffixed canvas element id (preview-lab's pattern:
    /// `transferControlToOffscreen` is one-shot, so ids are per mount).
    fn thumb_canvas_id(frame_id: &str, generation: u32) -> String {
        format!("{frame_id}-canvas-g{generation}")
    }
}
