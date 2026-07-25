//! Pure incremental Server-Sent-Events parser: bytes in, [`SseEvent`]s out.
//!
//! No IO. Feed arbitrary byte chunks (they may split lines, events, or UTF-8
//! sequences anywhere); complete events are returned as they are terminated
//! by a blank line. Handles `\n` and `\r\n` line endings, multi-line `data:`
//! fields (joined with `\n` per the SSE spec), and `:` comment lines. Fields
//! other than `event` and `data` (`id`, `retry`) are ignored.

/// One parsed SSE event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SseEvent {
    /// The `event:` field, if any.
    pub event: Option<String>,
    /// The joined `data:` lines.
    pub data: String,
}

/// Incremental SSE parser state.
#[derive(Debug, Default)]
pub struct SseParser {
    /// Unconsumed bytes (a partial line, possibly mid-UTF-8).
    buf: Vec<u8>,
    /// `event:` of the event being accumulated.
    event: Option<String>,
    /// `data:` lines of the event being accumulated.
    data_lines: Vec<String>,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a chunk; returns every event completed by it.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.buf.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some(line) = self.take_line() {
            if let Some(ev) = self.consume_line(&line) {
                events.push(ev);
            }
        }
        events
    }

    /// Pop one complete line from the buffer (without its terminator),
    /// or `None` if no full line is buffered yet. Splitting on the ASCII
    /// `\n` byte is UTF-8 safe: a chunk ending mid-sequence simply stays
    /// buffered until the newline arrives.
    fn take_line(&mut self) -> Option<String> {
        let nl = self.buf.iter().position(|&b| b == b'\n')?;
        let mut line: Vec<u8> = self.buf.drain(..=nl).collect();
        line.pop(); // the '\n'
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        Some(String::from_utf8_lossy(&line).into_owned())
    }

    /// Process one line; returns a completed event on a blank line.
    fn consume_line(&mut self, line: &str) -> Option<SseEvent> {
        if line.is_empty() {
            // Blank line: dispatch if any data was accumulated.
            if self.data_lines.is_empty() {
                self.event = None;
                return None;
            }
            let ev = SseEvent {
                event: self.event.take(),
                data: self.data_lines.join("\n"),
            };
            self.data_lines.clear();
            return Some(ev);
        }
        if line.starts_with(':') {
            return None; // comment
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "event" => self.event = Some(value.to_string()),
            "data" => self.data_lines.push(value.to_string()),
            _ => {} // id, retry, unknown fields
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_simple_event() {
        let mut p = SseParser::new();
        let evs = p.feed(b"event: ping\ndata: {\"type\": \"ping\"}\n\n");
        assert_eq!(
            evs,
            vec![SseEvent {
                event: Some("ping".into()),
                data: "{\"type\": \"ping\"}".into()
            }]
        );
    }

    #[test]
    fn survives_arbitrary_chunk_boundaries() {
        let input = "event: content_block_delta\r\ndata: {\"text\": \"héllo\"}\r\n\r\nevent: message_stop\ndata: {}\n\n";
        let bytes = input.as_bytes();
        // Split at every possible boundary pair, including mid-UTF-8 (the
        // 'é' is two bytes).
        for split in 0..bytes.len() {
            let mut p = SseParser::new();
            let mut evs = p.feed(&bytes[..split]);
            evs.extend(p.feed(&bytes[split..]));
            assert_eq!(evs.len(), 2, "split at {split}");
            assert_eq!(evs[0].event.as_deref(), Some("content_block_delta"));
            assert_eq!(evs[0].data, "{\"text\": \"héllo\"}");
            assert_eq!(evs[1].event.as_deref(), Some("message_stop"));
        }
    }

    #[test]
    fn byte_at_a_time_matches_whole_feed() {
        let input = b"data: a\ndata: b\n\n: comment line\n\nevent: e\ndata: c\n\n";
        let mut whole = SseParser::new();
        let expect = whole.feed(input);
        let mut p = SseParser::new();
        let mut got = Vec::new();
        for b in input {
            got.extend(p.feed(&[*b]));
        }
        assert_eq!(got, expect);
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn multi_line_data_joins_with_newline() {
        let mut p = SseParser::new();
        let evs = p.feed(b"data: line one\ndata: line two\n\n");
        assert_eq!(evs[0].data, "line one\nline two");
    }

    #[test]
    fn comments_and_unknown_fields_are_ignored() {
        let mut p = SseParser::new();
        let evs = p.feed(b": keep-alive\nid: 42\nretry: 1000\ndata: x\n\n");
        assert_eq!(
            evs,
            vec![SseEvent {
                event: None,
                data: "x".into()
            }]
        );
    }

    #[test]
    fn blank_line_without_data_dispatches_nothing() {
        let mut p = SseParser::new();
        assert!(p.feed(b"\n\n\nevent: e\n\n").is_empty());
    }

    #[test]
    fn recorded_anthropic_stream_shape() {
        // Abbreviated recording of a real /v1/messages stream (text turn).
        let fixture = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"usage\":{\"input_tokens\":25,\"output_tokens\":1}}}\n",
            "\n",
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n",
            "\n",
            "event: ping\n",
            "data: {\"type\": \"ping\"}\n",
            "\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n",
            "\n",
            "event: content_block_stop\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n",
            "\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":12}}\n",
            "\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n",
            "\n",
        );
        let mut p = SseParser::new();
        let evs = p.feed(fixture.as_bytes());
        let kinds: Vec<_> = evs.iter().map(|e| e.event.as_deref().unwrap()).collect();
        assert_eq!(
            kinds,
            [
                "message_start",
                "content_block_start",
                "ping",
                "content_block_delta",
                "content_block_stop",
                "message_delta",
                "message_stop"
            ]
        );
    }
}
