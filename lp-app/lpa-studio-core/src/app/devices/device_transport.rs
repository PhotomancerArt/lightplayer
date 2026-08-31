//! The platform seam the effects layer opens links through.
//!
//! `lpa-devices` inverts the transport dependency (it defines
//! [`Link`](lpa_devices::Link), `lpa-link` implements it) — but *something*
//! still has to know how to enumerate the grants this origin holds, pop a
//! chooser, and hand a grant back. That something is platform-shaped, so it
//! is a trait here and an implementation per platform:
//!
//! | build | implementation |
//! |---|---|
//! | wasm + `browser-serial-esp32` | `BrowserSerialTransport` (`browser_transport.rs`) over `BrowserSerialEsp32Provider` |
//! | host tests | a fake over `lpa_link::device_link::fake` (see `device_roster`'s tests) |
//! | anything else | none installed: the roster stays empty and says so |
//!
//! Everything here is `?Send` and single-threaded on purpose: the whole
//! studio is (`Rc` everywhere), and a serial port is not a thing to share
//! across threads in a browser.

use core::future::Future;
use core::pin::Pin;

use lpa_devices::link::{Link, LinkInfo};

/// A future the effects layer awaits inside a spawned task, never in the
/// fold path (invariant I7).
pub type DeviceTransportFuture<T> = Pin<Box<dyn Future<Output = T>>>;

/// One grant this origin holds: the model's static facts about it, plus the
/// live link that speaks to it.
///
/// Holding a grant is NOT being connected: the link is built closed, and
/// nothing touches the port until the model sends `LinkCommand::Open`. That
/// distinction is the model's to make, so the transport never pre-empts it.
pub struct GrantedLink {
    pub info: LinkInfo,
    pub link: Box<dyn Link>,
}

/// One coarse-effect call, in platform terms. The model's
/// [`EffectRequest`](lpa_devices::EffectRequest) is resolved into this by
/// the effects layer (build ids stay opaque; the board manifest is resolved
/// to its JSON before it crosses the seam, so a transport never depends on
/// `lpa-boards`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviceEffectCall {
    /// esptool flash of a packaged build. The chip guard and the pre-write
    /// base-MAC read live below this seam and are load-bearing.
    FlashFirmware { build_id: String },
    /// Write the board runtime manifest to `/hardware.json` over the app
    /// protocol (board-selection D4; effective next boot).
    WriteHardwareManifest { manifest_json: String },
    /// Run the `lpa-client` push conversation over the borrowed port: find
    /// the storage dir the board runs from, replace it, load it, verify the
    /// package hash.
    ///
    /// The files arrive already resolved — the app read them out of the
    /// library (or out of the live handle for a project open in this tab)
    /// before the gesture was folded, because the model must not carry
    /// project bytes through its journal.
    PushProject {
        files: Vec<(String, Vec<u8>)>,
        /// The library copy's canonical hash. A device that ends up with
        /// anything else is a failed push, not a quiet one.
        expected_hash: String,
        /// Where to write when the board reports nothing loaded — a
        /// freshly flashed board has no dir to replace.
        fallback_storage_id: String,
    },
}

/// What a finished effect learned, beyond succeeding.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceEffectFacts {
    /// One line for the activity outcome.
    pub summary: String,
    /// The base MAC the flash preflight read from efuse, already normalized
    /// — identity evidence for a blank board.
    pub probed_mac: Option<String>,
    /// The chip the operation talked to, as the tool reported it.
    pub chip_name: Option<String>,
}

/// Progress callback for a running effect: label + optional percent. Called
/// from inside the effect's own future; the effects layer turns each call
/// into an `ActivityMarker::Progress` event.
pub type DeviceEffectProgress = std::rc::Rc<dyn Fn(String, Option<u8>)>;

/// How the app reaches real ports.
pub trait DeviceTransport {
    /// A short label for logs ("browser Web Serial").
    fn label(&self) -> &'static str;

    /// Run one coarse effect against a granted endpoint, with **exclusive
    /// ownership of the wire** for the duration (the `device_manage.rs`
    /// discipline): the effects layer pauses the link's pump before calling
    /// this, and the platform below releases the port's reader/writer before
    /// any tool touches the port. Never hold a handle across a reset that
    /// re-enumerates (ADR 2026-07-30) — handles are re-derived from the
    /// endpoint afterwards.
    fn run_effect(
        &self,
        info: LinkInfo,
        call: DeviceEffectCall,
        progress: DeviceEffectProgress,
    ) -> DeviceTransportFuture<Result<DeviceEffectFacts, String>>;

    /// The grants this origin ALREADY holds, as closed links. No chooser, no
    /// prompt — this is the startup and hotplug sweep.
    ///
    /// ⚠️ Brave revokes Web Serial grants on reload where Chrome persists
    /// them, so an empty result is an ordinary answer, never an error.
    fn discover_granted(&self) -> DeviceTransportFuture<Result<Vec<GrantedLink>, String>>;

    /// Pop the platform's chooser and return what the user picked. `Ok(None)`
    /// = the user cancelled, which is not a failure.
    fn request_grant(&self) -> DeviceTransportFuture<Result<Option<GrantedLink>, String>>;

    /// Hand a grant back so the port stops being ours (the provider's
    /// `forget_endpoint`). Best-effort: a grant that cannot be revoked is
    /// worth a log line, not a stuck card.
    fn revoke_grant(&self, info: LinkInfo) -> DeviceTransportFuture<Result<(), String>>;
}
