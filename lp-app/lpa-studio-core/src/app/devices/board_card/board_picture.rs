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

use lpa_devices::view::{ActivityView, DeviceView, LoadedProject};

use super::board_card_input::BoardCardInput;
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
    let source = match feed.map(|feed| feed.liveness) {
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
        dim: matches!(source, PictureSource::Saved | PictureSource::Lens),
        light: input.update.and_then(|update| update.light),
    }
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
    preview_slot_sentence(input.view, activity, feed, input.link_kind())
        .or_else(|| feed.and_then(|feed| source_words(feed, activity)))
}

/// The feed the picture is drawn from: none on a locked board (it answers
/// nothing but its hello and the unlock).
fn picture_feed<'a>(input: &BoardCardInput<'a>) -> Option<&'a DeviceCardFeedView> {
    input.feed.filter(|_| !input.locked())
}

/// Where the picture came from and how old it is — today's pill: nothing
/// while an activity runs or before a frame exists.
pub(crate) fn source_words(
    feed: &DeviceCardFeedView,
    activity: Option<&ActivityView>,
) -> Option<String> {
    if activity.is_some() {
        return None;
    }
    feed.frame.as_ref()?;
    let age = || age_words(feed.frame_age_secs.unwrap_or_default());
    Some(match feed.liveness {
        FeedLiveness::Waiting => return None,
        // The board's engine rate off its heartbeat; a board that has not
        // reported one says "live" and nothing more.
        FeedLiveness::Live => match feed.engine_fps {
            Some(fps) => format!("live · {fps} fps"),
            None => "live".to_string(),
        },
        FeedLiveness::Stale | FeedLiveness::Offline => format!("last frame · {}", age()),
        FeedLiveness::Lens => "editor has the wire".to_string(),
    })
}

/// The sentence when the picture is not the whole story.
fn preview_slot_sentence(
    view: &DeviceView,
    activity: Option<&ActivityView>,
    feed: Option<&DeviceCardFeedView>,
    over: UiLinkKind,
) -> Option<String> {
    if activity.is_some() {
        return Some(preview_sentence(view, activity, over));
    }
    let Some(feed) = feed else {
        return Some(preview_sentence(view, activity, over));
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
            _ => preview_sentence(view, activity, over),
        }),
    }
}

/// Why there is no picture, in this state, in plain words.
fn preview_sentence(
    view: &DeviceView,
    activity: Option<&ActivityView>,
    over: UiLinkKind,
) -> String {
    if activity.is_none()
        && let Some(sentence) = firmware_face_preview_sentence(&view.firmware_face)
    {
        return sentence;
    }
    if let Some(activity) = activity {
        let label = activity.label.trim_end_matches(['…', '.', ' ']);
        return format!("{label}… the picture returns when the board does.");
    }
    if view.loaded_project == LoadedProject::Empty {
        return "Nothing loaded — no picture until something runs.".to_string();
    }
    // The card's live picture is not streamed over Bluetooth (that air time
    // is the board's ESP-NOW's too), so "coming" would be a promise. Over
    // USB and Wi‑Fi the feed runs, and the picture is on its way.
    if over == UiLinkKind::Bluetooth {
        return "No live picture over Bluetooth — Open in editor to see and control it."
            .to_string();
    }
    "No picture yet — the live feed is coming.".to_string()
}

#[cfg(test)]
mod tests {
    use lpa_devices::ActivityKind;
    use lpa_devices::view::FirmwareFace;

    use super::super::card_fixtures::{CardFixture, activity, feed};
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
        let usb = UiLinkKind::Usb;
        assert_eq!(
            preview_slot_sentence(&card, None, Some(&feed(FeedLiveness::Live, true)), usb),
            None
        );
        assert!(
            preview_slot_sentence(&card, None, Some(&feed(FeedLiveness::Live, false)), usb)
                .is_some_and(|s| s.contains("too large to preview")),
        );
        assert_eq!(
            preview_slot_sentence(&card, None, Some(&feed(FeedLiveness::Waiting, true)), usb),
            Some("Waiting for the first frame…".to_string())
        );
        let lens_no_frame = DeviceCardFeedView {
            frame: None,
            liveness: FeedLiveness::Lens,
            frame_age_secs: None,
            engine_fps: None,
        };
        assert_eq!(
            preview_slot_sentence(&card, None, Some(&lens_no_frame), usb),
            Some("Picture paused while the editor is open.".to_string())
        );
        assert_eq!(
            preview_slot_sentence(&card, None, None, usb),
            Some(preview_sentence(&card, None, usb))
        );
        let flashing = activity(ActivityKind::Flash, "Flashing firmware…", None);
        assert_eq!(
            preview_slot_sentence(
                &card,
                Some(&flashing),
                Some(&feed(FeedLiveness::Live, true)),
                usb
            ),
            Some(preview_sentence(&card, Some(&flashing), usb))
        );
    }

    /// Ported: every state's picture line says something honest.
    #[test]
    fn every_state_has_an_honest_preview_sentence() {
        let usb = UiLinkKind::Usb;
        let mut card = CardFixture::ready().view;
        assert_eq!(
            preview_sentence(&card, None, usb),
            "No picture yet — the live feed is coming."
        );
        card.loaded_project = LoadedProject::Empty;
        assert_eq!(
            preview_sentence(&card, None, usb),
            "Nothing loaded — no picture until something runs."
        );
        card.firmware_face = FirmwareFace::Blank;
        assert_eq!(
            preview_sentence(&card, None, usb),
            "Nothing running — a blank chip has no picture."
        );
        // An activity outranks the blank-chip reading, and the label's own
        // trailing ellipsis is not doubled.
        let flashing = activity(ActivityKind::Flash, "Flashing firmware…", None);
        assert_eq!(
            preview_sentence(&card, Some(&flashing), usb),
            "Flashing firmware… the picture returns when the board does."
        );
    }

    /// Ported: a network board is blocked for firmware on Bluetooth and the
    /// LAN alike, but only Bluetooth goes without a picture.
    #[test]
    fn a_wifi_card_waits_for_its_picture_and_never_names_bluetooth() {
        let card = CardFixture::ready().over(UiLinkKind::Wifi).view;
        assert_eq!(
            preview_sentence(&card, None, UiLinkKind::Wifi),
            "No picture yet — the live feed is coming."
        );
        assert_eq!(
            preview_sentence(&card, None, UiLinkKind::Relay),
            "No picture yet — the live feed is coming."
        );
        assert!(preview_sentence(&card, None, UiLinkKind::Bluetooth).contains("over Bluetooth"));
        // Through the card's own input: the link comes from the endpoint.
        let mut relay = CardFixture::ready().over(UiLinkKind::Relay);
        assert_eq!(
            picture_line(&relay.input()).as_deref(),
            Some("No picture yet — the live feed is coming.")
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
