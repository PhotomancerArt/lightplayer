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
}

impl RttEstimator {
    pub fn new(initial: Micros, min: Micros, max: Micros) -> Self {
        RttEstimator {
            srtt: None,
            rttvar: 0,
            rto: initial,
            min,
            max,
        }
    }

    pub fn sample(&mut self, r: Micros) {
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
}
