//! Words the live-picture surfaces share: the sim card's ▶ tab and the
//! device card's preview slot say the same things about a frame's age, so
//! they say them from one place.

/// A frame age in units that read naturally — core's [`age_words`], the one
/// rule every surface that names an age reads (the board card's status
/// corner among them).
///
/// [`age_words`]: lpa_studio_core::age_words
pub(crate) fn frame_age_label(age_secs: f64) -> String {
    lpa_studio_core::age_words(age_secs)
}
