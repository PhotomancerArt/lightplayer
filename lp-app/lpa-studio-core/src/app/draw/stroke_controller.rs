//! The stroke: reported fingers in, paced momentary touch writes out.
//!
//! The web reports where every finger is on a drawable picture
//! ([`crate::UiStroke`]), as often as the pointer moves. This turns that into
//! the writes a board can take (plan D11, Q8; the numbers are
//! [`super::stroke_feel`]'s):
//!
//! - each finger gets a small touch id ≥ 1 of its own for as long as it is
//!   down (the browser's pointer ids can be large, and are reused);
//! - positions go from the picture box to pattern space through the lamp
//!   layout's [`LampPictureFrame`];
//! - the first sample of a stroke is written at once, then at most one write
//!   per [`STROKE_SEND_INTERVAL`], carrying the newest positions (a tap is a
//!   one-sample dab, never dropped);
//! - a finger that rests is written again every [`STROKE_RENEW_INTERVAL`],
//!   so its set outlives its [`STROKE_TTL_MS`];
//! - the last finger up writes the empty set once, and then nothing: the
//!   board lets the writer go when its ttl runs out.
//!
//! Every write is a momentary [`PanelWriteOp`] holding a
//! [`PanelWriteValue::Map`] of [`Touch`] values keyed by touch id. Time is
//! the caller's (the controller's injected clock, in epoch seconds); nothing
//! here reads one.

use core::time::Duration;

use lpc_model::{PanelWriteValue, ToLpValue, Touch};

use super::stroke_feel::{
    STROKE_FEED_GAP, STROKE_FEED_LINGER, STROKE_RENEW_INTERVAL, STROKE_SEND_INTERVAL,
    STROKE_TTL_MS, TOUCH_ID_WRAP,
};
use super::{LampPictureFrame, UiDrawSurface, UiDrawTarget, UiStrokePointer};
use crate::PanelWriteOp;

/// A float's worth of slack on the pacing compares, so a write due at
/// exactly 33 ms is not pushed a tick later by rounding.
const DUE_EPSILON_SECS: f64 = 1e-6;

/// The stroke in progress, if any, and what the picture feed owes it.
#[derive(Clone, Debug)]
pub struct StrokeController {
    stroke: Option<Stroke>,
    /// The next touch id to hand a finger.
    next_id: u32,
    /// The picture keeps the stroke's pace until then (epoch seconds) once
    /// the last finger is up.
    feed_until: Option<f64>,
}

#[derive(Clone, Debug)]
struct Stroke {
    surface: UiDrawSurface,
    /// Fixed when the stroke starts: a gesture keeps writing where it began.
    target: UiDrawTarget,
    fingers: Vec<Finger>,
    /// When the last write went out; a stroke's first sample always writes.
    last_sent: f64,
    /// A position moved (or a finger came or went) since the last write.
    dirty: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Finger {
    pointer: u32,
    id: u32,
    /// Pattern space.
    pos: [f32; 2],
}

impl Default for StrokeController {
    fn default() -> Self {
        Self {
            stroke: None,
            next_id: 1,
            feed_until: None,
        }
    }
}

impl StrokeController {
    /// The fingers on `surface` now (non-empty). Returns the write to send
    /// now, if one is due: the first sample of a stroke always, later ones
    /// once [`STROKE_SEND_INTERVAL`] has passed since the last write.
    ///
    /// A stroke already live keeps its own target; `target` and `surface`
    /// only start one.
    pub fn sample(
        &mut self,
        now: f64,
        surface: UiDrawSurface,
        target: &UiDrawTarget,
        frame: &LampPictureFrame,
        pointers: &[UiStrokePointer],
    ) -> Option<PanelWriteOp> {
        if pointers.is_empty() {
            return self.release(now);
        }
        let mut fingers: Vec<Finger> = Vec::with_capacity(pointers.len());
        for pointer in pointers {
            let known = self
                .stroke
                .as_ref()
                .and_then(|stroke| stroke.finger(pointer.pointer))
                .map(|finger| finger.id);
            let id = match known {
                Some(id) => id,
                None => self.mint_id(),
            };
            fingers.push(Finger {
                pointer: pointer.pointer,
                id,
                pos: frame.picture_to_pattern([pointer.x, pointer.y]),
            });
        }
        match &mut self.stroke {
            Some(stroke) => {
                if stroke.fingers != fingers {
                    stroke.fingers = fingers;
                    stroke.dirty = true;
                }
                if stroke.dirty
                    && now - stroke.last_sent + DUE_EPSILON_SECS >= secs(STROKE_SEND_INTERVAL)
                {
                    return Some(self.send(now));
                }
                None
            }
            None => {
                self.stroke = Some(Stroke {
                    surface,
                    target: target.clone(),
                    fingers,
                    last_sent: now,
                    dirty: true,
                });
                Some(self.send(now))
            }
        }
    }

    /// Every finger is up: the empty set, once. `None` when no stroke was
    /// live (nothing to let go of).
    pub fn release(&mut self, now: f64) -> Option<PanelWriteOp> {
        let stroke = self.stroke.take()?;
        self.feed_until = Some(now + secs(STROKE_FEED_LINGER));
        Some(write(&stroke.target, &[]))
    }

    /// The picture stopped being drawable under the stroke (the session
    /// closed, the board went away): forget it without a write. The board
    /// lets the fingers go when their ttl runs out.
    pub fn cancel(&mut self) {
        self.stroke = None;
        self.feed_until = None;
    }

    /// The write due at `now` with no new sample: the newest positions once
    /// the send interval has passed, or a resting finger's renewal.
    pub fn due(&mut self, now: f64) -> Option<PanelWriteOp> {
        let stroke = self.stroke.as_ref()?;
        let interval = match stroke.dirty {
            true => STROKE_SEND_INTERVAL,
            false => STROKE_RENEW_INTERVAL,
        };
        (now - stroke.last_sent + DUE_EPSILON_SECS >= secs(interval)).then(|| self.send(now))
    }

    /// How long until [`Self::due`] has a write, while a stroke is live.
    pub fn next_due_in(&self, now: f64) -> Option<Duration> {
        let stroke = self.stroke.as_ref()?;
        let interval = match stroke.dirty {
            true => STROKE_SEND_INTERVAL,
            false => STROKE_RENEW_INTERVAL,
        };
        let wait = (stroke.last_sent + secs(interval) - now).max(0.0);
        // Whole milliseconds, rounded up (from whole microseconds, so a
        // float's last bit does not add one): a timer that fires a hair
        // early would only find nothing due.
        let micros = (wait * 1_000_000.0).round() as u64;
        Some(Duration::from_millis(micros.div_ceil(1000)))
    }

    /// The picture's pace while a stroke is live and for
    /// [`STROKE_FEED_LINGER`] after it; `None` otherwise.
    pub fn feed_gap(&self, now: f64) -> Option<Duration> {
        let lingering = self.feed_until.is_some_and(|until| now < until);
        (self.stroke.is_some() || lingering).then_some(STROKE_FEED_GAP)
    }

    /// The surface of the stroke in progress.
    pub fn surface(&self) -> Option<UiDrawSurface> {
        self.stroke.as_ref().map(|stroke| stroke.surface)
    }

    /// Write the live stroke's fingers now.
    fn send(&mut self, now: f64) -> PanelWriteOp {
        let stroke = self.stroke.as_mut().expect("a live stroke");
        stroke.last_sent = now;
        stroke.dirty = false;
        write(&stroke.target, &stroke.fingers)
    }

    fn mint_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = if id + 1 >= TOUCH_ID_WRAP { 1 } else { id + 1 };
        id
    }
}

impl Stroke {
    fn finger(&self, pointer: u32) -> Option<&Finger> {
        self.fingers.iter().find(|finger| finger.pointer == pointer)
    }
}

/// The momentary touch-set write for `fingers` (the empty set releases).
fn write(target: &UiDrawTarget, fingers: &[Finger]) -> PanelWriteOp {
    PanelWriteOp {
        scope: target.scope,
        channel: target.channel.clone(),
        value: PanelWriteValue::Map(
            fingers
                .iter()
                .map(|finger| (finger.id, Touch::new(finger.id, finger.pos).to_lp_value()))
                .collect(),
        ),
        ttl_ms: Some(STROKE_TTL_MS),
    }
}

fn secs(duration: Duration) -> f64 {
    duration.as_secs_f64()
}

#[cfg(test)]
mod tests {
    use lpc_model::{ControlLamp2d, ControlLayout2d, FromLpValue, LpValue, NodeId, Revision};

    use super::*;

    /// 120 Hz samples for a second write at most 30 a second, each with the
    /// newest positions, all momentary at 400 ms.
    #[test]
    fn a_120_hz_stroke_writes_at_most_30_times_a_second_with_the_newest_positions() {
        let (mut strokes, frame) = fixture();
        let mut writes: Vec<(f64, f32, PanelWriteOp)> = Vec::new();
        for sample in 0..120 {
            let now = 100.0 + f64::from(sample) / 120.0;
            let x = sample as f32 / 120.0;
            let write = strokes.sample(now, SURFACE, &target(), &frame, &[finger(42, x)]);
            writes.extend(write.map(|write| (now, x, write)));
            writes.extend(strokes.due(now).map(|write| (now, x, write)));
        }
        assert!(writes.len() <= 31, "{} writes in one second", writes.len());
        assert!(
            writes.len() >= 25,
            "the pace is 30 Hz, not less: {}",
            writes.len()
        );
        for pair in writes.windows(2) {
            assert!(
                pair[1].0 - pair[0].0 >= 0.033 - 1e-6,
                "two writes {} ms apart",
                (pair[1].0 - pair[0].0) * 1000.0
            );
        }
        for (_, x, write) in &writes {
            assert_eq!(write.ttl_ms, Some(400));
            assert_eq!(write.channel, "touches");
            // Each write holds the newest position: the sample it went
            // out with, in pattern space, under the stroke's one id.
            let expected = frame.picture_to_pattern([*x, 0.5]);
            assert_eq!(touches(write), vec![Touch::new(1, expected)]);
        }
    }

    /// A finger that stops moving is written again every 150 ms; lifting it
    /// writes the empty set once, and then nothing.
    #[test]
    fn a_resting_finger_renews_and_a_release_writes_the_empty_set_once() {
        let (mut strokes, frame) = fixture();
        let first = strokes
            .sample(10.0, SURFACE, &target(), &frame, &[finger(7, 0.5)])
            .expect("the first sample writes at once");
        assert_eq!(touches(&first).len(), 1);
        assert_eq!(strokes.next_due_in(10.0), Some(Duration::from_millis(150)));
        assert!(strokes.due(10.1).is_none(), "not yet");
        let renewal = strokes.due(10.15).expect("a renewal at 150 ms");
        assert_eq!(touches(&renewal), touches(&first));
        assert!(strokes.due(10.2).is_none());

        let release = strokes.release(10.25).expect("the release");
        assert_eq!(release.value, PanelWriteValue::Map(Vec::new()));
        assert_eq!(release.ttl_ms, Some(400));
        assert!(strokes.release(10.3).is_none(), "once");
        assert!(strokes.due(11.0).is_none(), "and then nothing");
        assert_eq!(strokes.next_due_in(11.0), None);
    }

    /// A tap: one sample, then up — a dab and its release, both written.
    #[test]
    fn a_tap_is_a_one_sample_dab() {
        let (mut strokes, frame) = fixture();
        let dab = strokes.sample(5.0, SURFACE, &target(), &frame, &[finger(9, 0.5)]);
        assert_eq!(dab.map(|write| touches(&write).len()), Some(1));
        let release = strokes.sample(5.005, SURFACE, &target(), &frame, &[]);
        assert_eq!(
            release.map(|write| write.value),
            Some(PanelWriteValue::Map(Vec::new()))
        );
    }

    /// Touch ids are core's: small, ≥ 1, stable per finger, never reused by
    /// a new finger in the same gesture.
    #[test]
    fn each_finger_keeps_its_own_small_id() {
        let (mut strokes, frame) = fixture();
        let one = strokes
            .sample(1.0, SURFACE, &target(), &frame, &[finger(9001, 0.2)])
            .expect("write");
        assert_eq!(ids(&one), vec![1]);
        let two = strokes
            .sample(
                1.1,
                SURFACE,
                &target(),
                &frame,
                &[finger(9001, 0.3), finger(3, 0.7)],
            )
            .expect("write");
        assert_eq!(ids(&two), vec![1, 2]);
        // The first finger lifts; another lands with the first's browser id.
        let swapped = strokes
            .sample(1.2, SURFACE, &target(), &frame, &[finger(3, 0.7)])
            .expect("write");
        assert_eq!(ids(&swapped), vec![2]);
        let again = strokes
            .sample(
                1.3,
                SURFACE,
                &target(),
                &frame,
                &[finger(3, 0.7), finger(9001, 0.1)],
            )
            .expect("write");
        assert_eq!(ids(&again), vec![2, 3]);
    }

    /// The picture runs at the editor's pace while drawing and for a second
    /// after the release, then goes back to its own.
    #[test]
    fn the_feed_is_fast_during_a_stroke_and_for_a_second_after() {
        let (mut strokes, frame) = fixture();
        assert_eq!(strokes.feed_gap(0.0), None);
        strokes.sample(1.0, SURFACE, &target(), &frame, &[finger(1, 0.5)]);
        assert_eq!(strokes.feed_gap(1.5), Some(Duration::from_millis(75)));
        strokes.release(2.0);
        assert_eq!(strokes.feed_gap(2.9), Some(Duration::from_millis(75)));
        assert_eq!(strokes.feed_gap(3.01), None);
    }

    const SURFACE: UiDrawSurface = UiDrawSurface::Project;

    fn fixture() -> (StrokeController, LampPictureFrame) {
        let lamps = (0..4)
            .map(|index| ControlLamp2d {
                lamp_index: index,
                sample_start: index * 3,
                center: [0.2 + 0.2 * index as f32, 0.5],
                radius: 0.01,
            })
            .collect();
        let layout = ControlLayout2d::new(Revision::new(1), 100, 50, lamps);
        (
            StrokeController::default(),
            LampPictureFrame::of(&layout).expect("lamps"),
        )
    }

    fn target() -> UiDrawTarget {
        UiDrawTarget {
            scope: lpc_wire::WireScopeRef::Module {
                owner: NodeId::new(4),
            },
            channel: "touches".to_string(),
        }
    }

    fn finger(pointer: u32, x: f32) -> UiStrokePointer {
        UiStrokePointer { pointer, x, y: 0.5 }
    }

    fn touches(write: &PanelWriteOp) -> Vec<Touch> {
        let PanelWriteValue::Map(entries) = &write.value else {
            panic!("a touch set is a map");
        };
        entries
            .iter()
            .map(|(key, value): &(u32, LpValue)| {
                let touch = Touch::from_lp_value(value).expect("a touch");
                assert_eq!(*key, touch.id, "keyed by its own id");
                touch
            })
            .collect()
    }

    fn ids(write: &PanelWriteOp) -> Vec<u32> {
        touches(write).iter().map(|touch| touch.id).collect()
    }
}
