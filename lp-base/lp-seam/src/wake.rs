//! The wake: how an emulator tells the guest a capability seam has
//! something for it.
//!
//! Seams are **pull-only**: the emulator answers calls the firmware makes,
//! and writes only memory a call handed it. The one exception is the wake,
//! which is how a pull-only seam avoids polling. There is one wake for every
//! seam:
//!
//! - **one pending word** in guest RAM, whose address is the table's
//!   `pending` field (`0` = this image has no wake), one bit per endpoint the
//!   firmware assigns;
//! - **one line**, the CPU software interrupt [`WAKE_FROM_CPU_INTR`] (on the
//!   ESP32-C6 the interrupt source [`WAKE_SOURCE_ESP32C6`]), bound at
//!   priority [`WAKE_PRIORITY`].
//!
//! The protocol, in this order on each side:
//!
//! - **the host** sets bits in the pending word (a read-modify-write between
//!   two guest instructions, so atomic), *then* raises the line the way
//!   another CPU would;
//! - **the guest's handler** clears the line, *then* swaps the word to zero
//!   and wakes the consumer of every bit it saw. A raise that lands between
//!   the two is seen by the next take, never lost.
//!
//! Two rules from the roadmap's G0, which this crate can only state:
//!
//! - **(a)** whatever a capability seam wakes runs on the firmware's **IO
//!   thread**, never the main or render executor. A firmware rule; the
//!   Bluetooth seam's plan builds the API that enforces it.
//! - **(b)** the emulator **paces** what it produces: never two raises
//!   outstanding, a minimum spacing between raises, a bounded queue per
//!   endpoint and a cap on what one take returns. Never an unbounded
//!   producer (the M0 spike's flood starved the render).
//!
//! **No firmware handler ships yet** (it lands with the Bluetooth seam), so
//! every shipped table's `pending` is `0`. The emulator half is built and
//! tested against a synthetic guest.
//!
//! If a wake is ever lost or doubled in a way this protocol cannot explain,
//! that is the roadmap's revisit trigger **R-WAKE**: stop and take it back to
//! a decision, never work around it in place.

/// The CPU software interrupt the wake raises: `FROM_CPU_INTR3`.
pub const WAKE_FROM_CPU_INTR: u8 = 3;

/// `FROM_CPU_INTR3`'s interrupt-matrix source number on the ESP32-C6.
pub const WAKE_SOURCE_ESP32C6: u16 = 25;

/// The priority the guest binds the wake line at.
pub const WAKE_PRIORITY: u8 = 1;
