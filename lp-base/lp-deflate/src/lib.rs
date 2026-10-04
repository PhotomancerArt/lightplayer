//! **lp-deflate**: a small `no_std`, no-alloc **raw deflate (RFC 1951)
//! decoder**.
//!
//! [`inflate`] decodes a raw deflate stream (stored, fixed-Huffman and
//! dynamic-Huffman blocks — RFC 1951 §3.2.3/§3.2.4/§3.2.6/§3.2.7) into a
//! caller-owned buffer, optionally with the buffer's own prefix standing in
//! as a **preset dictionary**: history a back-reference may copy from
//! without it having been produced by this call. That is exactly OTA's
//! shape (see `lp2025/2026-10-03-1330-ota-firmware-updates`, M4): each 4 KiB
//! firmware chunk is sent compressed against the 32 KiB of the image
//! already written just before it, which the device already holds, so the
//! transfer stays resumable chunk by chunk with no window to resend.
//!
//! General-purpose on purpose — OTA is the first user, but nothing here
//! names it. LED frame data (uncompressed today) is a plausible second one;
//! see `compression-notes.md` in the OTA split-link spike for the
//! measurements that motivated this crate.
//!
//! # Why deflate
//!
//! Browsers decode it natively (`DecompressionStream('deflate-raw')`), so a
//! web client pays nothing extra, and every host toolchain already has
//! zlib. The device is the one side that needs its own code, and only the
//! decoding half: nothing here encodes.
//!
//! # Provenance
//!
//! Written independently from RFC 1951 (the Internet Standards Document),
//! never adapted from zlib, miniz, or puff source — see `AGENTS.md`'s
//! license-discipline rule. `miniz_oxide` (MIT) is a dev-dependency only,
//! used as an oracle: this crate's tests check that it decodes what
//! `miniz_oxide` encodes, never the reverse. The design — a bit reader, a
//! canonical-Huffman decoder keyed on per-length counts, and the block loop
//! — started from a prototype written the same way for the OTA spike
//! (`lp2025/2026-10-01-1854-ota-split-link-spike`); this crate is that
//! decoder, carried over whole and reorganized into this crate's own file
//! layout.
//!
//! # Shape
//!
//! - `#![no_std]`, no `alloc`: every table is a fixed-size array sized to
//!   RFC 1951's own limits (288 literal/length symbols, 30 distance
//!   symbols, 19 code-length symbols), and the only "memory" `inflate` uses
//!   besides its stack is the caller's `buf`.
//! - [`Error`] covers every way a stream can fail to decode: truncated
//!   input, a corrupt block, a full output buffer, or a back-reference
//!   reaching before the buffer's start. `inflate` never panics — every
//!   array index into caller-controlled data goes through a checked
//!   accessor (`get`/`get_mut`) first.

#![no_std]

mod bits;
mod error;
mod huffman;
mod inflate;
mod tables;

pub use error::Error;
pub use inflate::inflate;
