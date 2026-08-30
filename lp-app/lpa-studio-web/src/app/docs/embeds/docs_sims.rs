//! Page-scoped docs sims: boot, view/tick loops, and — the part that
//! matters — teardown.
//!
//! An article that declares [`DocsSimSpec`]s gets one
//! [`DocsSimHost`] per spec when the page mounts. Each host is a **real**
//! `StudioController` + actor around its own browser-worker sim; embeds
//! read its change-gated `UiStudioView` out of a `Signal` and dispatch
//! real `UiAction`s back into it. Nothing here is a mirror or a shadow
//! controller (interactive-docs D1).
//!
//! # Lifetimes, and why they differ
//!
//! - the **provider is NOT remounted per article.** Under path routing an
//!   article switch re-renders [`DocsSimProvider`] in place with a new
//!   `sims` slice (a `key` on the component does not force a remount —
//!   that assumption shipped the "names sim `main`, which this page does
//!   not declare" defect: the registry kept the previous article's sims).
//!   The provider therefore detects the spec-slice change itself, retires
//!   the old page's hosts, and boots the new page's into a signal-backed
//!   registry every embed re-reads reactively.
//! - the **actor** is spawned with `wasm_bindgen_futures::spawn_local`, so
//!   it survives retirement and unmount alike. That is deliberate and load
//!   bearing: [`DocsSimHost::shutdown`] only *enqueues* the teardown
//!   (StopSimulator → Shutdown), and a dropped-but-unpolled actor future
//!   can never process that queue.
//! - the **view loop** is scope-tied (`spawn`) and additionally ends on
//!   its own when a retired host's actor closes the view channel. The
//!   **tick loop** holds only a [`std::rc::Weak`] to its host: retiring a
//!   sim drops the registry's `Rc`, the next upgrade fails, and the loop
//!   exits — otherwise every article switch would leave another immortal
//!   ticker running in the provider's scope.
//! - [`use_drop`] calls `shutdown()` on every *current* host. `DocsSimHost`
//!   also shuts down on `Drop` as a backstop, but the explicit call is the
//!   contract — see the type's docs in `lpa-studio-core`.
//!
//! Verified in a browser by patching `Worker.prototype.terminate`:
//! navigating away from the article calls it exactly twice (one per docs
//! sim), and a boot/teardown round trip adds exactly two more. Nothing
//! accumulates. If you change anything on this path, re-run that check —
//! a leak here is invisible until a reader's tab has ten dead workers in
//! it.
//!
//! # Visibility
//!
//! The sim firmware **self-ticks**: pausing does not freeze the pixels, it
//! freezes the *view pull* (and the view rebuild behind it). The tick loop
//! reads `document.hidden` each time round and simply skips the tick while
//! the tab is in the background, which costs one property read per beat
//! and needs no listener. Per-embed `IntersectionObserver` gating (stop
//! pulling for a sim scrolled far off screen) is a further step this phase
//! deliberately does not take — see the module docs in `super`.
//!
//! Consequence worth knowing: an article opened into a **background** tab
//! never pulls, so its sim canvases sit in their loading state until the
//! reader looks at the tab, then fill within one delay beat. That is the
//! intended trade (a hidden article should cost nothing), not a bug — but
//! it does mean automated checks of this page must fake
//! `document.hidden`, because a driven browser tab reports hidden.
//!
//! # What docs sims are not
//!
//! They are never roster sessions (D2): each lives in its own controller's
//! pool, never appears as a device card, and never takes the main app's
//! lens. Deploys ride `ProjectOp::OpenDocsExample`, which writes nothing
//! to the user's library.

use std::rc::Rc;

use dioxus::prelude::*;
use lpa_studio_core::{DocsSimHost, UiAction, UiStudioView};

use super::super::DocsSimSpec;

/// One booted sim, as an embed sees it.
#[derive(Clone)]
pub(crate) struct DocsSim {
    /// The article-facing handle (`sim=disc`).
    pub(crate) name: &'static str,
    /// The example this sim runs — what its reset re-deploys.
    pub(crate) example_id: &'static str,
    /// The host's latest view. `UiStudioView::empty()` until the first
    /// pull lands, which is what the embeds' loading states render from.
    pub(crate) view: Signal<UiStudioView>,
    /// `None` in story builds and on host builds: the embed still renders
    /// (from whatever the view signal holds) and dispatches nowhere.
    host: Option<Rc<DocsSimHost>>,
}

impl DocsSim {
    /// Dispatch into this sim's controller — the same path a Studio
    /// surface dispatches through.
    pub(crate) fn dispatch(&self, action: UiAction) {
        if let Some(host) = &self.host {
            host.dispatch(action);
        }
    }

    /// Re-deploy this sim's example: the docs "reset" (pristine files, the
    /// same running worker).
    pub(crate) fn reset(&self) {
        if let Some(host) = &self.host {
            host.reset(self.example_id);
        }
    }
}

/// The page-local registry embeds resolve `sim=` against.
///
/// Signal-backed on purpose: the provider swaps the whole sim list in place
/// on an article switch (no remount — see the module docs), and reading
/// through the signal is what re-points every embed at the new page's sims.
#[derive(Clone, Copy)]
pub(crate) struct DocsSimRegistry {
    sims: Signal<Rc<Vec<DocsSim>>>,
}

impl DocsSimRegistry {
    /// One sim by article handle.
    pub(crate) fn get(&self, name: &str) -> Option<DocsSim> {
        self.sims
            .read()
            .iter()
            .find(|sim| sim.name == name)
            .cloned()
    }

    /// Resolve a `sim=` argument, which may name several sims
    /// comma-separated (`sim=disc,grid` — the fan-out case). Returns the
    /// resolved sims in the order written, plus any handle that did not
    /// resolve so the caller can say so out loud.
    pub(crate) fn resolve(&self, argument: &str) -> (Vec<DocsSim>, Vec<String>) {
        let mut found = Vec::new();
        let mut missing = Vec::new();
        for name in argument.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            match self.get(name) {
                Some(sim) => found.push(sim),
                None => missing.push(name.to_string()),
            }
        }
        (found, missing)
    }
}

/// The main app's action dispatch, lent to the docs section so
/// `open-in-studio` can run the real open flow.
///
/// The handler is refreshed into the cell on every [`super::super::DocsPage`]
/// render rather than captured once, so a click always rides the live
/// dispatcher. Absent (stories, host builds) the button renders inert —
/// the settings-chip precedent.
#[derive(Clone)]
pub(crate) struct DocsStudioActions(
    pub(crate) Rc<std::cell::RefCell<Option<EventHandler<UiAction>>>>,
);

impl DocsStudioActions {
    /// An empty cell, filled by `DocsPage` each render.
    pub(crate) fn empty() -> Self {
        Self(Rc::new(std::cell::RefCell::new(None)))
    }

    /// Whether a dispatcher is available right now (read during render to
    /// decide whether the button is live or inert).
    pub(crate) fn is_live(&self) -> bool {
        self.0.borrow().is_some()
    }

    /// Dispatch, if the app lent us a dispatcher.
    pub(crate) fn dispatch(&self, action: UiAction) {
        if let Some(handler) = self.0.borrow().as_ref() {
            handler.call(action);
        }
    }
}

/// Boot this page's sims, drive them, and provide the registry to the
/// article below.
///
/// An article switch does NOT remount this component — the router swaps
/// the `sims` slice and the markdown underneath it in one re-render (a
/// `key` on the component does not force a remount). So the switch is
/// handled here: when the spec slice changes, the previous page's hosts
/// are shut down and the new page's boot into the registry signal, before
/// the children (the article's embeds) render against it.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DocsSimProvider(sims: &'static [DocsSimSpec], children: Element) -> Element {
    let mut booted = use_signal(|| Rc::new(Vec::<DocsSim>::new()));
    // Which spec slice `booted` currently serves. `PAGES` entries are
    // statics, so slice identity (pointer + length) is page identity.
    // Two pages declaring CONTENT-identical slices may or may not get
    // deduplicated into one static by the compiler; both outcomes are
    // correct here — shared identity keeps the warm sim across the
    // switch, distinct identity retires and re-boots the same spec.
    let active = use_hook(|| Rc::new(std::cell::Cell::new(None::<&'static [DocsSimSpec]>)));

    let switched = !matches!(active.get(), Some(current) if core::ptr::eq(current, sims));
    if switched {
        // LEAK-CRITICAL, part 1: retire the previous article's hosts. Only
        // this enqueued StopSimulator reaches `worker.terminate()`; the
        // retired tick loops exit on their next failed Weak upgrade once
        // the registry's Rc goes.
        for sim in booted.peek().iter() {
            if let Some(host) = &sim.host {
                host.shutdown();
            }
        }
        // Written during render, deliberately: this component never reads
        // `booted` reactively (peek only), and the swap must land before
        // the children render so an embed never resolves against the
        // previous article's registry.
        booted.set(Rc::new(boot_sims(sims)));
        active.set(Some(sims));
    }

    use_drop(move || {
        // LEAK-CRITICAL, part 2: the same retirement for leaving the docs
        // section entirely. Nothing in the actor/controller/pool/session
        // chain terminates the browser Worker on drop; see the lifecycle
        // contract on `lpa_studio_core::DocsSimHost`.
        for sim in booted.peek().iter() {
            if let Some(host) = &sim.host {
                host.shutdown();
            }
        }
    });

    use_context_provider(|| DocsSimRegistry { sims: booted });

    rsx! {
        {children}
    }
}

/// Boot one host per spec and start its loops. Host builds (unit tests)
/// and story builds boot nothing — there is no browser worker to talk to,
/// and stories must never spin one up.
#[allow(
    unused_variables,
    reason = "the specs are only reachable on the wasm arm"
)]
fn boot_sims(specs: &'static [DocsSimSpec]) -> Vec<DocsSim> {
    #[cfg(target_arch = "wasm32")]
    {
        if try_consume_context::<crate::app::home::gallery_preview::StaticThumbPreviews>().is_some()
        {
            // The story book's marker: no live workers under capture.
            return Vec::new();
        }
        specs.iter().map(boot_sim).collect()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Vec::new()
    }
}

/// Boot one docs sim: a fresh controller with only the platform seams it
/// genuinely needs, its actor spawned to outlive this page, and the two
/// loops that keep the view fresh.
///
/// The controller deliberately gets **no** log sink (a second drainer
/// silently steals the app actor's records), no agent hooks, no settings,
/// no clipboard, and no library. A docs sim is a shader on a simulated
/// board and nothing else.
#[cfg(target_arch = "wasm32")]
fn boot_sim(spec: &'static DocsSimSpec) -> DocsSim {
    use gloo_timers::future::TimeoutFuture;
    use lpa_studio_core::StudioController;

    let mut controller = StudioController::new(crate::web_app::now_secs);
    // The simulator's connect-ladder backoff runs on browser timers;
    // without this the core default resolves every sleep immediately.
    controller.set_sim_timers(crate::web_app::make_device_timers());
    controller.set_random(crate::library_host_opfs::random_bytes);

    let mut host = DocsSimHost::boot(
        spec.example_id,
        controller,
        // NOT the scope-tied `spawn`: the actor has to survive unmount long
        // enough to process the teardown queue.
        wasm_bindgen_futures::spawn_local,
        crate::web_app::make_pull_timer,
    );
    let mut view = Signal::new(UiStudioView::empty());
    if let Some(mut view_rx) = host.take_view() {
        spawn(async move {
            while let Some(next) = view_rx.recv().await {
                view.set(next);
            }
        });
    }

    let host = Rc::new(host);
    // Weak on purpose: the registry's Rc is this host's lifeline. When the
    // article switches (or the page unmounts) the registry drops it, the
    // upgrade fails, and this loop exits — a strong Rc here would keep a
    // retired host's ticker alive for as long as the docs section lives.
    let tick_host = Rc::downgrade(&host);
    spawn(async move {
        loop {
            let Some(live) = tick_host.upgrade() else {
                break;
            };
            let delay = live.next_refresh_delay();
            drop(live);
            TimeoutFuture::new(delay.as_millis() as u32).await;
            let Some(live) = tick_host.upgrade() else {
                break;
            };
            // Hidden tab: skip the pull. The sim keeps self-ticking; only
            // the view (and the rebuild behind it) freezes.
            if !document_hidden() {
                live.send_refresh_tick();
            }
        }
    });

    DocsSim {
        name: spec.name,
        example_id: spec.example_id,
        view,
        host: Some(host),
    }
}

/// Whether the tab is in the background right now.
#[cfg(target_arch = "wasm32")]
fn document_hidden() -> bool {
    web_sys::window()
        .and_then(|window| window.document())
        .is_some_and(|document| document.hidden())
}
