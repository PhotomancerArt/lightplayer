//! The frame a playlist holds across a switch.
//!
//! Only one entry is ever loaded (multi-pattern vision D8), so a switch
//! cannot crossfade two live entries: the old one is unloaded before the new
//! one loads. Instead the playlist copies what it showed on the switch's
//! first frame — its own output, one RGBA16 sample per lamp, in the order the
//! consumer asked for them — shows that copy until the new entry renders for
//! real, then fades from it (plan PD5).
//!
//! The buffer lives only while a switch runs: it is sized on the capture
//! frame and freed when the fade ends. Every frame between reuses it, so a
//! transition adds no per-frame allocation (the tick-alloc rule from defect
//! `2026-08-29-flash-write-wedges-under-zook-playback`).
//!
//! The held samples are replayed by POSITION in the stream: sample `i` of a
//! later frame is shown the held sample `i`. That is right only because a
//! fixture walks its lamps in the same order every frame — its sample points
//! are its mapping's, cached and replayed per frame
//! (`tests/playlist_switch.rs` pins this for the fixture consumer).

use alloc::vec::Vec;

use crate::node::NodeError;

/// A lamp-sized RGBA16 copy of a playlist's output, alive for one switch.
#[derive(Default)]
pub(super) struct PlaylistHeldFrame {
    /// Four `u16` words per lamp, in the consumer's sample order.
    samples: Vec<u16>,
    /// A capture completed: `samples` is a whole frame.
    held: bool,
    /// Words the last whole live frame streamed — the capture's size hint,
    /// so the capture reserves once instead of growing batch by batch.
    last_frame_words: usize,
}

impl PlaylistHeldFrame {
    /// Whether a whole frame is held.
    pub(super) fn is_held(&self) -> bool {
        self.held
    }

    /// Held words (four per lamp).
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.samples.len()
    }

    /// Remember how many words a live frame streamed (no allocation).
    pub(super) fn note_frame_words(&mut self, words: usize) {
        self.last_frame_words = words;
    }

    /// Start a capture: forget any held frame and reserve the last live
    /// frame's size. Fallible: an allocation the heap refuses skips the
    /// capture, it never aborts (abort-tier firmware).
    pub(super) fn begin_capture(&mut self) -> Result<(), NodeError> {
        self.samples.clear();
        self.held = false;
        self.reserve(self.last_frame_words)
    }

    /// Append one batch of the frame being captured.
    pub(super) fn capture(&mut self, batch: &[u16]) -> Result<(), NodeError> {
        self.reserve(batch.len())?;
        self.samples.extend_from_slice(batch);
        Ok(())
    }

    /// The capture frame ended: what was captured is the held frame.
    pub(super) fn finish_capture(&mut self) {
        self.held = !self.samples.is_empty();
    }

    /// The held words `offset..offset + len`, as far as the held frame
    /// reaches (a later frame that streams more lamps than were captured
    /// gets a short slice; the caller pads it).
    pub(super) fn words(&self, offset: usize, len: usize) -> &[u16] {
        let start = offset.min(self.samples.len());
        let end = offset.saturating_add(len).min(self.samples.len());
        &self.samples[start..end]
    }

    /// Mutable held words, for re-capturing a blended frame in place.
    pub(super) fn words_mut(&mut self, offset: usize, len: usize) -> &mut [u16] {
        let start = offset.min(self.samples.len());
        let end = offset.saturating_add(len).min(self.samples.len());
        &mut self.samples[start..end]
    }

    /// The switch is over: free the buffer.
    pub(super) fn release(&mut self) {
        self.samples = Vec::new();
        self.held = false;
    }

    /// Whether the buffer holds memory (a capture in progress, or a held
    /// frame).
    #[cfg(test)]
    pub(super) fn holds_memory(&self) -> bool {
        self.samples.capacity() > 0
    }

    fn reserve(&mut self, additional: usize) -> Result<(), NodeError> {
        self.samples.try_reserve(additional).map_err(|_| {
            NodeError::msg(alloc::format!(
                "playlist held frame: {additional} more words refused; the switch cuts instead \
                 of holding"
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_capture_holds_the_batches_in_stream_order() {
        let mut frame = PlaylistHeldFrame::default();
        frame.note_frame_words(8);
        frame.begin_capture().expect("reserve");
        frame.capture(&[1, 2, 3, 4]).expect("batch 0");
        frame.capture(&[5, 6, 7, 8]).expect("batch 1");
        assert!(!frame.is_held(), "not held until the capture frame ends");
        frame.finish_capture();

        assert!(frame.is_held());
        assert_eq!(frame.len(), 8);
        assert_eq!(frame.words(4, 4), &[5, 6, 7, 8]);
        assert_eq!(
            frame.words(6, 4),
            &[7, 8],
            "a longer stream gets a short slice"
        );
    }

    #[test]
    fn release_frees_the_buffer() {
        let mut frame = PlaylistHeldFrame::default();
        frame.begin_capture().expect("reserve");
        frame.capture(&[1, 2, 3, 4]).expect("batch");
        frame.finish_capture();
        assert!(frame.holds_memory());

        frame.release();

        assert!(!frame.is_held());
        assert!(
            !frame.holds_memory(),
            "the held frame lives only for a switch"
        );
    }

    #[test]
    fn an_empty_capture_holds_nothing() {
        let mut frame = PlaylistHeldFrame::default();
        frame.begin_capture().expect("reserve");
        frame.finish_capture();
        assert!(!frame.is_held());
    }
}
