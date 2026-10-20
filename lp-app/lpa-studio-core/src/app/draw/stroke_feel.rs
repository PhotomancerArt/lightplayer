//! How a stroke feels: every number that decides it, in one place.
//!
//! Each is plan `drawing-on-the-picture` Q8's lean (D13), a constant to tune
//! at a desk sitting rather than a setting. The idle time before a pattern's
//! "friends" come back is the pattern's own (D13), not Studio's.

use core::time::Duration;

use crate::app::studio::DEVICE_REFRESH_INTERVAL;

/// The least time between two touch writes: at most 30 a second (Q8). A
/// pointer moving faster than that collapses to its newest position; the
/// pattern draws the chord between samples itself.
pub const STROKE_SEND_INTERVAL: Duration = Duration::from_millis(33);

/// How long a touch set lives on the board without a renewal (`ttl_ms`,
/// Q8): at least three sends, plus a relay's jitter. A dropped link lets go
/// of the finger within this, with no help from Studio.
pub const STROKE_TTL_MS: u32 = 400;

/// How often a finger that rests (no new position) is written again, so its
/// set outlives [`STROKE_TTL_MS`] (Q8).
pub const STROKE_RENEW_INTERVAL: Duration = Duration::from_millis(150);

/// The picture's pace while a stroke is live and for
/// [`STROKE_FEED_LINGER`] after it: the editor's own lens gap, so the
/// lights catch up with the finger (Q8, Q14: the same rule over Bluetooth
/// and the relay).
pub const STROKE_FEED_GAP: Duration = DEVICE_REFRESH_INTERVAL;

/// How long the picture keeps [`STROKE_FEED_GAP`] after the last finger
/// lifts, before it goes back to its own gap (Q8).
pub const STROKE_FEED_LINGER: Duration = Duration::from_secs(1);

/// Touch ids run `1..TOUCH_ID_WRAP` and then start again at 1. Never 0 (an
/// empty slot), and never reused within a gesture: a finger that lifts and
/// another that lands are two strokes to the pattern.
pub const TOUCH_ID_WRAP: u32 = 1 << 16;
