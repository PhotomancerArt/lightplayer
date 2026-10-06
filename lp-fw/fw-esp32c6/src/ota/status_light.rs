//! The update light (DM22, roadmap Y4/Q1): core-only lights the strip the
//! engine recorded in `/.lp/status-light.json` one solid, dim colour — dark
//! yellow while a transfer is pending or running, dark red while the board
//! waits for its engine (or its engine keeps crashing) — re-lit on every
//! state change and once after each reset. No animation; a dark strip
//! otherwise.
//!
//! It never parses the hardware manifest (Q1) and reuses nothing of the
//! engine's RMT driver (engine code, never reachable from core-only): a
//! small blocking transmit on RMT channel 0 through esp-hal, at the WS2812
//! timings the engine's driver uses (`lp_ws281x::ChannelTiming::WS2812`:
//! T0H 400 ns, T0L 850 ns, T1H 800 ns, T1L 450 ns; 12.5 ns ticks at the
//! 80 MHz RMT clock).
//!
//! **The first [`MAX_LIT`] LEDs, from RMT RAM alone.** The plan preferred
//! the RMT's TX loop count (one LED's 24 pulse codes looped `count` times;
//! the C6's `tx_loop_num` is 10 bits, so up to 1023 LEDs), or else a
//! buffer refilled into RAM as it goes. Neither can be proven on the
//! emulated C6: its RMT models no TX loop mode, and its threshold is the
//! position the engine's own driver (`lp-ws281x`) programs, not the
//! every-half-window event esp-hal's refill waits for — a refilled frame
//! came out 56–62 LEDs too long there. So the light sends one frame that
//! fits the channel's RAM with no refill: all four blocks (192 words, the
//! RX blocks included, as the engine's driver takes them for one strip),
//! seven LEDs of 24 codes and the end marker. A sign of life on the strip's
//! first seven LEDs, the same on silicon and on its emulator.

use alloc::vec::Vec;

use esp_hal::Blocking;
use esp_hal::gpio::{AnyPin, Level};
use esp_hal::rmt::{Channel, PulseCode, Rmt, Tx, TxChannelConfig, TxChannelCreator};
use esp_hal::time::Rate;
use lpc_update::{BoardState, StatusLightRecord, light_for};

/// The most LEDs the light drives: what RMT RAM holds with no refill
/// (192 words: 7 × 24 pulse codes + the end marker).
pub const MAX_LIT: u32 = 7;

/// The RMT clock (80 MHz: a 12.5 ns tick).
const RMT_CLOCK: Rate = Rate::from_mhz(80);
/// WS2812 bit timings in ticks: `lp_ws281x::ChannelTiming::WS2812`'s
/// nanoseconds at 12.5 ns a tick.
const T0H: u16 = (lp_ws281x::ChannelTiming::WS2812.t0h_ns * 2 / 25) as u16;
const T0L: u16 = (lp_ws281x::ChannelTiming::WS2812.t0l_ns * 2 / 25) as u16;
const T1H: u16 = (lp_ws281x::ChannelTiming::WS2812.t1h_ns * 2 / 25) as u16;
const T1L: u16 = (lp_ws281x::ChannelTiming::WS2812.t1l_ns * 2 / 25) as u16;

/// GPIOs the light never drives: USB D-/D+ (12, 13; the host link) and the
/// SPI flash (24–30).
fn pin_is_safe(pin: u8) -> bool {
    pin <= 23 && pin != 12 && pin != 13
}

/// The light core-only drives, if the engine left a record it can light.
/// Its RMT channel stays bound to the pad for the light's life, idling low,
/// so the strip latches between frames.
pub struct StatusLight {
    record: StatusLightRecord,
    channel: Option<Channel<'static, Blocking, Tx>>,
    shown: Option<[u8; 3]>,
}

impl StatusLight {
    /// From the record's bytes (`None`: no record), and the RMT the core
    /// holds. `None` — stay dark — for no record, one this core cannot
    /// light, a pin it must not drive, or an RMT that will not configure.
    pub fn new(record: Option<&[u8]>, rmt: esp_hal::peripherals::RMT<'static>) -> Option<Self> {
        let Some(record) = record.and_then(StatusLightRecord::read) else {
            log::info!("[OTA] light: no status-light record this core can light — dark");
            return None;
        };
        if !pin_is_safe(record.pin) {
            log::warn!(
                "[OTA] light: GPIO{} is not a pin the core drives — dark",
                record.pin
            );
            return None;
        }
        let channel = match bind(rmt, record.pin) {
            Ok(channel) => channel,
            Err(why) => {
                log::warn!("[OTA] light: {why} — dark");
                return None;
            }
        };
        Some(Self {
            record,
            channel: Some(channel),
            shown: None,
        })
    }

    /// Light the strip for `state`, if that changed what it shows.
    pub fn show(&mut self, state: BoardState) {
        let rgb = light_for(state).unwrap_or([0, 0, 0]);
        if self.shown == Some(rgb) {
            return;
        }
        let Some(channel) = self.channel.take() else {
            return;
        };
        let lit = self.record.count.min(MAX_LIT);
        let frame = solid_frame(lit, self.record.wire_bytes(rgb));
        let sent = match channel.transmit(&frame) {
            Ok(tx) => match tx.wait() {
                Ok(channel) => {
                    self.channel = Some(channel);
                    true
                }
                Err((_, channel)) => {
                    self.channel = Some(channel);
                    false
                }
            },
            Err((_, channel)) => {
                self.channel = Some(channel);
                false
            }
        };
        if sent {
            self.shown = Some(rgb);
            log::info!(
                "[OTA] light: GPIO{} × {lit} LEDs r={} g={} b={} ({})",
                self.record.pin,
                rgb[0],
                rgb[1],
                rgb[2],
                super::update_edge::state_word(state)
            );
        } else {
            log::warn!("[OTA] light: the frame did not go out");
        }
    }
}

/// RMT channel 0 at 80 MHz, idling low, bound to `pin`.
fn bind(
    rmt: esp_hal::peripherals::RMT<'static>,
    pin: u8,
) -> Result<Channel<'static, Blocking, Tx>, &'static str> {
    let rmt = Rmt::new(rmt, RMT_CLOCK).map_err(|_| "the RMT clock would not configure")?;
    let config = TxChannelConfig::default()
        .with_clk_divider(1)
        .with_idle_output(true)
        .with_idle_output_level(Level::Low)
        .with_carrier_modulation(false)
        // All four blocks: the whole frame in RAM, no refill (module docs).
        .with_memsize(4);
    let channel = rmt
        .channel0
        .configure_tx(&config)
        .map_err(|_| "RMT channel 0 would not configure")?;
    // SAFETY: core-only runs no engine and no output driver, so nothing else
    // holds this pad; `pin_is_safe` kept the USB and flash pads out.
    Ok(channel.with_pin(unsafe { AnyPin::steal(pin) }))
}

/// One frame of `lit` LEDs, every one `wire` (bytes in the strip's order),
/// most significant bit first, and the end marker.
fn solid_frame(lit: u32, wire: [u8; 3]) -> Vec<PulseCode> {
    let bit = |one: bool| {
        if one {
            PulseCode::new(Level::High, T1H, Level::Low, T1L)
        } else {
            PulseCode::new(Level::High, T0H, Level::Low, T0L)
        }
    };
    let mut led = [PulseCode::end_marker(); 24];
    for (i, code) in led.iter_mut().enumerate() {
        let byte = wire[i / 8];
        *code = bit(byte & (0x80 >> (i % 8)) != 0);
    }
    let mut frame: Vec<PulseCode> = Vec::with_capacity(lit as usize * 24 + 1);
    for _ in 0..lit {
        frame.extend_from_slice(&led);
    }
    frame.push(PulseCode::end_marker());
    frame
}
