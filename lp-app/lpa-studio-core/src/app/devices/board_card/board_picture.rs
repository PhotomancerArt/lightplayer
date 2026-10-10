//! The card's picture, and the words that used to be drawn over it.
//!
//! The picture draws the board's lights or stays dark; nothing covers it
//! (Q10). Where it comes from is its source: the board's link (live, stale,
//! or waiting for its first frame), the editor's lens (the last frame it
//! left, dimmed), the last saved picture (dimmed), or nothing. While an
//! update holds the board's lights, the picture is their one colour.
//!
//! Today's card said why there was no picture with a sentence in the slot
//! (`preview_sentence`, `preview_slot_sentence`, `LOCKED_PREVIEW_SENTENCE`,
//! the firmware faces' sentences) and named the picture's source with a
//! pill ("live · 43 fps", "last frame · 12 s ago"). Those words are the
//! status corner's "Picture" line now, word for word ([`picture_line`]).

use lpa_devices::device::DeviceStatus;
use lpa_devices::view::{ActivityView, DeviceView, LoadedProject};

use super::board_card_input::BoardCardInput;
use super::held_board::{HELD_NO_PICTURE_LINE, HELD_PICTURE_LINE};
use super::ui_board_picture::{PictureSource, UiBoardPicture};
use crate::app::devices::age_words::age_words;
use crate::app::devices::device_card_feed_view::{DeviceCardFeedView, FeedLiveness};
use crate::app::devices::device_firmware_face::firmware_face_preview_sentence;
use crate::app::devices::ui_link_kind::UiLinkKind;

/// The picture's words on a board nothing has unlocked: it is locked, and
/// Unlock (the card's primary) is the way in.
pub const LOCKED_PREVIEW_SENTENCE: &str = "Locked — Unlock it to see what it runs.";

/// The card's picture.
pub(crate) fn board_picture(input: &BoardCardInput<'_>) -> UiBoardPicture {
    let feed = picture_feed(input);
    let lens_live = feed.is_some_and(lens_is_live);
    let source = match feed.map(|feed| feed.liveness) {
        // A board another tab holds has no link here: whatever the feed
        // says, the picture is the one that tab saved.
        Some(_) if input.held().is_some() => PictureSource::Saved,
        // The open session's own frames (CD8): the lens's, current.
        Some(_) if lens_live => PictureSource::Lens,
        Some(FeedLiveness::Live | FeedLiveness::Stale | FeedLiveness::Waiting) => {
            PictureSource::Link
        }
        Some(FeedLiveness::Offline) => PictureSource::Saved,
        Some(FeedLiveness::Lens) => PictureSource::Lens,
        None => PictureSource::None,
    };
    UiBoardPicture {
        source,
        frame: feed
            .and_then(|feed| feed.frame.clone())
            .filter(|frame| frame.display_layout.is_some()),
        // Last known rather than current: a saved picture, or the frame the
        // feed left when the lens took the wire. The lens's own frames are
        // current, and drawn so.
        dim: match source {
            PictureSource::Saved => true,
            PictureSource::Lens => !lens_live,
            PictureSource::Link | PictureSource::None => false,
        },
        light: input.update.and_then(|update| update.light),
    }
}

/// The picture is the lens session's own and current: live, or stale on a
/// board that stopped publishing, read by the open session (CD8).
pub(crate) fn lens_is_live(feed: &DeviceCardFeedView) -> bool {
    feed.from_lens && matches!(feed.liveness, FeedLiveness::Live | FeedLiveness::Stale)
}

/// The status corner's "Picture" line: the sentence that says why there is
/// no picture (or why it is not the whole story), else where the picture
/// comes from and how old it is. `None` when there is nothing to say.
pub(crate) fn picture_line(input: &BoardCardInput<'_>) -> Option<String> {
    // The show has stopped for an update: the board's lights are the
    // update's, and its sentence says why.
    if let Some(update) = input.update.filter(|update| update.light.is_some()) {
        return Some(update.sentence.clone());
    }
    // An update the show keeps running through (no light: a backup reads
    // the old firmware back first) does not take the picture away.
    let activity = input
        .view
        .activity
        .as_ref()
        .filter(|_| !input.update.is_some_and(|update| update.light.is_none()));
    if input.locked() && activity.is_none() {
        return Some(LOCKED_PREVIEW_SENTENCE.to_string());
    }
    let feed = picture_feed(input);
    // The picture of a board another tab holds is the one that tab saved.
    if input.held().is_some() {
        return Some(match feed.and_then(|feed| feed.frame.as_ref()) {
            Some(_) => HELD_PICTURE_LINE.to_string(),
            None => HELD_NO_PICTURE_LINE.to_string(),
        });
    }
    preview_slot_sentence(input.view, activity, feed)
        .or_else(|| feed.and_then(|feed| source_words(feed, activity, input.link_kind())))
}

/// The feed the picture is drawn from: none on a locked board (it answers
/// nothing but its hello and the unlock).
fn picture_feed<'a>(input: &BoardCardInput<'a>) -> Option<&'a DeviceCardFeedView> {
    input.feed.filter(|_| !input.locked())
}

/// Where the picture came from and how old it is — today's pill: nothing
/// while an activity runs or before a frame exists.
///
/// Over Bluetooth the live words also say how often the picture moves: the
/// card pulls there at a gentler pace (`DEVICE_CARD_FEED_BLE_INTERVAL`, one
/// to two pictures a second), so the board's "43 fps" beside a picture
/// that steps once a second would read as a stall.
pub(crate) fn source_words(
    feed: &DeviceCardFeedView,
    activity: Option<&ActivityView>,
    over: UiLinkKind,
) -> Option<String> {
    if activity.is_some() {
        return None;
    }
    feed.frame.as_ref()?;
    let age = || age_words(feed.frame_age_secs.unwrap_or_default());
    // The open session's frames, named as its (the ADR's §4: the details
    // say where the picture comes from), at its own engine rate. Its pace
    // is the session's reads, not the card feed's, so a Bluetooth link adds
    // no pace words here.
    if lens_is_live(feed) {
        return Some(match (feed.liveness, feed.engine_fps) {
            (FeedLiveness::Live, Some(fps)) => format!("live · {fps} fps · {LENS_SOURCE_WORDS}"),
            (FeedLiveness::Live, None) => format!("live · {LENS_SOURCE_WORDS}"),
            _ => format!("last frame · {} · {LENS_SOURCE_WORDS}", age()),
        });
    }
    Some(match feed.liveness {
        FeedLiveness::Waiting => return None,
        // The board's engine rate off its heartbeat; a board that has not
        // reported one says "live" and nothing more.
        FeedLiveness::Live => match (feed.engine_fps, over == UiLinkKind::Bluetooth) {
            (Some(fps), false) => format!("live · {fps} fps"),
            (None, false) => "live".to_string(),
            (Some(fps), true) => format!("live · {fps} fps · {BLUETOOTH_PICTURE_PACE}"),
            (None, true) => format!("live · {BLUETOOTH_PICTURE_PACE}"),
        },
        FeedLiveness::Stale | FeedLiveness::Offline => format!("last frame · {}", age()),
        FeedLiveness::Lens => "editor has the wire".to_string(),
    })
}

/// How often a Bluetooth card's picture moves, in the live words (see
/// [`source_words`]).
const BLUETOOTH_PICTURE_PACE: &str = "shown 1–2/s";

/// Where the connected card's picture comes from, in the picture line: the
/// editor's session, which reads the board's frames while it holds the
/// wire (CD8).
pub const LENS_SOURCE_WORDS: &str = "from the editor's session";

/// The sentence when the picture is not the whole story.
fn preview_slot_sentence(
    view: &DeviceView,
    activity: Option<&ActivityView>,
    feed: Option<&DeviceCardFeedView>,
) -> Option<String> {
    if activity.is_some() {
        return Some(preview_sentence(view, activity));
    }
    let Some(feed) = feed else {
        return Some(preview_sentence(view, activity));
    };
    match &feed.frame {
        Some(frame) if frame.display_layout.is_some() => None,
        // Frames without geometry: the board declined the display layout
        // (over the link's read budget at this scale).
        Some(_) => Some(
            "Frames are flowing, but this project's lamp layout is too large to preview over \
             this link."
                .to_string(),
        ),
        None => Some(match feed.liveness {
            FeedLiveness::Waiting => "Waiting for the first frame…".to_string(),
            // The editor lens holds this board's wire, so the feed does not
            // pull (ADR 2026-09-06, "never pull under a borrow").
            FeedLiveness::Lens => "Picture paused while the editor is open.".to_string(),
            _ => preview_sentence(view, activity),
        }),
    }
}

/// Why there is no picture, in this state, in plain words — the same on
/// every link: the feed runs over USB, Wi‑Fi and Bluetooth alike.
fn preview_sentence(view: &DeviceView, activity: Option<&ActivityView>) -> String {
    if activity.is_none()
        && let Some(sentence) = firmware_face_preview_sentence(&view.firmware_face)
    {
        return sentence;
    }
    if let Some(activity) = activity {
        let label = activity.label.trim_end_matches(['…', '.', ' ']);
        return format!("{label}… the picture returns when the board does.");
    }
    // A board Studio cannot reach has no feed coming: say when it was last
    // heard (today's remembered tile's words), never "coming".
    if view.status == DeviceStatus::Offline {
        return match &view.freshness_label {
            Some(heard) => format!("Not connected — {heard}."),
            None => "Not connected — Studio has not heard this board.".to_string(),
        };
    }
    if view.loaded_project == LoadedProject::Empty {
        return "Nothing on it yet — no picture until something runs.".to_string();
    }
    "No picture yet — the live feed is coming.".to_string()
}

#[cfg(test)]
mod tests {
    use lpa_devices::ActivityKind;
    use lpa_devices::view::FirmwareFace;

    use super::super::card_fixtures::{CardFixture, activity, feed, lens_feed};
    use super::*;
    use crate::{UpdateLight, UpdateRowKind, UpdateVersion};

    #[test]
    fn each_liveness_names_its_source_and_dims_what_is_not_current() {
        for (liveness, source, dim) in [
            (FeedLiveness::Live, PictureSource::Link, false),
            (FeedLiveness::Stale, PictureSource::Link, false),
            (FeedLiveness::Waiting, PictureSource::Link, false),
            (FeedLiveness::Offline, PictureSource::Saved, true),
            (FeedLiveness::Lens, PictureSource::Lens, true),
        ] {
            let mut fixture = CardFixture::ready();
            fixture.feed = Some(feed(liveness, true));
            let picture = board_picture(&fixture.input());
            assert_eq!(picture.source, source, "{liveness:?}");
            assert_eq!(picture.dim, dim, "{liveness:?}");
            assert_eq!(
                picture.frame.is_some(),
                liveness != FeedLiveness::Waiting,
                "{liveness:?}"
            );
        }
        let picture = board_picture(&CardFixture::ready().input());
        assert_eq!(picture.source, PictureSource::None);
        assert_eq!(picture.frame, None);
    }

    /// CD8: the open session's own frames are the lens's picture, current —
    /// not dimmed — and the corner reads the session's rate and names where
    /// the picture comes from. A board that stops publishing goes amber
    /// under the lens as it does under the feed, still the lens's.
    #[test]
    fn the_lens_sessions_frames_are_the_lens_picture_not_dimmed() {
        let mut fixture = CardFixture::ready();
        fixture.feed = Some(lens_feed());
        let picture = board_picture(&fixture.input());
        assert_eq!(picture.source, PictureSource::Lens);
        assert!(!picture.dim, "current, not last known");
        assert!(picture.frame.is_some());
        assert_eq!(
            picture_line(&fixture.input()).as_deref(),
            Some("live · 57 fps · from the editor's session")
        );
        let corner = super::super::status_corner::status_corner(&fixture.input(), &[]);
        assert_eq!(
            corner.reading.as_deref(),
            Some("57 fps"),
            "the session's rate, not the board's last word (58)"
        );

        let mut stale = lens_feed();
        stale.liveness = FeedLiveness::Stale;
        stale.frame_age_secs = Some(12.0);
        fixture.feed = Some(stale);
        let picture = board_picture(&fixture.input());
        assert_eq!(picture.source, PictureSource::Lens);
        assert!(!picture.dim);
        assert_eq!(
            picture_line(&fixture.input()).as_deref(),
            Some("last frame · 12 s ago · from the editor's session")
        );

        // Over Bluetooth the session's reads set the pace: no card-feed
        // pace words.
        let mut over_ble = CardFixture::ready().over(UiLinkKind::Bluetooth);
        over_ble.feed = Some(lens_feed());
        assert_eq!(
            picture_line(&over_ble.input()).as_deref(),
            Some("live · 57 fps · from the editor's session")
        );
    }

    #[test]
    fn a_frame_without_geometry_is_not_drawn() {
        let mut fixture = CardFixture::ready();
        fixture.feed = Some(feed(FeedLiveness::Live, false));
        assert_eq!(board_picture(&fixture.input()).frame, None);
        assert!(
            picture_line(&fixture.input())
                .is_some_and(|line| line.contains("too large to preview"))
        );
    }

    #[test]
    fn a_locked_board_has_no_picture_and_says_so() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Bluetooth).locked();
        fixture.feed = Some(feed(FeedLiveness::Live, true));
        let picture = board_picture(&fixture.input());
        assert_eq!(picture.frame, None);
        assert_eq!(picture.source, PictureSource::None);
        assert_eq!(
            picture_line(&fixture.input()).as_deref(),
            Some(LOCKED_PREVIEW_SENTENCE)
        );
    }

    #[test]
    fn the_update_light_holds_the_picture_and_its_sentence_is_the_line() {
        let mut fixture = CardFixture::ready();
        fixture.update = Some(update_words(Some(UpdateLight::DarkYellow)));
        assert_eq!(
            board_picture(&fixture.input()).light,
            Some(UpdateLight::DarkYellow)
        );
        assert_eq!(
            picture_line(&fixture.input()).as_deref(),
            Some("Updating to 2026.10.05-2 over USB… 40%. Keep the board powered.")
        );
    }

    /// The source words are today's pill, in the corner now.
    #[test]
    fn the_picture_line_names_the_pictures_provenance_and_age() {
        let line = |liveness| {
            let mut fixture = CardFixture::ready();
            fixture.feed = Some(feed(liveness, true));
            picture_line(&fixture.input())
        };
        assert_eq!(line(FeedLiveness::Live).as_deref(), Some("live · 43 fps"));
        assert_eq!(
            line(FeedLiveness::Stale).as_deref(),
            Some("last frame · 12 s ago")
        );
        assert_eq!(
            line(FeedLiveness::Offline).as_deref(),
            Some("last frame · 12 s ago")
        );
        assert_eq!(
            line(FeedLiveness::Lens).as_deref(),
            Some("editor has the wire")
        );
    }

    /// Ported from the old card: the sentence yields to a picture with
    /// geometry, names a picture without one, waits honestly, and otherwise
    /// stays the board's own — an activity always wins.
    #[test]
    fn the_slot_sentence_yields_to_the_picture() {
        let card = CardFixture::ready().view;
        assert_eq!(
            preview_slot_sentence(&card, None, Some(&feed(FeedLiveness::Live, true))),
            None
        );
        assert!(
            preview_slot_sentence(&card, None, Some(&feed(FeedLiveness::Live, false)))
                .is_some_and(|s| s.contains("too large to preview")),
        );
        assert_eq!(
            preview_slot_sentence(&card, None, Some(&feed(FeedLiveness::Waiting, true))),
            Some("Waiting for the first frame…".to_string())
        );
        let lens_no_frame = DeviceCardFeedView {
            frame: None,
            liveness: FeedLiveness::Lens,
            frame_age_secs: None,
            engine_fps: None,
            from_lens: false,
        };
        assert_eq!(
            preview_slot_sentence(&card, None, Some(&lens_no_frame)),
            Some("Picture paused while the editor is open.".to_string())
        );
        assert_eq!(
            preview_slot_sentence(&card, None, None),
            Some(preview_sentence(&card, None))
        );
        let flashing = activity(ActivityKind::Flash, "Flashing firmware…", None);
        assert_eq!(
            preview_slot_sentence(
                &card,
                Some(&flashing),
                Some(&feed(FeedLiveness::Live, true))
            ),
            Some(preview_sentence(&card, Some(&flashing)))
        );
    }

    /// Ported: every state's picture line says something honest.
    #[test]
    fn every_state_has_an_honest_preview_sentence() {
        let mut card = CardFixture::ready().view;
        assert_eq!(
            preview_sentence(&card, None),
            "No picture yet — the live feed is coming."
        );
        card.loaded_project = LoadedProject::Empty;
        assert_eq!(
            preview_sentence(&card, None),
            "Nothing on it yet — no picture until something runs."
        );
        card.firmware_face = FirmwareFace::Blank;
        assert_eq!(
            preview_sentence(&card, None),
            "Nothing running — a blank chip has no picture."
        );
        // An activity outranks the blank-chip reading, and the label's own
        // trailing ellipsis is not doubled.
        let flashing = activity(ActivityKind::Flash, "Flashing firmware…", None);
        assert_eq!(
            preview_sentence(&card, Some(&flashing)),
            "Flashing firmware… the picture returns when the board does."
        );
    }

    /// An offline board's picture line says it is not connected and when it
    /// was last heard — never that a feed is coming (today's remembered
    /// tile's words, ported from the page's offline tile).
    #[test]
    fn an_offline_boards_picture_line_says_when_it_was_heard() {
        let mut offline = CardFixture::offline();
        offline.view.freshness_label = Some("last heard 4 min ago".to_string());
        assert_eq!(
            picture_line(&offline.input()).as_deref(),
            Some("Not connected — last heard 4 min ago.")
        );
        offline.view.freshness_label = None;
        assert_eq!(
            picture_line(&offline.input()).as_deref(),
            Some("Not connected — Studio has not heard this board.")
        );
        // Over its last link too: an offline Bluetooth board promises no
        // Bluetooth picture either.
        let mut ble = CardFixture::offline().over(UiLinkKind::Bluetooth);
        ble.view.freshness_label = None;
        assert_eq!(
            picture_line(&ble.input()).as_deref(),
            Some("Not connected — Studio has not heard this board.")
        );
    }

    /// The picture runs on every link (#1062: over Bluetooth at a gentler
    /// pace), so no link says it has none, and a live Bluetooth picture's
    /// words say how often it moves.
    #[test]
    fn every_link_waits_for_its_picture_and_bluetooth_says_its_pace() {
        for link in [
            UiLinkKind::Usb,
            UiLinkKind::Wifi,
            UiLinkKind::Relay,
            UiLinkKind::Bluetooth,
        ] {
            let mut fixture = CardFixture::ready().over(link);
            assert_eq!(
                picture_line(&fixture.input()).as_deref(),
                Some("No picture yet — the live feed is coming."),
                "{link:?}"
            );
        }
        let mut live = CardFixture::ready().over(UiLinkKind::Bluetooth);
        live.feed = Some(feed(FeedLiveness::Live, true));
        assert_eq!(
            picture_line(&live.input()).as_deref(),
            Some("live · 43 fps · shown 1–2/s")
        );
        let mut no_fps = live.clone();
        no_fps.feed.as_mut().expect("a feed").engine_fps = None;
        assert_eq!(
            picture_line(&no_fps.input()).as_deref(),
            Some("live · shown 1–2/s")
        );
        // Stale reads the same on every link; Wi‑Fi's live words have no pace.
        let mut stale = CardFixture::ready().over(UiLinkKind::Bluetooth);
        stale.feed = Some(feed(FeedLiveness::Stale, true));
        assert_eq!(
            picture_line(&stale.input()).as_deref(),
            Some("last frame · 12 s ago")
        );
        let mut wifi = CardFixture::ready().over(UiLinkKind::Wifi);
        wifi.feed = Some(feed(FeedLiveness::Live, true));
        assert_eq!(
            picture_line(&wifi.input()).as_deref(),
            Some("live · 43 fps")
        );
    }

    fn update_words(light: Option<UpdateLight>) -> crate::UiDeviceUpdate {
        crate::UiDeviceUpdate {
            kind: UpdateRowKind::Progress,
            line: "Updating · 1 of 2 · 40%".to_string(),
            sentence: "Updating to 2026.10.05-2 over USB… 40%. Keep the board powered.".to_string(),
            light,
            chip: "Updating".to_string(),
            version: UpdateVersion::new("2026.10.03-1").display(),
            progress: None,
            standing: crate::UpdateStanding::Nothing,
        }
    }
}
