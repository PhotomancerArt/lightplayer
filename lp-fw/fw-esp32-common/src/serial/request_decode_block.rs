//! The largest single block decoding a JSON client request asks the heap
//! for, read off the request's shape before it is decoded: what the request
//! gate ([`super::server_payload::request_refusal`]) holds the heap's largest
//! free block against.
//!
//! A request's bulk is one long string: a file write's data, or since wire
//! 41 a shader edit's body (`lpc_model::body_bytes`: the text itself, or a
//! binary body's base64). Before wire 41 a shader edit was a JSON array of
//! byte values, and the gate, written for a file write's base64, asked a
//! fragmented board for three times the block that edit needed
//! (`docs/defects/2026-10-08-shader-edits-over-wi-fi-are-refused-board-memory-busy.md`).
//!
//! The estimate is the largest of what each value builds, decoded by
//! `serde_json::from_str` (`server_payload::decode_client_payload`):
//!
//! - **a string with an escape** (a shader's newlines) of `L` characters:
//!   `serde_json` unescapes it into its scratch buffer, grown by doubling
//!   past what it holds, so a block of under `2·L`; the body's own copy (at
//!   most `L`) is a block of its own, and smaller;
//! - **a string of only base64's alphabet**: decoded where it lies, into
//!   3/4 of `L` (a file write's base64, a binary body);
//! - **any other string**: copied where it lies, `L`;
//! - **an array**: its own `Vec` of up to 64 B an element (what the
//!   elements own is counted as their own values), and never more than 3/4
//!   of its text.
//!
//! A request nested deeper than the scanner follows is measured as one
//! escaped string as long as the whole message.

/// Containers the scan follows; deeper requests are measured whole.
const MAX_DEPTH: usize = 32;

/// The most one element of an array takes in its `Vec` (a request's
/// structs, strings' headers, numbers), beside what it owns, which is
/// counted as its own value.
const ELEMENT_BYTES: usize = 64;

/// The largest single block decoding `data` (a JSON client message) asks
/// for, by its shape.
pub fn request_decode_block(data: &[u8]) -> usize {
    shape_block(data).unwrap_or(2 * data.len())
}

/// One open array or object.
#[derive(Clone, Copy)]
struct Frame {
    array: bool,
    start: usize,
    elements: usize,
}

/// [`request_decode_block`] by shape; `None` if the request nests deeper
/// than [`MAX_DEPTH`].
fn shape_block(data: &[u8]) -> Option<usize> {
    let mut stack = [Frame {
        array: false,
        start: 0,
        elements: 0,
    }; MAX_DEPTH];
    let mut depth = 0usize;
    let mut largest = 0usize;
    let mut i = 0usize;
    while i < data.len() {
        let byte = data[i];
        match byte {
            b'"' => {
                let start = i + 1;
                i = start;
                let mut escaped = false;
                let mut base64 = true;
                while i < data.len() && data[i] != b'"' {
                    if data[i] == b'\\' {
                        escaped = true;
                        i += 2;
                    } else {
                        base64 &= is_base64(data[i]);
                        i += 1;
                    }
                }
                let len = i.min(data.len()) - start;
                let block = if escaped {
                    2 * len
                } else if base64 {
                    len * 3 / 4
                } else {
                    len
                };
                largest = largest.max(block);
                note_element(&mut stack, depth);
            }
            b'[' | b'{' => {
                note_element(&mut stack, depth);
                if depth == MAX_DEPTH {
                    return None;
                }
                stack[depth] = Frame {
                    array: byte == b'[',
                    start: i,
                    elements: 0,
                };
                depth += 1;
            }
            b']' | b'}' => {
                if depth > 0 {
                    depth -= 1;
                    let frame = stack[depth];
                    if frame.array {
                        // The array's own `Vec`, its elements' contents
                        // being their own blocks (counted as they close):
                        // at most `ELEMENT_BYTES` each, and never more than
                        // 3/4 of its text.
                        let grown = frame.elements.next_power_of_two().max(8);
                        let block = (grown * ELEMENT_BYTES).min((i + 1 - frame.start) * 3 / 4);
                        largest = largest.max(block);
                    }
                }
            }
            b'-' | b'0'..=b'9' => {
                while i + 1 < data.len()
                    && matches!(data[i + 1], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                {
                    i += 1;
                }
                note_element(&mut stack, depth);
            }
            b't' | b'f' | b'n' => {
                // `true`, `false`, `null`: one element.
                while i + 1 < data.len() && data[i + 1].is_ascii_alphabetic() {
                    i += 1;
                }
                note_element(&mut stack, depth);
            }
            _ => {}
        }
        i += 1;
    }
    Some(largest)
}

/// A character of standard, padded base64.
fn is_base64(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=')
}

/// A value starts inside the innermost open container: count it.
fn note_element(stack: &mut [Frame; MAX_DEPTH], depth: usize) {
    if depth == 0 {
        return;
    }
    let frame = &mut stack[depth - 1];
    if frame.array {
        frame.elements += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::string::String;

    /// Studio's shader edit, as it goes on the wire since wire 41: the
    /// choker shader as one escaped string, unescaped into a scratch buffer
    /// of under twice its length.
    #[test]
    fn a_shader_edit_needs_twice_its_text_for_the_unescape() {
        let message = replace_body_edit(&shader(1_971));
        let body = body_span(&message);
        assert!(message.len() < 2_400, "{}", message.len());
        assert_eq!(request_decode_block(message.as_bytes()), 2 * body);
    }

    /// A file write's base64 is decoded where it lies: the old rule,
    /// unchanged.
    #[test]
    fn a_file_write_still_needs_three_quarters_of_its_blob() {
        let mut write =
            String::from(r#"{"id":4242,"msg":{"filesystem":{"write":{"path":"/a","data":""#);
        write.push_str(&"QUFB".repeat(3_000));
        write.push_str(r#""}}}}"#);
        assert_eq!(request_decode_block(write.as_bytes()), 12_000 * 3 / 4);
    }

    /// Text with no escape is copied whole.
    #[test]
    fn plain_text_needs_its_own_length() {
        let text = "a b".repeat(1_000);
        let message = format!(r#"{{"id":1,"s":"{text}"}}"#);
        assert_eq!(request_decode_block(message.as_bytes()), 3_000);
    }

    /// Arrays, nesting and escapes.
    #[test]
    fn other_values_take_their_own_rules() {
        // Numbers: 3/4 of the array's text, at most.
        let floats = format!("{{\"id\":1,\"v\":[{}]}}", ["0.25"; 400].join(","));
        let span = 400 * 5 - 1 + 2;
        assert_eq!(request_decode_block(floats.as_bytes()), span * 3 / 4);
        // A short array of objects: its own Vec, not the objects' text; the
        // objects' strings are their own blocks.
        let text = "a b".repeat(300);
        let objects = format!(r#"{{"id":1,"v":[{{"s":"{text}"}},{{"s":"{text}"}}]}}"#);
        assert_eq!(request_decode_block(objects.as_bytes()), 900);
        // An escaped quote does not end a string, and makes it escaped.
        assert_eq!(
            request_decode_block(br#"{"id":1,"s":"ab\"cdefgh"}"#),
            2 * "ab\\\"cdefgh".len()
        );
        // Too deep to follow: the whole message as one escaped string.
        let deep = format!("{}{}", "[".repeat(40), "]".repeat(40));
        assert_eq!(request_decode_block(deep.as_bytes()), 2 * deep.len());
    }

    /// A shader-like body of `len` bytes, a newline a line.
    fn shader(len: usize) -> String {
        "vec3 color = texture(palette, vec2(t, 0.5)).rgb;\n"
            .chars()
            .cycle()
            .take(len)
            .collect()
    }

    /// The characters between the quotes of the edit's body.
    fn body_span(message: &str) -> usize {
        let start = message.find(r#""replace_body":""#).unwrap() + r#""replace_body":""#.len();
        let bytes = message.as_bytes();
        let mut i = start;
        while bytes[i] != b'"' {
            i += if bytes[i] == b'\\' { 2 } else { 1 };
        }
        i - start
    }

    /// Studio's `SetArtifactBody`/`ReplaceBody` mutation of `body`, as its
    /// JSON goes on the wire.
    fn replace_body_edit(body: &str) -> String {
        format!(
            r#"{{"id":31,"msg":{{"projectCommand":{{"handle":1,"command":{{"mutateOverlay":{{"request":{{"batch":{{"commands":[{{"id":7,"mutation":{{"set_artifact_body":{{"artifact":{{"path":"/shader.glsl"}},"edit":{{"replace_body":"{}"}}}}}}}}]}}}}}}}}}}}}}}"#,
            body.replace('\n', "\\n")
        )
    }
}
