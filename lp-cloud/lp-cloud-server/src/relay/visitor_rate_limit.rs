//! How fast one address may try boards it is not a member of.
//!
//! A board's relay id is its MAC — not a secret, and partly guessable from
//! the vendor's prefix. So every browser-leg open that is not a member
//! reaching its own board (a visitor, or an id that is not online at all)
//! takes a token from its address's bucket: [`BUCKET_CAPACITY`] at once,
//! then one every [`REFILL_EVERY`]. That bounds both a sweep for online
//! boards and the rate of password tries one address can put through the
//! relay — the board's own login backoff is the second, device-wide bound.
//!
//! Keyed by the client address the relay sees (`Fly-Client-IP` in
//! production). Time is an argument, so the tests drive it.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

/// Tries an address has in hand.
pub const BUCKET_CAPACITY: u32 = 20;
/// How often one try comes back.
pub const REFILL_EVERY: Duration = Duration::from_secs(30);
/// The table is swept of full buckets once it holds this many addresses.
const SWEEP_AT: usize = 4096;

/// See the module doc.
#[derive(Debug, Default)]
pub struct VisitorRateLimit {
    buckets: HashMap<IpAddr, Bucket>,
}

#[derive(Debug, Clone, Copy)]
struct Bucket {
    tokens: u32,
    /// When the bucket last gained a token (or was made).
    since: Instant,
}

impl VisitorRateLimit {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Take one try for `ip` at `now`: `false` when it has none left.
    /// No address (a test with no peer) shares one bucket.
    pub fn take(&mut self, ip: Option<IpAddr>, now: Instant) -> bool {
        if self.buckets.len() >= SWEEP_AT {
            self.sweep(now);
        }
        let ip = ip.unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        let bucket = self.buckets.entry(ip).or_insert(Bucket {
            tokens: BUCKET_CAPACITY,
            since: now,
        });
        refill(bucket, now);
        if bucket.tokens == 0 {
            return false;
        }
        bucket.tokens -= 1;
        true
    }

    /// Forget every address whose bucket has refilled: it would start full
    /// anyway.
    fn sweep(&mut self, now: Instant) {
        self.buckets.retain(|_, bucket| {
            refill(bucket, now);
            bucket.tokens < BUCKET_CAPACITY
        });
    }
}

fn refill(bucket: &mut Bucket, now: Instant) {
    let elapsed = now.saturating_duration_since(bucket.since);
    let earned = (elapsed.as_millis() / REFILL_EVERY.as_millis()) as u32;
    if earned == 0 {
        return;
    }
    bucket.tokens = bucket.tokens.saturating_add(earned).min(BUCKET_CAPACITY);
    bucket.since += REFILL_EVERY * earned;
    if bucket.tokens == BUCKET_CAPACITY {
        bucket.since = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_gets_twenty_tries_then_one_every_thirty_seconds() {
        let mut limit = VisitorRateLimit::new();
        let ip: IpAddr = "203.0.113.9".parse().unwrap();
        let start = Instant::now();
        for n in 0..BUCKET_CAPACITY {
            assert!(limit.take(Some(ip), start), "try {n}");
        }
        assert!(!limit.take(Some(ip), start));
        assert!(!limit.take(Some(ip), start + Duration::from_secs(29)));
        assert!(limit.take(Some(ip), start + Duration::from_secs(30)));
        assert!(!limit.take(Some(ip), start + Duration::from_secs(31)));
        assert!(limit.take(Some(ip), start + Duration::from_secs(61)));
    }

    #[test]
    fn addresses_do_not_share_a_bucket() {
        let mut limit = VisitorRateLimit::new();
        let start = Instant::now();
        let a: IpAddr = "203.0.113.9".parse().unwrap();
        let b: IpAddr = "198.51.100.4".parse().unwrap();
        for _ in 0..BUCKET_CAPACITY {
            limit.take(Some(a), start);
        }
        assert!(!limit.take(Some(a), start));
        assert!(limit.take(Some(b), start));
    }

    #[test]
    fn a_long_rest_refills_to_capacity_not_past_it() {
        let mut limit = VisitorRateLimit::new();
        let ip: IpAddr = "203.0.113.9".parse().unwrap();
        let start = Instant::now();
        limit.take(Some(ip), start);
        let later = start + Duration::from_secs(3600);
        for n in 0..BUCKET_CAPACITY {
            assert!(limit.take(Some(ip), later), "try {n}");
        }
        assert!(!limit.take(Some(ip), later));
    }
}
