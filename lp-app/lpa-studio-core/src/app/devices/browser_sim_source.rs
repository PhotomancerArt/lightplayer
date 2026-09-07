//! [`SimLinkSource`] over `fw-browser` workers (wasm only): what a running
//! sim actually is in the browser.
//!
//! The thinnest possible join, on purpose. `lpa-link`'s `BrowserWorkerLink`
//! already turns a worker's envelope channel into the model's `Link`, and
//! `BrowserWorkerLinkIo` already turns the same channel into the
//! `lpa_client::ClientIo` the exclusive-borrow conversations speak through.
//! What is left here is the three things neither of them can know: which
//! engine assets this page serves, what board the sim's record says it is
//! (its `target`, resolved to something a sim can honestly wear), and that
//! a running sim's control handle is the [`SimRuntimeControl`] the studio's
//! transport asks for.
//!
//! One worker per powered-on sim. Powering one off drops its backing, and
//! dropping a `BrowserWorkerHandle` terminates its worker — so a stopped sim
//! costs nothing and there is no lifecycle to get wrong beyond the one the
//! transport already owns.
//!
//! ⚠️ **wasm-only, so `just test` never sees it.** The model half it plugs
//! into is host-covered through the bench's own source; what is untested
//! here is the worker join itself, which the fw-browser smoke and the
//! browser walk cover instead (the plan's validation strategy).

use std::cell::RefCell;
use std::rc::Rc;

use lpa_link::device_link::browser_worker::{BrowserWorkerControl, BrowserWorkerLink};
use lpa_link::device_link::browser_worker_io::BrowserWorkerLinkIo;
use lpa_link::providers::browser_worker::{BrowserRuntimeTier, BrowserWorkerOptions};

use crate::app::library::ProjectTarget;

use super::device_transport::{DeviceTransportFuture, GrantedLink, LensLineTap, LensTapEvent};
use super::sim_record::sim_link_info;
use super::sim_transport::{SimBacking, SimLinkSource, SimRuntimeControl, SimSession};

/// Sims backed by `fw-browser` workers.
pub struct BrowserSimLinkSource {
    /// Where the engine assets live. Shared and late-filled because the
    /// hashed names come from a fetch the shell starts in `index.html`, and
    /// this source is built while the page is still assembling.
    options: Rc<RefCell<BrowserWorkerOptions>>,
}

impl BrowserSimLinkSource {
    /// A source that resolves the page's hashed engine URLs in the
    /// background — the SAME `resolved_engine_urls` every other worker boots
    /// from, so there is one place those names come from.
    ///
    /// Until the resolution lands the unhashed defaults stand, which is the
    /// documented degrade (a worker booted from them still boots, merely
    /// revalidated rather than immutable-cached). Nothing can be powered on
    /// before a user gesture, and the promise is one the page already
    /// started, so the window is a microtask wide.
    pub fn resolving() -> Self {
        let options = Rc::new(RefCell::new(BrowserWorkerOptions::default()));
        let resolved = Rc::clone(&options);
        wasm_bindgen_futures::spawn_local(async move {
            *resolved.borrow_mut() =
                lpa_link::providers::browser_worker::resolved_engine_urls().await;
        });
        Self { options }
    }

    /// A source pinned to `options` (the standalone smoke page, and any
    /// caller that already knows where its engine lives).
    pub fn new(options: BrowserWorkerOptions) -> Self {
        Self {
            options: Rc::new(RefCell::new(options)),
        }
    }
}

impl SimLinkSource for BrowserSimLinkSource {
    fn open(&self, session: &SimSession) -> Result<SimBacking, String> {
        let info = sim_link_info(&session.uid, &session.display_name);
        // WHAT THIS SIM WEARS. The record's `target` is the board it was
        // created as; a board with no checked-in runtime manifest cannot be
        // simulated honestly, so it runs as Desktop and SAYS so rather than
        // wearing a table nobody authored (`resolve_for_sim`).
        let (worn, notice) = ProjectTarget::from_manifest(Some(&session.target)).resolve_for_sim();
        if let Some(notice) = notice {
            // The source hands back a `SimBacking` or a failure and has no
            // notice channel of its own, so the fallback goes through the
            // same `log` the controller's power-on failures use rather than
            // going unsaid.
            log::warn!("sim {}: {notice}", session.uid);
        }
        // PD12: sims ask for the GPU tier and the worker answers with what
        // it granted — recorded and surfaced, never silent. PD6: the base
        // MAC is Studio-minted and travels as the runtime's identity, so
        // the hello the sim sends names the device the record already is.
        //
        // `resolve_for_sim` only ever hands back a target that HAS a
        // manifest, so the `None` arm is unreachable by construction — it
        // is a refusal rather than an unwrap because a sim that cannot say
        // what it is should fail to power on, not panic the tab.
        let runtime = worn
            .runtime_options(BrowserRuntimeTier::Gpu)
            .ok_or_else(|| format!("no hardware profile is checked in for {}", worn.board_id()))?
            .with_identity(session.base_mac.clone());
        let link = BrowserWorkerLink::new(
            info.clone(),
            self.options.borrow().clone().with_runtime(runtime),
        );
        // Taken BEFORE the link is boxed: the control shares the link's own
        // inner, so a restart is the same destroy-and-recreate a
        // `RunReset` performs and the effects layer keeps the link it
        // borrowed.
        let control = link.control();
        Ok(SimBacking {
            link: GrantedLink {
                link: Box::new(link),
                info,
            },
            control: Rc::new(WorkerRuntimeControl { control }),
        })
    }
}

/// [`SimRuntimeControl`] over a worker link's control handle.
struct WorkerRuntimeControl {
    control: BrowserWorkerControl,
}

impl SimRuntimeControl for WorkerRuntimeControl {
    fn restart(&self) -> DeviceTransportFuture<Result<(), String>> {
        let control = self.control.clone();
        Box::pin(async move { control.restart().await })
    }

    fn set_hardware_manifest(&self, manifest_json: String) {
        self.control.set_hardware_manifest(manifest_json);
    }

    fn client_io(&self, tap: Option<LensLineTap>) -> Result<Box<dyn lpa_client::ClientIo>, String> {
        let io = BrowserWorkerLinkIo::new(self.control.clone());
        Ok(Box::new(match tap {
            // The studio's tap vocabulary is its own; `lpa-link` stays
            // independent of it, so the join is one closure.
            Some(tap) => {
                let tap: Rc<dyn Fn(String)> =
                    Rc::new(move |line: String| tap(LensTapEvent::Line(line)));
                io.with_tap(tap)
            }
            None => io,
        }))
    }
}
