//! A fault injector for a host link's packets — **a test switch, off by
//! default**. It damages what crosses between a peripheral and its host, not
//! what a model does: every byte the guest wrote still left the block, and
//! the injector decides what the host then receives (and the same for the
//! host's packets on their way in).
//!
//! It exists to prove a link layer (plan `reliable-device-link`, M3) against
//! the shapes of loss a real link shows, on the emulator, deterministically:
//!
//! | fault | per packet | the shape it stands for |
//! |---|---|---|
//! | `drop` | the whole packet vanishes | a USB packet lost at either end |
//! | `tail` | the packet's last 1..n bytes vanish | a write torn short |
//! | `corrupt` | one bit flips | line noise (USB has a CRC; kept for completeness) |
//! | `run` | from a byte inside this packet, everything through the end of the next `run-packets` packets vanishes | macOS's tty overflowing under Chromium's `PARMRK` (M1: loss starts at a byte, resumes at a packet boundary, ~1 KB) |
//!
//! Rates are parts per million per packet and each direction has its own.
//! The dice are a seeded SplitMix64, so a run is reproducible from its spec.
//!
//! Spec text (`--usb-faults`), comma-separated `key=value`:
//! `in-drop`, `in-tail`, `in-corrupt`, `in-run`, `run-packets`, `out-drop`,
//! `out-tail`, `out-corrupt`, `seed`. `in` is device → host, `out` is
//! host → device. Rates accept `ppm` (a bare number) or a `%` suffix.
//!
//! ⚠️ Not in a machine's save-state blob: a snapshot restores the link with
//! no faults. The injector is how a run was configured, like a script.
//!
//! # A byte stream has no packets: [`StreamFaults`]
//!
//! A USB link moves packets, so "per packet" is the model's own unit. A UART
//! moves bytes, one symbol at a time, and has no packet to damage. Rather
//! than a second set of rates with a second meaning, [`StreamFaults`] cuts
//! the byte stream into fixed windows of [`STREAM_PACKET_BYTES`] — the size
//! of a full-speed USB packet — and hands each window to this same injector
//! as if it were a packet. So `in-drop=1%` is the same fraction of the same
//! amount of traffic on the C6's USB link and on the classic's UART, and a
//! soak on one is comparable with a soak on the other. The shapes carry over
//! as a UART shows them: a `drop` is 64 bytes the line lost, a `tail` the
//! last bytes of a window, a `corrupt` one bit of line noise, and a `run` a
//! loss that starts inside a window and swallows `run-packets` whole windows
//! after it (a host tty overflowing).

use alloc::collections::VecDeque;
use core::fmt;

/// Rates for one direction, parts per million per packet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirectionFaults {
    pub drop_ppm: u32,
    pub tail_ppm: u32,
    pub corrupt_ppm: u32,
    pub run_ppm: u32,
}

impl DirectionFaults {
    pub fn is_off(&self) -> bool {
        *self == DirectionFaults::default()
    }
}

/// What the injector did, per direction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FaultCounters {
    pub packets_seen: u64,
    pub packets_dropped: u64,
    pub tails_cut: u64,
    pub bits_flipped: u64,
    pub runs_started: u64,
    /// Every byte the host did not get (or the device did not), for any
    /// reason above.
    pub bytes_dropped: u64,
}

impl fmt::Display for FaultCounters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} packets, {} dropped, {} tails cut, {} bits flipped, {} runs, {} bytes lost",
            self.packets_seen,
            self.packets_dropped,
            self.tails_cut,
            self.bits_flipped,
            self.runs_started,
            self.bytes_dropped
        )
    }
}

/// The injector: rates, dice and counters for both directions.
#[derive(Clone, Debug, Default)]
pub struct LinkFaults {
    pub device_to_host: DirectionFaults,
    pub host_to_device: DirectionFaults,
    /// Whole packets a `run` swallows after the one it starts in.
    pub run_packets: u32,
    pub seed: u64,
    rng: u64,
    /// Packets a run still has to swallow (device → host only).
    run_left: u32,
    pub in_counters: FaultCounters,
    pub out_counters: FaultCounters,
}

impl LinkFaults {
    /// Parse a spec (module docs). An empty spec is no faults.
    pub fn parse(spec: &str) -> Result<LinkFaults, String> {
        let mut f = LinkFaults {
            run_packets: 16,
            seed: 1,
            ..LinkFaults::default()
        };
        for item in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let (key, value) = item
                .split_once('=')
                .ok_or_else(|| format!("`{item}`: expected key=value"))?;
            let key = key.trim();
            let value = value.trim();
            let whole = || -> Result<u64, String> {
                value
                    .parse::<u64>()
                    .map_err(|_| format!("`{key}={value}`: expected a whole number"))
            };
            match key {
                "in-drop" => f.device_to_host.drop_ppm = ppm(key, value)?,
                "in-tail" => f.device_to_host.tail_ppm = ppm(key, value)?,
                "in-corrupt" => f.device_to_host.corrupt_ppm = ppm(key, value)?,
                "in-run" => f.device_to_host.run_ppm = ppm(key, value)?,
                "out-drop" => f.host_to_device.drop_ppm = ppm(key, value)?,
                "out-tail" => f.host_to_device.tail_ppm = ppm(key, value)?,
                "out-corrupt" => f.host_to_device.corrupt_ppm = ppm(key, value)?,
                "run-packets" => f.run_packets = whole()? as u32,
                "seed" => f.seed = whole()?,
                _ => {
                    return Err(format!(
                        "unknown fault `{key}` (expected in-drop, in-tail, in-corrupt, in-run, \
                         run-packets, out-drop, out-tail, out-corrupt, seed)"
                    ));
                }
            }
        }
        f.rng = f.seed;
        Ok(f)
    }

    pub fn is_off(&self) -> bool {
        self.device_to_host.is_off() && self.host_to_device.is_off()
    }

    /// Damage one device → host packet in place; empty means it vanished.
    pub fn on_device_to_host(&mut self, packet: &mut Vec<u8>) {
        let rates = self.device_to_host;
        let mut c = self.in_counters;
        c.packets_seen += 1;
        let before = packet.len() as u64;
        if self.run_left > 0 {
            self.run_left -= 1;
            packet.clear();
        } else if self.roll(rates.run_ppm) && !packet.is_empty() {
            c.runs_started += 1;
            let at = self.below(packet.len() as u64) as usize;
            packet.truncate(at);
            self.run_left = self.run_packets;
        } else {
            self.damage(rates, packet, &mut c);
        }
        c.bytes_dropped += before - packet.len() as u64;
        self.in_counters = c;
    }

    /// Damage one host → device packet in place; empty means it vanished.
    pub fn on_host_to_device(&mut self, packet: &mut Vec<u8>) {
        let rates = self.host_to_device;
        let mut c = self.out_counters;
        c.packets_seen += 1;
        let before = packet.len() as u64;
        self.damage(rates, packet, &mut c);
        c.bytes_dropped += before - packet.len() as u64;
        self.out_counters = c;
    }

    fn damage(&mut self, rates: DirectionFaults, packet: &mut Vec<u8>, c: &mut FaultCounters) {
        if packet.is_empty() {
            return;
        }
        if self.roll(rates.drop_ppm) {
            c.packets_dropped += 1;
            packet.clear();
            return;
        }
        if self.roll(rates.tail_ppm) {
            c.tails_cut += 1;
            let cut = 1 + self.below(packet.len() as u64) as usize;
            packet.truncate(packet.len() - cut);
        }
        if self.roll(rates.corrupt_ppm) && !packet.is_empty() {
            c.bits_flipped += 1;
            let bit = self.below(packet.len() as u64 * 8);
            packet[(bit / 8) as usize] ^= 1 << (bit % 8);
        }
    }

    fn roll(&mut self, ppm: u32) -> bool {
        ppm > 0 && self.below(1_000_000) < u64::from(ppm)
    }

    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            return 0;
        }
        self.rng = self.rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) % n
    }
}

/// The window a byte stream is cut into for [`LinkFaults`] (module docs): a
/// full-speed USB packet, so one spec means the same thing on both links.
pub const STREAM_PACKET_BYTES: usize = 64;

/// What becomes of one byte of a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fate {
    Keep,
    Drop,
    Flip(u8),
}

/// [`LinkFaults`] over a byte stream, a window at a time (module docs).
///
/// Each window's fate is decided when its first byte crosses — the injector
/// damages a stand-in window of the right length, and the difference between
/// the stand-in and what came back says, byte by byte, which were lost and
/// which bits flipped — and then applied to the real bytes as they cross,
/// one at a time. So nothing is held back: a byte reaches the other side at
/// the cycle it would have without the injector, or never. The counters are
/// the injector's own, a window at a time.
#[derive(Clone, Debug)]
pub struct StreamFaults {
    faults: LinkFaults,
    to_host: VecDeque<Fate>,
    to_device: VecDeque<Fate>,
}

impl StreamFaults {
    pub fn new(faults: LinkFaults) -> Self {
        Self {
            faults,
            to_host: VecDeque::new(),
            to_device: VecDeque::new(),
        }
    }

    /// The injector, its rates and its counters.
    pub fn faults(&self) -> &LinkFaults {
        &self.faults
    }

    /// One byte from the device to the host: what the host gets, if anything.
    pub fn on_device_to_host(&mut self, byte: u8) -> Option<u8> {
        if self.to_host.is_empty() {
            let mut window = stand_in();
            self.faults.on_device_to_host(&mut window);
            self.to_host = fates(&window);
        }
        apply(self.to_host.pop_front(), byte)
    }

    /// One byte from the host to the device: what the device gets, if
    /// anything.
    pub fn on_host_to_device(&mut self, byte: u8) -> Option<u8> {
        if self.to_device.is_empty() {
            let mut window = stand_in();
            self.faults.on_host_to_device(&mut window);
            self.to_device = fates(&window);
        }
        apply(self.to_device.pop_front(), byte)
    }
}

/// A window whose every byte is its own index, so the damaged copy names
/// what happened to each position: a truncation keeps a prefix, and a byte
/// that no longer equals its index had the difference flipped.
fn stand_in() -> Vec<u8> {
    (0..STREAM_PACKET_BYTES as u8).collect()
}

fn fates(damaged: &[u8]) -> VecDeque<Fate> {
    (0..STREAM_PACKET_BYTES)
        .map(|i| match damaged.get(i) {
            None => Fate::Drop,
            Some(&b) if b == i as u8 => Fate::Keep,
            Some(&b) => Fate::Flip(b ^ i as u8),
        })
        .collect()
}

fn apply(fate: Option<Fate>, byte: u8) -> Option<u8> {
    match fate.unwrap_or(Fate::Keep) {
        Fate::Keep => Some(byte),
        Fate::Drop => None,
        Fate::Flip(mask) => Some(byte ^ mask),
    }
}

fn ppm(key: &str, value: &str) -> Result<u32, String> {
    if let Some(pct) = value.strip_suffix('%') {
        let p: f64 = pct
            .trim()
            .parse()
            .map_err(|_| format!("`{key}={value}`: expected a percentage"))?;
        if !(0.0..=100.0).contains(&p) {
            return Err(format!("`{key}={value}`: out of 0..100%"));
        }
        return Ok((p * 10_000.0).round() as u32);
    }
    let n: u32 = value
        .parse()
        .map_err(|_| format!("`{key}={value}`: expected parts per million, or N%"))?;
    if n > 1_000_000 {
        return Err(format!("`{key}={value}`: more than a million per million"));
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spec_parses_and_an_empty_one_is_off() {
        let f = LinkFaults::parse("in-drop=1%, in-tail=500, out-drop=0.1%, run-packets=8, seed=9")
            .unwrap();
        assert_eq!(f.device_to_host.drop_ppm, 10_000);
        assert_eq!(f.device_to_host.tail_ppm, 500);
        assert_eq!(f.host_to_device.drop_ppm, 1_000);
        assert_eq!((f.run_packets, f.seed), (8, 9));
        assert!(LinkFaults::parse("").unwrap().is_off());
        assert!(LinkFaults::parse("in-dorp=1").is_err());
        assert!(LinkFaults::parse("in-drop=200%").is_err());
    }

    #[test]
    fn rates_come_out_near_what_was_asked_and_every_byte_is_counted() {
        let mut f = LinkFaults::parse("in-drop=2%,in-tail=1%,in-corrupt=1%,seed=3").unwrap();
        let mut lost = 0u64;
        for _ in 0..100_000 {
            let mut p = vec![0x55u8; 64];
            f.on_device_to_host(&mut p);
            lost += 64 - p.len() as u64;
        }
        let c = f.in_counters;
        assert_eq!(c.packets_seen, 100_000);
        assert!((1_700..2_300).contains(&c.packets_dropped), "{c}");
        assert!((800..1_200).contains(&c.tails_cut), "{c}");
        assert!((800..1_200).contains(&c.bits_flipped), "{c}");
        assert_eq!(c.bytes_dropped, lost);
    }

    #[test]
    fn a_run_starts_inside_a_packet_and_swallows_the_next_ones() {
        let mut f = LinkFaults::parse("in-run=100%,run-packets=3,seed=1").unwrap();
        let mut p = vec![1u8; 64];
        f.on_device_to_host(&mut p);
        assert!(p.len() < 64, "the run starts inside the first packet");
        for _ in 0..3 {
            let mut q = vec![1u8; 64];
            f.on_device_to_host(&mut q);
            assert!(q.is_empty());
        }
        assert_eq!(f.in_counters.runs_started, 1);
    }

    #[test]
    fn a_stream_is_damaged_a_window_at_a_time_at_the_packet_rates() {
        let mut s = StreamFaults::new(
            LinkFaults::parse("in-drop=2%,in-tail=1%,in-corrupt=1%,out-drop=1%,seed=5").unwrap(),
        );
        let (mut kept, mut flipped) = (0u64, 0u64);
        for i in 0..(10_000 * STREAM_PACKET_BYTES) {
            let byte = (i % 251) as u8;
            match s.on_device_to_host(byte) {
                Some(b) if b == byte => kept += 1,
                Some(b) => {
                    assert_eq!((b ^ byte).count_ones(), 1, "one bit of line noise");
                    flipped += 1;
                }
                None => {}
            }
        }
        let c = s.faults().in_counters;
        assert_eq!(c.packets_seen, 10_000, "one window per 64 bytes");
        assert!((150..250).contains(&c.packets_dropped), "{c}");
        assert!((60..140).contains(&c.tails_cut), "{c}");
        assert_eq!(
            flipped, c.bits_flipped,
            "every flip counted, and no other damage"
        );
        let total = 10_000 * STREAM_PACKET_BYTES as u64;
        assert_eq!(
            kept + flipped + c.bytes_dropped,
            total,
            "every byte accounted for"
        );
        for _ in 0..(1_000 * STREAM_PACKET_BYTES) {
            s.on_host_to_device(0xA5);
        }
        assert_eq!(s.faults().out_counters.packets_seen, 1_000);
    }

    #[test]
    fn a_stream_run_swallows_whole_windows_after_the_one_it_starts_in() {
        let mut s =
            StreamFaults::new(LinkFaults::parse("in-run=100%,run-packets=2,seed=1").unwrap());
        let first: Vec<_> = (0..STREAM_PACKET_BYTES)
            .map(|_| s.on_device_to_host(1))
            .collect();
        let cut = first
            .iter()
            .position(Option::is_none)
            .expect("the run starts inside");
        assert!(
            first[cut..].iter().all(Option::is_none),
            "and runs to the window's end"
        );
        for _ in 0..(2 * STREAM_PACKET_BYTES) {
            assert_eq!(s.on_device_to_host(1), None, "the next two windows vanish");
        }
    }

    #[test]
    fn the_same_seed_damages_the_same_way() {
        let run = || {
            let mut f = LinkFaults::parse("in-drop=5%,in-tail=5%,seed=42").unwrap();
            (0..1000)
                .map(|_| {
                    let mut p = vec![7u8; 64];
                    f.on_device_to_host(&mut p);
                    p.len()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(run(), run());
    }
}
