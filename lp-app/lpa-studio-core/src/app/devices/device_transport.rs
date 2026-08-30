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

/// How the app reaches real ports.
pub trait DeviceTransport {
    /// A short label for logs ("browser Web Serial").
    fn label(&self) -> &'static str;

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
