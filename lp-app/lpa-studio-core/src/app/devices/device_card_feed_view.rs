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
//! While the editor lens holds a device's wire the feed cannot pull (design
//! pin: never pull under a borrow), but the lens session is already pulling
//! the same published frame at its own cadence for the editor. The card
//! draws THAT picture then ([`LensFrameSource`], joined here) — live, the
//! same pill as ever — and falls back to the dimmed last frame only until
//! the lens has produced a frame of its own.

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
    pub engine_fps: Option<u16>,
    pub liveness: FeedLiveness,
}

/// The editor lens's own picture of the device it is open on: the lens
/// session's composed frame across every output the board has published,
/// so the card keeps a live picture while the lens holds the wire and the
/// feed's own pull is paused. Built by the controller from the lens
/// mirror; `None` while no lens is on a device or it has no frame yet.
#[derive(Clone, Debug, PartialEq)]
pub struct LensFrameSource {
    /// The roster device the lens is on.
    pub device: DeviceId,
    pub frame: UiControlProductPreview,
    /// Seconds since the lens's frame clock last moved
    /// ([`super::DeviceFrameFeeds::observe_lens_frames`]).
    pub frame_age_secs: Option<f64>,
    /// The engine rate the lens session's own heartbeat reported — under
    /// the borrow the pump is paused, so the mirror's figure stops moving.
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
    /// "editor has the wire". Once the lens produces a frame the card
    /// draws it as [`Self::Live`] (or [`Self::Stale`]).
    Lens,
}

/// The card's feed view, or `None` when there is nothing honest to draw
/// (never fed and not feeding) — the card falls back to its sentence.
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
        // The lens is pulling on the card's behalf: its picture, judged the
        // way the feed's own would be on an open wire with nobody in the way.
        let liveness = feed_liveness(true, source.frame_age_secs, open, false, false)?;
        return Some(DeviceCardFeedView {
            frame: Some(source.frame.clone()),
            frame_age_secs: source.frame_age_secs,
            engine_fps: source.engine_fps.or(view.engine_fps),
            liveness,
        });
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
    })
}

/// The treatment table. Pure, so the card's five looks are one test.
/// `lens` is "the lens holds the wire and the card has no lens picture to
/// draw" — with one, the caller judges the lens's frame with `lens` false.
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
}
