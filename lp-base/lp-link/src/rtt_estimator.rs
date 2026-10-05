//! Retransmit timeout from measured round trips: the smoothed RTT plus four
//! times its mean deviation (Jacobson/Karels, as in TCP's RFC 6298), in
//! integer microseconds (the C6 has no FPU).

use crate::Micros;

pub struct RttEstimator {
    srtt: Option<Micros>,
    rttvar: Micros,
    rto: Micros,
    min: Micros,
    max: Micros,
    /// The most recent sample, and how many there have been (wrapping).
    last: Micros,
    samples: u32,
}

impl RttEstimator {
    pub fn new(initial: Micros, min: Micros, max: Micros) -> Self {
        RttEstimator {
            srtt: None,
            rttvar: 0,
            rto: initial,
            min,
            max,
            last: 0,
            samples: 0,
        }
    }

    pub fn sample(&mut self, r: Micros) {
        self.last = r;
        self.samples = self.samples.wrapping_add(1);
        match self.srtt {
            None => {
                self.srtt = Some(r);
                self.rttvar = r / 2;
            }
            Some(s) => {
                let err = s.abs_diff(r);
                self.rttvar = (3 * self.rttvar + err) / 4;
                self.srtt = Some((7 * s + r) / 8);
            }
        }
        let s = self.srtt.unwrap_or(r);
        self.rto = (s + (4 * self.rttvar).max(1_000)).clamp(self.min, self.max);
    }

    /// The current timeout, before backoff.
    pub fn rto(&self) -> Micros {
        self.rto
    }

    /// Smoothed RTT (the initial timeout until the first sample).
    pub fn srtt(&self) -> Micros {
        self.srtt.unwrap_or(self.rto)
    }

    /// The most recent round-trip sample and the running sample count
    /// (`(0, 0)` before the first). The count wraps; a reader that polls it
    /// takes a new sample whenever it moves.
    pub fn last_sample(&self) -> (Micros, u32) {
        (self.last, self.samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converges_and_clamps() {
        let mut r = RttEstimator::new(50_000, 5_000, 1_000_000);
        for _ in 0..50 {
            r.sample(2_000);
        }
        assert_eq!(r.srtt(), 2_000);
        assert_eq!(r.rto(), 5_000, "clamped to the minimum");
        r.sample(10_000_000);
        assert_eq!(r.rto(), 1_000_000, "clamped to the maximum");
    }

    #[test]
    fn the_last_sample_and_the_count_follow_every_sample() {
        let mut r = RttEstimator::new(50_000, 5_000, 1_000_000);
        assert_eq!(r.last_sample(), (0, 0), "nothing measured yet");
        assert_eq!(r.srtt(), 50_000, "the initial timeout stands in");
        r.sample(3_000);
        assert_eq!(r.last_sample(), (3_000, 1));
        assert_eq!(r.srtt(), 3_000, "the first sample is the estimate");
        r.sample(11_000);
        assert_eq!(r.last_sample(), (11_000, 2), "the raw sample, not smoothed");
        assert_eq!(r.srtt(), 4_000, "(7 * 3000 + 11000) / 8");
    }
}
