//! [`EmuLinkSource`] over the tab's own emulator Worker (wasm only): what a
//! running emu actually is in the browser.
//!
//! The thinnest possible join, like `browser_sim_source.rs`. `lpa-link`'s
//! `EmulatorTabStream` already turns the page's board into the byte pipe
//! `ByteStreamLink` drives, and `EmulatorTabControl` already answers the
//! verbs. What is left here is the two things neither of them can know:
//! which packaged build this target is born flashed with (D22), and that a
//! running board's control handle is the [`EmuRuntimeControl`] the studio's
//! transport asks for.
//!
//! One Worker per powered-on emu. Powering one off drops its backing, which
//! is what ends the Worker; the persisted 4 MiB image survives, because
//! powering off is not forgetting — [`EmuLinkSource::forget`] is.
//!
//! # Where the module URL comes from, and when
//!
//! From the page, at POWER-ON, inside the port's own boot — not from a
//! snapshot taken here. `window.__lpEngineAssets` is a promise of the
//! manifest that names the content-hashed sidecar, and a link is built at
//! power-on, which can be a page load's first action. A URL read any
//! earlier is the unhashed fallback, and the served `pkg/` has no such
//! file: that exact mistake 404'd every sim worker on the hashed-engine
//! build (G1, 2026-09-07). So the bridge awaits the manifest in its boot
//! chain and fails the board BY NAME when the key is absent ("this build
//! ships no emulator module"), which the link reports as an error event —
//! `open` here is synchronous, as the sim's is, and has nothing to await.
//!
//! ⚠️ **wasm-only, so `just test` never sees it.** The model half it plugs
//! into is host-covered through `emu_transport.rs`'s counting source.

use std::rc::Rc;

use lpa_link::device_link::byte_stream::ByteStreamLink;
use lpa_link::providers::browser_serial_esp32_options::BrowserSerialEsp32Options;
use lpa_link::providers::emulator_tab::{
    EmulatorTabControl, EmulatorTabOptions, EmulatorTabPort, EmulatorTabStream, delete_emu_flash,
};

use super::device_transport::{DeviceTransportFuture, GrantedLink, LensLineTap, LensTapEvent};
use super::emu_transport::{EmuBacking, EmuLinkSource, EmuRuntimeControl, EmuSession};
use super::sim_record::emu_link_info;

/// `path` as an absolute URL, resolved against the SITE ROOT.
///
/// The root, not the current address. `firmware/<build id>/` is published
/// at the site root by `lp-cli firmware package`, exactly as the engine
/// sidecars are (`/pkg/…`, `sync-engine-sidecar.sh`), so a `./firmware/…`
/// resolved against the page would name a different URL on `/devices` than
/// on `/p/<slug>-prj…` and be wrong on all but one of them.
///
/// A path that is already absolute comes back unchanged. A build with no
/// `window` gets the path back as it was, which fails later and visibly
/// rather than here and silently.
fn absolute_from_page(path: &str) -> String {
    let Some(origin) = web_sys::window().and_then(|window| window.location().origin().ok()) else {
        return path.to_string();
    };
    match web_sys::Url::new_with_base(path, &format!("{origin}/")) {
        Ok(url) => url.href(),
        Err(_) => path.to_string(),
    }
}

/// Emus backed by the tab's emulator Worker.
pub struct BrowserEmuLinkSource {
    /// A pinned module URL, or `None` — the served build — to ask the page
    /// at power-on. See the module docs.
    module_url: Option<String>,
    /// Where packaged firmware lives, shared with the serial flasher so a
    /// build id names the same bytes on both paths.
    firmware: BrowserSerialEsp32Options,
}

impl Default for BrowserEmuLinkSource {
    fn default() -> Self {
        Self::resolving()
    }
}

impl BrowserEmuLinkSource {
    /// A source whose boards resolve the page's hashed module URL at
    /// power-on. What a served Studio build uses.
    pub fn resolving() -> Self {
        Self {
            module_url: None,
            firmware: BrowserSerialEsp32Options::default(),
        }
    }

    /// A source pinned to `module_url` (a caller that already knows where
    /// its emulator lives).
    pub fn pinned(module_url: impl Into<String>) -> Self {
        Self {
            module_url: Some(module_url.into()),
            firmware: BrowserSerialEsp32Options::default(),
        }
    }

    /// Point the born-flashed fetch at a different packaged-firmware base.
    pub fn with_firmware_options(mut self, firmware: BrowserSerialEsp32Options) -> Self {
        self.firmware = firmware;
        self
    }

    /// The build a board of `target` is born flashed with (D22), or `None`
    /// when this build serves none for it — a blank chip, which is the
    /// card's needs-firmware face and its Flash verb, not a failure.
    ///
    /// **Absolute, resolved against the page.** The shared base path is
    /// relative (`./firmware`, `browser_serial_esp32_options.rs`), which is
    /// fine for the serial flasher because it fetches from the page. This
    /// one is fetched by the WORKER, whose base URL is
    /// `/lpa-link/emulator_worker.js` — so a relative path becomes
    /// `/lpa-link/firmware/…`, the SPA's catch-all answers it with
    /// `index.html`, and the board comes up blank with
    /// `Unexpected token '<'` in the journal (measured 2026-09-11, the
    /// first browser walk of the emu row). Resolving here, where the page
    /// is, keeps the value self-describing everywhere downstream.
    fn manifest_url_for(&self, target: &str) -> Option<String> {
        let board = lpa_boards::board_by_id(target)?;
        let build_id = lpa_boards::provisioning_build_id(Some(board), Some(board.family.as_str()))?;
        Some(absolute_from_page(
            &self.firmware.firmware_manifest_path(build_id),
        ))
    }
}

impl EmuLinkSource for BrowserEmuLinkSource {
    fn open(&self, session: &EmuSession) -> Result<EmuBacking, String> {
        let info = emu_link_info(&session.uid, &session.display_name);
        let manifest_url = self.manifest_url_for(&session.target);
        if manifest_url.is_none() {
            // Not a refusal: a blank chip is a legible state with a verb
            // on the card. But it is worth saying once, because a board
            // with no served build is usually a packaging gap.
            log::warn!(
                "emu {}: no served firmware build for {}; the chip comes up blank",
                session.uid,
                session.target
            );
        }
        let port = EmulatorTabPort::open(&EmulatorTabOptions {
            uid: session.uid.clone(),
            // PD6's rule, as for a sim: the base MAC is Studio-minted and
            // travels as the board's efuse identity, so the hello it sends
            // names the device the record already is.
            mac: session.base_mac.clone(),
            module_url: self.module_url.clone(),
            manifest_url,
            // The uid keys the image, so a Forget by uid finds it (D15).
            persist_key: Some(session.uid.clone()),
        })
        .map_err(|error| error.to_string())?;
        // The stream and the control share the port, which is what makes
        // the exclusive borrow mean something: an effect's io drains the
        // same buffer the paused pump would have.
        let link = ByteStreamLink::new(info.clone(), EmulatorTabStream::new(port));
        Ok(EmuBacking {
            link: GrantedLink {
                link: Box::new(link),
                info,
            },
            control: Rc::new(TabRuntimeControl {
                control: EmulatorTabControl::new(port),
            }),
        })
    }

    fn forget(&self, uid: &str) -> DeviceTransportFuture<Result<(), String>> {
        let uid = uid.to_string();
        Box::pin(async move {
            delete_emu_flash(&uid)
                .await
                .map_err(|error| error.to_string())
        })
    }
}

/// [`EmuRuntimeControl`] over a tab board's control handle.
struct TabRuntimeControl {
    control: EmulatorTabControl,
}

impl EmuRuntimeControl for TabRuntimeControl {
    fn flash_package(&self, manifest_url: String) -> DeviceTransportFuture<Result<String, String>> {
        let control = self.control.clone();
        Box::pin(async move {
            control
                .flash_package(&manifest_url)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn erase(&self) -> DeviceTransportFuture<Result<(), String>> {
        let control = self.control.clone();
        Box::pin(async move { control.erase().await.map_err(|error| error.to_string()) })
    }

    fn reset(&self) -> DeviceTransportFuture<Result<(), String>> {
        let control = self.control.clone();
        Box::pin(async move { control.reset().await.map_err(|error| error.to_string()) })
    }

    fn dilation(&self) -> Option<f64> {
        self.control.dilation()
    }

    fn is_starting(&self) -> bool {
        self.control.is_starting()
    }

    fn client_io(&self, tap: Option<LensLineTap>) -> Result<Box<dyn lpa_client::ClientIo>, String> {
        // The studio's tap vocabulary is its own; `lpa-link` stays
        // independent of it, so the join is one closure.
        let tap: Option<Rc<dyn Fn(String)>> = tap.map(|tap| {
            Rc::new(move |line: String| tap(LensTapEvent::Line(line))) as Rc<dyn Fn(String)>
        });
        Ok(self.control.client_io(tap))
    }
}
