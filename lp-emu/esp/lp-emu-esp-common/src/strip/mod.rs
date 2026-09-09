//! What is on the wire: decoders that turn a pad's edges back into the bytes
//! a strip would have shifted in.
//!
//! The counterpart of an output peripheral. The RMT model produces pulses,
//! [`crate::pins`] routes them to a pad, and a decoder here reads the pad's
//! edges the way a logic analyser would — with a tolerance, and with no
//! knowledge of who produced them. That independence is the point: the
//! decoder is a **second opinion** on the same frames the peripheral's own
//! word log describes, so the two agreeing is evidence and not a tautology.
//!
//! Chip-neutral, like the rest of this crate: a decoder is given the CPU
//! clock as a number and the wire timing as data.

pub mod ws281x;

pub use ws281x::{BitError, Frame, Ws281xDecoder, unpermute};
