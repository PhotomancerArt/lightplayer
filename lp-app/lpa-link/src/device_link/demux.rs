//! Whole serial lines → [`LinkEvent`]s: the `M!` demux, plus the byte-level
//! line splitter the browser controller already does in JS.
//!
//! On a serial wire, protocol frames and console output share one byte
//! stream: an `M!`-prefixed line IS a frame, everything else is device
//! output. Both matter to the model — frames are peer evidence, lines are how
//! a blank chip or somebody else's firmware gets diagnosed — so neither is
//! dropped here. Deciding which is which is ALL this module does; no
//! classification lives on this side of the seam.

use std::collections::VecDeque;

use lpa_devices::link::LinkEvent;

use crate::device_link::wire::decode_server_frame;

/// Demux one whole serial line into the event it is.
///
/// A malformed frame becomes [`LinkEvent::Error`] rather than being silently
/// discarded — the fold counts anomalies, and a wire that garbles every frame
/// must not read as a wire that is merely quiet.
///
/// Interleaved device output can corrupt a frame line by splicing into it
/// (logs and frames share the wire). When decoding fails and another `M!`
/// marker is embedded further along, decoding resyncs at it, mirroring the
/// shipped browser line wire's behavior.
pub fn demux_line(line: &str) -> LinkEvent {
    let Some(mut frame_json) = line.strip_prefix("M!") else {
        return LinkEvent::Line(line.to_string());
    };
    loop {
        match decode_server_frame(frame_json) {
            Ok(frame) => return LinkEvent::Frame(frame),
            Err(error) => match frame_json.find("M!").filter(|offset| *offset > 0) {
                Some(offset) => frame_json = &frame_json[offset + 2..],
                None => return LinkEvent::Error(error),
            },
        }
    }
}

/// Accumulates bytes and hands back whole lines.
///
/// The Rust twin of the browser controller's `drainCompleteLines`: split on
/// `\n`, strip a trailing `\r`, and keep the incomplete tail for the next
/// chunk. Bytes are buffered rather than decoded eagerly because a multi-byte
/// character can straddle a read boundary; each completed line is decoded
/// lossily, so a garbled byte costs that line and nothing after it.
#[derive(Debug, Default)]
pub struct LineSplitter {
    buffer: Vec<u8>,
}

impl LineSplitter {
    /// Feed a chunk; returns every line it completed.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(bytes);
        let mut lines = Vec::new();
        while let Some(newline) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.buffer.drain(..=newline).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            lines.push(String::from_utf8_lossy(&line).into_owned());
        }
        lines
    }

    /// Drop the incomplete tail. Called on a (re)open: a partial line from
    /// the previous port generation is not the start of this one's first
    /// line.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Bytes held back waiting for a newline. A tail that only grows is the
    /// mid-frame-cut signature.
    pub fn pending_bytes(&self) -> usize {
        self.buffer.len()
    }
}

/// Demux a chunk of bytes straight onto an event queue.
pub fn push_bytes(splitter: &mut LineSplitter, bytes: &[u8], events: &mut VecDeque<LinkEvent>) {
    for line in splitter.push(bytes) {
        events.push_back(demux_line(&line));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_devices::wire::ServerFrameBody;

    #[test]
    fn console_output_and_protocol_frames_come_out_of_one_stream() {
        let mut splitter = LineSplitter::default();
        let mut events = VecDeque::new();

        push_bytes(
            &mut splitter,
            b"ESP-ROM:esp32c6-20220919\r\nM!{\"id\":0,\"msg\":\"unloadProject\"}\n",
            &mut events,
        );

        assert!(matches!(
            events.pop_front(),
            Some(LinkEvent::Line(line)) if line == "ESP-ROM:esp32c6-20220919"
        ));
        assert!(matches!(
            events.pop_front(),
            Some(LinkEvent::Frame(frame)) if matches!(frame.body, ServerFrameBody::Other { .. })
        ));
        assert!(events.is_empty());
    }

    #[test]
    fn a_line_split_across_reads_is_delivered_once_whole() {
        let mut splitter = LineSplitter::default();

        assert!(splitter.push(b"M!{\"id\":0,").is_empty());
        assert!(
            splitter.pending_bytes() > 0,
            "the tail is held, not dropped"
        );
        let lines = splitter.push(b"\"msg\":\"unloadProject\"}\nnext\n");

        assert_eq!(
            lines,
            vec![
                "M!{\"id\":0,\"msg\":\"unloadProject\"}".to_string(),
                "next".to_string()
            ]
        );
        assert_eq!(splitter.pending_bytes(), 0);
    }

    #[test]
    fn a_reopen_drops_the_previous_generations_partial_line() {
        let mut splitter = LineSplitter::default();
        splitter.push(b"half a li");

        splitter.clear();
        let lines = splitter.push(b"whole\n");

        assert_eq!(lines, vec!["whole".to_string()]);
    }

    #[test]
    fn a_garbled_frame_is_an_anomaly_not_silence() {
        let event = demux_line("M!{not json");

        assert!(
            matches!(&event, LinkEvent::Error(message) if message.contains("malformed M! frame")),
            "{event:?}"
        );
    }

    /// Interleaved device output can splice into a frame line; the frame
    /// after the splice is still a frame.
    #[test]
    fn decoding_resyncs_at_an_embedded_marker() {
        let event =
            demux_line("M!{\"id\":0,\"msM![INIT] logM!{\"id\":9,\"msg\":\"unloadProject\"}");

        let LinkEvent::Frame(frame) = event else {
            panic!("expected a resynced frame, got {event:?}");
        };
        assert_eq!(frame.request_id, 9);
    }

    #[test]
    fn a_bare_marker_line_is_an_anomaly_rather_than_a_frame() {
        assert!(matches!(demux_line("M!"), LinkEvent::Error(_)));
    }
}
