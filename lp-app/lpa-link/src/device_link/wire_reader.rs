//! One port's reading half: bytes in, lines and wire messages out, and the
//! packed-reply opt-in kept honest along the way.
//!
//! [`WireReader`] is [`lpc_wire::WireStream`] (the frame-aware splitter every
//! host reader shares) plus [`lpc_wire::PackOptIn`] (when to ask a board to
//! pack its replies, and when to ask again). It exists so a port with **more
//! than one drainer** keeps one of each: the browser's Web Serial port is
//! drained by the model's link pump and, while the editor lens or a coarse
//! effect borrows the wire, by an `lpa-client` conversation. Two splitters
//! would each tear the frame that straddles the handover; two opt-in states
//! would disagree about what the board was asked.
//!
//! Sans-IO: time is the caller's (`now_ms`), and the opt-in request is handed
//! back as [`WireRead::Send`] for the caller to write.
//!
//! It also says, once per change, what the link's encoding is and why
//! ([`WireRead::Note`]) — a board that stays on JSON because its dictionary
//! is not this build's is otherwise invisible, since every reader decodes
//! both forms.

use std::cell::Cell;

use lpc_wire::{
    ClientMessage, PACK_OPT_IN_REQUEST_ID, PackOptIn, ServerMsgBody, WIRE_DICTIONARY_FINGERPRINT,
    WIRE_PROTO_VERSION, WIRE_STREAM_MAX_FRAME, WireChunk, WireEncoding, WireServerMessage,
    WireStream,
};

thread_local! {
    /// Whether this page's browser readers ask boards to pack. See
    /// [`set_packed_replies_wanted`].
    static PACKED_REPLIES_WANTED: Cell<bool> = const { Cell::new(true) };
}

/// Whether the browser's readers (Web Serial and the tab emulator) ask a
/// board to pack its replies. On by default; Studio's dev-only `?wire=json`
/// turns it off for a page, so the same build can be measured both ways.
/// Readers built after the call take it; one already reading keeps what it
/// had.
pub fn set_packed_replies_wanted(wanted: bool) {
    PACKED_REPLIES_WANTED.with(|cell| cell.set(wanted));
}

/// See [`set_packed_replies_wanted`].
pub fn packed_replies_wanted() -> bool {
    PACKED_REPLIES_WANTED.with(Cell::get)
}

/// One thing a [`WireReader`] produced, in stream order.
#[derive(Debug)]
pub enum WireRead {
    /// A console line (not a wire message).
    Line(String),
    /// One wire message, decoded once here so no reader decodes it twice.
    Frame(ReadFrame),
    /// A packed frame that could not be delivered (torn, too long, not
    /// decodable). Never silence.
    Error(String),
    /// Write this request to the board now, as an `M!{json}` line.
    Send(ClientMessage),
    /// The link's encoding changed, or was settled, and this says how. At
    /// most one per change; never per frame.
    Note(String),
}

/// One wire message and the form it came in.
#[derive(Debug)]
pub struct ReadFrame {
    /// The JSON its `M!` line carries (or, for a packed frame, would have).
    pub json: String,
    /// Whether it came as a packed frame.
    pub packed: bool,
    /// The message, or why the JSON did not decode (a JSON line with console
    /// text spliced into it — the demux resyncs those).
    pub message: Result<WireServerMessage, String>,
}

impl ReadFrame {
    /// The `M!{json}` line this message is, or stands for.
    pub fn to_line(&self) -> String {
        format!("M!{}", self.json)
    }
}

/// What the reader last said about the link, so it says each thing once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Told {
    Nothing,
    StaysJson,
    Packed,
    FellBack,
}

/// One port's reader. See the module docs.
pub struct WireReader {
    stream: WireStream,
    opt_in: PackOptIn,
    wanted: bool,
    told: Told,
    /// Whether a fallback has been noted on this link (only the first is).
    fallback_noted: bool,
    /// Packed frames since the link last (re)opened, for the notes.
    packed_frames: u64,
}

impl WireReader {
    /// A reader that asks for packed replies when `wanted`.
    pub fn new(wanted: bool) -> Self {
        Self {
            stream: WireStream::new(),
            opt_in: PackOptIn::new(wanted),
            wanted,
            told: Told::Nothing,
            fallback_noted: false,
            packed_frames: 0,
        }
    }

    /// The encoding the board is writing this link's replies in, as far as
    /// this side knows.
    pub fn encoding(&self) -> WireEncoding {
        self.opt_in.encoding()
    }

    /// Feed bytes as they arrive (any split) at `now_ms`, calling `on` for
    /// everything they complete, in stream order.
    pub fn push(&mut self, bytes: &[u8], now_ms: u64, mut on: impl FnMut(WireRead)) {
        let Self {
            stream,
            opt_in,
            wanted,
            told,
            fallback_noted,
            packed_frames,
        } = self;
        stream.push(bytes, |chunk| match chunk {
            WireChunk::Line(line) => on(WireRead::Line(line)),
            WireChunk::Error(error) => on(WireRead::Error(error)),
            WireChunk::Frame(frame) => {
                let packed = frame.is_packed();
                let message = lpc_wire::json::from_str::<WireServerMessage>(&frame.json)
                    .map_err(|error| format!("malformed M! frame: {error}"));
                let Ok(decoded) = &message else {
                    on(WireRead::Frame(ReadFrame {
                        json: frame.json,
                        packed,
                        message,
                    }));
                    return;
                };
                if packed {
                    *packed_frames += 1;
                }
                let before = opt_in.encoding();
                let step = opt_in.observe(decoded, packed, now_ms);
                let after = opt_in.encoding();
                let seen = Seen {
                    before,
                    after,
                    packed_frames: *packed_frames,
                };
                if let Some(note) = note_for(*wanted, told, fallback_noted, decoded, seen) {
                    on(WireRead::Note(note));
                }
                if let Some(request) = step.send {
                    on(WireRead::Send(request));
                }
                if step.deliver {
                    on(WireRead::Frame(ReadFrame {
                        json: frame.json,
                        packed,
                        message,
                    }));
                }
            }
        });
    }

    /// Forget a partial line or frame and the opt-in: the port (re)opened or
    /// the board reset, and the board has forgotten it was asked.
    pub fn clear(&mut self) {
        *self = Self::new(self.wanted);
    }
}

/// What one message moved: the encoding before and after it, and the packed
/// frames so far.
struct Seen {
    before: WireEncoding,
    after: WireEncoding,
    packed_frames: u64,
}

/// The one note (if any) this message earns. See [`Told`].
fn note_for(
    wanted: bool,
    told: &mut Told,
    fallback_noted: &mut bool,
    message: &WireServerMessage,
    seen: Seen,
) -> Option<String> {
    let Seen {
        before,
        after,
        packed_frames,
    } = seen;
    if !wanted {
        return None;
    }
    let own_answer = message.id == PACK_OPT_IN_REQUEST_ID;
    let (next, text) = match &message.msg {
        ServerMsgBody::Hello(hello) if hello.proto != WIRE_PROTO_VERSION => (
            Told::StaysJson,
            format!(
                "wire: replies stay JSON — the board speaks proto {}, this build {}",
                hello.proto, WIRE_PROTO_VERSION
            ),
        ),
        ServerMsgBody::Hello(hello) if hello.pack_dictionary == 0 => (
            Told::StaysJson,
            "wire: replies stay JSON — the board does not pack (its hello offers no dictionary)"
                .to_string(),
        ),
        ServerMsgBody::Hello(hello) if hello.pack_dictionary != WIRE_DICTIONARY_FINGERPRINT => (
            Told::StaysJson,
            format!(
                "wire: replies stay JSON — the board packs with dictionary {:#010x}, this build \
                 with {WIRE_DICTIONARY_FINGERPRINT:#010x}",
                hello.pack_dictionary
            ),
        ),
        ServerMsgBody::SetEncoding {
            encoding: WireEncoding::Json,
        } if own_answer => (
            Told::StaysJson,
            "wire: replies stay JSON — the board answered the opt-in with `json`".to_string(),
        ),
        _ if before == WireEncoding::Json && after == WireEncoding::Packed => (
            Told::Packed,
            format!(
                "wire: replies packed (JSON Pack, dictionary {WIRE_DICTIONARY_FINGERPRINT:#010x})"
            ),
        ),
        // A board that drops the opt-in (a de-enumerate, a reboot, a host
        // that stopped draining) is asked again by `PackOptIn`; the first
        // fallback is worth a line, a board that keeps doing it is not
        // worth one each time.
        _ if before == WireEncoding::Packed && after == WireEncoding::Json && !*fallback_noted => {
            *fallback_noted = true;
            (
                Told::FellBack,
                format!(
                    "wire: the board fell back to JSON after {packed_frames} packed frame(s); \
                     asking again"
                ),
            )
        }
        _ => return None,
    };
    // A repeat of what was last said is not a change. The one exception is
    // `Packed`: after a fallback, packing again IS news.
    if next == *told {
        return None;
    }
    *told = next;
    Some(text)
}

/// The length of the longest prefix of `bytes` that ends on a boundary: after
/// a newline outside any packed frame, or after a packed frame's closing
/// `0x00`. For a drainer that hands the rest back for the next one to read
/// (the tab emulator's conversation io), so a packed frame is never cut —
/// which a plain "up to the last newline" cut does, since a frame may hold
/// any byte, `\n` included.
///
/// `bytes` must start on a boundary (the previous call's cut). A frame body
/// longer than [`WIRE_STREAM_MAX_FRAME`] is treated as the torn frame it is
/// and the whole buffer is released, so a stray `0x00` cannot hold bytes back
/// forever.
pub fn complete_prefix_len(bytes: &[u8]) -> usize {
    let mut cut = 0;
    let mut frame_start: Option<usize> = None;
    for (at, &byte) in bytes.iter().enumerate() {
        match (byte, frame_start) {
            (0, None) => frame_start = Some(at),
            (0, Some(_)) => {
                frame_start = None;
                cut = at + 1;
            }
            (b'\n', None) => cut = at + 1,
            _ => {}
        }
    }
    match frame_start {
        Some(start) if bytes.len() - start > WIRE_STREAM_MAX_FRAME => bytes.len(),
        _ => cut,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_wire::server::hello::{BuildFacts, HardwareFacts, ServerHello};

    #[test]
    fn a_matching_hello_asks_and_the_answer_is_swallowed() {
        let mut reader = WireReader::new(true);
        let mut reads = Vec::new();

        reader.push(&json_line(&hello(WIRE_DICTIONARY_FINGERPRINT)), 0, |r| {
            reads.push(r)
        });
        assert!(matches!(&reads[0], WireRead::Send(ask) if ask.id == PACK_OPT_IN_REQUEST_ID));
        assert!(matches!(&reads[1], WireRead::Frame(frame) if !frame.packed));
        assert_eq!(reads.len(), 2);

        reads.clear();
        reader.push(&json_line(&answer(WireEncoding::Packed)), 5, |r| {
            reads.push(r)
        });
        assert_eq!(reads.len(), 1, "{reads:?}");
        assert!(matches!(&reads[0], WireRead::Note(note) if note.contains("replies packed")));
        assert_eq!(reader.encoding(), WireEncoding::Packed);

        reads.clear();
        reader.push(&packed(&other(4)), 10, |r| reads.push(r));
        assert!(matches!(
            reads.as_slice(),
            [WireRead::Line(empty), WireRead::Frame(frame)] if empty.is_empty() && frame.packed
        ));
    }

    #[test]
    fn a_board_with_another_dictionary_stays_json_and_says_so_once() {
        let mut reader = WireReader::new(true);
        let mut notes = Vec::new();
        for t in [0, 10_000] {
            reader.push(
                &json_line(&hello(WIRE_DICTIONARY_FINGERPRINT ^ 1)),
                t,
                |r| {
                    assert!(!matches!(r, WireRead::Send(_)), "never asked");
                    if let WireRead::Note(note) = r {
                        notes.push(note);
                    }
                },
            );
        }
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("stay JSON") && notes[0].contains("dictionary"));
    }

    #[test]
    fn a_board_that_does_not_pack_stays_json_and_says_so() {
        let mut reader = WireReader::new(true);
        let notes = notes_of(&mut reader, &json_line(&hello(0)), 0);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("does not pack"), "{notes:?}");
    }

    #[test]
    fn a_refusal_is_noted() {
        let mut reader = WireReader::new(true);
        notes_of(
            &mut reader,
            &json_line(&hello(WIRE_DICTIONARY_FINGERPRINT)),
            0,
        );
        let notes = notes_of(&mut reader, &json_line(&answer(WireEncoding::Json)), 1);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("answered the opt-in with `json`"));
    }

    #[test]
    fn a_fallback_is_noted_once_and_packing_again_is_news() {
        let mut reader = WireReader::new(true);
        notes_of(
            &mut reader,
            &json_line(&hello(WIRE_DICTIONARY_FINGERPRINT)),
            0,
        );
        notes_of(&mut reader, &json_line(&answer(WireEncoding::Packed)), 1);

        let fell = notes_of(&mut reader, &json_line(&other(0)), 5_000);
        assert_eq!(fell.len(), 1);
        assert!(fell[0].contains("fell back"), "{fell:?}");

        let again = notes_of(&mut reader, &packed(&other(0)), 5_100);
        assert!(again[0].contains("replies packed"), "{again:?}");

        // A second fallback is not worth another line.
        assert!(notes_of(&mut reader, &json_line(&other(0)), 20_000).is_empty());
    }

    #[test]
    fn a_host_that_wants_json_never_asks_and_never_notes() {
        let mut reader = WireReader::new(false);
        let mut reads = Vec::new();
        reader.push(&json_line(&hello(WIRE_DICTIONARY_FINGERPRINT)), 0, |r| {
            reads.push(r)
        });
        assert!(
            matches!(reads.as_slice(), [WireRead::Frame(_)]),
            "{reads:?}"
        );
    }

    #[test]
    fn clear_forgets_the_opt_in_and_the_partial_frame() {
        let mut reader = WireReader::new(true);
        notes_of(
            &mut reader,
            &json_line(&hello(WIRE_DICTIONARY_FINGERPRINT)),
            0,
        );
        notes_of(&mut reader, &json_line(&answer(WireEncoding::Packed)), 1);
        let frame = packed(&other(9));
        reader.push(&frame[..frame.len() / 2], 2, |_| {});

        reader.clear();

        assert_eq!(reader.encoding(), WireEncoding::Json);
        let mut reads = Vec::new();
        reader.push(b"after\n", 3, |r| reads.push(r));
        assert!(
            matches!(reads.as_slice(), [WireRead::Line(line)] if line == "after"),
            "{reads:?}"
        );
    }

    #[test]
    fn a_json_line_with_spliced_text_is_still_handed_on() {
        let mut reader = WireReader::new(true);
        let mut reads = Vec::new();
        reader.push(b"M!{\"id\":0,\"msM![INIT] log\n", 0, |r| reads.push(r));
        assert!(matches!(&reads[0], WireRead::Frame(frame) if frame.message.is_err()));
    }

    #[test]
    fn the_complete_prefix_never_cuts_a_packed_frame() {
        let frame = packed(&other(10));
        let body = &frame[1..];
        assert!(
            body.contains(&b'\n'),
            "id 10 puts a newline byte inside the frame body"
        );
        let mut bytes = b"boot line\n".to_vec();
        bytes.extend_from_slice(&frame);
        bytes.extend_from_slice(b"half a li");

        // Whole: up to the frame's closing zero, the partial line held back.
        assert_eq!(complete_prefix_len(&bytes), bytes.len() - "half a li".len());
        // Cut inside the frame: back to before it, whatever it holds.
        let inside = 10 + frame.len() - 2;
        assert_eq!(complete_prefix_len(&bytes[..inside]), 11);
        assert_eq!(complete_prefix_len(b"no newline"), 0);
    }

    fn notes_of(reader: &mut WireReader, bytes: &[u8], now_ms: u64) -> Vec<String> {
        let mut notes = Vec::new();
        reader.push(bytes, now_ms, |r| {
            if let WireRead::Note(note) = r {
                notes.push(note);
            }
        });
        notes
    }

    fn json_line(message: &WireServerMessage) -> Vec<u8> {
        format!("M!{}\n", lpc_wire::json::to_string(message).unwrap()).into_bytes()
    }

    fn packed(message: &WireServerMessage) -> Vec<u8> {
        let mut framed = vec![0u8; 4096];
        let n = lpc_wire::ser_packed_frame_to(&mut framed, message).unwrap();
        framed.truncate(n);
        framed
    }

    fn hello(pack_dictionary: u32) -> WireServerMessage {
        let hello = ServerHello {
            proto: WIRE_PROTO_VERSION,
            build: BuildFacts {
                features: vec![],
                package: "fw-esp32c6".to_string(),
                commit: "unknown".to_string(),
                dirty: false,
                profile: "release-esp32".to_string(),
            },
            hardware: HardwareFacts::default(),
            device_uid: None,
            pack_dictionary,
            auth: lpc_wire::HelloAuth::TRUSTED,
        };
        WireServerMessage::new(0, ServerMsgBody::Hello(hello))
    }

    fn answer(encoding: WireEncoding) -> WireServerMessage {
        WireServerMessage::new(
            PACK_OPT_IN_REQUEST_ID,
            ServerMsgBody::SetEncoding { encoding },
        )
    }

    fn other(id: u64) -> WireServerMessage {
        WireServerMessage::new(id, ServerMsgBody::StopAllProjects)
    }
}
