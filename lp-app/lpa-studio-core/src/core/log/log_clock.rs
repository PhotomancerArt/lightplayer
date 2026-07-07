//! Clock injection for push-time log stamping.

use std::rc::Rc;

/// A shared clock returning seconds since the Unix epoch as `f64` (fractional
/// part = sub-second precision).
///
/// The `StudioController` takes one at construction and stamps every
/// [`UiLogDraft`](super::UiLogDraft) with it at push time. Core never reads a
/// platform clock itself, so the crate stays platform-free: the web shell
/// passes `|| js_sys::Date::now() / 1000.0`, tests pass fixed or stepping
/// fakes. `Rc` (not `Box`) so the actor can share the controller's clock when
/// stamping progressive log updates.
pub type LogClock = Rc<dyn Fn() -> f64>;
