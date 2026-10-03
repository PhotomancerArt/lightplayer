//! Per-chip end-of-run figures for `lp-cli link rtt`'s `emu:` targets.
//!
//! [`super::rtt::EmuPipe`]'s report is the WS281x frames decoded off the
//! routed pads, plus the RMT refill race (`super::rtt::EmuPipe`'s C6-only
//! version before this phase). Chip-specific, and kept out of `rtt.rs`
//! (P1's own rule of thumb): the C6 and S3 share one `RefillStats` type
//! (`lp_emu_esp_common::ip::rmt`) but the classic's RMT is its own
//! implementation with its own `RefillStats` (`lp_emu_esp32v3::periph::rmt`)
//! of the same shape — no trait upstream unifies them — and each chip's RMT
//! has its own channel count, so a wrong loop bound panics
//! (`Rmt::refill_stats` indexes a `Vec` sized at build time). Three small,
//! near-identical bodies, not one generic over a type nothing unifies.

use serde_json::{Value, json};

use lp_emu_esp_common::strip::ws281x::Frame;

use super::rtt::stats;
use crate::commands::emu::link_host::{C6Board, S3Board, V3Board};

/// RMT TX channels each chip's refill stats may be asked about without
/// panicking (`lp_emu_esp32c6/s3`'s shared `ip::rmt` config; the classic's
/// own `periph::rmt`, `TX_CHANNELS = 8`).
const C6_TX_CHANNELS: usize = 2;
const S3_TX_CHANNELS: usize = 4;
const V3_TX_CHANNELS: usize = 8;

/// End-of-run figures only an emulator has, for one chip:
/// [`super::rtt::EmuPipe::report`]'s per-board half.
pub(super) trait ChipReport {
    fn chip_report(&mut self) -> Value;
}

impl ChipReport for C6Board {
    fn chip_report(&mut self) -> Value {
        let m = &mut self.machine;
        let mut pads = Vec::new();
        for (pad, _) in m.routed_pads() {
            let frames = m.frames(pad.0);
            if frames.is_empty() {
                continue;
            }
            pads.push(pad_entry(pad.0, frames, lp_emu_esp32c6::memmap::CYCLES_PER_US));
        }
        let mut refills = Vec::new();
        for ch in 0..C6_TX_CHANNELS {
            let r = m.rmt_refill_stats(ch);
            if let Some(v) = refill_entry(
                ch,
                RefillFields {
                    refills: r.refills,
                    unanswered: r.unanswered,
                    entry_max: r.entry_max,
                    fill_max: r.fill_max,
                    half_words: r.half_words,
                    entry_hist: &r.entry_hist,
                    fill_hist: &r.fill_hist,
                },
            ) {
                refills.push(v);
            }
        }
        json!({ "ws281x": pads, "rmt_refill": refills, "instructions": m.instructions() })
    }
}

impl ChipReport for S3Board {
    fn chip_report(&mut self) -> Value {
        let m = &mut self.machine;
        let mut pads = Vec::new();
        for (pad, _) in m.routed_pads() {
            let frames = m.frames(pad.0);
            if frames.is_empty() {
                continue;
            }
            pads.push(pad_entry(pad.0, frames, lp_emu_esp32s3::memmap::CYCLES_PER_US));
        }
        let mut refills = Vec::new();
        for ch in 0..S3_TX_CHANNELS {
            let r = m.rmt_refill_stats(ch);
            if let Some(v) = refill_entry(
                ch,
                RefillFields {
                    refills: r.refills,
                    unanswered: r.unanswered,
                    entry_max: r.entry_max,
                    fill_max: r.fill_max,
                    half_words: r.half_words,
                    entry_hist: &r.entry_hist,
                    fill_hist: &r.fill_hist,
                },
            ) {
                refills.push(v);
            }
        }
        json!({ "ws281x": pads, "rmt_refill": refills, "instructions": m.instructions() })
    }
}

impl ChipReport for V3Board {
    fn chip_report(&mut self) -> Value {
        let m = &mut self.machine;
        let mut pads = Vec::new();
        for (pad, _) in m.routed_pads() {
            let frames = m.frames(pad.0);
            if frames.is_empty() {
                continue;
            }
            pads.push(pad_entry(pad.0, frames, lp_emu_esp32v3::memmap::CYCLES_PER_US));
        }
        let mut refills = Vec::new();
        for ch in 0..V3_TX_CHANNELS {
            let r = m.rmt_refill_stats(ch);
            if let Some(v) = refill_entry(
                ch,
                RefillFields {
                    refills: r.refills,
                    unanswered: r.unanswered,
                    entry_max: r.entry_max,
                    fill_max: r.fill_max,
                    half_words: r.half_words,
                    entry_hist: &r.entry_hist,
                    fill_hist: &r.fill_hist,
                },
            ) {
                refills.push(v);
            }
        }
        json!({ "ws281x": pads, "rmt_refill": refills, "instructions": m.instructions() })
    }
}

/// One pad's decoded frames, as `super::rtt::EmuPipe::report` used to inline
/// it for the C6 alone.
fn pad_entry(pad: u8, frames: &[Frame], cycles_per_us: u64) -> Value {
    let starts: Vec<f64> = frames
        .iter()
        .map(|f| f.start as f64 / cycles_per_us as f64)
        .collect();
    let gaps: Vec<f64> = starts.windows(2).map(|w| (w[1] - w[0]) / 1000.0).collect();
    json!({
        "pad": pad,
        "frames": frames.len(),
        "errors": frames.iter().map(|f| f.error_count).sum::<u64>(),
        "incomplete": frames.iter().filter(|f| !f.is_complete()).count(),
        "leds": frames.last().map(|f| f.leds()),
        "interval_ms": stats(&gaps),
        "frame_starts_us": starts.iter().map(|s| s.round() as u64).collect::<Vec<_>>(),
    })
}

/// The fields of one RMT channel's refill race, read out of whichever
/// `RefillStats` the chip has (the C6/S3 and the classic do not share one
/// type, so this is built from the fields at each call site instead of
/// taking the struct itself).
struct RefillFields<'a> {
    refills: u64,
    unanswered: u64,
    entry_max: u64,
    fill_max: u64,
    half_words: u32,
    entry_hist: &'a [u64],
    fill_hist: &'a [u64],
}

/// One RMT channel's refill race, or `None` when it carried nothing (the
/// common case for a channel a project never drives).
fn refill_entry(ch: usize, r: RefillFields<'_>) -> Option<Value> {
    if r.refills == 0 && r.unanswered == 0 {
        return None;
    }
    Some(json!({
        "ch": ch,
        "refills": r.refills,
        "unanswered": r.unanswered,
        "entry_max_words": r.entry_max,
        "fill_max_words": r.fill_max,
        "half_words": r.half_words,
        "entry_hist": r.entry_hist,
        "fill_hist": r.fill_hist,
    }))
}
