//! Reassembles a recording's raw `wire` chunks into the messages they carry.
//!
//! A recording holds each transport chunk as it crossed the page's byte
//! chokepoint: a Web Serial write is a whole `M!{json}\n` line, but a read
//! is whatever the port delivered — half a line, three packed frames, a
//! board log line torn in two. So the chunks of each (transport, port,
//! direction) stream are concatenated and decoded the way `lp-cli wire
//! unpack` decodes a capture ([`WireUnpacker`]): JSON Pack frames become
//! their `M!{json}` line, JSON lines are read as they are, and everything
//! else is the board's own text. A frame that does not decode is reported,
//! never dropped.

use std::collections::HashMap;

use lpc_wire::WireUnpacker;
use serde_json::Value;

/// One (transport, port, direction) byte stream.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WireStreamKey {
    pub transport: String,
    pub port: String,
    /// `tx` (page → board) or `rx` (board → page).
    pub dir: String,
}

/// What a stream's bytes turned out to hold.
#[derive(Debug, Clone, PartialEq)]
pub enum WireItem {
    /// One protocol message. `packed` is its JSON Pack size on the wire
    /// (`None` for a plain `M!{json}` line); `line_len` is the size of its
    /// `M!{json}\n` line.
    Message {
        json: String,
        packed: Option<usize>,
        line_len: usize,
    },
    /// A line of anything else (the board's own log text), newline removed.
    Text(String),
    /// A frame that could not be decoded, and why.
    Undecodable(String),
}

/// Every stream of one recording, each with its own unpacker.
#[derive(Default)]
pub struct WireStreams {
    streams: HashMap<WireStreamKey, StreamState>,
}

impl WireStreams {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one chunk; returns what it completed, in order.
    pub fn push(&mut self, key: &WireStreamKey, bytes: &[u8]) -> Vec<WireItem> {
        self.streams.entry(key.clone()).or_default().push(bytes)
    }

    /// What the streams still hold when the recording ends: a torn packed
    /// frame, or a last line with no newline. Sorted for a stable order.
    pub fn finish(&mut self) -> Vec<(WireStreamKey, WireItem)> {
        let mut keys: Vec<_> = self.streams.keys().cloned().collect();
        keys.sort_by(|a, b| (&a.transport, &a.port, &a.dir).cmp(&(&b.transport, &b.port, &b.dir)));
        let mut items = Vec::new();
        for key in keys {
            let state = self.streams.get_mut(&key).expect("key from the map");
            for item in state.finish() {
                items.push((key.clone(), item));
            }
        }
        items
    }
}

#[derive(Default)]
struct StreamState {
    unpacker: WireUnpacker,
    /// Bytes of the current (not yet newline-terminated) text line.
    text: Vec<u8>,
}

impl StreamState {
    fn push(&mut self, bytes: &[u8]) -> Vec<WireItem> {
        let mut items = Vec::new();
        let mut out = Vec::new();
        // One byte at a time, so that a frame the unpacker delivers is known
        // to be exactly the line it appended in that push: the text around
        // it stays text, and the frame keeps its packed size.
        for byte in bytes {
            let mut frame = None;
            self.unpacker
                .push(core::slice::from_ref(byte), &mut out, |result| {
                    frame = Some(result)
                });
            match frame {
                Some(Ok(frame)) => {
                    let split = out.len().saturating_sub(frame.json_line_len);
                    self.take_text(&out[..split], &mut items);
                    let line = &out[split..];
                    items.push(message_item(line, Some(frame.wire_len)));
                }
                Some(Err(error)) => {
                    self.take_text(&out, &mut items);
                    items.push(WireItem::Undecodable(error));
                }
                None => self.take_text(&out, &mut items),
            }
            out.clear();
        }
        items
    }

    /// Append passed-through bytes to the text line, closing each line at
    /// its newline.
    fn take_text(&mut self, bytes: &[u8], items: &mut Vec<WireItem>) {
        for &byte in bytes {
            if byte == b'\n' {
                let line = std::mem::take(&mut self.text);
                if let Some(item) = text_item(&line) {
                    items.push(item);
                }
            } else {
                self.text.push(byte);
            }
        }
    }

    fn finish(&mut self) -> Vec<WireItem> {
        let mut items = Vec::new();
        if self.unpacker.in_frame() {
            items.push(WireItem::Undecodable(
                "the recording ended inside a packed frame".to_string(),
            ));
        }
        let line = std::mem::take(&mut self.text);
        if let Some(item) = text_item(&line) {
            items.push(item);
        }
        items
    }
}

/// A complete line (no newline): a JSON message if it is `M!{…}`, text
/// otherwise, nothing if blank.
fn text_item(line: &[u8]) -> Option<WireItem> {
    let trimmed = line.strip_suffix(b"\r").unwrap_or(line);
    if trimmed.starts_with(b"M!") {
        let mut with_newline = trimmed.to_vec();
        with_newline.push(b'\n');
        return Some(message_item(&with_newline, None));
    }
    let text = String::from_utf8_lossy(trimmed);
    let text = text.trim_end();
    if text.is_empty() {
        None
    } else {
        Some(WireItem::Text(text.to_string()))
    }
}

/// An `M!{json}\n` line as a message.
fn message_item(line: &[u8], packed: Option<usize>) -> WireItem {
    let body = line.strip_prefix(b"M!").unwrap_or(line);
    let body = body.strip_suffix(b"\n").unwrap_or(body);
    WireItem::Message {
        json: String::from_utf8_lossy(body).into_owned(),
        packed,
        line_len: line.len(),
    }
}

/// `kind id=… seq=… fin=…` for a message's JSON, or why it is not one.
///
/// The kind is the message's `msg` tag (`"hello"`, `{"projectRead":…}`),
/// one level deeper when that is itself a single tag
/// (`{"response":{"projectRead":…}}` → `response.projectRead`).
pub fn describe_message(json: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return format!("unparsed JSON ({} B)", json.len());
    };
    let mut parts = vec![message_kind(value.get("msg"))];
    if let Some(id) = value.get("id") {
        parts.push(format!("id={id}"));
    }
    if let Some(seq) = value.get("seq") {
        parts.push(format!("seq={seq}"));
    }
    if let Some(fin) = value.get("fin") {
        parts.push(format!("fin={fin}"));
    }
    parts.join(" ")
}

fn message_kind(msg: Option<&Value>) -> String {
    match msg {
        Some(Value::String(tag)) => tag.clone(),
        Some(Value::Object(map)) if map.len() == 1 => {
            let (tag, inner) = map.iter().next().expect("one entry");
            match inner {
                Value::Object(inner) if inner.len() == 1 => {
                    let (inner_tag, _) = inner.iter().next().expect("one entry");
                    format!("{tag}.{inner_tag}")
                }
                Value::String(inner_tag) => format!("{tag}.{inner_tag}"),
                _ => tag.clone(),
            }
        }
        Some(_) => "message".to_string(),
        None => "no msg".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::wire::handler::tests::packed_and_json;

    fn key(dir: &str) -> WireStreamKey {
        WireStreamKey {
            transport: "serial".into(),
            port: "3".into(),
            dir: dir.into(),
        }
    }

    #[test]
    fn a_packed_frame_split_across_chunks_decodes_once_with_its_size() {
        let (packed, json_line) = packed_and_json(4);
        let mut streams = WireStreams::new();
        let (a, b) = packed.split_at(5);
        let mut chunk = b"boot ok\n".to_vec();
        chunk.extend_from_slice(a);
        let first = streams.push(&key("rx"), &chunk);
        assert_eq!(first, vec![WireItem::Text("boot ok".into())]);
        let second = streams.push(&key("rx"), b);
        let [
            WireItem::Message {
                json,
                packed: Some(wire),
                line_len,
            },
        ] = second.as_slice()
        else {
            panic!("one packed message, got {second:?}");
        };
        assert_eq!(
            *wire,
            packed.len() - 1,
            "the firmware's leading newline is not the frame's"
        );
        assert_eq!(*line_len, json_line.len() - 1);
        assert!(json_line.contains(json.as_str()));
        assert_eq!(describe_message(json), "log id=4");
    }

    #[test]
    fn json_lines_and_text_are_read_as_lines_and_a_tail_is_kept() {
        let mut streams = WireStreams::new();
        let items = streams.push(
            &key("tx"),
            b"M!{\"id\":7,\"msg\":{\"projectRead\":{}}}\nhalf",
        );
        assert_eq!(items.len(), 1);
        let WireItem::Message {
            json, packed: None, ..
        } = &items[0]
        else {
            panic!("a JSON line, got {items:?}");
        };
        assert_eq!(describe_message(json), "projectRead id=7");
        assert_eq!(
            streams.finish(),
            vec![(key("tx"), WireItem::Text("half".into()))]
        );
    }

    #[test]
    fn a_torn_frame_at_the_end_is_named() {
        let (packed, _) = packed_and_json(4);
        let mut streams = WireStreams::new();
        assert!(streams.push(&key("rx"), &packed[..6]).is_empty());
        let rest = streams.finish();
        assert!(
            matches!(rest.as_slice(), [(_, WireItem::Undecodable(why))] if why.contains("ended inside"))
        );
    }

    #[test]
    fn a_nested_tag_names_both_levels() {
        assert_eq!(
            describe_message(
                r#"{"id":3,"seq":1,"fin":false,"msg":{"response":{"projectRead":{"x":1}}}}"#
            ),
            "response.projectRead id=3 seq=1 fin=false"
        );
        assert_eq!(describe_message(r#"{"id":0,"msg":"hello"}"#), "hello id=0");
    }
}
