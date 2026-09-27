//! **lp-link**: a prototype link layer for LightPlayer's device transports
//! (USB serial, BLE, later UDP and WebSocket). An investigation crate from the
//! plan `lp2025/2026-09-26-1720-reliable-device-link` (milestone M2); it is not
//! wired into the product.
//!
//! What it gives the layers above, over any transport:
//! - **Frames with a checksum** ([`frame`], [`crc`]): damaged or torn frames
//!   are detected and dropped, never delivered. On a byte stream frames are
//!   COBS-FF-encoded (no `0x00` or `0xFF` on the wire) between `0x00` delimiters ([`cobs`]) and bytes outside frames
//!   pass through as console text; on a datagram transport each datagram is
//!   one frame.
//! - **Channels** (0–7), each reliable or best effort ([`LinkConfig`]):
//!   control, the wire protocol, logs ([`LogRing`]).
//! - **Reliability**: sequence numbers, cumulative ACKs piggybacked on every
//!   frame, retransmission, flow control by an advertised window, an RTT-driven
//!   timeout. Four variants behind [`Arq`]: [`StopAndWait`], [`GoBackN`],
//!   [`SelectiveRepeat`], and [`NoArq`] for transports that are reliable
//!   already.
//! - **A lifecycle** ([`LinkEvent::Up`], [`LinkEvent::Reset`]): both ends learn
//!   when the other restarted, so per-link state (the learned dictionary) is
//!   reset in step instead of drifting.
//! - **Counters** ([`LinkCounters`]) for every recovery, so problems stay
//!   visible.
//!
//! Sans-IO: time ([`Micros`]) and the nonce are injected; no executor, no
//! clock. RAM is bounded by the config ([`Link::ram_bound`]): buffers are
//! allocated in [`Link::new`], and steady-state traffic allocates only the
//! `Vec` each delivered message is handed over in. The `sim` feature adds a
//! deterministic fault-injecting simulator ([`sim`]) and the `link-bench` tool.
//!
//! Design, principles and prior art: `README.md` beside this crate.

#![no_std]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod arq;
pub mod cobs;
pub mod crc;
mod datagram_queue;
pub mod deframer;
pub mod frame;
mod inbox;
#[cfg(feature = "lab")]
pub mod lab;
mod link;
mod link_config;
mod link_counters;
mod link_event;
pub mod log_ring;
mod rtt_estimator;
mod send_queue;
mod seq_num;
mod tx_queue;

#[cfg(feature = "sim")]
pub mod sim;

pub use arq::{Arq, GoBackN, NoArq, SelectiveRepeat, StopAndWait};
pub use crc::CrcKind;
pub use link::{Link, LinkState, SendError};
pub use link_config::{CH_CONTROL, CH_LOG, CH_PROTO, Framing, LinkConfig, MAX_MESSAGE};
pub use link_counters::LinkCounters;
pub use link_event::{LinkEvent, ResetReason};
pub use log_ring::LogRing;

/// Time, in microseconds, from any epoch the edge likes. Integer: the C6 has
/// no FPU (the repo's usual f64 seconds would be soft-float here).
pub type Micros = u64;
