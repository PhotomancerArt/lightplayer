//! How often the hub wants a board's pictures
//! ([`RelayFrame::PictureRate`](crate::RelayFrame::PictureRate), protocol 2,
//! hub → board).

use alloc::vec::Vec;

use crate::frame_reader::FrameReader;
use crate::relay_frame::RelayFrameError;
use crate::relay_limits::{MAX_IDLE_S, MAX_WATCHED_FOR_S, MIN_IDLE_S, MIN_WATCHED_MS};

/// The hub's picture cadence for a board. The hub owns the numbers; the
/// board clamps them ([`Self::clamped`]) and decays by itself: it counts as
/// watched for `watched_for_s` from the moment it hears this, then falls
/// back to idle with no word from the hub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PictureRate {
    /// Seconds between pictures while nobody watches; 0 = none.
    pub idle_s: u16,
    /// Milliseconds between pictures while watched.
    pub watched_ms: u16,
    /// How long, from now, the board counts as watched; 0 = not watched.
    pub watched_for_s: u16,
}

impl PictureRate {
    /// The rate as the board follows it: `watched_ms` at least
    /// [`MIN_WATCHED_MS`]; a non-zero `idle_s` between [`MIN_IDLE_S`] and
    /// [`MAX_IDLE_S`] (0 stays "none"); `watched_for_s` at most
    /// [`MAX_WATCHED_FOR_S`]. So the cloud can retune by deploy, and a hub
    /// bug cannot push a board past four pictures a second or keep it fast
    /// for long.
    #[must_use]
    pub fn clamped(self) -> Self {
        Self {
            idle_s: if self.idle_s == 0 {
                0
            } else {
                self.idle_s.clamp(MIN_IDLE_S, MAX_IDLE_S)
            },
            watched_ms: self.watched_ms.max(MIN_WATCHED_MS),
            watched_for_s: self.watched_for_s.min(MAX_WATCHED_FOR_S),
        }
    }

    /// The fields' bytes after the tag: three `u16` LE.
    pub(crate) fn put(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.idle_s.to_le_bytes());
        out.extend_from_slice(&self.watched_ms.to_le_bytes());
        out.extend_from_slice(&self.watched_for_s.to_le_bytes());
    }

    /// Read the fields [`Self::put`] writes. Any values decode; the board
    /// clamps them.
    pub(crate) fn read(r: &mut FrameReader<'_>) -> Result<Self, RelayFrameError> {
        Ok(Self {
            idle_s: r.u16()?,
            watched_ms: r.u16()?,
            watched_for_s: r.u16()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_board_clamps_the_rate() {
        let rate = |idle_s, watched_ms, watched_for_s| PictureRate {
            idle_s,
            watched_ms,
            watched_for_s,
        };
        assert_eq!(rate(60, 500, 15).clamped(), rate(60, 500, 15));
        assert_eq!(
            rate(0, 0, 0).clamped(),
            rate(0, 250, 0),
            "0 idle stays none"
        );
        assert_eq!(rate(1, 249, 300).clamped(), rate(10, 250, 300));
        assert_eq!(rate(9, 250, 301).clamped(), rate(10, 250, 300));
        assert_eq!(rate(10, 251, 1000).clamped(), rate(10, 251, 300));
        assert_eq!(rate(3600, 500, 0).clamped(), rate(3600, 500, 0));
        assert_eq!(
            rate(3601, u16::MAX, u16::MAX).clamped(),
            rate(3600, u16::MAX, 300)
        );
    }
}
