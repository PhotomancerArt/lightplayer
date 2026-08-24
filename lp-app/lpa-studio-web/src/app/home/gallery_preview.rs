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
//! # Which picture a thumb shows
//!
//! A **control-first** project — one whose root scope resolves `control.out`,
//! answered by the engine and carried on the slot's output frames — leads
//! with its LAMPS, the same rule the editor's module-face hero follows. The
//! raster present keeps running underneath (it is what ticks the engine and
//! carries the lamp answer home); only its canvas stays hidden. Everything
//! else is exactly the raster thumb it has always been.
//!
//! # How long a thumb renders: [`ThumbMode`]
//!
//! Gallery cards are [`ThumbMode::PosterFirst`]: a card wants *a picture*,
//! not a canvas that runs forever. A session-cached poster paints with no
//! lease at all; a miss leases a slot with a small
//! `PreviewProfile::frame_budget`, waits for the budget to be spent,
//! captures the poster ([`super::thumb_poster`]), and then stands the whole
//! thumb down — handle dropped, observer disconnected, poll loop ended.
//! Cards at rest hold nothing.
//!
//! The docs hero is [`ThumbMode::Live`]: one large canvas the reader is
//! actually watching, leased for as long as it is mounted. That is the
//! behavior every lease had before posters existed.
//!
//! # Motion on hover
//!
//! A poster-first card plays while the cursor is on it: the page's
//! [`HoveredCard`] signal names at most one card, that card takes a
//! continuous lease (no frame budget), and the motion fades in OVER the
//! poster it already had — so neither taking the lease nor dropping it can
//! blank the card. Hover is a courtesy: it writes no badge, spends none of
//! the at-rest recovery budget, and returns to the poster in silence when
//! anything goes wrong.
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
use lpa_studio_core::{PreviewSource, UiControlProductPreview};

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

impl ThumbPreviewBadge {
    /// Badge policy for browsing surfaces (2026-08-24, fidelity-tiers ADR
    /// decision-4 note): a badge appears only when something is actually
    /// wrong. The granted tier is not news — it stays log/wire-visible and
    /// belongs on diagnostic surfaces, not on cards a visitor is browsing.
    pub(crate) fn issue(self) -> Option<Self> {
        matches!(self, Self::Error { .. }).then_some(self)
    }
}

/// How long a thumb is willing to render (see the module docs).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ThumbMode {
    /// Hold the slot for as long as the consumer is mounted and visible —
    /// the docs hero, and every lease before posters existed.
    Live,
    /// [`Live`](Self::Live), and the RASTER canvas is the face even for a
    /// control-first product (whose raster normally presents hidden behind
    /// its lamps). The landing hero: its triangle is a screen showing the
    /// shader surface itself, not the fixture that consumes it.
    LiveRaster,
    /// Show a session-cached poster with no lease at all; on a miss, lease
    /// briefly, capture one, and release. Gallery cards.
    PosterFirst,
}

/// The gallery card the cursor is on, page-wide: `Some(`[`hover_card_key`]`)`.
///
/// A `Signal` in a context rather than per-card state, following the
/// `HoveredPatchCell` precedent (`app/node/face/patch_bay.rs`): one signal
/// holds one key, so **at most one card is hovered — and therefore at most
/// one hover lease exists — by construction**. Provided by the pages that
/// host card grids (Explore, Projects); a thumb mounted without one (a
/// story, the docs hero, a device card) simply never plays on hover.
#[derive(Clone, Copy)]
pub(crate) struct HoveredCard(pub(crate) Signal<Option<String>>);

/// The hover key for a card's preview source: the same namespaced source
/// string the poster cache is keyed by, so "which card" means exactly the
/// same thing in both places.
pub(crate) fn hover_card_key(source: &PreviewSource) -> String {
    crate::app::home::thumb_poster::poster_cache_key(source)
}

/// The hover wiring one card needs: `(onmouseenter, onmouseleave)` for the
/// page's [`HoveredCard`] signal. A card with no preview source, or one on a
/// page that provides no signal, gets inert handlers — including every
/// story, so static capture keeps no hover state at all.
///
/// The leave handler is **guarded** (`peek` against the current owner — the
/// `patch_surface.rs` precedent): sweeping the cursor across a grid delivers
/// the next card's enter before this card's leave, and an unguarded clear
/// would cancel the hover that had just started.
pub(crate) fn card_hover_handlers(
    source: Option<&PreviewSource>,
) -> (
    impl FnMut(MouseEvent) + 'static,
    impl FnMut(MouseEvent) + 'static,
) {
    let hover = try_consume_context::<HoveredCard>();
    let key = source.map(hover_card_key);
    let enter_key = key.clone();
    let enter = move |_: MouseEvent| {
        if let (Some(HoveredCard(mut hovered)), Some(key)) = (hover, enter_key.as_ref()) {
            hovered.set(Some(key.clone()));
        }
    };
    let leave = move |_: MouseEvent| {
        if let (Some(HoveredCard(mut hovered)), Some(key)) = (hover, key.as_ref()) {
            if hovered.peek().as_deref() == Some(key.as_str()) {
                hovered.set(None);
            }
        }
    };
    (enter, leave)
}

/// Per-render snapshot of one card thumb's live-preview state, produced by
/// [`use_thumb_preview`].
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ThumbPreview {
    /// The poster image (a PNG data URL) to show under the live layers:
    /// this session's captured frame for the thumb's source. `None` until
    /// one exists — and forever in [`ThumbMode::Live`], which never
    /// captures.
    pub poster: Option<String>,
    /// Stable per-mount element id for the thumb frame — the
    /// IntersectionObserver target.
    pub frame_id: String,
    /// The live canvas layer to mount, when this thumb has a preview
    /// source (wasm builds only; `None` renders the static stack).
    pub canvas: Option<ThumbCanvas>,
    /// The lamp field to draw over the canvas, for a control-first project
    /// whose output frames carry something drawable. `Some` IS the reveal
    /// signal — the field only exists once a frame has landed.
    pub lamps: Option<UiControlProductPreview>,
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
    /// A frame has reached this canvas AND the raster is the picture this
    /// thumb shows — reveal it over the gradient. A control-first slot
    /// leaves it `false`: it presents to a hidden canvas and draws lamps.
    pub revealed: bool,
}

/// Monotonic thumb-frame ids (one per mounted `CardThumb`).
static NEXT_THUMB_ID: AtomicU64 = AtomicU64::new(0);

/// Marker context provided by the story-book renderer: under it, thumbs
/// render the static stack only — no `PreviewHost` boot, no lease, and no
/// live badge. Live thumbs would race story capture (the host's fail-soft
/// error badges appear whenever the store probe loses the race), and the
/// badge states have dedicated static-injection stories instead.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StaticThumbPreviews;

/// Drive one card thumb's live preview and snapshot it for rendering.
///
/// `source: None` (stories, non-wasm builds, cards without previews) is
/// fully inert: no host, no canvas, no observer, no poster. With a source,
/// the thumb leases a `PreviewHost` slot when it first scrolls into view,
/// follows visibility edges with `set_visible`, reveals the canvas after
/// the first presented frame, and recovers from errors by remounting a
/// fresh canvas generation and leasing again (bounded; then parks on an
/// error badge). In [`ThumbMode::PosterFirst`] the lease is a poster
/// producer that stands down once it has one.
pub(crate) fn use_thumb_preview(source: Option<PreviewSource>, mode: ThumbMode) -> ThumbPreview {
    use_preview_slot(source, None, mode)
}

/// The always-live lease hook: same host, same visibility/recovery
/// discipline, with the slot's present cadence left to the caller. A docs
/// article's hero preview is a single large canvas the reader is actually
/// looking at, so it asks for a smoother rate — and never posters, because
/// motion is the point.
pub(crate) fn use_preview_lease(source: Option<PreviewSource>, fps: Option<f32>) -> ThumbPreview {
    use_preview_slot(source, fps, ThumbMode::Live)
}

/// [`use_preview_lease`], but the raster canvas reveals even for a
/// control-first product ([`ThumbMode::LiveRaster`]). The landing hero's
/// consumer surface: the brand triangle shows the shader itself.
pub(crate) fn use_preview_lease_raster(
    source: Option<PreviewSource>,
    fps: Option<f32>,
) -> ThumbPreview {
    use_preview_slot(source, fps, ThumbMode::LiveRaster)
}

/// The one hook behind [`use_thumb_preview`] and [`use_preview_lease`].
fn use_preview_slot(
    source: Option<PreviewSource>,
    fps: Option<f32>,
    mode: ThumbMode,
) -> ThumbPreview {
    let frame_id = use_hook(|| {
        let id = NEXT_THUMB_ID.fetch_add(1, Ordering::Relaxed);
        format!("gallery-thumb-{id}")
    });
    // Story capture: no source means no host, no lease, no capture and no
    // poster — the static stack is the whole face, deterministically.
    let source = if try_consume_context::<StaticThumbPreviews>().is_some() {
        None
    } else {
        source
    };
    #[cfg(target_arch = "wasm32")]
    {
        // Hover-to-play: the page's hovered-card signal paired with this
        // card's key in it. A story (source cleared above) and a page that
        // provides no signal are both inert — the hook never looks at hover.
        let hover = try_consume_context::<HoveredCard>().zip(source.as_ref().map(hover_card_key));
        wasm::use_live_thumb(frame_id, source, fps, mode, hover)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Host builds of this crate run unit tests only and never mount a
        // live preview; the static stack still renders.
        let _ = (source, fps, mode);
        ThumbPreview {
            poster: None,
            frame_id,
            canvas: None,
            lamps: None,
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
        ControlDisplayLayout, PreviewHost, PreviewHostConfig, PreviewProfile, PreviewSlotHandle,
        PreviewSlotRequest, PreviewSlotStatus, PreviewSource, PreviewTier, UiControlProductPreview,
        user_open_in_flight,
    };
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::Closure;

    use super::{HoveredCard, ThumbCanvas, ThumbMode, ThumbPreview, ThumbPreviewBadge};
    use crate::app::home::thumb_poster::{
        POSTER_HEIGHT, POSTER_WIDTH, cached_poster, canvas_poster, lamp_poster, pixel_poster,
        store_poster,
    };
    use crate::app::node::lamp_view::control_sample_layout_has_rgb;

    /// Status/visibility poll cadence per thumb. Change detection rides
    /// `status_revision()`, so each tick is a couple of cheap reads.
    const THUMB_POLL_MS: u32 = 200;
    /// Presents a poster lease is worth.
    ///
    /// Must be ≥ 2: on the GPU tier the lamp samples read back one frame
    /// behind the render (`docs/adr/2026-08-05-browser-sample-readback-is-
    /// async.md`) and a program's first frame is usually still black, so
    /// capturing frame 1 would poster a dark box. Three presents at the
    /// thumb cadence is a quarter of a second of rendering per card.
    const POSTER_FRAME_BUDGET: u32 = 3;
    /// Presenting poll ticks a poster lease gets to produce its picture
    /// before it gives up and releases (the card keeps its gradient).
    /// Counts only ticks where the slot is actually `Live`, so a queued
    /// deploy or a scrolled-away card never burns the budget.
    const POSTER_LIVE_TICK_LIMIT: u32 = 50;
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
    /// Cursor dwell before a hover takes its lease: long enough that
    /// sweeping the pointer across a grid deploys nothing, short enough that
    /// landing on a card still feels like a direct response.
    const HOVER_DEBOUNCE_MS: u32 = 120;
    /// Poll ticks a hover waits for the at-rest poster flow to finish before
    /// giving up on this card (see [`hold_hover_lease`]).
    const HOVER_WAIT_TICKS: u32 = 50;
    /// Canvas-generation remounts one hover may spend. A poster lease on the
    /// GPU tier consumed the card's canvas (`transferControlToOffscreen` is
    /// permanent), so the first hover lease on such a card is EXPECTED to
    /// fail once and remount; anything past that gives up quietly.
    const HOVER_REMOUNT_LIMIT: u8 = 2;

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
        /// Last consumed `output_frame_revision` — the lamp half's own cheap
        /// change detection, so a repeated publish repaints nothing.
        last_output_revision: Option<u64>,
        /// The newest output frame carried nothing this renderer can draw
        /// (see [`is_drawable_lamp_frame`]), so the thumb shows its raster
        /// instead of parking on a blank box.
        lamp_fallback: bool,
        /// Substantive failures so far (parks at [`THUMB_ERROR_LIMIT`]).
        errors: u8,
        /// Canvas-generation remounts so far (parks at
        /// [`THUMB_REMOUNT_LIMIT`]).
        remounts: u8,
        /// Poll ticks this poster lease has spent with the slot actually
        /// presenting (parks at [`POSTER_LIVE_TICK_LIMIT`]).
        poster_live_ticks: u32,
        /// The at-rest flow is finished — poster captured, given up, or
        /// parked — so hover-to-play may take the card from here. Hover
        /// never races the poster flow: one card holds one slot.
        at_rest: bool,
        /// Monotonic hover intent. Every hover edge bumps it, so the task a
        /// superseded edge spawned notices and exits.
        hover_epoch: u64,
        /// A hover lease is held: only then does the release path put the
        /// card back the way hover found it.
        hover_lease: bool,
        /// Canvas remounts the current hover has spent
        /// ([`HOVER_REMOUNT_LIMIT`]) — hover's own budget, never the
        /// at-rest one.
        hover_remounts: u8,
    }

    /// The wasm arm of [`super::use_preview_slot`].
    pub(super) fn use_live_thumb(
        frame_id: String,
        source: Option<PreviewSource>,
        fps: Option<f32>,
        mode: ThumbMode,
        hover: Option<(HoveredCard, String)>,
    ) -> ThumbPreview {
        // Canvas element generation: bumped on every recovery so a fresh
        // element mounts (a GPU-tier canvas is consumed by its transfer).
        let generation = use_signal(|| 0_u32);
        let badge = use_signal(|| None::<ThumbPreviewBadge>);
        let revealed = use_signal(|| false);
        let lamps = use_signal(|| None::<UiControlProductPreview>);
        // Resolved at mount, ahead of any host boot: a returning visitor's
        // card paints from the session cache on its very first render and
        // never leases anything (the "navigate back to /explore" case).
        let cached_source = source.clone();
        let poster = use_signal(move || match mode {
            ThumbMode::PosterFirst => cached_source.as_ref().and_then(cached_poster),
            ThumbMode::Live | ThumbMode::LiveRaster => None,
        });
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
                if poster.peek().is_some() {
                    // Cache hit: no host, no lease, no observer — and the
                    // thumb is already at rest, so a hover may lease at once.
                    state.borrow_mut().at_rest = true;
                    return;
                }
                let signals = ThumbSignals {
                    generation,
                    badge,
                    revealed,
                    lamps,
                    poster,
                };
                while drive_thumb(&state, &frame_id, &source, fps, mode.into(), signals)
                    == ThumbTick::Poll
                {
                    TimeoutFuture::new(THUMB_POLL_MS).await;
                }
                // Captured, gave up, or parked: whatever this card shows is
                // what it shows from now on — hover-to-play may take it.
                state.borrow_mut().at_rest = true;
            }
        });

        // Hover-to-play. The memo is the whole gate: a card with no page
        // signal, no source, or [`ThumbMode::Live`] is never "the hovered
        // card", so the effect below only ever runs its inert arm.
        let hovered = use_memo(move || match (&hover, mode) {
            (Some((HoveredCard(hovered), key)), ThumbMode::PosterFirst) => {
                hovered.read().as_deref() == Some(key.as_str())
            }
            _ => false,
        });
        let hover_state = Rc::clone(&state);
        let hover_frame = frame_id.clone();
        let hover_source = source.clone();
        use_effect(move || {
            let hovering = hovered();
            let Some(source) = hover_source.clone() else {
                return;
            };
            let state = Rc::clone(&hover_state);
            // Every hover edge supersedes the task the previous one spawned,
            // so a cursor sweeping the grid leaves nothing running behind it.
            let epoch = {
                let mut state = state.borrow_mut();
                state.hover_epoch = state.hover_epoch.wrapping_add(1);
                state.hover_epoch
            };
            if !hovering {
                release_hover(&mut state.borrow_mut(), poster, revealed);
                return;
            }
            let frame_id = hover_frame.clone();
            let signals = ThumbSignals {
                generation,
                badge,
                revealed,
                lamps,
                poster,
            };
            spawn(hold_hover_lease(state, epoch, frame_id, source, signals));
        });

        let drop_state = Rc::clone(&state);
        use_drop(move || {
            let mut state = drop_state.borrow_mut();
            stand_down(&mut state);
        });

        ThumbPreview {
            poster: poster(),
            canvas: source.as_ref().map(|_| ThumbCanvas {
                id: thumb_canvas_id(&frame_id, generation()),
                revealed: revealed(),
            }),
            lamps: lamps(),
            badge: badge(),
            frame_id,
        }
    }

    /// Everything one live thumb renders from, as the poll loop writes it.
    #[derive(Clone, Copy)]
    struct ThumbSignals {
        /// Canvas element generation, bumped on every recovery remount.
        generation: Signal<u32>,
        badge: Signal<Option<ThumbPreviewBadge>>,
        /// The raster canvas is this thumb's picture and has a frame on it.
        revealed: Signal<bool>,
        /// The lamp field, once a control-first slot has published a
        /// drawable one.
        lamps: Signal<Option<UiControlProductPreview>>,
        /// The captured poster, once this thumb has one.
        poster: Signal<Option<String>>,
    }

    /// Why a slot is leased — the three shapes [`drive_thumb`] serves. The
    /// first two are [`ThumbMode`] under another name; the third is the
    /// hover lease, which a poster-first card takes *after* its at-rest flow
    /// is done and holds only while the cursor stays on it.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum LeaseKind {
        /// [`ThumbMode::Live`]: hold the slot while mounted and visible.
        Live,
        /// [`ThumbMode::LiveRaster`]: [`Live`](Self::Live), and the raster
        /// canvas is the face even when the product is control-first.
        LiveRaster,
        /// [`ThumbMode::PosterFirst`] at rest: spend a frame budget, capture
        /// the poster, stand down.
        Poster,
        /// Motion while hovered: a continuous lease over the poster. Writes
        /// no badge and spends no at-rest recovery budget — hover is a
        /// courtesy, and its failures return to the poster in silence.
        Hover,
    }

    impl From<ThumbMode> for LeaseKind {
        fn from(mode: ThumbMode) -> Self {
            match mode {
                ThumbMode::Live => Self::Live,
                ThumbMode::LiveRaster => Self::LiveRaster,
                ThumbMode::PosterFirst => Self::Poster,
            }
        }
    }

    /// What the poll loop does after one [`drive_thumb`] tick.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ThumbTick {
        /// Come back in [`THUMB_POLL_MS`].
        Poll,
        /// This thumb is finished: it holds no slot, watches nothing, and
        /// nothing that can happen next would change what it shows. The
        /// loop ends here, so a gallery at rest costs zero timers.
        Rest,
    }

    /// Release everything the thumb holds: the slot lease (dropping the
    /// handle sends DestroyRuntime), the IntersectionObserver, and its JS
    /// callback.
    fn stand_down(state: &mut LiveThumbState) {
        if let Some(observer) = state.observer.take() {
            observer.disconnect();
        }
        state.observer_closure = None;
        state.handle = None;
    }

    /// Hold a continuous live lease for as long as this card stays hovered,
    /// then put it back on its poster.
    ///
    /// **Hover during the initial poster capture waits** for that flow to
    /// rest rather than racing it: one card never holds two slots, and what
    /// the motion fades in over is the very poster the flow was busy
    /// producing. (The alternative — promoting the poster lease in place —
    /// would have to unwind its frame budget, its capture and its stand-down
    /// mid-flight for a difference of about a second.)
    ///
    /// **Hover during a user open is dropped**, not deferred into a queue:
    /// the click owns the engine, and the pointer will say so again if it
    /// is still on the card afterwards. A hover lease already held when the
    /// open starts is left alone — the pause holds back new starts and
    /// never takes a running preview away.
    async fn hold_hover_lease(
        state: Rc<RefCell<LiveThumbState>>,
        epoch: u64,
        frame_id: String,
        source: PreviewSource,
        signals: ThumbSignals,
    ) {
        // Debounce first: a cursor crossing the grid must not deploy a
        // project per card it passes over.
        TimeoutFuture::new(HOVER_DEBOUNCE_MS).await;
        let mut waits = 0_u32;
        while hover_current(&state, epoch) && !state.borrow().at_rest {
            if waits >= HOVER_WAIT_TICKS {
                return; // the at-rest flow is taking far too long; leave it be
            }
            waits += 1;
            TimeoutFuture::new(THUMB_POLL_MS).await;
        }
        if !hover_current(&state, epoch) {
            return; // the cursor moved on during the wait
        }
        if user_open_in_flight() {
            // A user-initiated open owns the engine right now, and a hover
            // is a courtesy: drop it rather than queue it. Nothing stale
            // fires when the open settles — a hover that still matters
            // arrives again with the next pointer edge, debounce and all.
            return;
        }
        {
            let mut state = state.borrow_mut();
            state.hover_lease = true;
            state.hover_remounts = 0;
        }
        // `fps: None` — a hovered card is watched, so it renders at the
        // host's default cadence with no frame budget at all.
        while hover_current(&state, epoch)
            && drive_thumb(&state, &frame_id, &source, None, LeaseKind::Hover, signals)
                == ThumbTick::Poll
        {
            TimeoutFuture::new(THUMB_POLL_MS).await;
        }
    }

    /// Whether the hover task holding `epoch` is still the current intent.
    fn hover_current(state: &Rc<RefCell<LiveThumbState>>, epoch: u64) -> bool {
        state.borrow().hover_epoch == epoch
    }

    /// Drop the hover lease and put the card back the way hover found it.
    ///
    /// The poster has been under the canvas the whole time, so hiding a
    /// canvas whose slot just went away cannot blank the card; a control-
    /// first card's lamp field simply stops moving on its last frame, which
    /// is also a picture and also not a blank. A card with NO poster keeps
    /// its last hovered frame instead — never take away the only picture a
    /// card has.
    fn release_hover(
        state: &mut LiveThumbState,
        poster: Signal<Option<String>>,
        mut revealed: Signal<bool>,
    ) {
        if !state.hover_lease {
            return;
        }
        state.hover_lease = false;
        state.handle = None;
        state.last_revision = None;
        state.last_output_revision = None;
        if poster.peek().is_some() && *revealed.peek() {
            revealed.set(false);
        }
    }

    /// One poll tick: attach the observer once the frame exists, lease
    /// when first visible, fold slot status into the render signals, and —
    /// in [`LeaseKind::Poster`] — capture the poster and stand the thumb
    /// down once the slot has spent its frame budget.
    ///
    /// [`LeaseKind::Hover`] runs the same tick with the poster half, the
    /// badge writes and the at-rest budgets left out: what a hovered card
    /// asks for is motion over the picture it already has.
    fn drive_thumb(
        state_rc: &Rc<RefCell<LiveThumbState>>,
        frame_id: &str,
        source: &PreviewSource,
        fps: Option<f32>,
        kind: LeaseKind,
        signals: ThumbSignals,
    ) -> ThumbTick {
        let ThumbSignals {
            mut generation,
            mut badge,
            mut revealed,
            mut lamps,
            poster,
        } = signals;
        let poster_first = kind == LeaseKind::Poster;
        let hovering = kind == LeaseKind::Hover;
        let mut state = state_rc.borrow_mut();

        if state.observer.is_none() && !state.observer_broken {
            attach_observer(&mut state, state_rc, frame_id);
        }

        // Lease when the card is (or is assumed) visible and recovery
        // budget remains. The canvas for the current generation is already
        // mounted — the host's lease pipeline finds it by id.
        let assumed_visible = state.visible == Some(true) || state.observer_broken;
        let recovery_left =
            state.errors < THUMB_ERROR_LIMIT && state.remounts < THUMB_REMOUNT_LIMIT;
        if !recovery_left && poster_first {
            // Parked on an error badge over the gradient: nothing else is
            // going to happen to this card — until the cursor lands on it,
            // which leases through the hover arm below.
            stand_down(&mut state);
            return ThumbTick::Rest;
        }
        // A hovered card is on screen by definition, so hover neither waits
        // for the observer's first edge (the at-rest flow disconnected it at
        // stand-down) nor asks the at-rest recovery budgets for permission.
        // Its own bound is [`HOVER_REMOUNT_LIMIT`], spent in the error arm.
        let may_lease = hovering || (assumed_visible && recovery_left);
        if state.handle.is_none() && may_lease {
            if let Some(host) = preview_host() {
                let handle = host.lease(PreviewSlotRequest {
                    source: source.clone(),
                    canvas_id: thumb_canvas_id(frame_id, *generation.peek()),
                    fps,
                    // A poster lease buys a picture, not a preview: the
                    // host stops ticking the slot after this many presents
                    // while the runtime and the last frame stay readable.
                    profile: PreviewProfile {
                        frame_budget: poster_first.then_some(POSTER_FRAME_BUDGET),
                    },
                });
                state.last_revision = None;
                state.last_output_revision = None;
                state.poster_live_ticks = 0;
                state.handle = Some(handle);
            }
        }

        // Snapshot the handle's observables before mutating the state. The
        // output frame is read only when its revision moved — cloning a frame
        // is an Rc bump, but a repaint of the lamp field is not.
        let (presented_frames, revision, status, control_first, output_frame) = {
            let Some(handle) = &state.handle else {
                return ThumbTick::Poll;
            };
            let control_first = handle.control_first();
            let output_revision = handle.output_frame_revision();
            let output_frame = (control_first == Some(true)
                && state.last_output_revision != Some(output_revision))
            .then(|| (output_revision, handle.output_frame()));
            (
                handle.presented_frames(),
                handle.status_revision(),
                handle.status(),
                control_first,
                output_frame,
            )
        };
        let presented = presented_frames > 0;
        // A lease that actually presented proves the project renders: the
        // recovery budgets meter consecutive attempts WITHOUT progress, so
        // they reset here. Without this, an innocent card sharing a worker
        // with a permanently-failing project burns its whole remount budget
        // on that neighbour's recycles and parks even though every one of
        // its own leases was healthy.
        if presented && (state.errors != 0 || state.remounts != 0) {
            state.errors = 0;
            state.remounts = 0;
        }

        // The lamp half: a control-first project draws its published output.
        if let Some((output_revision, frame)) = output_frame {
            state.last_output_revision = Some(output_revision);
            if let Some(frame) = frame {
                state.lamp_fallback = !is_drawable_lamp_frame(&frame);
                if state.lamp_fallback {
                    if lamps.peek().is_some() {
                        lamps.set(None);
                    }
                } else {
                    lamps.set(Some(frame));
                }
            }
        }

        // Which layer owns the box. A control-first slot keeps its raster
        // canvas hidden — mounted and presenting, because the present is what
        // ticks the engine and carries the lamp answer home, but never shown —
        // until the lamps themselves land. Before that it is pending, exactly
        // like a visual slot before its first present: the gradient base.
        // A LiveRaster lease opts out: its consumer wants the shader surface
        // itself, so the raster reveals regardless of control-first.
        let raster_first = kind == LeaseKind::LiveRaster;
        let raster_revealed =
            presented && (raster_first || control_first != Some(true) || state.lamp_fallback);
        if *revealed.peek() != raster_revealed {
            revealed.set(raster_revealed);
        }
        // The tier the poster capture has to work with, kept before the
        // status is consumed below.
        let live_tier = match &status {
            PreviewSlotStatus::Live { tier, .. } => Some(*tier),
            _ => None,
        };
        if state.last_revision != Some(revision) {
            state.last_revision = Some(revision);
            match status {
                // Deploying keeps whatever the thumb showed; Suspended freezes
                // the canvas on its last frame (scroll-away) — both are
                // non-events for the overlay.
                PreviewSlotStatus::Deploying | PreviewSlotStatus::Suspended => {}
                PreviewSlotStatus::Live { tier, tier_reason } => {
                    // Hover writes no badge: a chip appearing on hover-in
                    // and changing on hover-out would be exactly the churn
                    // posters removed. The card keeps whatever its at-rest
                    // lease already said about it.
                    if !hovering {
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
                }
                PreviewSlotStatus::Error { reason } => {
                    // Release first: never reuse a canvas that a GPU-tier slot
                    // transferred. The host parks errored slots; recovery is
                    // ours (remount a fresh generation, lease again).
                    state.handle = None;
                    state.last_revision = None;
                    state.last_output_revision = None;
                    revealed.set(false);
                    // The lamps go with the canvas: the next lease redeploys the
                    // project and republishes them.
                    if lamps.peek().is_some() {
                        lamps.set(None);
                    }
                    // The transferred-canvas error is the EXPECTED recovery
                    // path after LRU eviction or a worker recycle (the host
                    // words it "already transferred — remount"), so it spends
                    // the remount budget, not the error budget. Matching on
                    // the message is a P5 candidate for a typed status flag.
                    let expected_remount = reason.contains("already transferred");
                    if hovering {
                        // Hover failures are quiet by construction: no badge
                        // (the at-rest machinery owns that) and none of the
                        // at-rest budgets, which a hover must never spend
                        // into parking a card the user only pointed at. The
                        // one retry worth making is the expected one — the
                        // poster lease's GPU canvas was consumed, so remount
                        // a fresh generation and lease again.
                        if expected_remount && state.hover_remounts < HOVER_REMOUNT_LIMIT {
                            state.hover_remounts = state.hover_remounts.saturating_add(1);
                            generation += 1;
                            return ThumbTick::Poll;
                        }
                        log::debug!("gallery hover preview #{frame_id} stood down: {reason}");
                        return ThumbTick::Rest;
                    }
                    if expected_remount {
                        state.remounts = state.remounts.saturating_add(1);
                    } else {
                        state.errors = state.errors.saturating_add(1);
                    }
                    if state.errors >= THUMB_ERROR_LIMIT || state.remounts >= THUMB_REMOUNT_LIMIT {
                        // Workers killed by an aborted wasm fetch mean the page
                        // is tearing down (refresh/navigation/HMR): the badge
                        // still parks, but the console stays quiet.
                        if lpa_studio_core::is_teardown_abort_reason(&reason) {
                            log::debug!("gallery preview #{frame_id} gave up: {reason}");
                        } else {
                            log::warn!("gallery preview #{frame_id} gave up: {reason}");
                        }
                        badge.set(Some(ThumbPreviewBadge::Error { reason }));
                    } else {
                        generation += 1; // fresh canvas element; re-lease next tick
                        if badge.peek().is_some() {
                            badge.set(None);
                        }
                    }
                    // The next tick re-leases (or stands the thumb down on
                    // an exhausted budget); there is nothing to capture from
                    // a slot that just died.
                    return ThumbTick::Poll;
                }
            }
        }

        if !poster_first {
            return ThumbTick::Poll;
        }
        capture_poster(
            &mut state,
            PosterCapture {
                frame_id,
                source,
                generation: *generation.peek(),
                presented_frames,
                live_tier,
                raster_revealed,
            },
            lamps,
            poster,
        )
    }

    /// Everything one poster-capture attempt reads about its slot.
    struct PosterCapture<'a> {
        frame_id: &'a str,
        source: &'a PreviewSource,
        /// Canvas generation currently mounted (the CPU-tier read target).
        generation: u32,
        presented_frames: u64,
        /// Granted tier, `Some` only while the slot is `Live`.
        live_tier: Option<PreviewTier>,
        /// The raster canvas is the picture this thumb shows (not a hidden
        /// engine-ticking present under a lamp field).
        raster_revealed: bool,
    }

    /// The poster half of a tick: once the slot has spent its frame budget,
    /// capture the picture its quadrant offers, cache it, and report that
    /// the thumb is finished.
    ///
    /// Waiting for the FULL budget (rather than the first present) is what
    /// makes the poster worth looking at: a program's first frame is
    /// usually black, and on the GPU tier the lamp samples read back a
    /// frame behind the render.
    fn capture_poster(
        state: &mut LiveThumbState,
        slot: PosterCapture<'_>,
        lamps: Signal<Option<UiControlProductPreview>>,
        mut poster: Signal<Option<String>>,
    ) -> ThumbTick {
        let Some(tier) = slot.live_tier else {
            // Deploying (possibly still queued behind the pool) or
            // suspended off-screen: the clock does not run.
            return ThumbTick::Poll;
        };
        state.poster_live_ticks = state.poster_live_ticks.saturating_add(1);
        if slot.presented_frames < u64::from(POSTER_FRAME_BUDGET) {
            if state.poster_live_ticks < POSTER_LIVE_TICK_LIMIT {
                return ThumbTick::Poll;
            }
            log::debug!(
                "gallery poster #{}: slot presented {} of {POSTER_FRAME_BUDGET} frames; \
                 releasing with the placeholder",
                slot.frame_id,
                slot.presented_frames
            );
            stand_down(state);
            return ThumbTick::Rest;
        }

        // Which quadrant this slot is in decides where the picture is read
        // from (see `thumb_poster`'s module docs). The lamp field is the
        // one this thumb is already drawing, so poster and live layer can
        // never be different pictures.
        let lamp_frame = lamps.peek();
        let captured = match (lamp_frame.as_ref(), slot.raster_revealed, tier) {
            // Control-first, either tier: the lamps ARE the card's picture,
            // rasterized from the slot's own output frame.
            (Some(frame), _, _) => lamp_poster(frame),
            // Shader-only on the CPU tier: the host blits those frames onto
            // the card's canvas with `putImageData`, so it reads back.
            (None, true, PreviewTier::Cpu) => {
                canvas_poster(&thumb_canvas_id(slot.frame_id, slot.generation))
            }
            // Shader-only on the GPU tier: the canvas was permanently
            // handed to the worker by `transferControlToOffscreen`, so
            // there is nothing readable on this thread. The WORKER captures
            // instead: one `capture_poster` round-trip (render at poster
            // size + async texture readback), answered through the handle.
            (None, true, PreviewTier::Gpu) => {
                let frame = state
                    .handle
                    .as_ref()
                    .and_then(|handle| handle.poster_frame());
                match frame {
                    Some(frame) => pixel_poster(&frame),
                    None => {
                        // Idempotent while pending; survives worker
                        // recycles until answered.
                        if let Some(handle) = &state.handle {
                            handle.request_poster_capture(POSTER_WIDTH, POSTER_HEIGHT);
                        }
                        if state.poster_live_ticks < POSTER_LIVE_TICK_LIMIT {
                            return ThumbTick::Poll;
                        }
                        log::debug!(
                            "gallery poster #{}: worker poster capture did not answer; \
                             releasing with the placeholder",
                            slot.frame_id
                        );
                        stand_down(state);
                        return ThumbTick::Rest;
                    }
                }
            }
            // A control-first slot that spent its budget without ever
            // publishing a drawable output frame: its raster is presenting
            // HIDDEN (it only exists to tick the engine), so capturing it
            // would poster a picture this card never shows. Nothing to
            // take — release and keep the gradient.
            (None, false, _) => {
                log::debug!(
                    "gallery poster #{}: control-first slot published no drawable output",
                    slot.frame_id
                );
                stand_down(state);
                return ThumbTick::Rest;
            }
        };
        match captured {
            Ok(data_url) => {
                store_poster(slot.source, data_url.clone());
                poster.set(Some(data_url));
            }
            // A failed capture is not worth a badge: the card keeps
            // whatever it is already showing, and costs nothing further.
            Err(error) => log::debug!("gallery poster #{}: {error}", slot.frame_id),
        }
        stand_down(state);
        ThumbTick::Rest
    }

    /// Whether an output frame is a picture this thumb can actually draw.
    ///
    /// [`LampView`] needs a 2D display layout with lamps in it, and RGB
    /// samples to colour them from. A control-first project with neither —
    /// the engine refusing dome-scale geometry over its wire budget, an
    /// output with zero lamps — falls back to the raster thumb: today's
    /// picture is a truer card than a permanently blank box. A frame that has
    /// simply not arrived yet is a different state (pending, the gradient),
    /// and it never reaches here.
    ///
    /// [`LampView`]: crate::app::node::lamp_view::LampView
    fn is_drawable_lamp_frame(frame: &UiControlProductPreview) -> bool {
        let has_lamps = matches!(
            frame.display_layout.as_deref(),
            Some(ControlDisplayLayout::Layout2d(layout)) if !layout.lamps.is_empty()
        );
        has_lamps && control_sample_layout_has_rgb(frame)
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
