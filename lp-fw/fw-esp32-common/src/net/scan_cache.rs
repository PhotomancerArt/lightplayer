//! What the radio last heard, for the server's scan probe.

use alloc::vec::Vec;
use lpc_wire::{HeardNetwork, NetworkScan};

/// The answer to a client's `NetworkScan` comes from here, never from a
/// scan run inside the request (one takes about two seconds and the server
/// answers within a frame). A list at most [`Self::FRESH_MS`] old is the
/// answer; an older one, or none, answers [`NetworkScan::Scanning`] and
/// asks the station for a new scan ([`Self::take_wanted`]). Every scan the
/// station runs, its own searches included, is recorded here.
///
/// Time is the caller's (`now_ms`): nothing here reads a clock.
#[derive(Debug, Clone, Default)]
pub struct ScanCache {
    last: Option<(u64, Vec<HeardNetwork>)>,
    wanted: bool,
}

impl ScanCache {
    /// How long a scan's list stays the answer.
    pub const FRESH_MS: u64 = 10_000;

    /// An empty cache: the first ask answers `scanning`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            last: None,
            wanted: false,
        }
    }

    /// The answer at `now_ms`: the fresh list, strongest first, or
    /// `scanning` (and a scan is wanted).
    pub fn answer(&mut self, now_ms: u64) -> NetworkScan {
        match &self.last {
            Some((at, heard)) if now_ms.saturating_sub(*at) <= Self::FRESH_MS => {
                NetworkScan::Heard(heard.clone())
            }
            _ => {
                self.wanted = true;
                NetworkScan::Scanning
            }
        }
    }

    /// A scan finished at `now_ms` and heard `heard` (any order). Keeps the
    /// strongest of any name heard twice.
    pub fn record(&mut self, now_ms: u64, mut heard: Vec<HeardNetwork>) {
        heard.sort_by(|a, b| b.rssi.cmp(&a.rssi));
        let mut unique: Vec<HeardNetwork> = Vec::with_capacity(heard.len());
        for network in heard {
            if !unique.iter().any(|kept| kept.ssid == network.ssid) {
                unique.push(network);
            }
        }
        self.last = Some((now_ms, unique));
        self.wanted = false;
    }

    /// Whether a client asked for a scan since the last one; clears it.
    pub fn take_wanted(&mut self) -> bool {
        core::mem::take(&mut self.wanted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec;

    fn network(ssid: &str, rssi: i8) -> HeardNetwork {
        HeardNetwork {
            ssid: String::from(ssid),
            rssi,
            secure: true,
        }
    }

    #[test]
    fn no_scan_yet_answers_scanning_and_asks_for_one() {
        let mut cache = ScanCache::new();
        assert_eq!(cache.answer(0), NetworkScan::Scanning);
        assert!(cache.take_wanted());
        assert!(!cache.take_wanted(), "taken once");
    }

    #[test]
    fn a_fresh_list_is_the_answer_strongest_first_and_once_per_name() {
        let mut cache = ScanCache::new();
        cache.record(
            1_000,
            vec![
                network("lp-cafe", -80),
                network("lp-walk-net", -50),
                network("lp-cafe", -60),
            ],
        );
        assert_eq!(
            cache.answer(1_000 + ScanCache::FRESH_MS),
            NetworkScan::Heard(vec![network("lp-walk-net", -50), network("lp-cafe", -60)])
        );
        assert!(!cache.take_wanted());
    }

    #[test]
    fn a_stale_list_answers_scanning_and_an_empty_one_is_heard_nothing() {
        let mut cache = ScanCache::new();
        cache.record(0, Vec::new());
        assert_eq!(cache.answer(5_000), NetworkScan::Heard(Vec::new()));
        assert_eq!(cache.answer(ScanCache::FRESH_MS + 1), NetworkScan::Scanning);
        assert!(cache.take_wanted());
    }
}
