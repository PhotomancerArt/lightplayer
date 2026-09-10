//! Behaviour engines: what a peripheral block *does*, with no register map.
//!
//! A peripheral block on an Espressif part is two things wearing one name:
//! what the hardware **does** — a FIFO pair draining at a baud, a counter
//! reaching an alarm, a flash command engine walking its phases — and
//! **where the guest pokes it**. The first is the same IP across three
//! generations of the part. The second is different on every one of them.
//!
//! So: behaviour, scheduled events and host streams may live in an engine.
//! A register offset, a bit position, a reset value, an interrupt source
//! number and a [`RegGrade`](crate::periph::RegGrade) may not. An engine
//! names its events (`rx_overflow`, `tx_done`); the chip's **view** maps
//! those names onto the bit positions its PAC declares, seeds its own reset
//! values, publishes its own grades, and owns the
//! [`Peripheral`](crate::periph::Peripheral) impl. The crate's neutrality
//! rule is not weakened by engines — engines are how it is kept once a
//! second chip arrives.
//!
//! Engines exist **only where the win is real**: a second chip's view would
//! otherwise re-implement scheduled behaviour with host-stream or fabric
//! coupling. A shared struct with no scheduling in it is not a win, and a
//! forced abstraction over two generations of different IP is a cost. See
//! the crate README's "Engines and views" for the worked `no` examples.
//!
//! # The shape
//!
//! No trait, no generics, no callbacks. A view owns its engine **by value**
//! and calls methods on it, passing its [`BusCx`](crate::periph::BusCx)
//! through. That keeps the borrow story trivial and every scheduling
//! decision visible at the call site.
//!
//! An engine never invents an [`EventId`](lp_emu_core::sched::EventId): it
//! does not know its peripheral index, and a renumbering would change *when*
//! events fire. The view packs the ids and hands them in.

pub mod timg;
pub mod uart;
