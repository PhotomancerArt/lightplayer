//! What the device card's preview slot draws: the feed's newest picture
//! joined with the device facts that say how to treat it.
//!
//! The join lives HERE, at the app view, and not in `lpa-devices`: frames
//! are not evidence, so `DeviceView` stays the model's verbatim projection
//! and the card takes this beside it. The treatment vocabulary is the
//! honest-device-preview ADR's (2026-08-06): a picture says where it came
//! from and how old it is, and a never-fed card gets no entry at all — the
//! card's own sentence is the honest thing then, never a plausible pattern.
//!
//! While the editor's lens holds a device's wire the feed cannot pull (the
//! design pin: never pull under a borrow), but the lens session is already
//! pulling the same published frame at its own cadence. The card draws THAT
//! picture then ([`LensFrameSource`], joined here), judged as a live feed's
//! would be and marked as the lens's ([`DeviceCardFeedView::from_lens`]), so
//! a connected card's picture keeps moving (the board card ADR, §4; CD8,
//! after closed PR #571). It falls back to the dimmed last frame
//! ([`FeedLiveness::Lens`]) only until the lens has a frame of its own.

use lpa_devices::identity::DeviceId;
use lpa_devices::{Device, DeviceView};

use super::device_effects::DeviceEffects;
use super::device_frame_feed::DeviceFrameFeed;
use crate::UiControlProductPreview;
use crate::app::studio::refresh_cadence::FRAME_STALE_AFTER_SECS;

/// One card's picture and its treatment.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceCardFeedView {
    /// The composed picture — every published output at its own offset.
    /// `display_layout` may be `None` when the board's layout exceeded the
    /// wire's read budget: bytes without geometry, which the card names
    /// rather than draws.
    pub frame: Option<UiControlProductPreview>,
    /// Seconds since the picture's revision last moved.
    pub frame_age_secs: Option<f64>,
    /// The board's own engine rate, off its heartbeat — the pill's "N fps".
    /// For the lens's picture, the rate the lens session's own heartbeat
    /// reported (under the borrow the roster's pump is paused).
    pub engine_fps: Option<u16>,
    pub liveness: FeedLiveness,
    /// The picture is the lens session's own ([`LensFrameSource`]): the
    /// board's frames as the open session reads them, not the roster feed's.
    pub from_lens: bool,
}

/// The editor lens's own picture of the device it is open on: the lens
/// session's composed frame across every output the board has published,
/// so the card keeps a live picture while the lens holds the wire and the
/// feed's own pull is paused. Built by the controller from the lens mirror
/// (`StudioController::lens_frame_source`); `None` while no lens is on a
/// device, or before it has a frame.
#[derive(Clone, Debug, PartialEq)]
pub struct LensFrameSource {
    /// The roster device the lens is on.
    pub device: DeviceId,
    pub frame: UiControlProductPreview,
    /// Seconds since the lens's frame clock last moved
    /// ([`super::DeviceFrameFeeds::observe_lens_frames`]).
    pub frame_age_secs: Option<f64>,
    /// The engine rate the lens session's own heartbeat reported.
    pub engine_fps: Option<u16>,
}

/// How the slot treats the picture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FeedLiveness {
    /// Feeding, no frame yet.
    Waiting,
    /// A frame younger than the stale threshold on a board that is
    /// answering: calm green.
    Live,
    /// The newest frame is older than the threshold while the board is
    /// still on an open wire: amber. A paused board goes amber honestly.
    Stale,
    /// The port is closed or gone: the last in-session frame, dimmed and
    /// veiled — last known, not current.
    Offline,
    /// The editor lens holds the wire, so the feed is paused, and the lens
    /// session has no frame of its own yet: the last frame, dimmed, with
    /// "editor has the wire". Once the lens has a frame the card draws it
    /// as [`Self::Live`] (or [`Self::Stale`]), marked as the lens's.
    Lens,
}

/// The card's feed view, or `None` when there is nothing honest to draw
/// (never fed and not feeding) — the card falls back to its sentence.
/// `lens_frame` is the lens session's picture, for the device it is on.
pub fn device_card_feed_view(
    device: &Device,
    view: &DeviceView,
    feed: Option<&DeviceFrameFeed>,
    effects: &DeviceEffects,
    page_visible: bool,
    lens_frame: Option<&LensFrameSource>,
    now: f64,
) -> Option<DeviceCardFeedView> {
    let link = device.evidence.link();
    let lens = link.is_some_and(|link| effects.lens_holds_wire(link));
    let open = device.evidence.presence.is_open();
    if let Some(source) = lens_frame.filter(|source| lens && source.device == view.id) {
        return lens_feed_view(source, open, view.engine_fps);
    }
    let frame = feed.and_then(DeviceFrameFeed::frame).cloned();
    let frame_age_secs = feed.and_then(|feed| feed.frame_age_secs(now));
    let feeding = feed.is_some_and(|feed| {
        feed.is_wanted()
            && !feed.is_parked()
            && page_visible
            && super::device_frame_feed::feed_target(device, effects).is_some()
    });
    let liveness = feed_liveness(frame.is_some(), frame_age_secs, open, lens, feeding)?;
    Some(DeviceCardFeedView {
        frame,
        frame_age_secs,
        engine_fps: view.engine_fps,
        liveness,
        from_lens: false,
    })
}

/// The lens's picture as the card draws it: the lens pulls on the card's
/// behalf, so its frame is judged the way the feed's own would be on an
/// open wire with nobody in the way (Live, or Stale once the lens's frame
/// clock stops), at the lens session's engine rate (else `board_fps`, the
/// roster's last word).
pub fn lens_feed_view(
    source: &LensFrameSource,
    open: bool,
    board_fps: Option<u16>,
) -> Option<DeviceCardFeedView> {
    let liveness = feed_liveness(true, source.frame_age_secs, open, false, false)?;
    Some(DeviceCardFeedView {
        frame: Some(source.frame.clone()),
        frame_age_secs: source.frame_age_secs,
        engine_fps: source.engine_fps.or(board_fps),
        liveness,
        from_lens: true,
    })
}

/// The treatment table. Pure, so the card's five looks are one test.
/// `lens` is "the lens holds the wire and the card has no lens picture to
/// draw": with one, the caller judges the lens's frame with `lens` false.
pub fn feed_liveness(
    has_frame: bool,
    frame_age_secs: Option<f64>,
    open: bool,
    lens: bool,
    feeding: bool,
) -> Option<FeedLiveness> {
    match (has_frame, open, lens) {
        // The lens outranks everything: the wire is spoken for, and the
        // last picture is the honest one to keep up.
        (true, _, true) => Some(FeedLiveness::Lens),
        (true, false, false) => Some(FeedLiveness::Offline),
        (true, true, false) => Some(match frame_age_secs {
            Some(age) if age >= FRAME_STALE_AFTER_SECS => FeedLiveness::Stale,
            _ => FeedLiveness::Live,
        }),
        // No frame yet: only an actively feeding card has something to
        // say ("waiting for the first frame"); otherwise the card's own
        // sentence is the truth.
        (false, _, _) if feeding => Some(FeedLiveness::Waiting),
        (false, _, _) => None,
    }
}

/// Every wanted-or-fed device's view, for `DeviceRosterView.feeds`.
pub fn device_card_feed_views(
    roster: &lpa_devices::Roster,
    views: &[DeviceView],
    feeds: &super::device_frame_feed::DeviceFrameFeeds,
    effects: &DeviceEffects,
    lens_frame: Option<&LensFrameSource>,
    now: f64,
) -> std::collections::BTreeMap<DeviceId, DeviceCardFeedView> {
    views
        .iter()
        .filter_map(|view| {
            let device = roster.device(view.id)?;
            let feed = feeds.get(view.id);
            device_card_feed_view(
                device,
                view,
                feed,
                effects,
                feeds.page_visible(),
                lens_frame,
                now,
            )
            .map(|feed_view| (view.id, feed_view))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_treatment_table() {
        use FeedLiveness::*;
        let fresh = Some(1.0);
        let old = Some(FRAME_STALE_AFTER_SECS);
        // (has_frame, age, open, lens, feeding) → treatment
        let rows: &[((bool, Option<f64>, bool, bool, bool), Option<FeedLiveness>)] = &[
            ((true, fresh, true, false, true), Some(Live)),
            ((true, old, true, false, true), Some(Stale)),
            ((true, fresh, true, false, false), Some(Live)),
            ((true, fresh, false, false, false), Some(Offline)),
            ((true, old, false, false, false), Some(Offline)),
            ((true, fresh, true, true, false), Some(Lens)),
            ((true, fresh, false, true, false), Some(Lens)),
            ((false, None, true, false, true), Some(Waiting)),
            ((false, None, true, false, false), None),
            ((false, None, false, false, false), None),
            ((false, None, true, true, false), None),
        ];
        for ((has_frame, age, open, lens, feeding), expected) in rows {
            assert_eq!(
                feed_liveness(*has_frame, *age, *open, *lens, *feeding),
                *expected,
                "has_frame={has_frame} age={age:?} open={open} lens={lens} feeding={feeding}"
            );
        }
    }

    /// The lens rows (#571's, redone): while the lens holds the wire and has
    /// a frame of its own, the card draws it as a live feed would be drawn —
    /// Live while the lens's frame clock moves, Stale once it stops, Offline
    /// once the port is gone — marked as the lens's, at the lens session's
    /// engine rate (the roster's last word only when the lens has none).
    #[test]
    fn the_lens_rows() {
        use FeedLiveness::*;
        let fresh = Some(0.5);
        let old = Some(FRAME_STALE_AFTER_SECS + 1.0);
        // (age, open) → treatment
        let rows: &[((Option<f64>, bool), FeedLiveness)] = &[
            ((fresh, true), Live),
            ((old, true), Stale),
            ((None, true), Live),
            ((fresh, false), Offline),
        ];
        for ((age, open), expected) in rows {
            let view = lens_feed_view(&source(*age, Some(57)), *open, Some(12)).expect("drawn");
            assert_eq!(view.liveness, *expected, "age={age:?} open={open}");
            assert!(view.from_lens);
            assert_eq!(view.frame_age_secs, *age);
            assert_eq!(view.engine_fps, Some(57), "the lens session's rate");
            assert_eq!(view.frame.map(|frame| frame.revision), Some(9));
        }
        let quiet = lens_feed_view(&source(fresh, None), true, Some(12)).expect("drawn");
        assert_eq!(quiet.engine_fps, Some(12), "the roster's last word");
    }

    fn source(frame_age_secs: Option<f64>, engine_fps: Option<u16>) -> LensFrameSource {
        LensFrameSource {
            device: DeviceId(7),
            frame: UiControlProductPreview {
                revision: 9,
                extent: lpc_model::ControlExtent::new(1, 12),
                sample_format: crate::UiControlSampleFormat::Srgb8,
                sample_layout: lpc_model::ControlSampleLayout { spans: Vec::new() },
                display_layout: None,
                bytes: std::rc::Rc::from(vec![0u8; 12]),
            },
            frame_age_secs,
            engine_fps,
        }
    }
}
