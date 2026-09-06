//! The GPIO calibration payload: protocol and duty ramp.
//!
//! The host asks for one GPIO at a time and owns all calibration state; the
//! device opens the pin, drives a soft-timed square wave, and reports. That
//! split is what keeps the firmware side small, and it is also what makes the
//! interesting half portable: the line protocol and the duty ramp are pure
//! arithmetic over bytes and microseconds, with no esp-hal in sight.
//!
//! So they live here, `no_std` and `alloc`-free, tested on the host. What stays
//! in `fw-esp32c6` is board init, `UsbSerialJtag`, `AnyPin::steal` and
//! `embassy_time` — the parts that genuinely need a chip.
//!
//! The wire format is unchanged: `lp-cli hardware calibrate` parses these exact
//! lines, and moving the code was not licence to move the protocol.

pub mod duty;
pub mod protocol;

pub use duty::{DUTY_MAX_PERCENT, DUTY_MIN_PERCENT, DUTY_PERIOD_US, DUTY_STEP_PERCENT, DutyRamp};
pub use protocol::{
    CAL_READY_PREFIX, Command, LineParser, PulseRequest, Response, classify_pulse, supports_gpio,
};
